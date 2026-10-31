//! Библиотека текстов: обычные `.lrc`-файлы в папке `lyrics` рядом с программой.
//!
//! Найденные в интернете тексты сохраняются сюда как `Артист - Название.lrc`, и при
//! повторном прослушивании берутся из папки. Свои файлы можно класть туда же под тем
//! же именем — они всегда в приоритете и никогда не перезаписываются.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tauri::{AppHandle, Manager};

use super::{lrc, matching, LyricsResult, Query};

/// Метка в заголовке файлов, созданных программой.
const MARK: &str = "[by:AutoLyrics";

/// Папка с текстами: `AUTOLYRICS_LYRICS_DIR`, затем `lyrics` рядом с программой,
/// а если туда нельзя писать (например, Program Files) — в папке данных приложения.
pub fn dir(app: &AppHandle) -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(custom) = std::env::var_os("AUTOLYRICS_LYRICS_DIR") {
            candidates.push(PathBuf::from(custom));
        }
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            candidates.push(exe_dir.join("lyrics"));
        }
        if let Ok(data) = app.path().app_data_dir() {
            candidates.push(data.join("lyrics"));
        }
        candidates.into_iter().find(|d| writable(d))
    })
    .as_deref()
}

fn writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-test");
    let ok = fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

/// Ищет файл для трека. `force` пропускает файлы, созданные программой
/// (чтобы «Искать текст заново» действительно искал), но не пользовательские.
pub fn read(dir: &Path, q: &Query, force: bool) -> Option<LyricsResult> {
    let wanted = name_variants(q);
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let is_lrc = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("lrc"));
        let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if !is_lrc || !stem_matches(&stem, q, &wanted) {
            continue;
        }

        let Ok(bytes) = fs::read(&path) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim_start_matches('\u{feff}');
        let header = text.lines().take(8).find(|l| l.trim_start().starts_with(MARK));
        if header.is_some() && force {
            continue;
        }

        let synced = lrc::parse(text);
        if synced.len() < 3 {
            continue;
        }
        let (source, origin) = match header {
            Some(line) => {
                let name = line
                    .trim()
                    .trim_start_matches(MARK)
                    .trim_start_matches(" / ")
                    .trim_end_matches(']');
                (if name.is_empty() { "AutoLyrics" } else { name }.to_string(), "folder")
            }
            None => (String::new(), "file"),
        };
        return Some(LyricsResult {
            found: true,
            source,
            origin: origin.to_string(),
            synced,
            duration_ms: read_length(text),
            ..Default::default()
        });
    }
    None
}

/// Длительность из стандартного тега `[length: m:ss]` (0 — нет тега).
fn read_length(text: &str) -> u64 {
    text.lines()
        .take(10)
        .find_map(|l| l.trim().strip_prefix("[length:"))
        .and_then(|rest| rest.trim_end_matches(']').trim().split_once(':'))
        .and_then(|(m, s)| Some(m.trim().parse::<u64>().ok()? * 60 + s.trim().parse::<u64>().ok()?))
        .map_or(0, |secs| secs * 1000)
}

/// Сохраняет найденный текст как `.lrc`. Чужие файлы не трогает.
pub fn write(dir: &Path, q: &Query, album: &str, result: &LyricsResult) {
    if result.synced.is_empty() {
        return;
    }
    let path = dir.join(format!("{}.lrc", sanitize(&format!("{} - {}", q.artist, q.title))));
    if let Ok(existing) = fs::read_to_string(&path) {
        if !existing.lines().take(8).any(|l| l.trim_start().starts_with(MARK)) {
            return; // файл пользователя
        }
    }

    let mut out = format!("[ti:{}]\n[ar:{}]\n[al:{}]\n", q.title, q.artist, album);
    if result.duration_ms > 0 {
        let secs = result.duration_ms / 1000;
        out.push_str(&format!("[length:{}:{:02}]\n", secs / 60, secs % 60));
    }
    out.push_str(&format!("{MARK} / {}]\n\n", result.source));
    for line in &result.synced {
        out.push_str(&format!("{}{}\n", stamp(line.t), line.text));
    }
    let _ = fs::write(path, out);
}

/// Допустимые «плоские» имена файла для трека: как пришло из плеера, очищенное и по каждому артисту.
fn name_variants(q: &Query) -> Vec<String> {
    let mut names = vec![
        format!("{} - {}", q.artist, q.title),
        format!("{} - {}", q.main_artist, q.clean_title),
    ];
    names.extend(q.artists.iter().map(|a| format!("{a} - {}", q.clean_title)));

    let mut flat: Vec<String> = Vec::new();
    for name in names {
        let name = matching::flat(&name);
        if !name.is_empty() && !flat.contains(&name) {
            flat.push(name);
        }
    }
    flat
}

/// Имя файла подходит треку: совпало целиком или совпали название и хотя бы один артист
/// («Miyagi & Эндшпиль - Banger (feat. …)» подходит и для «Miyagi» / «Banger»).
fn stem_matches(stem: &str, q: &Query, wanted: &[String]) -> bool {
    if wanted.contains(&matching::flat(stem)) {
        return true;
    }
    let Some((artist, title)) = stem.split_once(" - ") else {
        return false;
    };
    if matching::flat(&matching::normalize_title(title)) != matching::flat(&q.clean_title) {
        return false;
    }
    let wanted_artists: Vec<String> = q.artists.iter().map(|a| matching::flat(a)).collect();
    matching::artist_list(artist)
        .iter()
        .any(|a| wanted_artists.contains(&matching::flat(a)))
}

fn stamp(ms: i64) -> String {
    let ms = ms.max(0);
    format!("[{:02}:{:02}.{:02}]", ms / 60_000, (ms / 1000) % 60, (ms / 10) % 100)
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    cleaned.trim().trim_end_matches('.').trim().chars().take(120).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::Line;

    fn query(artist: &str, title: &str) -> Query {
        Query {
            title: title.into(),
            artist: artist.into(),
            album: String::new(),
            duration_s: 0.0,
            clean_title: matching::normalize_title(title),
            main_artist: matching::primary_artist(artist),
            artists: matching::artist_list(artist),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("autolyrics-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn result() -> LyricsResult {
        LyricsResult {
            found: true,
            source: "NetEase".into(),
            synced: vec![
                Line { t: 1_500, text: "первая".into() },
                Line { t: 65_250, text: String::new() },
                Line { t: 70_000, text: "вторая".into() },
            ],
            duration_ms: 179_000,
            ..Default::default()
        }
    }

    #[test]
    fn roundtrip_and_force() {
        let dir = temp_dir("roundtrip");
        let q = query("Miyagi & Эндшпиль", "Banger (feat. OISEAU & PAPILLON)");
        write(&dir, &q, "", &result());

        let name = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().file_name();
        assert!(name.to_string_lossy().ends_with(".lrc"));

        let found = read(&dir, &q, false).expect("файл найден");
        assert_eq!(found.synced.len(), 3);
        assert_eq!(found.synced[1].t, 65_250);
        assert_eq!(found.source, "NetEase");
        assert_eq!(found.origin, "folder");
        assert_eq!(found.duration_ms, 179_000);
        // «искать заново» не берёт файл, созданный программой
        assert!(read(&dir, &q, true).is_none());
        // тот же трек под очищенным названием находит тот же файл
        assert!(read(&dir, &query("Miyagi", "Banger"), false).is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn user_file_wins_and_is_not_overwritten() {
        let dir = temp_dir("user");
        let q = query("Asper X", "Вместе");
        let own = "[00:01.00]моя строка\n[00:03.00]ещё одна\n[00:05.00]и ещё\n";
        fs::write(dir.join("Asper X - Вместе.lrc"), own).unwrap();

        let found = read(&dir, &q, true).expect("пользовательский файл найден даже при force");
        assert_eq!(found.origin, "file");

        write(&dir, &q, "", &result());
        assert_eq!(fs::read_to_string(dir.join("Asper X - Вместе.lrc")).unwrap(), own);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn illegal_characters_are_replaced() {
        assert_eq!(sanitize("AC/DC - Who Made Who?"), "AC_DC - Who Made Who_");
    }
}
