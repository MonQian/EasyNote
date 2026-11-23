use std::sync::{mpsc, Mutex};
use tauri::AppHandle;
use windows_sys::Win32::{System::Threading::GetCurrentThreadId, UI::{Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT}, WindowsAndMessaging::{GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_HOTKEY}}};

const WAKE: u32 = WM_APP + 17;
pub struct ShortcutState { sender: mpsc::Sender<(String, mpsc::Sender<Result<(), String>>)>, thread_id: u32, pub active: Mutex<bool> }

fn parse(value: &str) -> Result<(u32, u32), String> {
    let parts: Vec<_> = value.split('+').collect();
    if parts.len() < 3 { return Err("快捷键至少需要两个修饰键".into()); }
    let mut mods = MOD_NOREPEAT;
    for part in &parts[..parts.len()-1] {
        let flag = match *part { "Ctrl" => MOD_CONTROL, "Alt" => MOD_ALT, "Shift" => MOD_SHIFT, _ => return Err("仅支持 Ctrl、Alt、Shift 修饰键".into()) };
        if mods & flag != 0 { return Err("修饰键重复".into()); }
        mods |= flag;
    }
    let key = parts[parts.len()-1];
    let vk = if key.len() == 1 { let b = key.as_bytes()[0]; if b.is_ascii_uppercase() || b.is_ascii_digit() { b as u32 } else { return Err("按键需为 A-Z、0-9 或 F1-F12".into()) } }
        else if let Some(n) = key.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) { if (1..=12).contains(&n) { 0x6F+n } else { return Err("功能键需为 F1-F12".into()) } }
        else { return Err("按键需为 A-Z、0-9 或 F1-F12".into()) };
    Ok((mods, vk))
}

pub fn start(app: AppHandle, initial: String) -> ShortcutState {
    let (tx, rx) = mpsc::channel::<(String, mpsc::Sender<Result<(), String>>) >();
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::spawn(move || unsafe {
        let mut msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
        let tid = GetCurrentThreadId();
        let _ = ready_tx.send(tid);
        let mut registered = false;
        let mut current = String::new();
        let mut register = |candidate: String| -> Result<(), String> {
            let (mods, key) = parse(&candidate)?;
            if registered { UnregisterHotKey(std::ptr::null_mut(), 1); }
            if RegisterHotKey(std::ptr::null_mut(), 1, mods, key) == 0 {
                if registered { let (old_mods, old_key) = parse(&current).unwrap(); registered = RegisterHotKey(std::ptr::null_mut(), 1, old_mods, old_key) != 0; }
                return Err("快捷键已被其他程序占用，请换一个组合".into());
            }
            registered = true; current = candidate; Ok(())
        };
        if let Err(error) = register(initial) { eprintln!("Shortcut registration: {error}"); }
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message == WAKE { while let Ok((candidate, reply)) = rx.try_recv() { let _ = reply.send(register(candidate)); } }
            if msg.message == WM_HOTKEY { super::toggle_click_through(&app); }
        }
    });
    ShortcutState { sender: tx, thread_id: ready_rx.recv().unwrap_or(0), active: Mutex::new(false) }
}

pub fn update(state: &ShortcutState, candidate: String) -> Result<(), String> {
    parse(&candidate)?;
    let (tx, rx) = mpsc::channel();
    state.sender.send((candidate, tx)).map_err(|e| e.to_string())?;
    if unsafe { PostThreadMessageW(state.thread_id, WAKE, 0, 0) } == 0 { return Err("无法更新快捷键".into()); }
    rx.recv().map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests { use super::parse; #[test] fn validates_shortcuts() { assert!(parse("Ctrl+Alt+Shift+F12").is_ok()); assert!(parse("Ctrl+Alt+Q").is_ok()); assert!(parse("Ctrl+Q").is_err()); assert!(parse("Ctrl+Ctrl+Q").is_err()); } }
