//! Предсказание следующего трека. Пока играет песня из альбома, заранее находим следующую
//! (обложка, жанр, длительность) и заранее ищем её текст — он сохраняется в папку с текстами.
//! Когда песня закончится, распознаватель сразу покажет «заготовку», а Shazam лишь подтвердит
//! или исправит её (см. `recognizer::on_track_end`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use tauri::AppHandle;

use crate::media_watcher::{self, TrackInfo};
use crate::{album, lyrics};

/// Пауза перед запросами: при быстром листании треков лишнего не запрашиваем.
const SETTLE: Duration = Duration::from_millis(2500);

/// По умолчанию выключено: заготовка может на несколько секунд показать не ту песню.
static ENABLED: AtomicBool = AtomicBool::new(false);
static NEXT: Mutex<Option<Prediction>> = Mutex::new(None);

#[derive(Clone)]
pub struct Prediction {
    /// Для какого трека (id) сделана заготовка.
    pub for_track: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub genre: Option<String>,
    /// Обложка как data-URL.
    pub cover: Option<String>,
}

#[tauri::command]
pub fn set_predict_next(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        if let Ok(mut slot) = NEXT.lock() {
            *slot = None;
        }
    }
}

/// Заготовка для следующего трека после `track_id`, если она есть.
pub fn take_for(track_id: u64) -> Option<Prediction> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    NEXT.lock().ok()?.clone().filter(|p| p.for_track == track_id)
}

fn current_track_id() -> Option<u64> {
    media_watcher::snapshot().track.map(|t| t.id)
}

/// Запускается при каждом новом треке (из любого источника — плеера или распознавания).
pub fn schedule(app: &AppHandle, info: &TrackInfo) {
    if !ENABLED.load(Ordering::Relaxed) || info.title.is_empty() {
        return;
    }
    let (id, title, artist, album) = (info.id, info.title.clone(), info.artist.clone(), info.album.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        if current_track_id() != Some(id) {
            return;
        }
        let Some(next) = album::next_track(&title, &artist, &album).await else {
            return;
        };
        let cover = fetch_cover(next.artwork.as_deref()).await;
        if current_track_id() != Some(id) {
            return;
        }
        if let Ok(mut slot) = NEXT.lock() {
            *slot = Some(Prediction {
                for_track: id,
                title: next.title.clone(),
                artist: next.artist.clone(),
                album: next.album.clone(),
                duration_ms: next.duration_ms,
                genre: next.genre.clone(),
                cover,
            });
        }
        // Текст следующей песни ищем заранее: к её началу он уже в папке.
        lyrics::prefetch(app, next.title, next.artist, next.album, next.duration_ms).await;
    });
}

async fn fetch_cover(url: Option<&str>) -> Option<String> {
    let bytes = lyrics::client().get(url?).send().await.ok()?.bytes().await.ok()?;
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" };
    Some(format!("data:{mime};base64,{}", STANDARD.encode(&bytes)))
}
