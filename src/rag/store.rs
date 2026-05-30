/// SQLite-backed vector store for RAG chunks.
///
/// Embeddings are stored as raw f32 blobs.
/// Retrieval uses cosine similarity computed in Rust — no vector DB needed.

use anyhow::Result;
use rusqlite::Connection;

// ── Schema ────────────────────────────────────────────────────────────────────

pub fn init_rag_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS rag_indexes (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            root_path   TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            file_count  INTEGER NOT NULL DEFAULT 0,
            chunk_count INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS rag_chunks (
            id          TEXT PRIMARY KEY,
            index_id    TEXT NOT NULL REFERENCES rag_indexes(id) ON DELETE CASCADE,
            file_path   TEXT NOT NULL,
            chunk_text  TEXT NOT NULL,
            embedding   BLOB NOT NULL,
            start_byte  INTEGER NOT NULL DEFAULT 0,
            end_byte    INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_rag_chunks_index
            ON rag_chunks(index_id);
    ")?;
    Ok(())
}

// ── Index management ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RagIndex {
    pub id:          String,
    pub name:        String,
    pub root_path:   String,
    pub file_count:  i64,
    pub chunk_count: i64,
}

pub fn upsert_index(conn: &Connection, name: &str, root_path: &str) -> Result<String> {
    // Delete old index with same name if present
    conn.execute("DELETE FROM rag_indexes WHERE name = ?1", [name])?;

    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO rag_indexes (id, name, root_path, created_at) VALUES (?1,?2,?3,?4)",
        rusqlite::params![id, name, root_path, now],
    )?;
    Ok(id)
}

pub fn update_index_counts(conn: &Connection, index_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE rag_indexes SET
            chunk_count = (SELECT COUNT(*) FROM rag_chunks WHERE index_id = ?1),
            file_count  = (SELECT COUNT(DISTINCT file_path) FROM rag_chunks WHERE index_id = ?1)
         WHERE id = ?1",
        [index_id],
    )?;
    Ok(())
}

pub fn list_indexes(conn: &Connection) -> Result<Vec<RagIndex>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, root_path, file_count, chunk_count FROM rag_indexes ORDER BY rowid DESC"
    )?;
    let rows = stmt.query_map([], |row| Ok(RagIndex {
        id:          row.get(0)?,
        name:        row.get(1)?,
        root_path:   row.get(2)?,
        file_count:  row.get(3)?,
        chunk_count: row.get(4)?,
    }))?;
    Ok(rows.flatten().collect())
}

pub fn delete_index(conn: &Connection, name: &str) -> Result<bool> {
    let n = conn.execute("DELETE FROM rag_indexes WHERE name = ?1", [name])?;
    Ok(n > 0)
}

pub fn get_index_by_name(conn: &Connection, name: &str) -> Result<Option<RagIndex>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, root_path, file_count, chunk_count FROM rag_indexes WHERE name = ?1"
    )?;
    let mut rows = stmt.query_map([name], |row| Ok(RagIndex {
        id:          row.get(0)?,
        name:        row.get(1)?,
        root_path:   row.get(2)?,
        file_count:  row.get(3)?,
        chunk_count: row.get(4)?,
    }))?;
    Ok(rows.next().and_then(|r| r.ok()))
}

pub fn get_latest_index(conn: &Connection) -> Result<Option<RagIndex>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, root_path, file_count, chunk_count FROM rag_indexes ORDER BY rowid DESC LIMIT 1"
    )?;
    let mut rows = stmt.query_map([], |row| Ok(RagIndex {
        id:          row.get(0)?,
        name:        row.get(1)?,
        root_path:   row.get(2)?,
        file_count:  row.get(3)?,
        chunk_count: row.get(4)?,
    }))?;
    Ok(rows.next().and_then(|r| r.ok()))
}

// ── Chunk storage ─────────────────────────────────────────────────────────────

pub fn insert_chunk(
    conn:       &Connection,
    index_id:   &str,
    file_path:  &str,
    chunk_text: &str,
    embedding:  &[f32],
    start_byte: usize,
    end_byte:   usize,
) -> Result<()> {
    let id = ulid::Ulid::new().to_string();
    let blob = floats_to_bytes(embedding);
    conn.execute(
        "INSERT INTO rag_chunks (id, index_id, file_path, chunk_text, embedding, start_byte, end_byte)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![id, index_id, file_path, chunk_text, blob, start_byte as i64, end_byte as i64],
    )?;
    Ok(())
}

// ── Retrieval ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub file_path:  String,
    pub chunk_text: String,
    pub score:      f32,
}

/// Return top-k chunks from an index by cosine similarity to query embedding.
pub fn search(
    conn:      &Connection,
    index_id:  &str,
    query_emb: &[f32],
    top_k:     usize,
) -> Result<Vec<RetrievedChunk>> {
    let mut stmt = conn.prepare(
        "SELECT file_path, chunk_text, embedding FROM rag_chunks WHERE index_id = ?1"
    )?;

    let mut scored: Vec<(f32, String, String)> = stmt
        .query_map([index_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?
        .flatten()
        .map(|(path, text, blob)| {
            let emb = bytes_to_floats(&blob);
            let score = cosine_similarity(query_emb, &emb);
            (score, path, text)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    Ok(scored.into_iter().take(top_k).map(|(score, file_path, chunk_text)| {
        RetrievedChunk { file_path, chunk_text, score }
    }).collect())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 { return 0.0; }
    let dot: f32  = a[..len].iter().zip(b[..len].iter()).map(|(x, y)| x * y).sum();
    let na:  f32  = a[..len].iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb:  f32  = b[..len].iter().map(|x| x * x).sum::<f32>().sqrt();
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
