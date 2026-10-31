//! Распознавание трека по звуку — для плееров, которые не сообщают трек
//! системе (например, веб-плеер в Яндекс Браузере).
//!
//! Берём последние секунды системного звука, считаем отпечаток по алгоритму
//! Shazam и спрашиваем сервис, что это за песня. Ответ содержит смещение
//! фрагмента внутри трека — по нему запускаем собственные часы, и тексты
//! синхронизируются без помощи плеера.
//!
//! Раз в `RECHECK` секунд проверяем снова: ловим смену трека и перемотку.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::Value;
use songrec::fingerprinting::communication::{
    obtain_raw_cover_image, recognize_song_from_signature_with_config,
};
use songrec::{Config, SignatureGenerator};
use tauri::{AppHandle, Emitter};

use crate::audio_capture;
use crate::lyrics::{matching, Query};
use crate::predict;
use crate::media_watcher::{self, Playback, TrackInfo, OWNER_AUDIO, OWNER_NONE, OWNER_SMTC};

const TICK: Duration = Duration::from_millis(300);
/// Громкость (RMS), выше которой считаем, что на ПК что-то играет.
const AUDIBLE_RMS: f32 = 0.003;
/// Тишина дольше этого — пауза.
const PAUSE_AFTER: Duration = Duration::from_millis(1500);
/// Тишина дольше этого — трек забыт.
const DROP_AFTER: Duration = Duration::from_secs(30);
/// Как часто перепроверять уже распознанный трек.
const RECHECK: Duration = Duration::from_secs(25);
const FIRST_CLIP_S: f32 = 7.0;
const RETRY_CLIP_S: f32 = 12.0;
const RECHECK_CLIP_S: f32 = 8.0;
const FORCED_CLIP_S: f32 = 5.5;
/// Если по кнопке «Определить сейчас» нашлась та же песня, а пользователь, возможно, только что
/// переключил трек, в записи ещё слышен хвост прежней. Через VERIFY_AFTER после нажатия
/// (не дольше двух секунд ожидания) проверяем ещё раз по последним VERIFY_CLIP_S секундам.
const VERIFY_AFTER: Duration = Duration::from_millis(2000);
const VERIFY_CLIP_S: f32 = 4.5;
/// Сколько ждём после старта «заготовки», чтобы в записи накопилось достаточно нового звука.
const PREDICTION_VERIFY_AFTER: Duration = Duration::from_millis(4500);
/// Паузы между неудачными попытками, секунды.
const BACKOFF_S: [u64; 4] = [5, 12, 25, 45];
/// Запас после расчётного конца трека: позиция по часам может чуть отставать.
const END_GRACE_MS: f64 = 1500.0;
/// Расхождение позиции, после которого считаем, что трек перемотали.
const SEEK_TOLERANCE_MS: f64 = 1500.0;
/// `offset` в ответе Shazam — позиция начала нашего фрагмента в треке.
const OFFSET_IS_SAMPLE_START: bool = true;

/* ---------- Данные для интерфейса ---------- */

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover_url: Option<String>,
    pub genre: Option<String>,
    #[serde(skip)]
    cover: Option<String>,
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub items: Vec<Candidate>,
    pub selected: usize,
}

#[derive(Clone, Serialize)]
struct Status {
    state: &'static str,
}

#[derive(Clone, Serialize)]
struct Notice {
    code: &'static str,
    artist: String,
    title: String,
}

/* ---------- Команды из интерфейса ---------- */

struct Commands {
    force: bool,
    select: Option<usize>,
}

static ENABLED: AtomicBool = AtomicBool::new(true);
static DURATION: Mutex<Option<(u64, u64)>> = Mutex::new(None);
static COMMANDS: Mutex<Commands> = Mutex::new(Commands {
    force: false,
    select: None,
});
static VIEW: Mutex<View> = Mutex::new(View {
    items: Vec::new(),
    selected: 0,
});

#[tauri::command]
pub fn set_audio_recognition(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// «Определить заново» — не ждём расписания.
#[tauri::command]
pub fn recognize_now() {
    if let Ok(mut cmd) = COMMANDS.lock() {
        cmd.force = true;
    }
}

/// Пользователь выбрал другой вариант из списка найденных.
#[tauri::command]
pub fn select_candidate(index: usize) {
    if let Ok(mut cmd) = COMMANDS.lock() {
        cmd.select = Some(index);
    }
}

/// Фронтенд сообщает длительность распознанного трека (её даёт источник текста):
/// по ней понимаем, когда трек закончился, и сразу определяем следующий.
#[tauri::command]
pub fn set_track_duration(track_id: u64, duration_ms: u64) {
    if let Ok(mut slot) = DURATION.lock() {
        *slot = Some((track_id, duration_ms));
    }
}

#[tauri::command]
pub fn get_candidates() -> View {
    VIEW.lock().map(|v| v.clone()).unwrap_or_default()
}

/* ---------- Распознаватель ---------- */

#[derive(Clone)]
struct Entry {
    cand: Candidate,
    /// Позиция в треке в момент `anchor_at` (мс).
    anchor_pos_ms: f64,
    anchor_at: Instant,
}

struct Active {
    id: u64,
    key: String,
    pos_ms: f64,
    at: Instant,
    playing: bool,
    /// 0 — неизвестна.
    duration_ms: u64,
}

impl Active {
    fn pos(&self, now: Instant) -> f64 {
        if self.playing {
            self.pos_ms + ms_since(self.at, now)
        } else {
            self.pos_ms
        }
    }
}

enum Outcome {
    Match(Vec<Entry>),
    NoMatch,
    Failed(String),
}

pub fn start(app: AppHandle) {
    thread::spawn(move || {
        let mut recognizer = Recognizer::new(app);
        loop {
            recognizer.tick();
            thread::sleep(TICK);
        }
    });
}

struct Recognizer {
    app: AppHandle,
    tx: mpsc::Sender<Outcome>,
    rx: mpsc::Receiver<Outcome>,
    pending: bool,
    active: Option<Active>,
    /// Все варианты для текущей песни; первый найденный — лучший по версии сервиса.
    entries: Vec<Entry>,
    selected: usize,
    /// Пользователь выбрал вариант вручную — перепроверки его не переопределяют.
    pinned: bool,
    attempts: usize,
    misses: u32,
    next_attempt: Instant,
    next_check: Instant,
    audible_since: Option<Instant>,
    silent_since: Option<Instant>,
    last_status: &'static str,
    started: Instant,
    /// Пользователь нажал «Определить сейчас» — ждём, пока получится запустить.
    force: bool,
    /// Идущая попытка запущена по просьбе пользователя — ответим ему сообщением.
    forced_attempt: bool,
    /// Идущая попытка — проверочная (после «та же песня» по кнопке).
    verifying: bool,
    /// Когда запустить проверку.
    verify_at: Option<Instant>,
    /// На экране «заготовка» — предсказанный следующий трек; Shazam подтвердит или исправит.
    tentative: bool,
    /// Когда нажали «Определить сейчас» (с этого момента в записи звук уже новый).
    forced_started: Instant,
}

fn debug_enabled() -> bool {
    std::env::var_os("AUTOLYRICS_DEBUG").is_some()
}

impl Recognizer {
    fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        let now = Instant::now();
        Self {
            app,
            tx,
            rx,
            pending: false,
            active: None,
            entries: Vec::new(),
            selected: 0,
            pinned: false,
            attempts: 0,
            misses: 0,
            next_attempt: now,
            next_check: now,
            audible_since: None,
            silent_since: None,
            last_status: "",
            started: now,
            force: false,
            forced_attempt: false,
            verifying: false,
            verify_at: None,
            tentative: false,
            forced_started: now,
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let (force_cmd, select) = take_commands();
        if force_cmd {
            self.force = true;
            self.pinned = false; // новое определение может отменить ручной выбор
        }

        // Плеер сам сообщает трек, либо распознавание выключено — молчим.
        let smtc = media_watcher::owner() == OWNER_SMTC;
        if smtc || !ENABLED.load(Ordering::Relaxed) {
            if self.active.is_some() || !self.entries.is_empty() {
                self.reset(false);
            }
            if std::mem::take(&mut self.force) {
                self.notice(if smtc { "smtc" } else { "disabled" }, "", "");
            }
            self.status("off");
            return;
        }

        let level = audio_capture::level(0.5);
        let audible = level > AUDIBLE_RMS;
        if debug_enabled() && now.duration_since(self.started).as_millis() % 3000 < 300 {
            eprintln!(
                "[recognizer] rms={level:.4} audible={audible} active={} pending={}",
                self.active.is_some(),
                self.pending
            );
        }
        if audible {
            self.silent_since = None;
            self.audible_since.get_or_insert(now);
        } else {
            self.audible_since = None;
            self.silent_since.get_or_insert(now);
        }
        let silent_for = self.silent_since.map_or(Duration::ZERO, |t| now - t);
        let playing = silent_for < PAUSE_AFTER;
        if !audible && std::mem::take(&mut self.force) {
            self.notice("silent", "", "");
        }

        while let Ok(outcome) = self.rx.try_recv() {
            self.pending = false;
            self.on_outcome(outcome, now);
        }

        if let Some(index) = select {
            if index < self.entries.len() {
                self.pinned = true;
                self.start_track(index, now);
            }
        }

        if let Some((id, duration_ms)) = DURATION.lock().ok().and_then(|mut d| d.take()) {
            if let Some(active) = self.active.as_mut().filter(|a| a.id == id) {
                active.duration_ms = duration_ms;
            }
        }

        if let Some(active) = &mut self.active {
            if active.playing != playing {
                active.pos_ms = active.pos(now);
                active.at = now;
                active.playing = playing;
                if playing {
                    // после паузы может оказаться уже другой трек — проверим чуть позже
                    self.next_check = now + Duration::from_secs(9);
                }
            }
            let pb = Playback {
                track_id: active.id,
                position_ms: active.pos(now).max(0.0) as u64,
                duration_ms: 0,
                playing: active.playing,
            };
            media_watcher::publish_playback(&self.app, &pb);

            let ended = active.playing
                && active.duration_ms > 0
                && active.pos(now) >= active.duration_ms as f64 + END_GRACE_MS;
            if silent_for >= DROP_AFTER {
                self.reset(true);
                self.status("idle");
                return;
            }
            if ended {
                self.on_track_end(now);
                return;
            }
        }

        if self.pending {
            self.status("recognizing");
            return;
        }

        if !audible {
            self.verify_at = None; // звук пропал — проверять нечем
        }
        if self.verify_at.is_some_and(|at| now >= at) && self.active.is_some() {
            self.verify_at = None;
            self.verifying = true;
            self.spawn_attempt(VERIFY_CLIP_S);
            return;
        }

        let force = std::mem::take(&mut self.force);
        let clip_secs = match &self.active {
            None => {
                if !audible {
                    self.status("idle");
                    return;
                }
                self.status("listening");
                let clip = if self.attempts == 0 { FIRST_CLIP_S } else { RETRY_CLIP_S };
                let heard = self.audible_since.map_or(Duration::ZERO, |t| now - t);
                let ready = heard.as_secs_f32() >= clip && now >= self.next_attempt;
                if !(force || ready) {
                    return;
                }
                clip
            }
            Some(_) => {
                if !(force || (playing && now >= self.next_check)) {
                    return;
                }
                RECHECK_CLIP_S
            }
        };
        if force {
            self.forced_attempt = true;
            self.forced_started = now;
            self.verify_at = None;
            self.notice("recognizing", "", "");
        }
        self.spawn_attempt(if force { FORCED_CLIP_S } else { clip_secs });
    }

    /// Короткое сообщение пользователю (всплывающая подпись). Передаём код, а не текст:
    /// перевод на выбранный язык делает интерфейс.
    fn notice(&self, code: &'static str, artist: &str, title: &str) {
        let _ = self.app.emit(
            "notice",
            Notice {
                code,
                artist: artist.to_string(),
                title: title.to_string(),
            },
        );
    }

    fn spawn_attempt(&mut self, clip_secs: f32) {
        self.pending = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(recognize(clip_secs));
        });
    }

    fn on_outcome(&mut self, outcome: Outcome, now: Instant) {
        if debug_enabled() {
            match &outcome {
                Outcome::Match(found) => {
                    for e in found {
                        eprintln!(
                            "[recognizer] match: {} — {} (pos at clip end {:.1}s)",
                            e.cand.artist,
                            e.cand.title,
                            e.anchor_pos_ms / 1000.0
                        );
                    }
                }
                Outcome::NoMatch => eprintln!("[recognizer] no match"),
                Outcome::Failed(m) => eprintln!("[recognizer] failed: {m}"),
            }
        }
        let forced = std::mem::take(&mut self.forced_attempt);
        let verifying = std::mem::take(&mut self.verifying);
        let user_asked = forced || (verifying && !self.tentative);
        match outcome {
            Outcome::Match(found) => {
                self.attempts = 0;
                self.misses = 0;
                self.next_check = now + RECHECK;
                self.on_match(found, now, forced, verifying);
            }
            Outcome::NoMatch => {
                if user_asked {
                    self.notice("nomatch", "", "");
                }
                self.status("nomatch");
                self.register_miss(now);
            }
            Outcome::Failed(message) => {
                eprintln!("[recognizer] {message}");
                if user_asked {
                    self.notice("noconn", "", "");
                }
                self.status("error");
                self.register_miss(now);
            }
        }
    }

    fn register_miss(&mut self, now: Instant) {
        let delay = BACKOFF_S[self.attempts.min(BACKOFF_S.len() - 1)];
        self.attempts += 1;
        self.next_attempt = now + Duration::from_secs(delay);
        if self.active.is_some() {
            self.misses += 1;
            self.next_check = now + Duration::from_secs(12);
            // Дважды подряд ничего не нашли — скорее всего, пошёл другой трек.
            if self.misses >= 3 && !self.pinned {
                self.reset(true);
            }
        }
    }

    fn on_match(&mut self, found: Vec<Entry>, now: Instant, forced: bool, verifying: bool) {
        let Some(best) = found.first().cloned() else { return };

        // Показана заготовка (предсказанный трек): Shazam её подтверждает или исправляет.
        let was_tentative = std::mem::take(&mut self.tentative);
        if was_tentative {
            let predicted = self.entries.first().map(|e| e.cand.clone());
            if predicted.is_some_and(|p| same_song(&p, &best.cand)) {
                // Угадали: подменяем заготовку настоящим результатом, экран не трогаем.
                self.entries.clear();
                for entry in found {
                    self.merge(entry);
                }
                if let Some(active) = &mut self.active {
                    active.key = best.cand.key.clone();
                    active.pos_ms = best.anchor_pos_ms + ms_since(best.anchor_at, now);
                    active.at = now;
                }
                self.selected = 0;
                self.verify_at = None;
                self.sync_view();
                return;
            }
            // Не угадали — дальше обычный путь: показываем то, что нашёл Shazam.
        }

        let same_as_active = self.active.as_ref().is_some_and(|a| a.key == best.cand.key);
        let known = self.entries.iter().any(|e| e.cand.key == best.cand.key);

        if same_as_active || (self.pinned && known) {
            for entry in found {
                self.merge(entry);
            }
            if let Some(active) = &mut self.active {
                if same_as_active {
                    // позиция по новому отпечатку
                    let fresh = best.anchor_pos_ms + ms_since(best.anchor_at, now);
                    if forced || (fresh - active.pos(now)).abs() > SEEK_TOLERANCE_MS {
                        active.pos_ms = fresh;
                        active.at = now;
                    }
                }
            }
            if forced && !verifying && same_as_active {
                // В записи мог остаться хвост прежнего трека — не торопимся подтверждать.
                self.verify_at = Some((self.forced_started + VERIFY_AFTER).max(now));
                self.notice("verifying", "", "");
            } else if forced || verifying {
                self.notice("confirmed", &best.cand.artist, &best.cand.title);
            }
            self.sync_view();
            return;
        }

        // Новая песня: прежний список вариантов больше не актуален.
        self.entries.clear();
        self.pinned = false;
        for entry in found {
            self.merge(entry);
        }
        self.start_track(0, now);
        if (forced || verifying) && !was_tentative {
            self.notice("found", &best.cand.artist, &best.cand.title);
        }
    }

    fn merge(&mut self, entry: Entry) {
        match self.entries.iter_mut().find(|e| e.cand.key == entry.cand.key) {
            Some(existing) => {
                existing.anchor_pos_ms = entry.anchor_pos_ms;
                existing.anchor_at = entry.anchor_at;
            }
            None => self.entries.push(entry),
        }
    }

    fn start_track(&mut self, index: usize, now: Instant) {
        let entry = &self.entries[index];
        let pos = entry.anchor_pos_ms + ms_since(entry.anchor_at, now);
        let id = media_watcher::next_track_id();
        let info = TrackInfo {
            id,
            title: entry.cand.title.clone(),
            artist: entry.cand.artist.clone(),
            album: entry.cand.album.clone(),
            duration_ms: 0,
            cover: entry.cand.cover.clone(),
            source: "audio".into(),
            genre: entry.cand.genre.clone(),
        };
        self.active = Some(Active {
            id,
            key: entry.cand.key.clone(),
            pos_ms: pos,
            at: now,
            playing: true,
            duration_ms: 0,
        });
        self.selected = index;
        self.verify_at = None;
        media_watcher::set_owner(OWNER_AUDIO);
        media_watcher::publish_track(&self.app, &info);
        self.sync_view();
    }

    /// Трек доиграл до конца. Если следующий идёт без паузы, звук не прерывается и
    /// прежняя песня осталась бы на экране — поэтому забываем её и слушаем заново.
    fn on_track_end(&mut self, now: Instant) {
        if debug_enabled() {
            eprintln!("[recognizer] трек доиграл до конца — определяю следующий");
        }
        let ended_id = self.active.as_ref().map(|a| a.id);
        if let Some(next) = ended_id.and_then(predict::take_for) {
            self.start_predicted(next, now);
            return;
        }
        self.reset(true);
        // считаем «новым» звук, который идёт с этого момента
        self.audible_since = Some(now);
        self.last_status = "";
        self.status("listening");
    }

    /// Песня кончилась, а следующая в альбоме уже известна: показываем её сразу (со
    /// спрятанным заранее текстом), а Shazam через несколько секунд подтвердит или исправит.
    fn start_predicted(&mut self, next: predict::Prediction, now: Instant) {
        if debug_enabled() {
            eprintln!("[recognizer] заготовка следующего трека: {} — {}", next.artist, next.title);
        }
        let cand = Candidate {
            key: format!("predicted|{}|{}", next.artist, next.title),
            title: next.title,
            artist: next.artist,
            album: next.album,
            cover_url: None,
            genre: next.genre,
            cover: next.cover,
        };
        self.entries.clear();
        self.pinned = false;
        self.attempts = 0;
        self.misses = 0;
        // к моменту срабатывания новая песня уже играет около END_GRACE_MS
        self.entries.push(Entry {
            cand,
            anchor_pos_ms: END_GRACE_MS,
            anchor_at: now,
        });
        self.start_track(0, now);
        if let Some(active) = &mut self.active {
            active.duration_ms = next.duration_ms;
        }
        self.tentative = true;
        // сначала проверка по свежему звуку, а плановая перепроверка — после неё
        self.verify_at = Some(now + PREDICTION_VERIFY_AFTER);
        self.next_check = now + Duration::from_secs(20);
    }

    fn reset(&mut self, announce_none: bool) {
        let had_track = self.active.take().is_some();
        self.entries.clear();
        self.selected = 0;
        self.pinned = false;
        self.verify_at = None;
        self.tentative = false;
        self.attempts = 0;
        self.misses = 0;
        self.next_attempt = Instant::now();
        if media_watcher::owner() == OWNER_AUDIO {
            media_watcher::set_owner(OWNER_NONE);
        }
        if had_track && announce_none {
            media_watcher::publish_none(&self.app);
            media_watcher::request_resync();
        }
        self.sync_view();
    }

    fn sync_view(&self) {
        let view = View {
            items: self.entries.iter().map(|e| e.cand.clone()).collect(),
            selected: self.selected,
        };
        if let Ok(mut shared) = VIEW.lock() {
            *shared = view.clone();
        }
        let _ = self.app.emit("candidates", view);
    }

    fn status(&mut self, state: &'static str) {
        if self.last_status != state {
            self.last_status = state;
            let _ = self.app.emit("recognition", Status { state });
        }
    }
}

/// Одна и та же песня по названию и артисту (ключи у iTunes и Shazam разные).
fn same_song(a: &Candidate, b: &Candidate) -> bool {
    let q = Query::new(&a.title, &a.artist, "", 0);
    matching::score(&q, &b.title, &b.artist, None).is_some()
}

fn take_commands() -> (bool, Option<usize>) {
    match COMMANDS.lock() {
        Ok(mut cmd) => (std::mem::take(&mut cmd.force), cmd.select.take()),
        Err(_) => (false, None),
    }
}

fn ms_since(earlier: Instant, now: Instant) -> f64 {
    now.saturating_duration_since(earlier).as_secs_f64() * 1000.0
}

/* ---------- Распознавание (выполняется в отдельном потоке) ---------- */

fn recognize(clip_secs: f32) -> Outcome {
    let Some(clip) = audio_capture::recent(clip_secs) else {
        return Outcome::Failed("в буфере ещё мало звука".into());
    };
    let pcm = audio_capture::to_16k_i16(&clip.samples, clip.rate);
    let signature = SignatureGenerator::make_signature_from_buffer(&pcm);

    let config = Config::default().with_quiet_mode(true);
    let response = match recognize_song_from_signature_with_config(&signature, &config) {
        Ok(json) => json,
        Err(err) => return Outcome::Failed(format!("сервис распознавания недоступен: {err}")),
    };

    if std::env::var_os("AUTOLYRICS_DEBUG").is_some() {
        let path = std::env::temp_dir().join("autolyrics_last_response.json");
        let _ = std::fs::write(path, serde_json::to_vec_pretty(&response).unwrap_or_default());
    }

    let Some(track) = response.get("track") else {
        return Outcome::NoMatch;
    };
    let Some(mut best) = parse_candidate(track) else {
        return Outcome::NoMatch;
    };

    // Где мы в треке на момент `clip.end`.
    let offset_s = response
        .pointer("/matches/0/offset")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let clip_s = f64::from(clip.seconds());
    let pos_at_end_ms = if OFFSET_IS_SAMPLE_START {
        (offset_s + clip_s) * 1000.0
    } else {
        offset_s * 1000.0
    };

    best.cover = best
        .cover_url
        .as_deref()
        .and_then(|url| obtain_raw_cover_image(url).ok())
        .map(|bytes| data_url(&bytes));

    Outcome::Match(vec![Entry {
        cand: best,
        anchor_pos_ms: pos_at_end_ms,
        anchor_at: clip.end,
    }])
}

fn parse_candidate(track: &Value) -> Option<Candidate> {
    let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_owned);

    let title = text(track.get("title"))?;
    let artist = text(track.get("subtitle")).unwrap_or_default();
    let key = text(track.get("key")).unwrap_or_else(|| format!("{artist}|{title}").to_lowercase());

    let album = track
        .get("sections")
        .and_then(Value::as_array)
        .and_then(|sections| {
            sections
                .iter()
                .find(|s| s.get("type").and_then(Value::as_str) == Some("SONG"))
        })
        .and_then(|section| section.get("metadata"))
        .and_then(Value::as_array)
        .and_then(|meta| {
            meta.iter()
                .find(|m| m.get("title").and_then(Value::as_str) == Some("Album"))
        })
        .and_then(|m| text(m.get("text")))
        .unwrap_or_default();

    Some(Candidate {
        key,
        title,
        artist,
        album,
        cover_url: text(track.pointer("/images/coverart")),
        genre: text(track.pointer("/genres/primary")),
        cover: None,
    })
}

fn data_url(bytes: &[u8]) -> String {
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else {
        "image/jpeg"
    };
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}
