/// RAG tool implementations: index_dir, search_index, clear_index.

use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;
use walkdir::WalkDir;

use crate::rag::{chunker, embed_batch, embed_one, store};

// ── index_dir ─────────────────────────────────────────────────────────────────

pub fn index_dir(params: &Value, cwd: &PathBuf, db: &crate::db::Db) -> Result<String> {
    // Always index the current working directory — the model cannot specify an arbitrary path.
    let dir = cwd.clone();

    if !dir.exists() {
        anyhow::bail!("Working directory not found: {}", dir.display());
    }

    let name = dir.to_string_lossy().to_string(); // index keyed by full path

    let custom_exts: Vec<String> = params["extensions"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.trim_start_matches('.').to_lowercase())).collect())
        .unwrap_or_default();

    // Initialise embedder — downloads on first use, user was warned by IndexConfirm dialog.
    crate::rag::embedder()?;

    let conn = db.lock().unwrap();
    let index_id = store::upsert_index(&conn, &name, &dir.to_string_lossy())?;
    drop(conn);

    let mut files_indexed = 0usize;
    let mut chunks_total  = 0usize;
    let mut skipped       = 0usize;

    for entry in WalkDir::new(&dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            !chunker::SKIP_DIRS.contains(&name.as_str())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        // Check extension
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        let fname_lower = path.file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let is_special = matches!(fname_lower.as_str(), "dockerfile" | "makefile" | "rakefile");

        let allowed = if !custom_exts.is_empty() {
            custom_exts.contains(&ext)
        } else {
            is_special || chunker::INDEX_EXTENSIONS.contains(&ext.as_str())
        };

        if !allowed { continue; }

        // Skip large files (> 512 KB)
        if entry.metadata().map(|m| m.len()).unwrap_or(0) > 524_288 {
            skipped += 1;
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(s)  => s,
            Err(_) => { skipped += 1; continue; } // binary or unreadable
        };

        let rel_path = path.strip_prefix(&dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string());

        let chunks = chunker::chunk_file(&rel_path, &content);
        if chunks.is_empty() { continue; }

        // Embed in batches of 32
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
        for (batch_chunks, batch_texts) in chunks.chunks(32).zip(texts.chunks(32)) {
            let embeddings = match embed_batch(batch_texts) {
                Ok(e)  => e,
                Err(_) => continue,
            };
            let conn = db.lock().unwrap();
            for (chunk, emb) in batch_chunks.iter().zip(embeddings.iter()) {
                store::insert_chunk(
                    &conn, &index_id, &chunk.file_path,
                    &chunk.text, emb,
                    chunk.start_byte, chunk.end_byte,
                )?;
            }
        }

        files_indexed += 1;
        chunks_total  += chunks.len();
    }

    let conn = db.lock().unwrap();
    store::update_index_counts(&conn, &index_id)?;

    Ok(format!(
        "Index '{}' created.\n{} files indexed · {} chunks · {} files skipped\nPath: {}",
        name, files_indexed, chunks_total, skipped, dir.display()
    ))
}

// ── search_index ──────────────────────────────────────────────────────────────

pub fn search_index(params: &Value, db: &crate::db::Db) -> Result<String> {
    let query = params["query"].as_str()
        .ok_or_else(|| anyhow::anyhow!("query is required"))?;

    let top_k = params["top_k"].as_u64().unwrap_or(5) as usize;
    let top_k = top_k.min(20);

    let conn = db.lock().unwrap();

    let index = if let Some(name) = params["index_name"].as_str() {
        store::get_index_by_name(&conn, name)?
            .ok_or_else(|| anyhow::anyhow!("Index '{}' not found", name))?
    } else {
        store::get_latest_index(&conn)?
            .ok_or_else(|| anyhow::anyhow!("No indexes found. Run index_dir first."))?
    };

    let query_emb = embed_one(query)?;
    let results   = store::search(&conn, &index.id, &query_emb, top_k)?;

    if results.is_empty() {
        return Ok(format!("No results found in index '{}'.", index.name));
    }

    let mut out = format!("Results from index '{}' for: \"{}\"\n\n", index.name, query);
    for (i, chunk) in results.iter().enumerate() {
        out.push_str(&format!(
            "── [{}] {} (score: {:.3}) ──\n{}\n\n",
            i + 1, chunk.file_path, chunk.score, chunk.chunk_text.trim()
        ));
    }
    Ok(out)
}

// ── clear_index ───────────────────────────────────────────────────────────────

pub fn clear_index(params: &Value, db: &crate::db::Db) -> Result<String> {
    let name = params["name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("name is required"))?;

    let conn = db.lock().unwrap();
    if store::delete_index(&conn, name)? {
        Ok(format!("Index '{}' deleted.", name))
    } else {
        Ok(format!("Index '{}' not found.", name))
    }
}

// ── list_indexes ──────────────────────────────────────────────────────────────

pub fn list_indexes(db: &crate::db::Db) -> Result<String> {
    let conn = db.lock().unwrap();
    let indexes = store::list_indexes(&conn)?;
    if indexes.is_empty() {
        return Ok("No indexes found. Use index_dir to create one.".to_string());
    }
    let mut out = String::from("RAG indexes:\n");
    for idx in &indexes {
        out.push_str(&format!(
            "  {} — {} files · {} chunks · {}\n",
            idx.name, idx.file_count, idx.chunk_count, idx.root_path
        ));
    }
    Ok(out)
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn resolve_path(path: &str, cwd: &PathBuf) -> PathBuf {
    let p = if path.starts_with('~') {
        let rest = path.trim_start_matches("~/").trim_start_matches('~');
        crate::startup::real_home_dir().join(rest)
    } else {
        PathBuf::from(path)
    };
    if p.is_absolute() { p } else { cwd.join(p) }
}

/// Build a RAG context string scoped strictly to the given working directory's index.
/// Returns None if no index exists for this directory or nothing is relevant.
pub fn retrieve_context(query: &str, db: &crate::db::Db, working_dir: &str, top_k: usize) -> Option<String> {
    let conn = db.lock().ok()?;
    // Only use the index that was created for THIS working directory
    let index = store::get_index_for_dir(&conn, working_dir).ok()??;
    // Never trigger embedder init from auto-injection — only use if already warm
    let query_emb = crate::rag::embed_one_if_ready(query)?;
    let results = store::search(&conn, &index.id, &query_emb, top_k).ok()?;
    if results.is_empty() { return None; }

    let mut relevant = vec![];
    for chunk in &results {
        if chunk.score < 0.25 { continue; }
        relevant.push(chunk);
    }
    if relevant.is_empty() { return None; }

    let mut ctx = String::from("Relevant context from the current project:\n\n");
    for chunk in relevant {
        ctx.push_str(&format!("// {}\n{}\n\n", chunk.file_path, chunk.chunk_text.trim()));
    }
    Some(ctx)
}
