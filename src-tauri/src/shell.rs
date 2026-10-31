//! Оболочка приложения: значок в трее и автозапуск вместе с Windows.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

/// Пункты меню трея — подписи переводит интерфейс (команда `set_tray_labels`).
pub struct TrayLabels {
    show: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

/// Запущено с `--minimized` (так стартует автозапуск) — окно остаётся в трее.
pub fn started_minimized() -> bool {
    std::env::args().any(|arg| arg == "--minimized")
}

pub fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show AutoLyrics", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut tray = TrayIconBuilder::new()
        .tooltip("AutoLyrics")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    app.manage(TrayLabels { show, quit });
    Ok(())
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub fn set_tray_labels(labels: State<TrayLabels>, show: String, quit: String) {
    let _ = labels.show.set_text(show);
    let _ = labels.quit.set_text(quit);
}

/// Открывать из интерфейса разрешено только ссылку обмена Steam: так команду нельзя
/// использовать, чтобы запустить что-то постороннее.
fn is_support_url(url: &str) -> bool {
    url.starts_with("https://steamcommunity.com/tradeoffer/new/")
        && url.len() < 400
        && !url.chars().any(|c| c.is_whitespace() || matches!(c, '"' | '<' | '>' | '^' | '|'))
}

/// «Поддержать проект» — открыть ссылку обмена Steam в браузере (или в клиенте Steam).
#[tauri::command]
pub fn open_support_link(url: String) -> Result<(), String> {
    if !is_support_url(&url) {
        return Err("недопустимая ссылка".into());
    }
    // без оболочки (cmd): адрес передаётся как есть
    std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// «Свернуть» — спрятать окно в трей.
#[tauri::command]
pub fn hide_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let result = if enabled { launcher.enable() } else { launcher.disable() };
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::is_support_url;

    #[test]
    fn accepts_only_steam_trade_links() {
        assert!(is_support_url("https://steamcommunity.com/tradeoffer/new/?partner=123456789&token=AbCdEfGh"));
        assert!(!is_support_url("https://evil.example/tradeoffer/new/?partner=1"));
        assert!(!is_support_url("http://steamcommunity.com/tradeoffer/new/?partner=1"));
        assert!(!is_support_url("https://steamcommunity.com/tradeoffer/new/?partner=1 & calc.exe"));
        assert!(!is_support_url("https://steamcommunity.com/tradeoffer/new/?partner=1\"|calc"));
        assert!(!is_support_url(""));
    }
}