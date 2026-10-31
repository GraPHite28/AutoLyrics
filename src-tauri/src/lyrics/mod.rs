//! Поиск текстов сразу в нескольких источниках + библиотека `.lrc`-файлов (см. `store`).
//!
//! Источники опрашиваются параллельно, но результат выбирается по приоритету:
//! LRCLIB → NetEase → QQ Music → KuGou (синхронизированные), затем lyrics.ovh
//! (обычный текст). Так быстрый ответ запасного источника не вытесняет
//! более точный основной, а ожидание не складывается из таймаутов.

mod kugou;
mod lrc;
mod lrclib;
pub(crate) mod matching;
mod musixmatch;
mod netease;
mod ovh;
mod qq;
mod store;

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[derive(Clone, Serialize, Deserialize)]
pub struct Line {
    /// Время начала строки, мс от начала трека.
    pub t: i64,
    pub text: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LyricsResult {
    pub found: bool,
    /// Название источника (для меню).
    pub source: String,
    /// Как получен: пусто — из интернета, `folder` — из папки с текстами (сохранён раньше),
    /// `file` — пользовательский .lrc, `partial` — неполный текст. Подписи переводит интерфейс.
    #[serde(default)]
    pub origin: String,
    pub synced: Vec<Line>,
    pub plain: Option<String>,
    pub instrumental: bool,
    /// Длительность трека по данным источника (0 — неизвестна). Нужна, чтобы
    /// понять, когда трек закончился, если плеер этого не сообщает.
    #[serde(default)]
    pub duration_ms: u64,
}

/// Что ищем. Название и артист хранятся и «как есть», и очищенными.
#[derive(Clone)]
pub struct Query {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_s: f64,
    pub clean_title: String,
    pub main_artist: String,
    /// Все артисты по отдельности — сервисы записывают их по-разному.
    pub artists: Vec<String>,
}

impl Query {
    pub(crate) fn new(title: &str, artist: &str, album: &str, duration_ms: u64) -> Self {
        Self {
            title: title.trim().to_string(),
            artist: artist.trim().to_string(),
            album: album.trim().to_string(),
            duration_s: duration_ms as f64 / 1000.0,
            clean_title: matching::normalize_title(title),
            main_artist: matching::primary_artist(artist),
            artists: matching::artist_list(artist),
        }
    }
}

pub(crate) fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
            )
            .timeout(Duration::from_secs(8))
            .build()
            .expect("не удалось создать HTTP-клиент")
    })
}

/// Сколько всего ждём источники, прежде чем отвечать тем, что успело найтись.
const SEARCH_DEADLINE: Duration = Duration::from_secs(7);

type Found = Result<Option<LyricsResult>, reqwest::Error>;
type Task = tauri::async_runtime::JoinHandle<Found>;

/// Ключ Musixmatch из настроек приложения (пустая строка — убрать ключ).
#[tauri::command]
pub fn set_musixmatch_key(key: String) {
    musixmatch::set_key(&key);
}

#[tauri::command]
pub async fn fetch_lyrics(
    app: AppHandle,
    title: String,
    artist: String,
    album: String,
    duration_ms: u64,
    force: Option<bool>,
) -> Result<LyricsResult, String> {
    let q = Query::new(&title, &artist, &album, duration_ms);
    let library = store::dir(&app);

    // Сначала папка с текстами (там же лежат и пользовательские .lrc).
    if let Some(found) = library.and_then(|dir| store::read(dir, &q, force.unwrap_or(false))) {
        return Ok(found);
    }

    let result = find_everywhere(q.clone()).await?;

    // Сохраняем в папку только синхронизированные: обычный текст в следующий раз
    // стоит поискать заново — вдруг появится синхронизированный.
    if let Some(dir) = library {
        store::write(dir, &q, &album, &result);
    }
    Ok(result)
}

/// Заранее ищет текст (например, следующей песни альбома) и сохраняет его в папку с текстами.
pub(crate) async fn prefetch(app: AppHandle, title: String, artist: String, album: String, duration_ms: u64) {
    let Some(dir) = store::dir(&app) else { return };
    let q = Query::new(&title, &artist, &album, duration_ms);
    if store::read(dir, &q, false).is_some() {
        return; // уже есть
    }
    if let Ok(result) = find_everywhere(q.clone()).await {
        store::write(dir, &q, &album, &result);
    }
}

/// Открывает папку с текстами в проводнике.
#[tauri::command]
pub fn open_lyrics_dir(app: AppHandle) -> Result<(), String> {
    let dir = store::dir(&app).ok_or("папка с текстами недоступна")?;
    std::process::Command::new("explorer")
        .arg(dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn find_everywhere(q: Query) -> Result<LyricsResult, String> {
    let mut sources: Vec<(&str, Task)> = vec![
        ("LRCLIB", spawn_source(lrclib::find, q.clone())),
        ("NetEase", spawn_source(netease::find, q.clone())),
        ("QQ Music", spawn_source(qq::find, q.clone())),
        ("KuGou", spawn_source(kugou::find, q.clone())),
        // обычный полный текст лучше неполного бесплатного от Musixmatch — он идёт позже
        ("lyrics.ovh", spawn_source(ovh::find, q.clone())),
    ];
    if musixmatch::key().is_some() {
        sources.push(("Musixmatch", spawn_source(musixmatch::find, q)));
    }
    let total = sources.len();
    let debug = std::env::var_os("AUTOLYRICS_DEBUG").is_some();

    let mut plain_fallback: Option<LyricsResult> = None;
    let mut errors: Vec<String> = Vec::new();
    let mut primary_failed = false;

    // Медленный или зависший источник не должен задерживать ответ остальных.
    let deadline = tokio::time::Instant::now() + SEARCH_DEADLINE;

    let mut queue = sources.into_iter();
    while let Some((name, task)) = queue.next() {
        let joined = match tokio::time::timeout_at(deadline, task).await {
            Ok(joined) => joined,
            Err(_) => {
                primary_failed |= name == "LRCLIB";
                errors.push(format!("{name}: нет ответа"));
                continue;
            }
        };
        match joined {
            Ok(Ok(Some(result))) => {
                if debug {
                    eprintln!("[lyrics] {name}: найден ({} строк)", result.synced.len());
                }
                if !result.synced.is_empty() || result.instrumental {
                    for (_, rest) in queue {
                        rest.abort(); // лучший вариант уже есть — остальные не нужны
                    }
                    return Ok(result);
                }
                plain_fallback.get_or_insert(result);
            }
            Ok(Ok(None)) => {
                if debug {
                    eprintln!("[lyrics] {name}: нет");
                }
            }
            Ok(Err(err)) => {
                if debug {
                    eprintln!("[lyrics] {name}: ошибка {err}");
                }
                primary_failed |= name == "LRCLIB";
                errors.push(format!("{name}: {err}"));
            }
            Err(err) => errors.push(format!("{name}: {err}")),
        }
    }

    if let Some(plain) = plain_fallback {
        return Ok(plain);
    }
    // Основной источник или большинство остальных не ответили — это сбой сети,
    // а не «текста нет»: пользователю лучше предложить повторить поиск.
    if primary_failed || errors.len() * 2 > total {
        return Err(errors.join("; "));
    }
    Ok(LyricsResult::default())
}

/// Запускает источник; при ошибке сети один раз повторяет запрос.
fn spawn_source<F, Fut>(find: F, q: Query) -> Task
where
    F: Fn(Query) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Found> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        match find(q.clone()).await {
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(700)).await;
                find(q).await
            }
            ok => ok,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живая проверка источников (нужен интернет): `cargo test live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_providers() {
        let cases = [
            ("Numb", "Linkin Park", "Meteora", 187_000),
            ("Сансара", "Баста", "", 362_000),
            ("Если зовут", "Asper X", "", 0),
            ("Banger (feat. OISEAU & PAPILLON)", "Miyagi & Эндшпиль", "", 0),
            ("Shape of You", "Ed Sheeran", "", 0),
        ];
        tauri::async_runtime::block_on(async {
            for (title, artist, album, ms) in cases {
                let q = Query::new(title, artist, album, ms);
                println!("\n=== {artist} — {title}");
                let runs: [(&str, Found); 5] = [
                    ("LRCLIB", lrclib::find(q.clone()).await),
                    ("NetEase", netease::find(q.clone()).await),
                    ("QQ", qq::find(q.clone()).await),
                    ("KuGou", kugou::find(q.clone()).await),
                    ("ovh", ovh::find(q.clone()).await),
                ];
                for (name, res) in runs {
                    match res {
                        Ok(Some(r)) => println!(
                            "{name:8} ok: synced={} plain={} first={:?}",
                            r.synced.len(),
                            r.plain.is_some(),
                            r.synced.first().map(|l| (l.t, l.text.clone()))
                        ),
                        Ok(None) => println!("{name:8} —"),
                        Err(e) => println!("{name:8} ERROR {e}"),
                    }
                }
                let all = find_everywhere(q).await;
                println!(
                    "-> итог: {}",
                    all.map(|r| format!("{} synced={}", r.source, r.synced.len()))
                        .unwrap_or_else(|e| format!("ошибка {e}"))
                );
            }
        });
    }
}
