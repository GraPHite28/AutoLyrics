//! Следующий трек в альбоме — по открытому поиску iTunes (без ключа).
//!
//! Если человек слушает альбом подряд, следующую песню можно угадать заранее:
//! найти текущую в каталоге, взять её альбом и следующий по номеру трек.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::lyrics::{client, matching, Query};

#[derive(Clone, Debug, PartialEq)]
pub struct AlbumTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub disc: u32,
    pub number: u32,
    pub duration_ms: u64,
    pub genre: Option<String>,
    /// Ссылка на обложку (600×600).
    pub artwork: Option<String>,
}

/// Состав альбомов, уже запрошенных у iTunes: при прослушивании альбома подряд
/// второй и последующие треки не требуют нового запроса.
fn albums() -> &'static Mutex<HashMap<i64, Vec<AlbumTrack>>> {
    static ALBUMS: OnceLock<Mutex<HashMap<i64, Vec<AlbumTrack>>>> = OnceLock::new();
    ALBUMS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Следующий трек альбома после песни `title` — `artist` (`album_hint` помогает выбрать
/// нужное издание). `None`, если песня не найдена или она последняя.
pub async fn next_track(title: &str, artist: &str, album_hint: &str) -> Option<AlbumTrack> {
    let q = Query::new(title, artist, album_hint, 0);
    // Название альбома в запросе помогает найти именно студийное издание среди синглов и концертных.
    let term = format!("{} {} {}", q.main_artist, q.clean_title, album_hint);
    let search = get_json(
        "https://itunes.apple.com/search",
        &[
            ("term", term.trim()),
            ("media", "music"),
            ("entity", "song"),
            ("limit", "15"),
            ("lang", "en_us"),
        ],
    )
    .await?;
    let (collection, disc, number) = pick_current(&search, &q, album_hint)?;
    let tracks = album_tracks(collection).await?;
    pick_next(&tracks, disc, number)
}

async fn album_tracks(collection: i64) -> Option<Vec<AlbumTrack>> {
    if let Some(known) = albums().lock().ok()?.get(&collection) {
        return Some(known.clone());
    }
    let id = collection.to_string();
    let lookup = get_json(
        "https://itunes.apple.com/lookup",
        &[("id", id.as_str()), ("entity", "song"), ("lang", "en_us")],
    )
    .await?;
    let tracks = parse_album(&lookup);
    if tracks.is_empty() {
        return None;
    }
    albums().lock().ok()?.insert(collection, tracks.clone());
    Some(tracks)
}

async fn get_json(url: &str, params: &[(&str, &str)]) -> Option<Value> {
    client()
        .get(url)
        .query(params)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}

/// Какая запись поиска — наша песня: (id альбома, диск, номер трека).
fn pick_current(response: &Value, q: &Query, album_hint: &str) -> Option<(i64, u32, u32)> {
    let results = response.get("results")?.as_array()?;
    let hint = matching::flat(album_hint);
    let scores: Vec<Option<i32>> = results
        .iter()
        .map(|r| {
            let base = matching::score(
                q,
                r.get("trackName").and_then(Value::as_str).unwrap_or(""),
                r.get("artistName").and_then(Value::as_str).unwrap_or(""),
                None,
            )?;
            // тот же альбом, что назвал плеер или Shazam, — предпочтительнее
            let same_album = !hint.is_empty()
                && r.get("collectionName")
                    .and_then(Value::as_str)
                    .is_some_and(|c| matching::flat(c) == hint);
            Some(base + if same_album { 15 } else { 0 })
        })
        .collect();
    let best = &results[*matching::rank(&scores, 1).first()?];
    Some((
        best.get("collectionId")?.as_i64()?,
        best.get("discNumber").and_then(Value::as_u64).unwrap_or(1) as u32,
        best.get("trackNumber")?.as_u64()? as u32,
    ))
}

/// Треки альбома из ответа `lookup` (первая запись — сам альбом, её пропускаем).
fn parse_album(response: &Value) -> Vec<AlbumTrack> {
    let Some(results) = response.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    results
        .iter()
        .filter(|r| r.get("wrapperType").and_then(Value::as_str) == Some("track"))
        .filter_map(|r| {
            Some(AlbumTrack {
                title: text(r, "trackName")?,
                artist: text(r, "artistName").unwrap_or_default(),
                album: text(r, "collectionName").unwrap_or_default(),
                disc: r.get("discNumber").and_then(Value::as_u64).unwrap_or(1) as u32,
                number: r.get("trackNumber")?.as_u64()? as u32,
                duration_ms: r.get("trackTimeMillis").and_then(Value::as_u64).unwrap_or(0),
                genre: text(r, "primaryGenreName"),
                artwork: text(r, "artworkUrl100").map(|u| u.replace("100x100bb", "600x600bb")),
            })
        })
        .collect()
}

/// Трек, который идёт после (диск, номер) в порядке альбома.
fn pick_next(tracks: &[AlbumTrack], disc: u32, number: u32) -> Option<AlbumTrack> {
    let mut ordered: Vec<&AlbumTrack> = tracks.iter().collect();
    ordered.sort_by_key(|t| (t.disc, t.number));
    let position = ordered.iter().position(|t| t.disc == disc && t.number == number)?;
    // Через границу диска не идём: следующий диск у юбилейных изданий часто — концертная
    // запись или бонусы, и угадывание было бы ошибкой.
    ordered.get(position + 1).filter(|t| t.disc == disc).map(|t| (*t).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn track(disc: u32, number: u32, title: &str) -> Value {
        json!({
            "wrapperType": "track", "kind": "song", "trackName": title, "artistName": "Miyagi & Эндшпиль",
            "collectionName": "ANABIOS", "discNumber": disc, "trackNumber": number,
            "trackTimeMillis": 180000 + number * 1000, "primaryGenreName": "Hip-Hop/Rap",
            "artworkUrl100": "https://example.com/art/100x100bb.jpg"
        })
    }

    #[test]
    fn next_follows_album_order_even_if_lookup_is_shuffled() {
        let response = json!({"results": [
            {"wrapperType": "collection", "collectionName": "ANABIOS"},
            track(1, 3, "Три"), track(1, 1, "Раз"), track(1, 2, "Два"),
        ]});
        let tracks = parse_album(&response);
        assert_eq!(tracks.len(), 3);
        assert_eq!(pick_next(&tracks, 1, 1).unwrap().title, "Два");
        assert_eq!(pick_next(&tracks, 1, 2).unwrap().title, "Три");
        assert!(pick_next(&tracks, 1, 3).is_none(), "после последнего трека следующего нет");
        let next = pick_next(&tracks, 1, 1).unwrap();
        assert_eq!(next.duration_ms, 182_000);
        assert_eq!(next.artwork.as_deref(), Some("https://example.com/art/600x600bb.jpg"));
    }

    #[test]
    fn does_not_guess_across_discs() {
        let response = json!({"results": [track(1, 2, "Конец первого"), track(2, 1, "Начало второго")]});
        let tracks = parse_album(&response);
        assert!(pick_next(&tracks, 1, 2).is_none());
        assert!(pick_next(&tracks, 2, 1).is_none());
    }

    #[test]
    fn prefers_the_album_named_by_the_player() {
        let q = Query::new("Banger (feat. OISEAU & PAPILLON)", "Miyagi & Эндшпиль", "ANABIOS", 0);
        let mk = |collection: i64, name: &str, number: u32| {
            json!({"trackName": "Banger", "artistName": "Miyagi & Эндшпиль", "collectionId": collection,
                   "collectionName": name, "discNumber": 1, "trackNumber": number})
        };
        let response = json!({"results": [mk(1, "Hits Collection", 7), mk(2, "ANABIOS", 4)]});
        assert_eq!(pick_current(&response, &q, "ANABIOS"), Some((2, 1, 4)));
    }

    /// Живая проверка (нужен интернет): `cargo test live_next -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_next_track() {
        tauri::async_runtime::block_on(async {
            for (title, artist, album) in [
                ("Faint", "Linkin Park", "Meteora"),
                ("Numb", "Linkin Park", "Meteora"),
                ("Banger (feat. OISEAU & PAPILLON)", "Miyagi & Эндшпиль", "ANABIOS"),
                ("Сансара", "Баста", ""),
            ] {
                let next = next_track(title, artist, album).await;
                println!(
                    "{artist} — {title}  →  {}",
                    next.map_or("(нет)".to_string(), |n| format!(
                        "{} — {} [#{} {}s, {:?}, обложка: {}]",
                        n.artist, n.title, n.number, n.duration_ms / 1000, n.genre, n.artwork.is_some()
                    ))
                );
            }
        });
    }

    #[test]
    fn unknown_song_gives_nothing() {
        let q = Query::new("Ничего", "Никто", "", 0);
        let response = json!({"results": [
            {"trackName": "Banger", "artistName": "Tismé", "collectionId": 5, "trackNumber": 1}
        ]});
        assert!(pick_current(&response, &q, "").is_none());
    }
}
