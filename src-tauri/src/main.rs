#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod album;
mod analyzer;
mod audio_capture;
mod genre;
mod lyrics;
mod media_watcher;
mod predict;
mod recognizer;
mod shell;

#[tauri::command]
fn current_state() -> media_watcher::Snapshot {
    media_watcher::snapshot()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            shell::setup_tray(app)?;
            // окно создано скрытым: при автозапуске (--minimized) оно остаётся в трее
            if !shell::started_minimized() {
                shell::show_main_window(app.handle());
            }
            audio_capture::start();
            analyzer::start(app.handle().clone());
            media_watcher::start(app.handle().clone());
            recognizer::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            current_state,
            shell::set_tray_labels,
            shell::hide_window,
            shell::open_support_link,
            shell::get_autostart,
            shell::set_autostart,
            genre::fetch_genre,
            predict::set_predict_next,
            lyrics::fetch_lyrics,
            lyrics::set_musixmatch_key,
            lyrics::open_lyrics_dir,
            recognizer::set_audio_recognition,
            recognizer::set_auto_recheck,
            recognizer::recognize_now,
            recognizer::select_candidate,
            recognizer::set_track_duration,
            recognizer::get_candidates
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить AutoLyrics");
}
