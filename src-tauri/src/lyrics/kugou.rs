//! KuGou (kugou.com): сначала ищем песню, затем текст по её хэшу.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

use super::{client, lrc, matching, LyricsResult, Query};

struct Song {
    hash: String,
    title: String,
    artist: String,
    duration_s: f64,
}

pub async fn find(q: Query) -> Result<Option<LyricsResult>, reqwest::Error> {
    for keyword in matching::keywords(&q) {
        let songs = search(&keyword).await?;
        let scores: Vec<_> = songs
            .iter()
            .map(|s| matching::score(&q, &s.title, &s.artist, Some(s.duration_s)))
            .collect();
        for i in matching::rank(&scores, 3) {
            if let Some(result) = lyric(&q, &keyword, &songs[i]).await? {
                return Ok(Some(result));
            }
        }
    }
    Ok(None)
}

async fn search(keyword: &str) -> Result<Vec<Song>, reqwest::Error> {
    let response: Value = client()
        .get("https://mobileservice.kugou.com/api/v3/search/song")
        .query(&[("format", "json"), ("keyword", keyword), ("page", "1"), ("pagesize", "10")])
        .send()
        .await?
        .json()
        .await?;

    let songs = response
        .pointer("/data/info")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|s| {
                    Some(Song {
                        hash: s.get("hash")?.as_str()?.to_string(),
                        title: s.get("songname")?.as_str()?.to_string(),
                        artist: s.get("singername")?.as_str()?.to_string(),
                        duration_s: s.get("duration").and_then(Value::as_f64).unwrap_or(0.0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(songs)
}

async fn lyric(q: &Query, keyword: &str, song: &Song) -> Result<Option<LyricsResult>, reqwest::Error> {
    let duration_ms = ((song.duration_s * 1000.0) as u64).to_string();
    let found: Value = client()
        .get("https://lyrics.kugou.com/search")
        .query(&[
            ("ver", "1"),
            ("man", "yes"),
            ("client", "pc"),
            ("keyword", keyword),
            ("duration", duration_ms.as_str()),
            ("hash", song.hash.as_str()),
        ])
        .send()
        .await?
        .json()
        .await?;

    let Some(candidate) = found
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
    else {
        return Ok(None);
    };
    let id = value_to_string(candidate.get("id"));
    let access_key = value_to_string(candidate.get("accesskey"));
    if id.is_empty() || access_key.is_empty() {
        return Ok(None);
    }

    let download: Value = client()
        .get("https://lyrics.kugou.com/download")
        .query(&[
            ("ver", "1"),
            ("client", "pc"),
            ("id", id.as_str()),
            ("accesskey", access_key.as_str()),
            ("fmt", "lrc"),
            ("charset", "utf8"),
        ])
        .send()
        .await?
        .json()
        .await?;

    let Some(bytes) = download
        .get("content")
        .and_then(Value::as_str)
        .and_then(|c| STANDARD.decode(c).ok())
    else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&bytes);
    let synced = lrc::strip_credits(lrc::parse(text.trim_start_matches('\u{feff}')), q);
    if synced.len() < 3 {
        return Ok(None);
    }
    Ok(Some(LyricsResult {
        found: true,
        source: "KuGou".into(),
        synced,
        duration_ms: (song.duration_s * 1000.0) as u64,
        ..Default::default()
    }))
}

fn value_to_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}
