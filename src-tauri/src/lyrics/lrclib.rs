//! LRCLIB (https://lrclib.net) — бесплатная база синхронизированных текстов, без ключа.

use reqwest::StatusCode;
use serde::Deserialize;

use super::{client, lrc, matching, LyricsResult, Query};

const BASE: &str = "https://lrclib.net/api";
const USER_AGENT: &str = "AutoLyrics/0.1.0 (desktop lyrics viewer)";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    track_name: Option<String>,
    artist_name: Option<String>,
    duration: Option<f64>,
    instrumental: Option<bool>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

pub async fn find(q: Query) -> Result<Option<LyricsResult>, reqwest::Error> {
    // 1. Точное попадание по метаданным плеера.
    if let Some(item) = get(&q.title, &q.artist, &q.album, q.duration_s).await? {
        if let Some(result) = to_result(&item) {
            return Ok(Some(result));
        }
    }

    // 2. То же, но с очищенным названием и основным артистом.
    if q.clean_title != q.title || q.main_artist != q.artist {
        if let Some(item) = get(&q.clean_title, &q.main_artist, &q.album, q.duration_s).await? {
            if let Some(result) = to_result(&item) {
                return Ok(Some(result));
            }
        }
    }

    // 3. Поиск с выбором лучшего кандидата. Артист в базе может быть записан
    //    иначе («Miyagi & Эндшпиль feat. …»), поэтому пробуем разные запросы.
    let mut attempts: Vec<Vec<(&str, String)>> = vec![vec![
        ("track_name", q.clean_title.clone()),
        ("artist_name", q.main_artist.clone()),
    ]];
    for keyword in matching::keywords(&q).into_iter().take(3) {
        attempts.push(vec![("q", keyword)]);
    }

    for params in attempts {
        let params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let candidates = search(&params).await?;
        let scores: Vec<Option<i32>> = candidates.iter().map(|item| score(&q, item)).collect();
        if let Some(result) = matching::rank(&scores, 1)
            .first()
            .and_then(|&i| to_result(&candidates[i]))
        {
            return Ok(Some(result));
        }
    }
    Ok(None)
}

async fn get(
    title: &str,
    artist: &str,
    album: &str,
    dur_s: f64,
) -> Result<Option<Item>, reqwest::Error> {
    let mut query: Vec<(&str, String)> = vec![
        ("track_name", title.to_string()),
        ("artist_name", artist.to_string()),
    ];
    if !album.is_empty() {
        query.push(("album_name", album.to_string()));
    }
    if dur_s > 0.0 {
        query.push(("duration", format!("{}", dur_s.round() as u64)));
    }

    let response = client()
        .get(format!("{BASE}/get"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&query)
        .send()
        .await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    // сбой сервиса (5xx) — это ошибка, а не «текста нет»: пусть сработает повтор
    Ok(Some(response.error_for_status()?.json::<Item>().await?))
}

async fn search(params: &[(&str, &str)]) -> Result<Vec<Item>, reqwest::Error> {
    let response = client()
        .get(format!("{BASE}/search"))
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(params)
        .send()
        .await?;
    response.error_for_status()?.json::<Vec<Item>>().await
}

/// Оценка кандидата; `None` — не подходит. Синхронный текст предпочтительнее.
fn score(q: &Query, item: &Item) -> Option<i32> {
    let has_synced = item.synced_lyrics.as_deref().is_some_and(|s| !s.trim().is_empty());
    let has_plain = item.plain_lyrics.as_deref().is_some_and(|s| !s.trim().is_empty());
    if !has_synced && !has_plain && item.instrumental != Some(true) {
        return None;
    }
    let base = matching::score(
        q,
        item.track_name.as_deref().unwrap_or(""),
        item.artist_name.as_deref().unwrap_or(""),
        item.duration,
    )?;
    Some(base + if has_synced { 50 } else { 0 })
}

fn to_result(item: &Item) -> Option<LyricsResult> {
    if item.instrumental == Some(true) {
        return Some(LyricsResult {
            found: true,
            source: "LRCLIB".into(),
            instrumental: true,
            ..Default::default()
        });
    }

    let synced = item
        .synced_lyrics
        .as_deref()
        .map(lrc::parse)
        .unwrap_or_default();
    let plain = item.plain_lyrics.clone().filter(|p| !p.trim().is_empty());

    if synced.is_empty() && plain.is_none() {
        return None;
    }
    Some(LyricsResult {
        found: true,
        source: "LRCLIB".into(),
        origin: String::new(),
        synced,
        plain,
        instrumental: false,
        duration_ms: item.duration.map_or(0, |d| (d * 1000.0) as u64),
    })
}
