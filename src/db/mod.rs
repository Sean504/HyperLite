/// SQLite database layer using rusqlite (bundled).

use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::session::message::{Message, Part, Role, Session};

pub type Db = Arc<Mutex<Connection>>;

/// Open (or create) the HyperLite database and run migrations.
pub fn open(db_path: &PathBuf) -> Result<Db> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(db_path)?;

    // WAL mode for concurrent access
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;

    // Run migrations
    run_migrations(&conn)?;

    Ok(Arc::new(Mutex::new(conn)))
}

fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(include_str!("../../migrations/001_initial.sql"))?;
    conn.execute_batch(include_str!("../../migrations/002_agents_drafts.sql"))?;
    Ok(())
}

// ── Session operations ────────────────────────────────────────────────────────

pub fn insert_session(db: &Db, session: &Session) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO sessions (id, title, model_id, provider_id, cwd, parent_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            session.id, session.title, session.model_id, session.provider_id,
            session.cwd, session.parent_id, session.created_at, session.updated_at
        ],
    )?;
    Ok(())
}

pub fn update_session_title(db: &Db, id: &str, title: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![title, now, id],
    )?;
    Ok(())
}

pub fn touch_session(db: &Db, id: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().unwrap();
    conn.execute("UPDATE sessions SET updated_at = ?1 WHERE id = ?2", params![now, id])?;
    Ok(())
}

pub fn delete_session(db: &Db, id: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_sessions(db: &Db) -> Result<Vec<Session>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, title, model_id, provider_id, cwd, parent_id, created_at, updated_at
         FROM sessions ORDER BY updated_at DESC LIMIT 200"
    )?;

    let sessions = stmt.query_map([], |row| {
        Ok(Session {
            id:          row.get(0)?,
            title:       row.get(1)?,
            model_id:    row.get(2)?,
            provider_id: row.get(3)?,
            cwd:         row.get(4)?,
            parent_id:   row.get(5)?,
            created_at:  row.get(6)?,
            updated_at:  row.get(7)?,
        })
    })?.filter_map(|r| r.ok()).collect();

    Ok(sessions)
}

pub fn load_session(db: &Db, id: &str) -> Result<Option<Session>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, title, model_id, provider_id, cwd, parent_id, created_at, updated_at
         FROM sessions WHERE id = ?1"
    )?;
    let mut rows = stmt.query(params![id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(Session {
            id:          row.get(0)?,
            title:       row.get(1)?,
            model_id:    row.get(2)?,
            provider_id: row.get(3)?,
            cwd:         row.get(4)?,
            parent_id:   row.get(5)?,
            created_at:  row.get(6)?,
            updated_at:  row.get(7)?,
        }))
    } else {
        Ok(None)
    }
}

// ── Message operations ────────────────────────────────────────────────────────

pub fn insert_message(db: &Db, msg: &Message) -> Result<()> {
    let parts_json = serde_json::to_string(&msg.parts)?;
    let role_str = match msg.role { Role::User => "user", Role::Assistant => "assistant" };
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO messages (id, session_id, role, parts_json, model, duration_ms, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            msg.id, msg.session_id, role_str, parts_json,
            msg.model, msg.duration_ms.map(|d| d as i64), msg.created_at
        ],
    )?;
    Ok(())
}

pub fn update_message_parts(db: &Db, msg: &Message) -> Result<()> {
    let parts_json = serde_json::to_string(&msg.parts)?;
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE messages SET parts_json = ?1, model = ?2, duration_ms = ?3 WHERE id = ?4",
        params![parts_json, msg.model, msg.duration_ms.map(|d| d as i64), msg.id],
    )?;
    Ok(())
}

pub fn load_messages(db: &Db, session_id: &str) -> Result<Vec<Message>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, parts_json, model, duration_ms, created_at
         FROM messages WHERE session_id = ?1 ORDER BY created_at ASC"
    )?;

    let messages = stmt.query_map(params![session_id], |row| {
        let role_str: String = row.get(2)?;
        let parts_json: String = row.get(3)?;
        let duration_ms: Option<i64> = row.get(5)?;
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?,
            role_str, parts_json, row.get::<_, Option<String>>(4)?,
            duration_ms, row.get::<_, i64>(6)?))
    })?
    .filter_map(|r| r.ok())
    .filter_map(|(id, session_id, role_str, parts_json, model, duration_ms, created_at)| {
        let role = match role_str.as_str() {
            "user"      => Role::User,
            "assistant" => Role::Assistant,
            _           => return None,
        };
        let parts: Vec<Part> = serde_json::from_str(&parts_json).unwrap_or_default();
        Some(Message {
            id, session_id, role, parts, model,
            duration_ms: duration_ms.map(|d| d as u64),
            created_at,
        })
    })
    .collect();

    Ok(messages)
}

pub fn delete_all_messages(db: &Db, session_id: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])?;
    Ok(())
}

pub fn delete_messages_from(db: &Db, session_id: &str, from_created_at: i64) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "DELETE FROM messages WHERE session_id = ?1 AND created_at >= ?2",
        params![session_id, from_created_at],
    )?;
    Ok(())
}

// ── KV store ──────────────────────────────────────────────────────────────────

// ── Agent operations ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AgentRow {
    pub id:           String,
    pub name:         String,
    pub description:  Option<String>,
    pub model:        Option<String>,
    pub provider:     Option<String>,
    pub system:       Option<String>,
    pub allowed_tools: Option<String>,
    pub created_at:   i64,
}

pub fn list_agents(db: &Db) -> Result<Vec<AgentRow>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, name, description, model, provider, system, allowed_tools, created_at
         FROM agents ORDER BY created_at ASC"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(AgentRow {
            id:            row.get(0)?,
            name:          row.get(1)?,
            description:   row.get(2)?,
            model:         row.get(3)?,
            provider:      row.get(4)?,
            system:        row.get(5)?,
            allowed_tools: row.get(6)?,
            created_at:    row.get(7)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    Ok(rows)
}

pub fn insert_agent(db: &Db, agent: &AgentRow) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO agents (id, name, description, model, provider, system, allowed_tools, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![agent.id, agent.name, agent.description, agent.model,
                agent.provider, agent.system, agent.allowed_tools, agent.created_at],
    )?;
    Ok(())
}

pub fn delete_agent(db: &Db, id: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM agents WHERE id = ?1", params![id])?;
    Ok(())
}

// ── Draft operations ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DraftRow {
    pub id:         String,
    pub label:      String,
    pub content:    String,
    pub created_at: i64,
}

pub fn list_drafts(db: &Db) -> Result<Vec<DraftRow>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, label, content, created_at FROM drafts ORDER BY created_at DESC"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(DraftRow {
            id:         row.get(0)?,
            label:      row.get(1)?,
            content:    row.get(2)?,
            created_at: row.get(3)?,
        })
    })?.filter_map(|r| r.ok()).collect();
    Ok(rows)
}

pub fn insert_draft(db: &Db, draft: &DraftRow) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO drafts (id, label, content, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![draft.id, draft.label, draft.content, draft.created_at],
    )?;
    Ok(())
}

pub fn delete_draft(db: &Db, id: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM drafts WHERE id = ?1", params![id])?;
    Ok(())
}

// ── KV store ──────────────────────────────────────────────────────────────────

pub fn kv_get(db: &Db, key: &str) -> Option<String> {
    let conn = db.lock().unwrap();
    conn.query_row("SELECT value FROM kv_store WHERE key = ?1", params![key], |r| r.get(0)).ok()
}

pub fn kv_set(db: &Db, key: &str, value: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO kv_store (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}
