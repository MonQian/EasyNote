use std::{fs, path::Path};
use chrono::{DateTime, Local, Utc};
use serde::Deserialize;
use crate::{now_ms, Todo};

#[derive(Deserialize)]
pub struct ExportOptions {
    pub format: String,
    pub days: u16,
    pub kinds: Vec<String>,
    pub extras: Vec<String>,
}

const EXTRA_FIELDS: [&str; 5] = ["repeat", "repeat_until", "duration", "priority", "reminder"];

impl ExportOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !["xlsx", "docx", "pdf"].contains(&self.format.as_str()) ||
           ![0, 5, 15, 30, 120, 365].contains(&self.days) ||
           self.kinds.is_empty() || self.kinds.iter().any(|v| !["work", "life", "study", "other"].contains(&v.as_str())) ||
           self.extras.iter().any(|v| !EXTRA_FIELDS.contains(&v.as_str())) { return Err("导出选项无效".into()); }
        Ok(())
    }
}

fn date(ms: Option<u64>) -> String {
    ms.and_then(|v| DateTime::<Utc>::from_timestamp_millis(v as i64))
        .map(|v| v.with_timezone(&Local).format("%Y/%m/%d").to_string()).unwrap_or_default()
}
fn time(ms: Option<u64>, specified: bool) -> String {
    if !specified { return String::new(); }
    ms.and_then(|v| DateTime::<Utc>::from_timestamp_millis(v as i64))
        .map(|v| v.with_timezone(&Local).format("%H:%M").to_string()).unwrap_or_default()
}
fn kind(v: &str) -> &'static str { match v { "work" => "工作", "life" => "生活", "study" => "学习", _ => "其他" } }
fn priority(v: &str) -> &'static str { match v { "high" => "高", "low" => "低", _ => "普通" } }
fn repeat(v: &str) -> &'static str { match v { "daily" => "每天", "weekly" => "每周", "monthly" => "每月", _ => "不重复" } }
fn extra_label(v: &str) -> &'static str { match v { "repeat" => "重复", "repeat_until" => "结束重复", "duration" => "时长", "priority" => "优先级", _ => "提醒" } }
fn extra(todo: &Todo, v: &str) -> String { match v {
    "repeat" => repeat(&todo.repeat).into(),
    "repeat_until" => date(todo.repeat_until),
    "duration" => format!("{} 分钟", todo.duration_minutes),
    "priority" => priority(&todo.priority).into(),
    "reminder" => {
        let mut parts: Vec<String> = todo.remind_offsets.iter().map(|v| format!("{} 天前", v / 1440)).collect();
        if let Some(at) = todo.remind_at { parts.push(format!("{} {}", date(Some(at)), time(Some(at), true))); }
        if parts.is_empty() && todo.remind_before >= 0 { parts.push(format!("{} 分钟前", todo.remind_before)); }
        parts.join("、")
    }, _ => String::new()
} }
fn headers(opts: &ExportOptions) -> Vec<String> {
    let mut v = vec!["序号", "日期", "时间", "项目", "类型", "备注"].into_iter().map(str::to_string).collect::<Vec<_>>();
    v.extend(opts.extras.iter().map(|e| extra_label(e).to_string())); v
}
fn row(todo: &Todo, index: usize, opts: &ExportOptions) -> Vec<String> {
    let mut v = vec![index.to_string(), date(todo.due_at), time(todo.due_at, todo.time_specified), todo.title.clone(), kind(&todo.kind).into(), todo.memo.clone()];
    v.extend(opts.extras.iter().map(|e| extra(todo, e))); v
}

fn record_line(todo: &Todo, opts: &ExportOptions) -> String {
    let base = [date(todo.due_at), time(todo.due_at, todo.time_specified).replace(':', "："), todo.title.clone(), kind(&todo.kind).into(), todo.memo.clone()].join("-");
    let extras = opts.extras.iter().map(|field| extra(todo, field)).filter(|value| !value.is_empty()).collect::<Vec<_>>();
    if extras.is_empty() { base } else { format!("{base} （{}）", extras.join("-")) }
}

pub fn select(mut todos: Vec<Todo>, opts: &ExportOptions) -> Vec<Todo> {
    select_at(&mut todos, opts, now_ms());
    todos
}

fn select_at(todos: &mut Vec<Todo>, opts: &ExportOptions, now: u64) {
    let cutoff = now.saturating_sub(opts.days as u64 * 86_400_000);
    todos.retain(|todo| opts.kinds.contains(&todo.kind) && (opts.days == 0 || todo.due_at.or(todo.completed_at).is_some_and(|at| at >= cutoff && at <= now)));
    let order = |kind: &str| match kind { "work" => 0, "study" => 1, "life" => 2, _ => 3 };
    todos.sort_by_key(|todo| (order(&todo.kind), todo.due_at.unwrap_or(u64::MAX), todo.title.clone()));
}

pub fn write(path: &Path, todos: &[Todo], opts: &ExportOptions) -> Result<(), String> {
    match opts.format.as_str() {
        "xlsx" => xlsx(path, todos, opts),
        "docx" => docx(path, todos, opts),
        "pdf" => pdf(path, todos, opts),
        _ => Err("格式无效".into()),
    }
}

fn xlsx(path: &Path, todos: &[Todo], opts: &ExportOptions) -> Result<(), String> {
    use rust_xlsxwriter::{Workbook, Format};
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.set_name("历史待办").map_err(|e| e.to_string())?;
    let header = Format::new().set_bold();
    for (col, value) in headers(opts).iter().enumerate() { sheet.write_string_with_format(0, col as u16, value, &header).map_err(|e| e.to_string())?; }
    for (i, todo) in todos.iter().enumerate() {
        for (col, value) in row(todo, i + 1, opts).iter().enumerate() {
            sheet.write_string((i + 1) as u32, col as u16, value).map_err(|e| e.to_string())?;
        }
    }
    for col in 0..headers(opts).len() { sheet.set_column_width(col as u16, if col == 3 || col == 5 { 32 } else { 18 }).map_err(|e| e.to_string())?; }
    book.save(path).map_err(|e| e.to_string())
}

fn docx(path: &Path, todos: &[Todo], opts: &ExportOptions) -> Result<(), String> {
    use docx_rs::{Docx, Paragraph, Run};
    let mut doc = Docx::new().add_paragraph(Paragraph::new().add_run(Run::new().add_text("历史待办").bold()));
    for (i, todo) in todos.iter().enumerate() {
        doc = doc.add_paragraph(Paragraph::new().add_run(Run::new().add_text(format!("{}. {}", i + 1, record_line(todo, opts)))));
    }
    doc.build().pack(fs::File::create(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn pdf(path: &Path, todos: &[Todo], opts: &ExportOptions) -> Result<(), String> {
    use printpdf::{Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt, TextItem};
    let font_bytes = fs::read(r"C:\Windows\Fonts\simhei.ttf").map_err(|e| format!("无法读取中文字体：{e}"))?;
    let mut warnings = Vec::new();
    let font = ParsedFont::from_bytes(&font_bytes, 0, &mut warnings).ok_or("无法加载中文字体")?;
    let mut doc = PdfDocument::new("历史待办");
    let font_id = doc.add_font(&font);
    let mut pages = Vec::new();
    let mut ops = Vec::new();
    let mut y = 278.0f32;
    let mut lines = vec!["历史待办".to_string()];
    for (i, todo) in todos.iter().enumerate() {
        lines.push(format!("{}. {}", i + 1, record_line(todo, opts)));
    }
    for line in lines {
        for chunk in line.chars().collect::<Vec<_>>().chunks(55) {
            if y < 20.0 {
                pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
                ops = Vec::new();
                y = 278.0;
            }
            ops.push(Op::StartTextSection);
            ops.push(Op::SetTextCursor { pos: Point::new(Mm(15.0), Mm(y)) });
            ops.push(Op::SetFont { font: PdfFontHandle::External(font_id.clone()), size: Pt(11.0) });
            ops.push(Op::ShowText { items: vec![TextItem::Text(chunk.iter().collect())] });
            ops.push(Op::EndTextSection);
            y -= 7.0;
        }
    }
    pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
    let mut save_options = PdfSaveOptions::default();
    save_options.subset_fonts = false;
    save_options.optimize = false;
    let bytes = doc.with_pages(pages).save(&save_options, &mut Vec::new());
    fs::write(path, bytes).map_err(|e| e.to_string())
}
