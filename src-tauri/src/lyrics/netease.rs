//! NetEase Cloud Music (music.163.com) — большая база, хорошо покрывает и русскую музыку.

use reqwest::header::REFERER;
use serde_json::Value;

use super::{client, lrc, matching, LyricsResult, Query};

const SITE: &str = "https://music.163.com/";

struct Song {
    id: i64,
    title: String,
    artist: String,
    duration_s: Option<f64>,
}

pub async fn find(q: Query) -> Result<Option<LyricsResult>, reqwest::Error> {
    for keyword in matching::keywords(&q) {
        let songs = search(&keyword).await?;
        let scores: Vec<_> = songs
            .iter()
            .map(|s| matching::score(&q, &s.title, &s.artist, s.duration_s))
            .collect();
        for i in matching::rank(&scores, 3) {
            if let Some(result) = lyric(&q, songs[i].id, songs[i].duration_s).await? {
                return Ok(Some(result));
            }
        }
    }
    Ok(None)
}

async fn search(keyword: &str) -> Result<Vec<Song>, reqwest::Error> {
    let response: Value = client()
        .post("https://music.163.com/api/search/get")
        .header(REFERER, SITE)
        .form(&[("s", keyword), ("type", "1"), ("offset", "0"), ("limit", "10")])
        .send()
        .await?
        .json()
        .await?;

    let songs = response
        .pointer("/result/songs")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|s| {
                    let artist = s
                        .get("artists")?
                        .as_array()?
                        .iter()
                        .filter_map(|a| a.get("name").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join(", ");
                    Some(Song {
                        id: s.get("id")?.as_i64()?,
                        title: s.get("name")?.as_str()?.to_string(),
                        artist,
                        duration_s: s.get("duration").and_then(Value::as_f64).map(|ms| ms / 1000.0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(songs)
}

async fn lyric(
    q: &Query,
    id: i64,
    duration_s: Option<f64>,
) -> Result<Option<LyricsResult>, reqwest::Error> {
    let response: Value = client()
        .get("https://music.163.com/api/song/lyric")
        .header(REFERER, SITE)
        .query(&[("id", id.to_string().as_str()), ("lv", "1"), ("kv", "1"), ("tv", "-1")])
        .send()
        .await?
        .json()
        .await?;

    let text = response.pointer("/lrc/lyric").and_then(Value::as_str).unwrap_or("");
    let synced = lrc::strip_credits(lrc::parse(text), q);
    if synced.len() < 3 {
        return Ok(None);
    }
    Ok(Some(LyricsResult {
        found: true,
        source: "NetEase".into(),
        synced,
        duration_ms: duration_s.map_or(0, |d| (d * 1000.0) as u64),
        ..Default::default()
    }))
}
