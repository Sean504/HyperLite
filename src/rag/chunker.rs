/// File chunking for RAG indexing.
///
/// Strategy by file type:
///   Code files  → split at function/class boundaries, fallback to fixed-size
///   Text/Markdown → split at paragraph boundaries
///   Everything  → fixed-size with 20% overlap

const MAX_CHUNK_CHARS: usize = 1500;
const OVERLAP_CHARS:   usize = 150;
const MIN_CHUNK_CHARS: usize = 40;

/// Directories to skip during indexing
pub const SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "__pycache__", ".cache",
    "dist", "build", ".next", "vendor", ".venv", "venv",
];

/// File extensions to index
pub const INDEX_EXTENSIONS: &[&str] = &[
    // Code
    "rs", "py", "js", "ts", "tsx", "jsx", "go", "java", "cpp", "c", "h",
    "hpp", "cs", "rb", "php", "swift", "kt", "scala", "sh", "bash", "zsh",
    "lua", "r", "jl", "ex", "exs", "clj", "hs", "ml", "fs",
    // Config / data
    "toml", "yaml", "yml", "json", "xml", "html", "css", "scss",
    "env", "conf", "config", "ini",
    // Docs
    "md", "txt", "rst",
    // Build
    "dockerfile", "makefile",
];

#[derive(Debug, Clone)]
pub struct Chunk {
    pub text:       String,
    pub file_path:  String,
    pub start_byte: usize,
    pub end_byte:   usize,
}

pub fn chunk_file(path: &str, content: &str) -> Vec<Chunk> {
    if content.trim().is_empty() { return vec![]; }

    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();

    let raw_chunks = match ext.as_str() {
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "java" |
        "cpp" | "c" | "h" | "hpp" | "cs" | "rb" | "swift" | "kt" => {
            chunk_by_function(content)
        }
        "md" | "txt" | "rst" => chunk_by_paragraph(content),
        _ => chunk_fixed(content),
    };

    raw_chunks.into_iter()
        .filter(|(text, _, _)| text.trim().len() >= MIN_CHUNK_CHARS)
        .map(|(text, start, end)| Chunk {
            text,
            file_path: path.to_string(),
            start_byte: start,
            end_byte: end,
        })
        .collect()
}

/// Split at function/class/impl/def boundaries.
fn chunk_by_function(content: &str) -> Vec<(String, usize, usize)> {
    let boundary_patterns = [
        "\nfn ", "\npub fn ", "\nasync fn ", "\npub async fn ",
        "\ndef ", "\nclass ", "\nimpl ", "\nstruct ", "\nenum ",
        "\nfunction ", "\nconst ", "\ntype ", "\ninterface ",
        "\nfunc ", "\nmethod ", "\n# ", // markdown heading fallback
    ];

    let mut split_points: Vec<usize> = vec![0];
    for pat in &boundary_patterns {
        let mut pos = 0;
        while let Some(idx) = content[pos..].find(pat) {
            let abs = pos + idx + 1; // +1 to skip the leading \n
            split_points.push(abs);
            pos = abs + 1;
        }
    }
    split_points.sort_unstable();
    split_points.dedup();

    let bytes = content.as_bytes();
    let mut chunks = vec![];
    let mut i = 0;

    while i < split_points.len() {
        let start = split_points[i];
        let end   = split_points.get(i + 1).copied().unwrap_or(content.len());
        let text  = &content[start..end];

        if text.len() <= MAX_CHUNK_CHARS {
            chunks.push((text.to_string(), start, end));
            i += 1;
        } else {
            // Section too large — sub-chunk it
            for (t, s, e) in chunk_fixed_range(text, start) {
                chunks.push((t, s, e));
            }
            i += 1;
        }
    }

    // Merge tiny consecutive chunks
    merge_small(chunks, content.len())
}

/// Split at blank-line paragraph boundaries.
fn chunk_by_paragraph(content: &str) -> Vec<(String, usize, usize)> {
    let mut chunks = vec![];
    let mut buf    = String::new();
    let mut start  = 0usize;
    let mut pos    = 0usize;

    for line in content.lines() {
        let line_len = line.len() + 1; // +1 for \n

        if line.trim().is_empty() && !buf.trim().is_empty() {
            if buf.len() >= MIN_CHUNK_CHARS {
                chunks.push((buf.clone(), start, pos));
            }
            buf.clear();
            start = pos + line_len;
        } else {
            buf.push_str(line);
            buf.push('\n');
        }

        pos += line_len;

        if buf.len() >= MAX_CHUNK_CHARS {
            chunks.push((buf.clone(), start, pos));
            buf.clear();
            start = pos;
        }
    }
    if buf.trim().len() >= MIN_CHUNK_CHARS {
        chunks.push((buf, start, pos));
    }
    chunks
}

/// Fixed-size chunks with overlap for unknown file types.
fn chunk_fixed(content: &str) -> Vec<(String, usize, usize)> {
    chunk_fixed_range(content, 0)
}

fn chunk_fixed_range(content: &str, offset: usize) -> Vec<(String, usize, usize)> {
    let mut chunks = vec![];
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0usize;

    while i < chars.len() {
        let end = (i + MAX_CHUNK_CHARS).min(chars.len());
        let text: String = chars[i..end].iter().collect();
        let byte_start = offset + chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>();
        let byte_end   = offset + chars[..end].iter().map(|c| c.len_utf8()).sum::<usize>();
        chunks.push((text, byte_start, byte_end));
        if end == chars.len() { break; }
        i += MAX_CHUNK_CHARS.saturating_sub(OVERLAP_CHARS);
    }
    chunks
}

/// Merge consecutive tiny chunks until they reach MIN_CHUNK_CHARS.
fn merge_small(chunks: Vec<(String, usize, usize)>, _total: usize) -> Vec<(String, usize, usize)> {
    let mut out: Vec<(String, usize, usize)> = vec![];
    for (text, start, end) in chunks {
        if let Some(last) = out.last_mut() {
            if last.0.len() < MIN_CHUNK_CHARS * 2 {
                last.0.push_str(&text);
                last.2 = end;
                continue;
            }
        }
        out.push((text, start, end));
    }
    out
}
