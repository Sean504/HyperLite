/// Persistent cross-session memory.
///
/// Stores facts the model should carry across every session.
/// Examples: user preferences, project details, recurring patterns.
///
/// Storage: SQLite table `memory` — same db, no new dependencies.
/// Retrieval: fastembed similarity (reuses RAG embedder) — top-N most
///            relevant memories are prepended to the system prompt.

use anyhow::Result;
use rusqlite::Connection;

// ── Schema ────────────────────────────────────────────────────────────────────

pub fn init_memory_table(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS memory (
            id             TEXT PRIMARY KEY,
            content        TEXT NOT NULL,
            category       TEXT NOT NULL DEFAULT 'general',
            source_session TEXT,
            created_at     TEXT NOT NULL,
            last_used_at   TEXT,
            use_count      INTEGER NOT NULL DEFAULT 0,
            embedding      BLOB
        );
    ")?;
    Ok(())
}

// ── Memory entry ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Memory {
    pub id:       String,
    pub content:  String,
    pub category: String,
    pub created_at: String,
    pub use_count:  i64,
}

// ── Write ─────────────────────────────────────────────────────────────────────

pub fn save(conn: &Connection, content: &str, category: &str, session_id: Option<&str>) -> Result<String> {
    let id  = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Only embed if the model is already loaded — never trigger a download from save
    let embedding = crate::rag::embed_one_if_ready(content)
        .map(|v| floats_to_bytes(&v));

    conn.execute(
        "INSERT INTO memory (id, content, category, source_session, created_at, embedding)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, content, category, session_id, now, embedding],
    )?;
    Ok(id)
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool> {
    let n = conn.execute("DELETE FROM memory WHERE id = ?1", [id])?;
    Ok(n > 0)
}

pub fn clear_all(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM memory", [])?)
}

// ── Read ──────────────────────────────────────────────────────────────────────

pub fn list_all(conn: &Connection) -> Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT id, content, category, created_at, use_count
         FROM memory ORDER BY created_at DESC"
    )?;
    let rows = stmt.query_map([], |row| Ok(Memory {
        id:         row.get(0)?,
        content:    row.get(1)?,
        category:   row.get(2)?,
        created_at: row.get(3)?,
        use_count:  row.get(4)?,
    }))?;
    Ok(rows.flatten().collect())
}

pub fn count(conn: &Connection) -> usize {
    conn.query_row("SELECT COUNT(*) FROM memory", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as usize
}

/// Retrieve top-k most relevant memories for a given query using cosine similarity.
/// Falls back to most-recent if embeddings aren't available.
pub fn retrieve_relevant(conn: &Connection, query: &str, top_k: usize) -> Vec<Memory> {
    let all = match list_all(conn) {
        Ok(m) => m,
        Err(_) => return vec![],
    };
    if all.is_empty() { return vec![]; }

    // Use semantic retrieval only if embedder is already warm — never block on download
    if let Some(query_emb) = crate::rag::embed_one_if_ready(query) {
        let mut scored: Vec<(f32, Memory)> = all.into_iter().filter_map(|m| {
            // Load embedding from db for this memory
            let emb_bytes: Option<Vec<u8>> = conn.query_row(
                "SELECT embedding FROM memory WHERE id = ?1",
                [&m.id],
                |r| r.get(0),
            ).ok().flatten();

            if let Some(bytes) = emb_bytes {
                let emb = bytes_to_floats(&bytes);
                let score = cosine_similarity(&query_emb, &emb);
                Some((score, m))
            } else {
                Some((0.0, m))
            }
        }).collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Update use_count for retrieved memories
        let results: Vec<Memory> = scored.into_iter()
            .filter(|(score, _)| *score > 0.15)
            .take(top_k)
            .map(|(_, m)| {
                let _ = conn.execute(
                    "UPDATE memory SET use_count = use_count + 1, last_used_at = ?1 WHERE id = ?2",
                    rusqlite::params![chrono::Utc::now().to_rfc3339(), m.id],
                );
                m
            })
            .collect();

        if !results.is_empty() { return results; }
    }

    // Fallback: most recent (re-fetch since all was consumed)
    list_all(conn).unwrap_or_default().into_iter().take(top_k).collect()
}

/// Build the memory context string to prepend to the system prompt.
pub fn build_context(db: &crate::db::Db, query: &str) -> Option<String> {
    let conn = db.lock().ok()?;
    let memories = retrieve_relevant(&conn, query, 10);
    if memories.is_empty() { return None; }

    let mut ctx = String::from("Things I know about you and your preferences:\n");
    for m in &memories {
        ctx.push_str(&format!("- {}\n", m.content));
    }
    Some(ctx)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 { return 0.0; }
    let dot: f32 = a[..len].iter().zip(b[..len].iter()).map(|(x, y)| x * y).sum();
    let na:  f32 = a[..len].iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb:  f32 = b[..len].iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}

fn floats_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v { out.extend_from_slice(&f.to_le_bytes()); }
    out
}

fn bytes_to_floats(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
