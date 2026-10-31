//! Определение текущего трека через Windows SMTC
//! (System Media Transport Controls) — работает с любым плеером,
//! который сообщает о себе системе: Яндекс Музыка, Spotify, браузеры, VLC…

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

const POLL_INTERVAL: Duration = Duration::from_millis(400);
const COVER_RETRIES: u8 = 12;
/// Разница между эпохой Windows FILETIME (1601) и Unix (1970) в 100-нс тиках.
const FILETIME_UNIX_DIFF: i64 = 116_444_736_000_000_000;

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub cover: Option<String>,
    pub source: String,
    /// Жанр, если его уже сообщил источник (Shazam); иначе фронтенд спросит отдельно.
    pub genre: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    pub track_id: u64,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub playing: bool,
}

#[derive(Clone, Serialize, Default)]
pub struct Snapshot {
    pub track: Option<TrackInfo>,
    pub playback: Option<Playback>,
}

#[derive(Clone, Serialize)]
struct CoverEvent {
    id: u64,
    cover: String,
}

/// Последнее известное состояние: окно может открыться позже первых событий.
static LAST: Mutex<Snapshot> = Mutex::new(Snapshot {
    track: None,
    playback: None,
});

pub fn snapshot() -> Snapshot {
    LAST.lock().map(|s| s.clone()).unwrap_or_default()
}

/// Кто сейчас определяет трек для интерфейса.
/// SMTC главный: если плеер сам сообщает трек, распознавание по звуку молчит.
pub const OWNER_NONE: u8 = 0;
pub const OWNER_SMTC: u8 = 1;
pub const OWNER_AUDIO: u8 = 2;

static OWNER: AtomicU8 = AtomicU8::new(OWNER_NONE);
/// Распознавание отдало управление — SMTC должен заново показать свой трек.
static RESYNC: AtomicBool = AtomicBool::new(false);
static NEXT_TRACK_ID: AtomicU64 = AtomicU64::new(1);

pub fn owner() -> u8 {
    OWNER.load(Ordering::Relaxed)
}

pub fn set_owner(owner: u8) {
    OWNER.store(owner, Ordering::Relaxed);
}

pub fn request_resync() {
    RESYNC.store(true, Ordering::Relaxed);
}

pub fn next_track_id() -> u64 {
    NEXT_TRACK_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn publish_track(app: &AppHandle, info: &TrackInfo) {
    if let Ok(mut last) = LAST.lock() {
        last.track = Some(info.clone());
        last.playback = None;
    }
    let _ = app.emit("track", Some(info.clone()));
    crate::predict::schedule(app, info); // заранее готовим следующий трек альбома
}

pub fn publish_cover(app: &AppHandle, track_id: u64, cover: String) {
    if let Ok(mut last) = LAST.lock() {
        if let Some(track) = last.track.as_mut().filter(|t| t.id == track_id) {
            track.cover = Some(cover.clone());
        }
    }
    let _ = app.emit("track_cover", CoverEvent { id: track_id, cover });
}

pub fn publish_playback(app: &AppHandle, playback: &Playback) {
    if let Ok(mut last) = LAST.lock() {
        last.playback = Some(playback.clone());
    }
    let _ = app.emit("playback", playback.clone());
}

pub fn publish_none(app: &AppHandle) {
    if let Ok(mut last) = LAST.lock() {
        *last = Snapshot::default();
    }
    let _ = app.emit("track", Option::<TrackInfo>::None);
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut watcher = Watcher::new();
        loop {
            if let Err(err) = watcher.tick(&app) {
                eprintln!("[media_watcher] {err}");
                // менеджер мог «протухнуть» — пересоздадим на следующем тике
                watcher.manager = None;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

struct Watcher {
    manager: Option<Manager>,
    key: Option<String>,
    track_id: u64,
    cover_tries: u8,
    has_cover: bool,
    /// Запасные локальные часы — если плеер не сообщает время обновления позиции.
    local_pos_ms: f64,
    last_tick: Instant,
}

impl Watcher {
    fn new() -> Self {
        Self {
            manager: None,
            key: None,
            track_id: 0,
            cover_tries: 0,
            has_cover: false,
            local_pos_ms: 0.0,
            last_tick: Instant::now(),
        }
    }

    fn tick(&mut self, app: &AppHandle) -> windows::core::Result<()> {
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last_tick).as_secs_f64() * 1000.0;
        self.last_tick = now;

        if self.manager.is_none() {
            self.manager = Some(Manager::RequestAsync()?.get()?);
        }
        let manager = self.manager.clone().expect("manager задан выше");

        let Some(session) = pick_session(&manager)? else {
            return self.no_session(app);
        };

        let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
        if RESYNC.swap(false, Ordering::Relaxed) {
            self.key = None;
        }
        if playing {
            if owner() == OWNER_AUDIO {
                self.key = None; // перехватываем у распознавания — покажем трек плеера
            }
            set_owner(OWNER_SMTC);
        } else if owner() == OWNER_AUDIO {
            return Ok(()); // плеер на паузе, а звук идёт — трек определяет распознавание
        } else if owner() == OWNER_SMTC {
            set_owner(OWNER_NONE); // плеер встал на паузу — звук может идти из другого источника
        }
        let props = session.TryGetMediaPropertiesAsync()?.get()?;
        let title = props.Title()?.to_string();
        let artist = props.Artist()?.to_string();
        let album = props.AlbumTitle()?.to_string();
        if title.trim().is_empty() && artist.trim().is_empty() {
            return self.no_session(app);
        }

        let timeline = session.GetTimelineProperties()?;
        let start_ms = timeline.StartTime()?.Duration / 10_000;
        let end_ms = timeline.EndTime()?.Duration / 10_000;
        let duration_ms = (end_ms - start_ms).max(0) as u64;

        let key = format!("{artist}\u{1}{title}\u{1}{album}");
        let changed = self.key.as_deref() != Some(key.as_str());
        if changed {
            self.key = Some(key);
            self.track_id = next_track_id();
            self.cover_tries = 0;
            self.has_cover = false;
            self.local_pos_ms = 0.0;

            let info = TrackInfo {
                id: self.track_id,
                title,
                artist,
                album,
                duration_ms,
                cover: None,
                source: session
                    .SourceAppUserModelId()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                genre: None,
            };
            publish_track(app, &info);
        }

        // Обложка часто появляется с задержкой — пробуем несколько тиков.
        if !self.has_cover && self.cover_tries < COVER_RETRIES {
            self.cover_tries += 1;
            if let Ok(Some(cover)) = props.Thumbnail().and_then(read_cover) {
                self.has_cover = true;
                publish_cover(app, self.track_id, cover);
            }
        }

        if playing {
            self.local_pos_ms += dt_ms;
        }
        let raw_pos_ms = timeline.Position()?.Duration / 10_000;
        let updated_at = timeline.LastUpdatedTime()?.UniversalTime;
        let mut position_ms = if updated_at > 0 {
            let extra = if playing {
                (unix_now_filetime() - updated_at) / 10_000
            } else {
                0
            };
            raw_pos_ms + extra.max(0)
        } else {
            self.local_pos_ms as i64
        };
        if duration_ms > 0 {
            position_ms = position_ms.min(duration_ms as i64);
        }

        let playback = Playback {
            track_id: self.track_id,
            position_ms: position_ms.max(0) as u64,
            duration_ms,
            playing,
        };
        publish_playback(app, &playback);
        Ok(())
    }

    fn no_session(&mut self, app: &AppHandle) -> windows::core::Result<()> {
        if RESYNC.swap(false, Ordering::Relaxed) {
            self.key = None;
        }
        if owner() == OWNER_SMTC {
            set_owner(OWNER_NONE);
        }
        // Если трек определяется по звуку, он на экране — не трогаем.
        if owner() != OWNER_AUDIO && self.key.take().is_some() {
            publish_none(app);
        }
        Ok(())
    }
}

/// Предпочитаем сессию, которая прямо сейчас играет; иначе — текущую системную.
fn pick_session(manager: &Manager) -> windows::core::Result<Option<Session>> {
    let sessions = manager.GetSessions()?;
    let mut first = None;
    for i in 0..sessions.Size()? {
        let session = sessions.GetAt(i)?;
        if session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing {
            return Ok(Some(session));
        }
        if first.is_none() {
            first = Some(session);
        }
    }
    Ok(manager.GetCurrentSession().ok().or(first))
}

fn read_cover(reference: IRandomAccessStreamReference) -> windows::core::Result<Option<String>> {
    let stream = reference.OpenReadAsync()?.get()?;
    let size = stream.Size()? as u32;
    if size == 0 {
        return Ok(None);
    }
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(size)?.get()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;

    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else {
        "image/jpeg"
    };
    Ok(Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes))))
}

fn unix_now_filetime() -> i64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (nanos / 100) as i64 + FILETIME_UNIX_DIFF
}
