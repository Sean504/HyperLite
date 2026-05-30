/// RAG (Retrieval-Augmented Generation) for HyperLite.
///
/// Indexes local files using fastembed (all-MiniLM-L6-v2, ~22 MB).
/// Embeddings stored in SQLite as f32 blobs.
/// Retrieval uses cosine similarity in pure Rust — no vector DB process.

pub mod chunker;
pub mod store;

use anyhow::Result;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::PathBuf;
use std::sync::OnceLock;

static EMBEDDER: OnceLock<TextEmbedding> = OnceLock::new();

/// Path where the embedding model is cached.
fn model_cache_dir() -> PathBuf {
    crate::startup::real_home_dir()
        .join(".hyperlite")
        .join("embeddings")
}

/// Initialise (or return cached) embedder. Downloads model on first call (~22 MB).
pub fn embedder() -> Result<&'static TextEmbedding> {
    if let Some(e) = EMBEDDER.get() { return Ok(e); }

    let cache = model_cache_dir();
    std::fs::create_dir_all(&cache)?;

    let model = TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::AllMiniLML6V2)
            .with_cache_dir(cache)
            .with_show_download_progress(false),
    )?;

    // OnceLock::set returns Err if already set (race) — just return whatever is there
    let _ = EMBEDDER.set(model);
    Ok(EMBEDDER.get().unwrap())
}

/// Embed a single string. Returns a normalised f32 vector.
pub fn embed_one(text: &str) -> Result<Vec<f32>> {
    let emb = embedder()?;
    let mut vecs = emb.embed(vec![text], None)?;
    Ok(vecs.remove(0))
}

/// Embed a batch of strings.
pub fn embed_batch(texts: &[&str]) -> Result<Vec<Vec<f32>>> {
    let emb = embedder()?;
    Ok(emb.embed(texts.to_vec(), None)?)
}
