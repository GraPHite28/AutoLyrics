//! lyrics.ovh — запасной источник обычного (несинхронного) текста.

use reqwest::Url;
use serde_json::Value;

use super::{client, LyricsResult, Query};

pub async fn find(q: Query) -> Result<Option<LyricsResult>, reqwest::Error> {
    if q.main_artist.is_empty() || q.clean_title.is_empty() {
        return Ok(None);
    }
    let mut url = Url::parse("https://api.lyrics.ovh/v1/").expect("корректный адрес");
    if let Ok(mut segments) = url.path_segments_mut() {
        segments.pop_if_empty().push(&q.main_artist).push(&q.clean_title);
    }

    let response = client().get(url).send().await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let body: Value = response.json().await?;
    let text = body.get("lyrics").and_then(Value::as_str).unwrap_or("").trim();
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some(LyricsResult {
        found: true,
        source: "lyrics.ovh".into(),
        plain: Some(text.to_string()),
        ..Default::default()
    }))
}
