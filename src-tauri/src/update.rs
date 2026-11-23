use std::process::Command;

const RELEASES_PAGE: &str = "https://github.com/MonQian/EasyNote/releases";

#[tauri::command]
pub fn open_update_page() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let program = "xdg-open";

    Command::new(program).arg(RELEASES_PAGE).spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开发布页：{error}"))
}
