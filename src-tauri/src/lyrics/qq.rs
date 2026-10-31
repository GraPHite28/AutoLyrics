//! QQ Music (y.qq.com) — сильна на западной и азиатской музыке.

use reqwest::header::REFERER;
use serde_json::Value;

use super::{client, lrc, matching, LyricsResult, Query};

const SITE: &str = "https://y.qq.com/";

struct Song {
    mid: String,
    title: String,
    artist: String,
}

pub async fn find(q: Query) -> Result<Option<LyricsResult>, reqwest::Error> {
    for keyword in matching::keywords(&q) {
        let songs = search(&keyword).await?;
        // длительности в быстром поиске нет — сверяем только название и артиста
        let scores: Vec<_> = songs
            .iter()
            .map(|s| matching::score(&q, &s.title, &s.artist, None))
            .collect();
        for i in matching::rank(&scores, 3) {
            if let Some(result) = lyric(&q, &songs[i].mid).await? {
                return Ok(Some(result));
            }
        }
    }
    Ok(None)
}

async fn search(keyword: &str) -> Result<Vec<Song>, reqwest::Error> {
    let response: Value = client()
        .get("https://c.y.qq.com/splcloud/fcgi-bin/smartbox_new.fcg")
        .header(REFERER, SITE)
        .query(&[("key", keyword), ("format", "json")])
        .send()
        .await?
        .json()
        .await?;

    let songs = response
        .pointer("/data/song/itemlist")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|s| {
                    Some(Song {
                        mid: s.get("mid")?.as_str()?.to_string(),
                        title: s.get("name")?.as_str()?.to_string(),
                        artist: s.get("singer")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(songs)
}

async fn lyric(q: &Query, mid: &str) -> Result<Option<LyricsResult>, reqwest::Error> {
    let response: Value = client()
        .get("https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg")
        .header(REFERER, SITE)
        .query(&[
            ("songmid", mid),
            ("g_tk", "5381"),
            ("format", "json"),
            ("inCharset", "utf8"),
            ("outCharset", "utf-8"),
            ("nobase64", "1"),
        ])
        .send()
        .await?
        .json()
        .await?;

    let text = response.get("lyric").and_then(Value::as_str).unwrap_or("");
    let text = text
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&");
    let synced = lrc::strip_credits(lrc::parse(&text), q);
    if synced.len() < 3 {
        return Ok(None);
    }
    Ok(Some(LyricsResult {
        found: true,
        source: "QQ Music".into(),
        synced,
        ..Default::default()
    }))
}
