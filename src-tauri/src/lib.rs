use std::{fs, path::PathBuf, sync::Mutex, time::{Duration, SystemTime, UNIX_EPOCH}};
use chrono::{DateTime, Months, Utc};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_notification::NotificationExt;
use rusqlite::Connection;
mod storage;
mod export;
mod update;
#[cfg(windows)] mod shortcut;
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Serialize, Deserialize)]
struct Todo {
    id: String, title: String, memo: String,
    due_at: Option<u64>, #[serde(default)] time_specified: bool,
    repeat: String, repeat_until: Option<u64>,
    remind_before: i32, remind_method: String, priority: String, kind: String,
    completed_at: Option<u64>, last_reminded_at: Option<u64>,
    #[serde(default = "default_duration")] duration_minutes: u32,
    #[serde(default)] remind_offsets: Vec<u32>,
    #[serde(default)] reminded_offsets: Vec<u32>,
    #[serde(default)] remind_at: Option<u64>,
    #[serde(default)] reminded_custom: bool,
}

#[derive(Deserialize)]
struct TodoInput {
    title: String, memo: String, due_at: Option<u64>, time_specified: bool, repeat: String,
    repeat_until: Option<u64>, remind_before: i32, remind_method: String,
    priority: String, kind: String,
    duration_minutes: u32, remind_offsets: Vec<u32>, remind_at: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Position { x: i32, y: i32, monitor: Option<String> }

#[derive(Clone, Serialize, Deserialize)]
struct NotePage { id: String, text: String, priority: String, edited_at: u64 }

#[derive(Clone, Serialize, Deserialize)]
struct Note {
    text: String,
    #[serde(default)] pages: Vec<NotePage>,
    #[serde(default)] active_page: usize,
    theme: String,
    #[serde(default = "default_opacity")]
    opacity: u8,
    priority: String,
    edited_at: u64,
    position: Option<Position>,
    #[serde(default)] window_width: Option<u16>,
    #[serde(default = "default_note_height")] note_height: u16,
    #[serde(default)] todo_height: Option<u16>,
    #[serde(default)]
    todos: Vec<Todo>,
    #[serde(default = "default_cleanup_days")]
    cleanup_days: u16,
    #[serde(default)] db_retention_days: u16,
    #[serde(default = "default_cleanup_mode")] cleanup_mode: String,
    #[serde(default = "default_visible")]
    show_todos: bool,
    #[serde(default = "default_visible")]
    show_note: bool,
    #[serde(default = "default_layer")]
    window_layer: String,
    #[serde(default = "default_shortcut")]
    click_through_shortcut: String,
    #[serde(default)] reminder_sound: bool,
    #[serde(default = "default_reminder_mode")] reminder_mode: String,
    #[serde(default)] mute_app: bool,
    #[serde(default = "default_notification_volume")] notification_volume: u8,
    #[serde(default = "default_notification_tone")] notification_tone: String,
    #[serde(default = "default_sort_order")] sort_order: String,
}

fn default_visible() -> bool { true }
fn default_layer() -> String { "top".into() }
fn default_shortcut() -> String { "Ctrl+Alt+Shift+F12".into() }
fn default_opacity() -> u8 { 72 }
fn default_duration() -> u32 { 15 }
fn default_cleanup_mode() -> String { "both".into() }
fn default_reminder_mode() -> String { "popup".into() }
fn default_sort_order() -> String { "tpk".into() }
fn default_notification_volume() -> u8 { 60 }
fn default_notification_tone() -> String { "chime".into() }
fn default_note_height() -> u16 { 251 }
fn default_cleanup_days() -> u16 { 15 }
fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64 }
fn expired_at(todo: &Todo, now: u64) -> Option<u64> {
    todo.due_at.map(|due| if todo.time_specified { due } else { due.saturating_add(86_400_000) }).filter(|due| *due < now)
}

impl Default for Note {
    fn default() -> Self {
        Self { text: String::new(), pages: vec![NotePage { id: uuid::Uuid::new_v4().to_string(), text: String::new(), priority: "gray".into(), edited_at: 0 }], active_page: 0, theme: "mist".into(), opacity: default_opacity(), priority: "gray".into(), edited_at: 0, position: None, window_width: None, note_height: default_note_height(), todo_height: None, todos: Vec::new(), cleanup_days: default_cleanup_days(), db_retention_days: 0, cleanup_mode: default_cleanup_mode(), show_todos: true, show_note: true, window_layer: default_layer(), click_through_shortcut: default_shortcut(), reminder_sound: false, reminder_mode: default_reminder_mode(), mute_app: false, notification_volume: default_notification_volume(), notification_tone: default_notification_tone(), sort_order: default_sort_order() }
    }
}

struct NoteState { path: PathBuf, db: Mutex<Connection>, note: Mutex<Note>, editing_todo: Mutex<Option<String>> }

#[tauri::command]
fn form_todo(state: tauri::State<NoteState>) -> Option<Todo> {
    let id = state.editing_todo.lock().unwrap().clone()?;
    state.note.lock().unwrap().todos.iter().find(|todo| todo.id == id).cloned()
}

fn write_note(path: &PathBuf, note: &Note) -> Result<(), String> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let temporary = path.with_extension("json.tmp");
    let mut persisted = serde_json::to_value(note).map_err(|e| e.to_string())?;
    persisted.as_object_mut().ok_or("便签数据格式无效")?.remove("todos");
    fs::write(&temporary, serde_json::to_vec_pretty(&persisted).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(temporary, path).map_err(|e| e.to_string())
}

#[tauri::command]
fn load_note(state: tauri::State<NoteState>) -> Note { state.note.lock().unwrap().clone() }

#[tauri::command]
fn load_custom_sound(state: tauri::State<NoteState>) -> Result<Vec<u8>, String> {
    let path = state.path.with_file_name("notification-sound.bin");
    fs::read(path).map_err(|e| e.to_string())
}

fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { Some("image/png") }
    else if bytes.starts_with(b"\xff\xd8\xff") { Some("image/jpeg") }
    else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" { Some("image/webp") }
    else { None }
}

#[tauri::command]
fn load_background_image(state: tauri::State<NoteState>) -> Result<Option<(String, Vec<u8>)>, String> {
    let path = state.path.with_file_name("background-image.bin");
    if !path.exists() { return Ok(None); }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mime = image_mime(&bytes).ok_or("背景图片格式无效")?;
    Ok(Some((mime.into(), bytes)))
}

#[tauri::command]
fn load_background_source(state: tauri::State<NoteState>) -> Result<Option<(String, Vec<u8>)>, String> {
    let source = state.path.with_file_name("background-source.bin");
    let path = if source.exists() { source } else { state.path.with_file_name("background-image.bin") };
    if !path.exists() { return Ok(None); }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mime = image_mime(&bytes).ok_or("背景图片格式无效")?;
    Ok(Some((mime.into(), bytes)))
}

#[tauri::command]
fn save_cropped_background(app: tauri::AppHandle, bytes: Vec<u8>) -> Result<(), String> {
    if bytes.len() > 20_000_000 || !bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) { return Err("裁切图片必须是小于 20 MB 的 PNG".into()); }
    let state = app.state::<NoteState>();
    fs::write(state.path.with_file_name("background-image.bin"), bytes).map_err(|e| e.to_string())?;
    let mut note = state.note.lock().unwrap();
    note.theme = "custom".into();
    write_note(&state.path, &note)?;
    let _ = app.emit("background-image-changed", ());
    let _ = app.emit("theme-changed", "custom");
    Ok(())
}

#[tauri::command]
fn save_note(state: tauri::State<NoteState>, mut note: Note) -> Result<(), String> {
    if !["mist", "night", "warm", "custom"].contains(&note.theme.as_str()) ||
       !["gray", "red", "yellow", "green"].contains(&note.priority.as_str()) {
        return Err("Invalid note settings".into());
    }
    if note.pages.is_empty() || note.active_page >= note.pages.len() || note.pages.iter().any(|page| !["gray", "red", "yellow", "green"].contains(&page.priority.as_str())) {
        return Err("Invalid note pages".into());
    }
    if note.window_width.is_some_and(|width| !(200..=2000).contains(&width)) { return Err("窗口宽度无效".into()); }
    if !(100..=1200).contains(&note.note_height) || note.todo_height.is_some_and(|height| !(58..=1200).contains(&height)) { return Err("窗口高度无效".into()); }
    let mut current = state.note.lock().unwrap();
    // Tray settings win over any frontend save already in flight.
    note.theme = current.theme.clone();
    note.opacity = current.opacity;
    note.todos = current.todos.clone();
    note.cleanup_days = current.cleanup_days;
    note.db_retention_days = current.db_retention_days;
    note.cleanup_mode = current.cleanup_mode.clone();
    note.show_todos = current.show_todos;
    note.show_note = current.show_note;
    note.window_layer = current.window_layer.clone();
    note.click_through_shortcut = current.click_through_shortcut.clone();
    note.reminder_sound = current.reminder_sound;
    note.reminder_mode = current.reminder_mode.clone();
    note.mute_app = current.mute_app;
    note.notification_volume = current.notification_volume;
    note.notification_tone = current.notification_tone.clone();
    note.sort_order = current.sort_order.clone();
    write_note(&state.path, &note)?;
    *current = note;
    Ok(())
}

#[tauri::command]
fn add_todo(app: tauri::AppHandle, state: tauri::State<NoteState>, input: TodoInput) -> Result<(), String> {
    validate_todo_input(&input)?;
    let mut note = state.note.lock().unwrap();
    let todo = todo_from_input(input, uuid::Uuid::new_v4().to_string());
    storage::upsert(&state.db.lock().unwrap(), &todo)?;
    note.todos.push(todo);
    drop(note);
    let _ = app.emit("todos-changed", ());
    if let Some(window) = app.get_webview_window("add-todo") { let _ = window.hide(); }
    Ok(())
}

fn validate_todo_input(input: &TodoInput) -> Result<(), String> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 120 { return Err("项目名称需为 1–120 个字符".into()); }
    if !["none","daily","weekly","monthly"].contains(&input.repeat.as_str()) ||
       !["app","system"].contains(&input.remind_method.as_str()) ||
       !["work","life","study","other"].contains(&input.kind.as_str()) ||
       !["low","normal","high"].contains(&input.priority.as_str()) ||
       input.duration_minutes > 10_080 ||
       input.remind_offsets.iter().any(|value| ![1440, 2880, 4320, 10080].contains(value)) { return Err("待办选项无效".into()); }
    if !input.remind_offsets.is_empty() && (input.due_at.is_none() || !input.time_specified) { return Err("设置提醒需要日期和时间".into()); }
    if input.remind_at.is_some_and(|at| at == 0 || input.due_at.is_some_and(|due| at > due)) { return Err("自定义提醒日期不能晚于待办时间".into()); }
    if input.repeat != "none" && input.due_at.is_none() { return Err("重复待办需要日期和时间".into()); }
    if input.repeat_until.zip(input.due_at).is_some_and(|(until, due)| until < due) { return Err("结束重复日期不能早于待办日期".into()); }
    Ok(())
}

fn todo_from_input(input: TodoInput, id: String) -> Todo {
    Todo { id, title: input.title.trim().into(), memo: input.memo.trim().into(),
        due_at: input.due_at, time_specified: input.time_specified, repeat: input.repeat, repeat_until: input.repeat_until,
        remind_before: input.remind_before, remind_method: input.remind_method,
        priority: input.priority, kind: input.kind, completed_at: None, last_reminded_at: None,
        duration_minutes: input.duration_minutes, remind_offsets: input.remind_offsets, reminded_offsets: Vec::new(), remind_at: input.remind_at, reminded_custom: false }
}

#[tauri::command]
fn update_todo(app: tauri::AppHandle, state: tauri::State<NoteState>, id: String, input: TodoInput) -> Result<(), String> {
    validate_todo_input(&input)?;
    let mut note = state.note.lock().unwrap();
    let todo = note.todos.iter_mut().find(|todo| todo.id == id).ok_or("待办不存在")?;
    let completed_at = todo.completed_at;
    let mut updated = todo_from_input(input, id);
    updated.completed_at = completed_at;
    storage::upsert(&state.db.lock().unwrap(), &updated)?;
    *todo = updated;
    drop(note);
    *state.editing_todo.lock().unwrap() = None;
    let _ = app.emit("todos-changed", ());
    if let Some(window) = app.get_webview_window("add-todo") { let _ = window.hide(); }
    Ok(())
}

fn next_due(due: u64, repeat: &str) -> Option<u64> {
    let date = DateTime::<Utc>::from_timestamp_millis(due as i64)?;
    let next = match repeat {
        "daily" => date.checked_add_signed(chrono::Duration::days(1))?,
        "weekly" => date.checked_add_signed(chrono::Duration::weeks(1))?,
        "monthly" => date.checked_add_months(Months::new(1))?,
        _ => return None,
    };
    Some(next.timestamp_millis() as u64)
}

#[tauri::command]
fn toggle_todo(app: tauri::AppHandle, state: tauri::State<NoteState>, id: String) -> Result<(), String> {
    let mut note = state.note.lock().unwrap();
    let todo = note.todos.iter_mut().find(|item| item.id == id).ok_or("待办不存在")?;
    if todo.completed_at.is_some() { todo.completed_at = None; }
    else if let Some(mut next) = todo.due_at.and_then(|due| next_due(due, &todo.repeat)) {
        while next <= now_ms() {
            if let Some(following) = next_due(next, &todo.repeat) { next = following; }
            else { break; }
        }
        if todo.repeat_until.is_none_or(|until| next <= until) {
            todo.due_at = Some(next); todo.last_reminded_at = None; todo.reminded_offsets.clear();
        } else { todo.completed_at = Some(now_ms()); }
    } else { todo.completed_at = Some(now_ms()); }
    storage::upsert(&state.db.lock().unwrap(), todo)?;
    drop(note);
    let _ = app.emit("todos-changed", ());
    Ok(())
}

#[tauri::command]
fn delete_todo(app: tauri::AppHandle, state: tauri::State<NoteState>, id: String) -> Result<(), String> {
    let mut note = state.note.lock().unwrap();
    if !note.todos.iter().any(|item| item.id == id) { return Err("待办不存在".into()); }
    if !storage::delete(&state.db.lock().unwrap(), &id)? { return Err("数据库中未找到待办".into()); }
    note.todos.retain(|item| item.id != id);
    drop(note);
    let _ = app.emit("todos-changed", ());
    Ok(())
}

#[tauri::command]
fn show_todo_form(app: &tauri::AppHandle) -> Result<(), String> {
    let main = app.get_webview_window("main").ok_or("主窗口不存在")?;
    let add = app.get_webview_window("add-todo").ok_or("添加窗口不存在")?;
    let pos = main.outer_position().map_err(|e| e.to_string())?;
    let size = main.outer_size().map_err(|e| e.to_string())?;
    // Keep the form adjacent to the main window, including near screen edges.
    let (x, y) = if let Ok(Some(monitor)) = main.current_monitor() {
        let left = monitor.position().x;
        let top = monitor.position().y;
        let right = left + monitor.size().width as i32;
        let bottom = top + monitor.size().height as i32;
        let add_size = add.outer_size().map_err(|e| e.to_string())?;
        let add_width = add_size.width as i32;
        let gap = 2;
        let right_x = pos.x + size.width as i32 + gap;
        let left_x = pos.x - add_width - gap;
        let x = if right_x + add_width <= right { right_x }
            else if left_x >= left { left_x }
            else {
                let main_x = left.max(right - size.width as i32 - add_width - gap);
                main.set_position(tauri::PhysicalPosition::new(main_x, pos.y)).map_err(|e| e.to_string())?;
                main_x + size.width as i32 + gap
            };
        (x, pos.y.clamp(top, (bottom - add_size.height as i32).max(top)))
    } else { (pos.x + size.width as i32 + 2, pos.y) };
    add.set_position(tauri::PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    add.show().map_err(|e| e.to_string())?;
    add.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_add_todo(app: tauri::AppHandle, state: tauri::State<NoteState>) -> Result<(), String> {
    *state.editing_todo.lock().unwrap() = None;
    show_todo_form(&app)?;
    let _ = app.emit("todo-add-opened", ());
    Ok(())
}

#[tauri::command]
fn open_edit_todo(app: tauri::AppHandle, state: tauri::State<NoteState>, id: String) -> Result<(), String> {
    if !state.note.lock().unwrap().todos.iter().any(|todo| todo.id == id) { return Err("待办不存在".into()); }
    *state.editing_todo.lock().unwrap() = Some(id.clone());
    show_todo_form(&app)?;
    let _ = app.emit("todo-edit-opened", id);
    Ok(())
}

#[tauri::command]
fn close_add_todo(app: tauri::AppHandle) { if let Some(window) = app.get_webview_window("add-todo") { let _ = window.hide(); } }

#[tauri::command]
async fn export_todos(app: tauri::AppHandle, state: tauri::State<'_, NoteState>, options: export::ExportOptions) -> Result<String, String> {
    options.validate()?;
    let todos = storage::list(&state.db.lock().unwrap(), true)?;
    let todos = export::select(todos, &options);
    if todos.is_empty() { return Err("所选时间范围和类型内没有待办；时间按待办计划日期筛选，可选择更长范围或“导出所有”".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        let name = format!("EasyNote-历史待办.{}", options.format);
        let selected = app.dialog().file().set_file_name(&name).add_filter("待办文档", &[&options.format]).blocking_save_file();
        let Some(selected) = selected else { return Ok("已取消".to_string()); };
        let path = selected.into_path().map_err(|e| e.to_string())?;
        let path = path.with_extension(&options.format);
        export::write(&path, &todos, &options)?;
        Ok(format!("已导出 {} 条待办：{}", todos.len(), path.display()))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn close_export(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("export") { let _ = window.hide(); }
}

fn process_todos(app: &tauri::AppHandle) {
    let state = app.state::<NoteState>();
    let mut note = state.note.lock().unwrap();
    let now = now_ms();
    let before = note.todos.len();
    let cleanup = note.cleanup_days;
    let cleanup_mode = note.cleanup_mode.clone();
    note.todos.retain(|todo| {
        if cleanup == 0 { return true; }
        let expired = expired_at(todo, now);
        let completed = todo.completed_at;
        let since = match cleanup_mode.as_str() {
            "expired" => expired,
            "completed" => completed,
            _ => expired.zip(completed).map(|(due, done)| due.max(done)),
        };
        let keep = since.is_none_or(|at| at.saturating_add(cleanup as u64 * 86_400_000) > now);
        if !keep {
            if let Err(error) = storage::archive(&state.db.lock().unwrap(), &todo.id) {
                eprintln!("Todo archive error: {error}"); return true;
            }
        }
        keep
    });
    let deleted = before != note.todos.len();
    let mut reminders = Vec::new();
    for todo in &mut note.todos {
        if todo.completed_at.is_some() { continue; }
        let previous = (todo.reminded_custom, todo.reminded_offsets.len(), todo.last_reminded_at);
        if let Some(at) = todo.remind_at {
            if !todo.reminded_custom && now >= at && now < at.saturating_add(86_400_000) {
                todo.reminded_custom = true;
                reminders.push(todo.title.clone());
            }
        }
        if let Some(due) = todo.due_at {
            if !todo.remind_offsets.is_empty() {
                for offset in &todo.remind_offsets {
                    let trigger = due.saturating_sub(*offset as u64 * 60_000);
                    if now >= trigger && now < trigger.saturating_add(86_400_000) && !todo.reminded_offsets.contains(offset) {
                        todo.reminded_offsets.push(*offset);
                        reminders.push(todo.title.clone());
                    }
                }
            } else if todo.remind_before >= 0 && todo.last_reminded_at.is_none() {
                let trigger = due.saturating_sub(todo.remind_before as u64 * 60_000);
                if now >= trigger && now <= due.saturating_add(86_400_000) {
                    todo.last_reminded_at = Some(now);
                    reminders.push(todo.title.clone());
                }
            }
        }
        if previous != (todo.reminded_custom, todo.reminded_offsets.len(), todo.last_reminded_at) {
            if let Err(error) = storage::upsert(&state.db.lock().unwrap(), todo) { eprintln!("Reminder save error: {error}"); }
        }
    }
    if let Err(error) = storage::purge_archived(&state.db.lock().unwrap(), note.db_retention_days) { eprintln!("Database cleanup error: {error}"); }
    let reminder_mode = note.reminder_mode.clone();
    drop(note);
    if deleted { let _ = app.emit("todos-changed", ()); }
    for title in reminders {
        if reminder_mode != "sound" {
            if let Err(error) = app.notification().builder().title("EasyNote 待办提醒").body(&title).silent().show() {
                eprintln!("Notification error: {error}");
            }
        }
        let _ = app.emit("todo-reminder", title);
    }
}

fn toggle_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => { let _ = window.hide(); }
            Ok(false) => { let _ = window.show(); let _ = window.set_focus(); }
            Err(error) => eprintln!("Window visibility error: {error}"),
        }
    }
}

#[cfg(windows)]
fn toggle_click_through(app: &tauri::AppHandle) {
    let state = app.state::<shortcut::ShortcutState>();
    let mut active = state.active.lock().unwrap();
    if let Some(window) = app.get_webview_window("main") {
        if window.set_ignore_cursor_events(!*active).is_ok() {
            *active = !*active;
            let _ = app.emit("click-through-changed", *active);
        }
    }
}

#[tauri::command]
fn set_click_through_shortcut(app: tauri::AppHandle, value: String) -> Result<(), String> {
    #[cfg(windows)] {
        let state = app.state::<NoteState>();
        let shortcuts = app.state::<shortcut::ShortcutState>();
        let mut note = state.note.lock().unwrap();
        let previous = note.click_through_shortcut.clone();
        shortcut::update(&shortcuts, value.clone())?;
        note.click_through_shortcut = value;
        if let Err(error) = write_note(&state.path, &note) {
            note.click_through_shortcut = previous.clone();
            let _ = shortcut::update(&shortcuts, previous);
            return Err(error);
        }
        Ok(())
    }
    #[cfg(not(windows))] { let _ = (app, value); Err("仅支持 Windows".into()) }
}

fn apply_window_layer(app: &tauri::AppHandle, layer: &str) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口不存在")?;
    window.set_always_on_top(layer == "top").map_err(|e| e.to_string())?;
    #[cfg(windows)]
    if layer == "desktop" {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindow, SetWindowPos, GW_HWNDPREV, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
        let hwnd = window.hwnd().map_err(|e| e.to_string())?;
        let shell = unsafe { GetShellWindow() };
        if shell.is_null() { return Err("桌面窗口不可用".into()); }
        let above_shell = unsafe { GetWindow(shell, GW_HWNDPREV) };
        if above_shell.is_null() { return Err("无法确定桌面窗口层级".into()); }
        if above_shell == hwnd.0 as _ { return Ok(()); }
        let ok = unsafe { SetWindowPos(hwnd.0 as _, above_shell, 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE) };
        if ok == 0 { return Err(std::io::Error::last_os_error().to_string()); }
    }
    Ok(())
}

#[cfg(windows)]
fn other_window_is_fullscreen(app: &tauri::AppHandle) -> bool {
    use windows_sys::Win32::{Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST}, UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, IsIconic}};
    let foreground = unsafe { GetForegroundWindow() };
    let Some(main) = app.get_webview_window("main") else { return false };
    let Ok(own) = main.hwnd() else { return false };
    if foreground.is_null() || foreground == own.0 as _ || unsafe { IsIconic(foreground) } != 0 { return false; }
    let mut bounds = unsafe { std::mem::zeroed() };
    let monitor = unsafe { MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST) };
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if monitor.is_null() || unsafe { GetWindowRect(foreground, &mut bounds) } == 0 || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 { return false; }
    bounds.left <= info.rcMonitor.left && bounds.top <= info.rcMonitor.top && bounds.right >= info.rcMonitor.right && bounds.bottom >= info.rcMonitor.bottom
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![load_note, load_custom_sound, load_background_image, load_background_source, save_cropped_background, save_note, set_click_through_shortcut, form_todo, add_todo, update_todo, toggle_todo, delete_todo, open_add_todo, open_edit_todo, close_add_todo, export_todos, close_export, update::open_update_page])
        .setup(|app| {
            // The OS registration is the source of truth for startup state.
            app.handle().plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))?;
            let path = app.path().app_data_dir()?.join("notes.json");
            if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
            let mut note: Note = if path.exists() {
                serde_json::from_slice(&fs::read(&path)?)?
            } else { Note::default() };
            let mut db = storage::open(&path.with_file_name("todos.sqlite3")).map_err(std::io::Error::other)?;
            if !note.todos.is_empty() {
                if storage::count(&db).map_err(std::io::Error::other)? == 0 {
                    let backup = path.with_file_name("notes.pre-sqlite.json");
                    if !backup.exists() { fs::copy(&path, backup)?; }
                    let tx = db.transaction()?;
                    for todo in &note.todos { storage::upsert(&tx, todo).map_err(std::io::Error::other)?; }
                    tx.commit()?;
                }
                note.todos.clear();
                write_note(&path, &note).map_err(std::io::Error::other)?;
            }
            note.todos = storage::list(&db, false).map_err(std::io::Error::other)?;
            if note.pages.is_empty() {
                note.pages.push(NotePage { id: uuid::Uuid::new_v4().to_string(), text: note.text.clone(), priority: note.priority.clone(), edited_at: note.edited_at });
            }
            if note.reminder_sound && note.reminder_mode == "popup" { note.reminder_mode = "both".into(); }
            note.active_page = note.active_page.min(note.pages.len() - 1);
            let theme: String = note.theme.clone();
            note.opacity = note.opacity.min(100);
            let opacity = note.opacity;
            let cleanup_days = note.cleanup_days;
            let cleanup_mode = note.cleanup_mode.clone();
            let db_retention_days = note.db_retention_days;
            let show_todos = note.show_todos;
            let show_note = note.show_note;
            let window_layer = note.window_layer.clone();
            let reminder_mode = note.reminder_mode.clone();
            let mute_app = note.mute_app;
            let notification_volume = note.notification_volume;
            let notification_tone = note.notification_tone.clone();
            let sort_order = note.sort_order.clone();
            let click_through_shortcut = note.click_through_shortcut.clone();
            app.manage(NoteState { path, db: Mutex::new(db), note: Mutex::new(note), editing_todo: Mutex::new(None) });
            #[cfg(windows)] app.manage(shortcut::start(app.handle().clone(), click_through_shortcut));
            #[cfg(not(windows))] let _ = click_through_shortcut;
            apply_window_layer(app.handle(), &window_layer).map_err(std::io::Error::other)?;
            let ticker = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                loop { process_todos(&ticker); std::thread::sleep(Duration::from_secs(60)); }
            });
            #[cfg(windows)] {
                let layer_app = app.handle().clone();
                let initially_topmost = window_layer == "top";
                std::thread::spawn(move || {
                    let mut topmost = initially_topmost;
                    loop {
                        std::thread::sleep(Duration::from_millis(750));
                        let layer = layer_app.state::<NoteState>().note.lock().unwrap().window_layer.clone();
                        if layer != "top" { topmost = false; continue; }
                        let wanted = !other_window_is_fullscreen(&layer_app);
                        if wanted != topmost {
                            if let Some(window) = layer_app.get_webview_window("main") {
                                if window.set_always_on_top(wanted).is_ok() { topmost = wanted; }
                            }
                        }
                    }
                });
            }

            let mist = CheckMenuItemBuilder::with_id("theme_mist", "雾白").checked(theme == "mist").build(app)?;
            let night = CheckMenuItemBuilder::with_id("theme_night", "夜岩").checked(theme == "night").build(app)?;
            let warm = CheckMenuItemBuilder::with_id("theme_warm", "暖纸").checked(theme == "warm").build(app)?;
            let custom = CheckMenuItemBuilder::with_id("theme_custom", "图片背景").checked(theme == "custom").build(app)?;
            let startup = CheckMenuItemBuilder::with_id("startup", "开机自启")
                .checked(app.autolaunch().is_enabled().unwrap_or(false)).build(app)?;
            let todos_visible = CheckMenuItemBuilder::with_id("show_todos", "显示待办").checked(show_todos).build(app)?;
            let note_visible = CheckMenuItemBuilder::with_id("show_note", "显示便签").checked(show_note).build(app)?;
            let layer_desktop = CheckMenuItemBuilder::with_id("layer_desktop", "仅桌面上").checked(window_layer == "desktop").build(app)?;
            let layer_top = CheckMenuItemBuilder::with_id("layer_top", "置于顶层").checked(window_layer == "top").build(app)?;
            let layer_menu = SubmenuBuilder::new(app, "悬浮窗层级").items(&[&layer_desktop, &layer_top]).build()?;
            let reminder_popup = CheckMenuItemBuilder::with_id("reminder_popup", "仅弹窗").checked(reminder_mode == "popup").build(app)?;
            let reminder_with_sound = CheckMenuItemBuilder::with_id("reminder_both", "弹窗和声音").checked(reminder_mode == "both").build(app)?;
            let reminder_only_sound = CheckMenuItemBuilder::with_id("reminder_sound", "仅声音").checked(reminder_mode == "sound").build(app)?;
            let mute = CheckMenuItemBuilder::with_id("mute_app", "关闭程序音量").checked(mute_app).build(app)?;
            let volume_values = [0u8, 20, 40, 60, 80, 100];
            let volume_options = volume_values.map(|value| CheckMenuItemBuilder::with_id(format!("volume_{value}"), format!("{value}%")).checked(notification_volume == value).build(app)).into_iter().collect::<Result<Vec<_>, _>>()?;
            let mut volume_menu = SubmenuBuilder::new(app, "通知音量");
            for option in &volume_options { volume_menu = volume_menu.item(option); }
            let volume_menu = volume_menu.build()?;
            let tone_values = ["chime", "bell", "soft"];
            let tone_labels = ["清脆", "铃声", "轻柔"];
            let tone_options = tone_values.iter().zip(tone_labels).map(|(value, label)| CheckMenuItemBuilder::with_id(format!("tone_{value}"), label).checked(notification_tone == *value).build(app)).collect::<Result<Vec<_>, _>>()?;
            let tone_custom = CheckMenuItemBuilder::with_id("tone_custom", "自定义音频…").checked(notification_tone == "custom").build(app)?;
            let mut tone_menu = SubmenuBuilder::new(app, "提示音");
            for option in &tone_options { tone_menu = tone_menu.item(option); }
            let tone_menu = tone_menu.separator().item(&tone_custom).build()?;
            let reminder_menu = SubmenuBuilder::new(app, "提醒方式").items(&[&reminder_popup, &reminder_with_sound, &reminder_only_sound]).build()?;
            let sort_orders = ["tpk", "tkp", "ptk", "pkt", "ktp", "kpt"];
            let sort_labels = ["时间 · 优先级 · 类型", "时间 · 类型 · 优先级", "优先级 · 时间 · 类型", "优先级 · 类型 · 时间", "类型 · 时间 · 优先级", "类型 · 优先级 · 时间"];
            let sort_options = sort_orders.iter().zip(sort_labels).map(|(order, label)|
                CheckMenuItemBuilder::with_id(format!("sort_{order}"), label).checked(sort_order == *order).build(app)
            ).collect::<Result<Vec<_>, _>>()?;
            let mut sort_menu = SubmenuBuilder::new(app, "排序方式");
            for option in &sort_options { sort_menu = sort_menu.item(option); }
            let sort_menu = sort_menu.build()?;
            let opacity_current = MenuItemBuilder::with_id("opacity_current", format!("当前：{opacity}%"))
                .enabled(false).build(app)?;
            let opacity_presets = [0u8, 10, 25, 40, 50, 60, 75, 90, 100].map(|value| {
                CheckMenuItemBuilder::with_id(format!("opacity_{value}"), format!("{value}%"))
                    .checked(opacity == value).build(app)
            });
            let opacity_presets = opacity_presets.into_iter().collect::<Result<Vec<_>, _>>()?;
            let mut opacity_menu = SubmenuBuilder::new(app, "透明度")
                .item(&opacity_current).separator();
            for preset in &opacity_presets { opacity_menu = opacity_menu.item(preset); }
            let opacity_menu = opacity_menu.build()?;
            let cleanup_current = MenuItemBuilder::with_id("cleanup_current", format!("当前：{}", if cleanup_days == 0 { "不自动删除".to_string() } else { format!("{cleanup_days} 天") }))
                .enabled(false).build(app)?;
            let cleanup_options = [0u16, 7, 15, 30, 60].map(|days| {
                CheckMenuItemBuilder::with_id(format!("cleanup_{days}"), if days == 0 { "不自动删除".to_string() } else { format!("{days} 天") })
                    .checked(cleanup_days == days).build(app)
            }).into_iter().collect::<Result<Vec<_>, _>>()?;
            let cleanup_expired = CheckMenuItemBuilder::with_id("cleanup_mode_expired", "只清理已过期").checked(cleanup_mode == "expired").build(app)?;
            let cleanup_completed = CheckMenuItemBuilder::with_id("cleanup_mode_completed", "只清理已完成").checked(cleanup_mode == "completed").build(app)?;
            let cleanup_both = CheckMenuItemBuilder::with_id("cleanup_mode_both", "已过期且已完成").checked(cleanup_mode == "both").build(app)?;
            let cleanup_menu = SubmenuBuilder::new(app, "待办清理").items(&[&cleanup_expired, &cleanup_completed, &cleanup_both]).build()?;
            let mut cleanup_time_menu = SubmenuBuilder::new(app, "待办清理时间").item(&cleanup_current);
            for option in &cleanup_options { cleanup_time_menu = cleanup_time_menu.item(option); }
            let cleanup_time_menu = cleanup_time_menu.build()?;
            let retention_days = [0u16, 15, 30, 60, 120, 365];
            let retention_labels = ["永不", "15 天", "30 天", "60 天", "120 天", "一年"];
            let retention_options = retention_days.iter().zip(retention_labels).map(|(days, label)|
                CheckMenuItemBuilder::with_id(format!("retention_{days}"), label).checked(db_retention_days == *days).build(app)
            ).collect::<Result<Vec<_>, _>>()?;
            let mut retention_menu = SubmenuBuilder::new(app, "数据库清理时间");
            for option in &retention_options { retention_menu = retention_menu.item(option); }
            let retention_menu = retention_menu.build()?;
            let themes = SubmenuBuilder::new(app, "主题切换").items(&[&mist, &night, &warm, &custom]).separator().text("theme_choose_image", "选择背景图片…").text("theme_crop_image", "调整图片范围…").build()?;
            let menu = MenuBuilder::new(app)
                .text("toggle", "显示/隐藏窗口")
                .text("click_through", "切换鼠标穿透")
                .text("shortcut_settings", "设置穿透快捷键…")
                .item(&todos_visible).item(&note_visible)
                .separator()
                .item(&themes).item(&opacity_menu).item(&layer_menu).item(&sort_menu).item(&reminder_menu)
                .separator()
                .item(&cleanup_menu).item(&cleanup_time_menu).item(&retention_menu).text("export_history", "导出历史待办")
                .separator().item(&mute).item(&volume_menu).item(&tone_menu);
            let menu = menu.separator().item(&startup).separator().text("check_update", "检查更新").text("tips", "使用手册").separator().text("quit", "退出 EasyNote").build()?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().ok_or("Missing tray icon")?.clone())
                .tooltip("EasyNote")
                .menu(&menu)
                // Right click owns the menu; left click toggles the note.
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        toggle_window(tray.app_handle());
                    }
                })
                .on_menu_event(move |app, event| match event.id().0.as_str() {
                    "toggle" => toggle_window(app),
                    "click_through" => { #[cfg(windows)] toggle_click_through(app); },
                    "shortcut_settings" => { if let Some(window) = app.get_webview_window("shortcut-settings") { let _ = window.show(); let _ = window.set_focus(); } },
                    "export_history" => {
                        if let Some(window) = app.get_webview_window("export") { let _ = window.show(); let _ = window.set_focus(); }
                    }
                    id if id.starts_with("retention_") => {
                        if let Ok(days) = id.trim_start_matches("retention_").parse::<u16>() {
                            if !retention_days.contains(&days) { return; }
                            let state = app.state::<NoteState>();
                            let mut note = state.note.lock().unwrap();
                            let previous = note.db_retention_days;
                            note.db_retention_days = days;
                            if let Err(error) = write_note(&state.path, &note) { note.db_retention_days = previous; eprintln!("Retention setting error: {error}"); return; }
                            for (index, option) in retention_options.iter().enumerate() { let _ = option.set_checked(retention_days[index] == days); }
                            drop(note);
                            process_todos(app);
                        }
                    }
                    id if id.starts_with("sort_") => {
                        let selected = id.trim_start_matches("sort_");
                        if !sort_orders.contains(&selected) { return; }
                        let state = app.state::<NoteState>();
                        let mut note = state.note.lock().unwrap();
                        let previous = note.sort_order.clone();
                        note.sort_order = selected.into();
                        if let Err(error) = write_note(&state.path, &note) {
                            note.sort_order = previous;
                            eprintln!("Sort setting error: {error}"); return;
                        }
                        for (index, option) in sort_options.iter().enumerate() { let _ = option.set_checked(sort_orders[index] == selected); }
                        let _ = app.emit("sort-changed", selected);
                    }
                    "tips" => { if let Some(window) = app.get_webview_window("manual") { let _ = window.show(); let _ = window.set_focus(); } },
                    "check_update" => {
                        if let Some(window) = app.get_webview_window("update") {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = app.emit_to("update", "check-update", ());
                        }
                    },
                    "reminder_popup" | "reminder_both" | "reminder_sound" | "mute_app" => {
                        let state = app.state::<NoteState>();
                        let mut note = state.note.lock().unwrap();
                        let old_mode = note.reminder_mode.clone();
                        let old_sound = note.reminder_sound;
                        let old_mute = note.mute_app;
                        if event.id().0 == "mute_app" { note.mute_app = !note.mute_app; }
                        else { note.reminder_mode = event.id().0.trim_start_matches("reminder_").into(); note.reminder_sound = note.reminder_mode != "popup"; }
                        if let Err(error) = write_note(&state.path, &note) {
                            note.reminder_mode = old_mode; note.reminder_sound = old_sound; note.mute_app = old_mute;
                            eprintln!("Reminder setting error: {error}"); return;
                        }
                        let _ = reminder_popup.set_checked(note.reminder_mode == "popup");
                        let _ = reminder_with_sound.set_checked(note.reminder_mode == "both");
                        let _ = reminder_only_sound.set_checked(note.reminder_mode == "sound");
                        let _ = mute.set_checked(note.mute_app);
                        let _ = app.emit("reminder-settings-changed", (note.reminder_mode.clone(), note.mute_app));
                    }
                    id if id.starts_with("volume_") || id.starts_with("tone_") => {
                        if id == "tone_custom" {
                            let app = app.clone();
                            let tone_options = tone_options.clone();
                            let tone_custom = tone_custom.clone();
                            let volume_options = volume_options.clone();
                            app.dialog().file().add_filter("音频文件", &["mp3", "wav", "ogg"]).pick_file(move |selected| {
                                let Some(selected) = selected else { return; };
                                let Ok(path) = selected.into_path() else { return; };
                                let Ok(metadata) = fs::metadata(&path) else { return; };
                                if metadata.len() > 5_000_000 { eprintln!("提示音文件不能超过 5 MB"); return; }
                                let state = app.state::<NoteState>();
                                let target = state.path.with_file_name("notification-sound.bin");
                                if let Err(error) = fs::copy(&path, target) { eprintln!("Custom sound copy error: {error}"); return; }
                                let mut note = state.note.lock().unwrap();
                                let old_tone = note.notification_tone.clone();
                                note.notification_tone = "custom".into();
                                if let Err(error) = write_note(&state.path, &note) { note.notification_tone = old_tone; eprintln!("Custom sound save error: {error}"); return; }
                                for option in &tone_options { let _ = option.set_checked(false); }
                                let _ = tone_custom.set_checked(true);
                                for (index, option) in volume_options.iter().enumerate() { let _ = option.set_checked(note.notification_volume == volume_values[index]); }
                                let _ = app.emit("notification-sound-changed", (note.notification_volume, note.notification_tone.clone()));
                            });
                            return;
                        }
                        let state = app.state::<NoteState>();
                        let mut note = state.note.lock().unwrap();
                        let old_volume = note.notification_volume;
                        let old_tone = note.notification_tone.clone();
                        if let Some(value) = id.strip_prefix("volume_") {
                            let Ok(value) = value.parse::<u8>() else { return };
                            if !volume_values.contains(&value) { return; }
                            note.notification_volume = value;
                        } else if let Some(value) = id.strip_prefix("tone_") {
                            if !tone_values.contains(&value) { return; }
                            note.notification_tone = value.into();
                        }
                        if let Err(error) = write_note(&state.path, &note) {
                            note.notification_volume = old_volume;
                            note.notification_tone = old_tone;
                            eprintln!("Notification sound setting error: {error}"); return;
                        }
                        for (index, option) in volume_options.iter().enumerate() { let _ = option.set_checked(note.notification_volume == volume_values[index]); }
                        for (index, option) in tone_options.iter().enumerate() { let _ = option.set_checked(note.notification_tone == tone_values[index]); }
                        let _ = tone_custom.set_checked(note.notification_tone == "custom");
                        let _ = app.emit("notification-sound-changed", (note.notification_volume, note.notification_tone.clone()));
                    }
                    "layer_desktop" | "layer_top" => {
                        let selected = event.id().0.trim_start_matches("layer_");
                        let state = app.state::<NoteState>();
                        let mut note = state.note.lock().unwrap();
                        let previous = note.window_layer.clone();
                        if let Err(error) = apply_window_layer(app, selected) { eprintln!("Window layer error: {error}"); return; }
                        note.window_layer = selected.into();
                        if let Err(error) = write_note(&state.path, &note) {
                            note.window_layer = previous.clone();
                            let _ = apply_window_layer(app, &previous);
                            eprintln!("Window layer save error: {error}"); return;
                        }
                        let _ = layer_desktop.set_checked(selected == "desktop");
                        let _ = layer_top.set_checked(selected == "top");
                    }
                    "show_todos" | "show_note" => {
                        let state = app.state::<NoteState>();
                        let mut note = state.note.lock().unwrap();
                        let is_todos = event.id().0 == "show_todos";
                        let previous = if is_todos { note.show_todos } else { note.show_note };
                        if is_todos { note.show_todos = !previous; } else { note.show_note = !previous; }
                        if let Err(error) = write_note(&state.path, &note) {
                            if is_todos { note.show_todos = previous; } else { note.show_note = previous; }
                            eprintln!("Visibility setting error: {error}"); return;
                        }
                        let visible = !previous;
                        let _ = if is_todos { todos_visible.set_checked(visible) } else { note_visible.set_checked(visible) };
                        let _ = app.emit("visibility-changed", (note.show_todos, note.show_note));
                    }
                    "quit" => app.exit(0),
                    "startup" => {
                        let manager = app.autolaunch();
                        let enabled = manager.is_enabled().unwrap_or(false);
                        let result = if enabled { manager.disable() } else { manager.enable() };
                        if let Err(error) = result { eprintln!("Autostart error: {error}"); }
                        let _ = startup.set_checked(manager.is_enabled().unwrap_or(enabled));
                    }
                    id if id.starts_with("cleanup_mode_") => {
                        let selected = id.trim_start_matches("cleanup_mode_");
                        if !["expired", "completed", "both"].contains(&selected) { return; }
                        let state = app.state::<NoteState>();
                        let mut note = state.note.lock().unwrap();
                        let previous = note.cleanup_mode.clone();
                        note.cleanup_mode = selected.into();
                        if let Err(error) = write_note(&state.path, &note) { note.cleanup_mode = previous; eprintln!("Cleanup mode error: {error}"); return; }
                        drop(note);
                        let _ = cleanup_expired.set_checked(selected == "expired");
                        let _ = cleanup_completed.set_checked(selected == "completed");
                        let _ = cleanup_both.set_checked(selected == "both");
                        process_todos(app);
                    }
                    id if id.starts_with("cleanup_") => {
                        if let Ok(days) = id.trim_start_matches("cleanup_").parse::<u16>() {
                            let state = app.state::<NoteState>();
                            let mut note = state.note.lock().unwrap();
                            note.cleanup_days = days;
                            if let Err(error) = write_note(&state.path, &note) { eprintln!("Cleanup setting error: {error}"); return; }
                            drop(note);
                            let _ = cleanup_current.set_text(format!("当前：{}", if days == 0 { "不自动删除".to_string() } else { format!("{days} 天") }));
                            for (index, option) in cleanup_options.iter().enumerate() { let _ = option.set_checked([0, 7, 15, 30, 60][index] == days); }
                            process_todos(app);
                        }
                    }
                    id if id.starts_with("opacity_") => {
                        let state = app.state::<NoteState>();
                        let mut note = state.note.lock().unwrap();
                        let previous = note.opacity;
                        let next = id.trim_start_matches("opacity_").parse::<u8>().unwrap_or(previous);
                        note.opacity = next;
                        if let Err(error) = write_note(&state.path, &note) {
                            note.opacity = previous;
                            eprintln!("Opacity save error: {error}");
                            return;
                        }
                        drop(note);
                        let _ = opacity_current.set_text(format!("当前：{next}%"));
                        for (index, preset) in opacity_presets.iter().enumerate() {
                            let _ = preset.set_checked([0, 10, 25, 40, 50, 60, 75, 90, 100][index] == next);
                        }
                        let _ = app.emit("opacity-changed", next);
                    }
                    "theme_choose_image" => {
                        let app = app.clone();
                        let mist = mist.clone();
                        let night = night.clone();
                        let warm = warm.clone();
                        let custom = custom.clone();
                        app.dialog().file().add_filter("图片文件", &["png", "jpg", "jpeg", "webp"]).pick_file(move |selected| {
                            let Some(selected) = selected else { return; };
                            let Ok(path) = selected.into_path() else { return; };
                            let Ok(bytes) = fs::read(&path) else { return; };
                            if bytes.len() > 20_000_000 || image_mime(&bytes).is_none() {
                                eprintln!("背景图片需为 PNG、JPEG 或 WebP，且不超过 20 MB"); return;
                            }
                            let state = app.state::<NoteState>();
                            if let Err(error) = fs::write(state.path.with_file_name("background-source.bin"), &bytes) { eprintln!("Background source save error: {error}"); return; }
                            let target = state.path.with_file_name("background-image.bin");
                            if let Err(error) = fs::write(target, bytes) { eprintln!("Background image save error: {error}"); return; }
                            let mut note = state.note.lock().unwrap();
                            let previous = note.theme.clone();
                            note.theme = "custom".into();
                            if let Err(error) = write_note(&state.path, &note) { note.theme = previous; eprintln!("Theme save error: {error}"); return; }
                            let _ = mist.set_checked(false);
                            let _ = night.set_checked(false);
                            let _ = warm.set_checked(false);
                            let _ = custom.set_checked(true);
                            let _ = app.emit("background-image-changed", ());
                            let _ = app.emit("theme-changed", "custom");
                            if let Some(window) = app.get_webview_window("crop-background") { let _ = window.show(); let _ = window.set_focus(); }
                        });
                    }
                    "theme_crop_image" => {
                        if let Some(window) = app.get_webview_window("crop-background") { let _ = window.show(); let _ = window.set_focus(); }
                    }
                    "theme_custom" | "theme_mist" | "theme_night" | "theme_warm" => {
                        let selected = event.id().0.trim_start_matches("theme_");
                        let state = app.state::<NoteState>();
                        if selected == "custom" && !state.path.with_file_name("background-image.bin").exists() { return; }
                        let mut note = state.note.lock().unwrap();
                        note.theme = selected.to_string();
                        if let Err(error) = write_note(&state.path, &note) { eprintln!("Theme save error: {error}"); }
                        let _ = app.emit("theme-changed", selected);
                        let _ = mist.set_checked(selected == "mist");
                        let _ = night.set_checked(selected == "night");
                        let _ = warm.set_checked(selected == "warm");
                        let _ = custom.set_checked(selected == "custom");
                    }
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(false) = event {
                let app = window.app_handle();
                let layer = app.state::<NoteState>().note.lock().unwrap().window_layer.clone();
                if layer == "desktop" { let _ = apply_window_layer(app, "desktop"); }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("EasyNote failed to start");
}

#[cfg(test)]
mod tests {
    use super::{image_mime, next_due, write_note, Note, Todo, storage, export};

    #[test]
    fn background_image_accepts_supported_signatures() {
        assert_eq!(image_mime(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(image_mime(b"\xff\xd8\xffrest"), Some("image/jpeg"));
        assert_eq!(image_mime(b"RIFFxxxxWEBPrest"), Some("image/webp"));
        assert_eq!(image_mime(b"not an image"), None);
    }

    #[test]
    fn old_note_without_opacity_keeps_default() {
        let note: Note = serde_json::from_str(
            r#"{"text":"hello","theme":"mist","priority":"gray","edited_at":0,"position":null}"#,
        ).unwrap();
        assert_eq!(note.opacity, 72);
        assert_eq!(note.cleanup_days, 15);
        assert!(note.todos.is_empty());
        assert!(note.show_todos);
        assert!(note.show_note);
        assert_eq!(note.window_layer, "top");
        assert_eq!(note.window_width, None);
        assert_eq!(note.note_height, 251);
        assert_eq!(note.todo_height, None);
    }

    #[test]
    fn monthly_repeat_advances_calendar_month() {
        let jan = 1_769_644_800_000u64; // 2026-01-29 UTC
        let next = next_due(jan, "monthly").unwrap();
        assert!(next > jan);
    }

    #[test]
    fn note_file_can_be_replaced_on_windows() {
        let path = std::env::temp_dir().join(format!("easynote-test-{}.json", uuid::Uuid::new_v4()));
        let mut note = Note::default();
        write_note(&path, &note).unwrap();
        note.text = "updated".into();
        note.window_width = Some(365);
        write_note(&path, &note).unwrap();
        let saved: Note = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.text, "updated");
        assert_eq!(saved.window_width, Some(365));
        assert!(serde_json::to_value(Note::default()).unwrap().get("todos").is_some());
        assert!(serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap().get("todos").is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn json_omits_todos_and_sqlite_keeps_archived_history() {
        let folder = std::env::temp_dir().join(format!("easynote-db-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder).unwrap();
        let mut note = Note::default();
        let todo = Todo { id: "one".into(), title: "搬运物料".into(), memo: "主楼222".into(), due_at: Some(1_795_000_000_000), time_specified: true, repeat: "none".into(), repeat_until: None, remind_before: -1, remind_method: "app".into(), priority: "normal".into(), kind: "work".into(), completed_at: None, last_reminded_at: None, duration_minutes: 15, remind_offsets: vec![], reminded_offsets: vec![], remind_at: None, reminded_custom: false };
        note.todos.push(todo.clone());
        let path = folder.join("notes.json");
        write_note(&path, &note).unwrap();
        let saved: Note = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(saved.todos.is_empty());
        let conn = storage::open(&folder.join("todos.sqlite3")).unwrap();
        storage::upsert(&conn, &todo).unwrap();
        storage::archive(&conn, &todo.id).unwrap();
        assert!(storage::list(&conn, false).unwrap().is_empty());
        assert_eq!(storage::list(&conn, true).unwrap()[0].title, "搬运物料");
        assert_eq!(storage::purge_archived(&conn, 0).unwrap(), 0);
        conn.execute("UPDATE todos SET archived_at=1 WHERE id='one'", []).unwrap();
        assert_eq!(storage::purge_archived(&conn, 15).unwrap(), 1);
        assert!(storage::list(&conn, true).unwrap().is_empty());
        storage::upsert(&conn, &todo).unwrap();
        assert!(storage::delete(&conn, &todo.id).unwrap());
        assert!(storage::list(&conn, true).unwrap().is_empty());
        assert_eq!(storage::count(&conn).unwrap(), 0);
        drop(conn);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn all_export_formats_write_real_documents() {
        let folder = std::env::temp_dir().join(format!("easynote-export-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&folder).unwrap();
        let todo = Todo { id: "one".into(), title: "搬运物料".into(), memo: "主楼222".into(), due_at: Some(1_795_000_000_000), time_specified: true, repeat: "none".into(), repeat_until: None, remind_before: -1, remind_method: "app".into(), priority: "normal".into(), kind: "work".into(), completed_at: None, last_reminded_at: None, duration_minutes: 15, remind_offsets: vec![], reminded_offsets: vec![], remind_at: None, reminded_custom: false };
        for (format, signature) in [("xlsx", b"PK".as_slice()), ("docx", b"PK".as_slice()), ("pdf", b"%PDF".as_slice())] {
            let opts = export::ExportOptions { format: format.into(), days: 0, kinds: vec!["work".into()], extras: vec!["priority".into()] };
            let path = folder.join(format!("history.{format}"));
            export::write(&path, &[todo.clone()], &opts).unwrap();
            assert!(std::fs::read(&path).unwrap().starts_with(signature));
        }
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn recent_export_excludes_future_and_older_due_dates() {
        let now = super::now_ms();
        let base = Todo { id: "one".into(), title: "任务".into(), memo: String::new(), due_at: Some(now - 2 * 86_400_000), time_specified: true, repeat: "none".into(), repeat_until: None, remind_before: -1, remind_method: "app".into(), priority: "normal".into(), kind: "work".into(), completed_at: None, last_reminded_at: None, duration_minutes: 15, remind_offsets: vec![], reminded_offsets: vec![], remind_at: None, reminded_custom: false };
        let mut future = base.clone(); future.id = "future".into(); future.due_at = Some(now + 86_400_000);
        let mut old = base.clone(); old.id = "old".into(); old.due_at = Some(now - 7 * 86_400_000);
        let options = export::ExportOptions { format: "xlsx".into(), days: 5, kinds: vec!["work".into()], extras: vec![] };
        let selected = export::select(vec![future, old, base], &options);
        assert_eq!(selected.iter().map(|todo| todo.id.as_str()).collect::<Vec<_>>(), vec!["one"]);
    }
}
