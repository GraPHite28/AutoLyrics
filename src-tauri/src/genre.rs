//! Жанр трека по открытому поиску iTunes (без ключа) — для плееров, которые жанр не сообщают.
//! Для треков, распознанных по звуку, жанр приходит от Shazam и сюда не доходит.

use serde_json::Value;

use crate::lyrics::{client, matching, Query};

#[tauri::command]
pub async fn fetch_genre(title: String, artist: String) -> Option<String> {
    let q = Query::new(&title, &artist, "", 0);
    let term = format!("{} {}", q.main_artist, q.clean_title).trim().to_string();
    if term.is_empty() {
        return None;
    }

    let response: Value = client()
        .get("https://itunes.apple.com/search")
        .query(&[
            ("term", term.as_str()),
            ("media", "music"),
            ("entity", "song"),
            ("limit", "10"),
            ("lang", "en_us"),
        ])
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    best_genre(&response, &q)
}

/// Жанр самого подходящего трека из ответа iTunes.
fn best_genre(response: &Value, q: &Query) -> Option<String> {
    let results = response.get("results")?.as_array()?;
    let scores: Vec<Option<i32>> = results
        .iter()
        .map(|r| {
            matching::score(
                q,
                r.get("trackName").and_then(Value::as_str).unwrap_or(""),
                r.get("artistName").and_then(Value::as_str).unwrap_or(""),
                None,
            )
        })
        .collect();
    let best = *matching::rank(&scores, 1).first()?;
    results[best]
        .get("primaryGenreName")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn picks_genre_of_the_matching_song() {
        let q = Query::new("Banger (feat. OISEAU & PAPILLON)", "Miyagi & Эндшпиль", "", 0);
        let response = json!({"results": [
            {"trackName": "Banger", "artistName": "Tismé", "primaryGenreName": "Dance"},
            {"trackName": "Banger", "artistName": "Miyagi & Эндшпиль", "primaryGenreName": "Hip-Hop/Rap"},
        ]});
        assert_eq!(best_genre(&response, &q).as_deref(), Some("Hip-Hop/Rap"));
    }

    #[test]
    fn no_match_gives_no_genre() {
        let q = Query::new("Ничего", "Никто", "", 0);
        let response = json!({"results": [
            {"trackName": "Banger", "artistName": "Tismé", "primaryGenreName": "Dance"},
        ]});
        assert!(best_genre(&response, &q).is_none());
    }
}
