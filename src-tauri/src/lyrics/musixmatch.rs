//! Musixmatch — официальный API (https://developer.musixmatch.com), нужен ключ пользователя.
//!
//! Синхронизированные строки (`matcher.subtitle.get`) доступны только на платных тарифах;
//! на бесплатном ключе сервис отвечает ошибкой доступа, и мы берём обычный текст
//! (`matcher.lyrics.get`) — на бесплатном тарифе он приходит неполным.

use std::sync::Mutex;

use serde_json::Value;

use super::{client, lrc, LyricsResult, Query};

const BASE: &str = "https://api.musixmatch.com/ws/1.1";
/// Допустимое расхождение длительности при поиске синхронизированного текста, секунды.
const DURATION_DEVIATION_S: u32 = 6;

static KEY: Mutex<Option<String>> = Mutex::new(None);

pub fn set_key(key: &str) {
    let key = key.trim();
    if let Ok(mut slot) = KEY.lock() {
        *slot = (!key.is_empty()).then(|| key.to_string());
    }
}

/// Ключ из настроек приложения, а если его нет — из `MUSIXMATCH_API_KEY`.
pub fn key() -> Option<String> {
    let saved = KEY.lock().ok().and_then(|k| k.clone());
    saved.or_else(|| {
        std::env::var("MUSIXMATCH_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
    })
}

pub async fn find(q: Query) -> Result<Option<LyricsResult>, reqwest::Error> {
    let Some(key) = key() else { return Ok(None) };

    // 1. Синхронизированный текст (платный тариф).
    let mut params: Vec<(&str, String)> = vec![
        ("apikey", key.clone()),
        ("q_track", q.clean_title.clone()),
        ("q_artist", q.main_artist.clone()),
        ("subtitle_format", "lrc".into()),
    ];
    if q.duration_s > 0.0 {
        params.push(("f_subtitle_length", format!("{}", q.duration_s.round() as u32)));
        params.push(("f_subtitle_length_max_deviation", DURATION_DEVIATION_S.to_string()));
    }
    let response: Value = client()
        .get(format!("{BASE}/matcher.subtitle.get"))
        .query(&params)
        .send()
        .await?
        .json()
        .await?;
    if let Some(result) = parse_subtitle(&response, &q) {
        return Ok(Some(result));
    }

    // 2. Обычный текст.
    let response: Value = client()
        .get(format!("{BASE}/matcher.lyrics.get"))
        .query(&[
            ("apikey", key.as_str()),
            ("q_track", q.clean_title.as_str()),
            ("q_artist", q.main_artist.as_str()),
        ])
        .send()
        .await?
        .json()
        .await?;
    Ok(parse_lyrics(&response))
}

fn status_ok(response: &Value) -> bool {
    response.pointer("/message/header/status_code").and_then(Value::as_i64) == Some(200)
}

fn parse_subtitle(response: &Value, q: &Query) -> Option<LyricsResult> {
    if !status_ok(response) {
        return None;
    }
    let body = response
        .pointer("/message/body/subtitle/subtitle_body")
        .and_then(Value::as_str)?;
    let synced = lrc::strip_credits(lrc::parse(body), q);
    if synced.len() < 3 {
        return None;
    }
    Some(LyricsResult {
        found: true,
        source: "Musixmatch".into(),
        synced,
        ..Default::default()
    })
}

fn parse_lyrics(response: &Value) -> Option<LyricsResult> {
    if !status_ok(response) {
        return None;
    }
    let lyrics = response.pointer("/message/body/lyrics")?;
    if lyrics.get("instrumental").and_then(Value::as_i64) == Some(1) {
        return Some(LyricsResult {
            found: true,
            source: "Musixmatch".into(),
            instrumental: true,
            ..Default::default()
        });
    }

    // В конце бесплатного ответа идёт служебная приписка («****** This Lyrics is NOT for
    // Commercial use ******» и номер) — убираем её.
    let text: Vec<&str> = lyrics
        .get("lyrics_body")
        .and_then(Value::as_str)?
        .lines()
        .take_while(|line| !line.trim_start().starts_with("***"))
        .collect();
    let text = text.join("\n").trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(LyricsResult {
        found: true,
        source: "Musixmatch".into(),
        origin: "partial".into(),
        plain: Some(text),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn query() -> Query {
        Query {
            title: "Вместе".into(),
            artist: "Asper X".into(),
            album: String::new(),
            duration_s: 186.0,
            clean_title: "Вместе".into(),
            main_artist: "Asper X".into(),
            artists: vec!["Asper X".into()],
        }
    }

    #[test]
    fn parses_synced_subtitle() {
        let response = json!({"message": {
            "header": {"status_code": 200},
            "body": {"subtitle": {"subtitle_body": "[00:01.00] первая\n[00:03.50] вторая\n[00:06.00] третья"}}
        }});
        let result = parse_subtitle(&response, &query()).expect("текст разобран");
        assert_eq!(result.synced.len(), 3);
        assert_eq!(result.synced[1].t, 3500);
        assert_eq!(result.source, "Musixmatch");
    }

    #[test]
    fn subtitle_denied_on_free_plan() {
        let response = json!({"message": {"header": {"status_code": 401}, "body": []}});
        assert!(parse_subtitle(&response, &query()).is_none());
    }

    #[test]
    fn plain_lyrics_drop_footer() {
        let response = json!({"message": {
            "header": {"status_code": 200},
            "body": {"lyrics": {"instrumental": 0,
                "lyrics_body": "строка один\nстрока два\n\n...\n\n******* This Lyrics is NOT for Commercial use *******\n(1409617926771)"}}
        }});
        let result = parse_lyrics(&response).expect("текст разобран");
        assert_eq!(result.plain.as_deref(), Some("строка один\nстрока два\n\n..."));
        assert!(result.synced.is_empty());
    }

    #[test]
    fn not_found_has_empty_body() {
        let response = json!({"message": {"header": {"status_code": 404}, "body": []}});
        assert!(parse_lyrics(&response).is_none());
    }
}
