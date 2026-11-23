use std::path::Path;
use rusqlite::{params, Connection};
use crate::{now_ms, Todo};

pub fn open(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS todos (
        id TEXT PRIMARY KEY, payload TEXT NOT NULL, due_at INTEGER, kind TEXT NOT NULL,
        archived_at INTEGER, updated_at INTEGER NOT NULL
    ); CREATE INDEX IF NOT EXISTS idx_todos_archived ON todos(archived_at);
    CREATE INDEX IF NOT EXISTS idx_todos_due ON todos(due_at);") .map_err(|e| e.to_string())?;
    Ok(conn)
}

pub fn upsert(conn: &Connection, todo: &Todo) -> Result<(), String> {
    conn.execute("INSERT INTO todos(id,payload,due_at,kind,archived_at,updated_at)
        VALUES (?1,?2,?3,?4,NULL,?5)
        ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,due_at=excluded.due_at,
        kind=excluded.kind,updated_at=excluded.updated_at",
        params![todo.id, serde_json::to_string(todo).map_err(|e| e.to_string())?, todo.due_at, todo.kind, now_ms()])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn archive(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute("UPDATE todos SET archived_at=?2, updated_at=?2 WHERE id=?1 AND archived_at IS NULL", params![id, now_ms()]).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool, String> {
    conn.execute("DELETE FROM todos WHERE id=?1", [id])
        .map(|affected| affected != 0)
        .map_err(|e| e.to_string())
}

pub fn list(conn: &Connection, include_archived: bool) -> Result<Vec<Todo>, String> {
    let sql = if include_archived { "SELECT payload FROM todos" } else { "SELECT payload FROM todos WHERE archived_at IS NULL" };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
    rows.map(|row| serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())).collect()
}

pub fn count(conn: &Connection) -> Result<u64, String> {
    conn.query_row("SELECT COUNT(*) FROM todos", [], |row| row.get(0)).map_err(|e| e.to_string())
}

pub fn purge_archived(conn: &Connection, days: u16) -> Result<usize, String> {
    if days == 0 { return Ok(0); }
    let cutoff = now_ms().saturating_sub(days as u64 * 86_400_000);
    conn.execute("DELETE FROM todos WHERE archived_at IS NOT NULL AND archived_at < ?1", [cutoff]).map_err(|e| e.to_string())
}
