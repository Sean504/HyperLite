/// Project Context Scanner — Repo Codex
///
/// When HyperLite is opened inside a directory, this module:
///   1. Detects if it's a git repo
///   2. Identifies the tech stack (language, frameworks, dependencies)
///   3. Reads key metadata (README excerpt, recent commits, file counts)
///   4. Builds a compact system-prompt prefix (≤ 800 tokens)
///   5. Caches the context keyed on git HEAD hash
///
/// The injected context tells the model exactly what project it's working in
/// without the user needing to explain it each time.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

// ── Project Info ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectContext {
    pub root:             PathBuf,
    pub is_git:           bool,
    pub git_branch:       Option<String>,
    pub git_remote:       Option<String>,
    pub recent_commits:   Vec<GitCommit>,
    pub tech_stack:       TechStack,
    pub file_stats:       FileStats,
    pub readme_excerpt:   Option<String>,
    pub key_files:        Vec<String>,
    /// Git HEAD hash — used to invalidate cache
    pub head_hash:        Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitCommit {
    pub hash:    String,
    pub author:  String,
    pub date:    String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TechStack {
    /// Primary language (most .ext files)
    pub primary_language:   Option<String>,
    /// All detected languages with line/file counts
    pub languages:          Vec<LanguageStat>,
    /// Detected framework / build system
    pub frameworks:         Vec<String>,
    /// Key dependency files found
    pub manifest_files:     Vec<String>,
    /// Detected project type
    pub project_type:       ProjectType,
    /// Key dependencies (from manifests, up to 20)
    pub key_dependencies:   Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguageStat {
    pub language:    String,
    pub file_count:  usize,
    pub line_count:  usize,
    pub percentage:  f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum ProjectType {
    #[default]
    Unknown,
    RustCrate,
    RustWorkspace,
    NodeJs,
    Python,
    Go,
    Java,
    Kotlin,
    CSharp,
    Cpp,
    Ruby,
    Php,
    Swift,
    Flutter,
    Monorepo,
    DataScience,
    WebFrontend,
}

impl ProjectType {
    pub fn label(&self) -> &'static str {
        match self {
            ProjectType::Unknown        => "Unknown",
            ProjectType::RustCrate      => "Rust Crate",
            ProjectType::RustWorkspace  => "Rust Workspace",
            ProjectType::NodeJs         => "Node.js",
            ProjectType::Python         => "Python",
            ProjectType::Go             => "Go",
            ProjectType::Java           => "Java",
            ProjectType::Kotlin         => "Kotlin",
            ProjectType::CSharp         => "C#",
            ProjectType::Cpp            => "C/C++",
            ProjectType::Ruby           => "Ruby",
            ProjectType::Php            => "PHP",
            ProjectType::Swift          => "Swift",
            ProjectType::Flutter        => "Flutter/Dart",
            ProjectType::Monorepo       => "Monorepo",
            ProjectType::DataScience    => "Data Science",
            ProjectType::WebFrontend    => "Web Frontend",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileStats {
    pub total_files:    usize,
    pub total_lines:    usize,
    pub source_files:   usize,
    pub largest_files:  Vec<(String, usize)>,  // (path, lines)
}

// ── Scanner ───────────────────────────────────────────────────────────────────

/// Scan the given directory and build project context.
/// This is I/O heavy — call from a background task.
pub fn scan(dir: &Path) -> ProjectContext {
    let mut ctx = ProjectContext::default();
    ctx.root = dir.to_path_buf();

    // Git info
    ctx.is_git      = is_git_repo(dir);
    ctx.head_hash   = git_head_hash(dir);
    ctx.git_branch  = git_branch(dir);
    ctx.git_remote  = git_remote(dir);
    ctx.recent_commits = git_recent_commits(dir, 8);

    // File scan
    let (lang_map, file_stats) = scan_files(dir);
    ctx.file_stats = file_stats;

    // Tech stack detection
    ctx.tech_stack = detect_stack(dir, &lang_map);

    // README
    ctx.readme_excerpt = read_readme(dir);

    // Key files (config, entry points, etc.)
    ctx.key_files = find_key_files(dir);

    ctx
}

/// Build the system prompt prefix to inject into every conversation.
/// Kept compact — target ≤ 600 tokens.
pub fn build_system_prefix(ctx: &ProjectContext) -> String {
    if !ctx.is_git && ctx.tech_stack.project_type == ProjectType::Unknown {
        return String::new();
    }

    let mut lines = vec![];
    lines.push("## Project Context".to_string());

    // Project type + languages
    if ctx.tech_stack.project_type != ProjectType::Unknown {
        lines.push(format!("**Type:** {}", ctx.tech_stack.project_type.label()));
    }

    // Languages
    if !ctx.tech_stack.languages.is_empty() {
        let lang_str: Vec<String> = ctx.tech_stack.languages.iter()
            .take(5)
            .map(|l| {
                if l.percentage > 0.0 {
                    format!("{} ({:.0}%)", l.language, l.percentage)
                } else {
                    l.language.clone()
                }
            })
            .collect();
        lines.push(format!("**Languages:** {}", lang_str.join(", ")));
    }

    // Frameworks
    if !ctx.tech_stack.frameworks.is_empty() {
        lines.push(format!("**Frameworks:** {}", ctx.tech_stack.frameworks.join(", ")));
    }

    // Key dependencies
    if !ctx.tech_stack.key_dependencies.is_empty() {
        let deps: Vec<&str> = ctx.tech_stack.key_dependencies.iter()
            .take(12)
            .map(|s| s.as_str())
            .collect();
        lines.push(format!("**Key deps:** {}", deps.join(", ")));
    }

    // File stats
    lines.push(format!(
        "**Files:** {} source files, ~{} lines",
        ctx.file_stats.source_files,
        ctx.file_stats.total_lines
    ));

    // Git info
    if ctx.is_git {
        if let Some(ref branch) = ctx.git_branch {
            lines.push(format!("**Branch:** {}", branch));
        }
        if let Some(ref remote) = ctx.git_remote {
            // Truncate long remote URLs
            let display = if remote.len() > 60 { &remote[..60] } else { remote.as_str() };
            lines.push(format!("**Remote:** {}", display));
        }
    }

    // README excerpt
    if let Some(ref readme) = ctx.readme_excerpt {
        let excerpt: String = readme.lines().take(6).collect::<Vec<_>>().join("\n");
        if !excerpt.trim().is_empty() {
            lines.push(String::new());
            lines.push("**README (excerpt):**".to_string());
            lines.push(excerpt);
        }
    }

    // Recent commits
    if !ctx.recent_commits.is_empty() {
        lines.push(String::new());
        lines.push("**Recent commits:**".to_string());
        for c in ctx.recent_commits.iter().take(5) {
            // Truncate long messages
            let msg = if c.message.len() > 72 {
                format!("{}…", &c.message[..70])
            } else {
                c.message.clone()
            };
            lines.push(format!("  {} {} — {}", &c.hash[..7], c.date, msg));
        }
    }

    // Key files
    if !ctx.key_files.is_empty() {
        let files: Vec<&str> = ctx.key_files.iter().take(8).map(|s| s.as_str()).collect();
        lines.push(format!("**Key files:** {}", files.join(", ")));
    }

    lines.push(String::new());
    lines.push("You are a helpful AI assistant working inside this project. Use the project context above to give accurate, relevant answers.".to_string());

    lines.join("\n")
}

// ── Git helpers ───────────────────────────────────────────────────────────────

fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
        || Command::new("git")
            .args(["rev-parse", "--git-dir"])
            .current_dir(dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn git_head_hash(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else { None }
}

fn git_branch(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir)
        .output().ok()?;
    if out.status.success() {
        let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if branch != "HEAD" { Some(branch) } else { None }
    } else { None }
}

fn git_remote(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(dir)
        .output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else { None }
}

fn git_recent_commits(dir: &Path, n: usize) -> Vec<GitCommit> {
    let format = "%H|%an|%ar|%s";
    let out = Command::new("git")
        .args(["log", &format!("-{}", n), &format!("--pretty=format:{}", format)])
        .current_dir(dir)
        .output();

    let out = match out {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() < 4 { return None; }
            Some(GitCommit {
                hash:    parts[0].to_string(),
                author:  parts[1].to_string(),
                date:    parts[2].to_string(),
                message: parts[3].to_string(),
            })
        })
        .collect()
}

// ── File scanner ──────────────────────────────────────────────────────────────

/// Language extension → language name mapping
fn ext_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "rs"                    => Some("Rust"),
        "py" | "pyi" | "pyw"   => Some("Python"),
        "js" | "mjs" | "cjs"   => Some("JavaScript"),
        "ts" | "mts" | "cts"   => Some("TypeScript"),
        "jsx"                   => Some("JSX"),
        "tsx"                   => Some("TSX"),
        "go"                    => Some("Go"),
        "java"                  => Some("Java"),
        "kt" | "kts"            => Some("Kotlin"),
        "cs"                    => Some("C#"),
        "c" | "h"               => Some("C"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("C++"),
        "rb"                    => Some("Ruby"),
        "php"                   => Some("PHP"),
        "swift"                 => Some("Swift"),
        "dart"                  => Some("Dart"),
        "ex" | "exs"            => Some("Elixir"),
        "hs"                    => Some("Haskell"),
        "ml" | "mli"            => Some("OCaml"),
        "scala"                 => Some("Scala"),
        "clj" | "cljs"          => Some("Clojure"),
        "lua"                   => Some("Lua"),
        "r"                     => Some("R"),
        "jl"                    => Some("Julia"),
        "sql"                   => Some("SQL"),
        "sh" | "bash" | "zsh"  => Some("Shell"),
        "ps1"                   => Some("PowerShell"),
        "html" | "htm"          => Some("HTML"),
        "css" | "scss" | "sass" => Some("CSS"),
        "vue"                   => Some("Vue"),
        "svelte"                => Some("Svelte"),
        "toml" | "yaml" | "yml" | "json" => None, // config, not language
        _                       => None,
    }
}

fn count_lines(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn scan_files(dir: &Path) -> (HashMap<String, (usize, usize)>, FileStats) {
    let mut lang_map: HashMap<String, (usize, usize)> = HashMap::new(); // lang -> (files, lines)
    let mut total_files = 0usize;
    let mut total_lines = 0usize;
    let mut source_files = 0usize;
    let mut file_lines: Vec<(String, usize)> = vec![];

    // Skip these directories
    let skip_dirs = ["node_modules", ".git", "target", "dist", "build", ".cache",
                     "__pycache__", ".tox", "venv", ".venv", "vendor", "third_party"];

    for entry in WalkDir::new(dir)
        .follow_links(false)
        .max_depth(8)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !skip_dirs.iter().any(|s| *s == name.as_ref())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        total_files += 1;
        let path = entry.path();
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if let Some(lang) = ext_to_language(&ext) {
            let lines = count_lines(path);
            let entry = lang_map.entry(lang.to_string()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += lines;
            source_files += 1;
            total_lines += lines;

            let rel = path.strip_prefix(dir)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            file_lines.push((rel, lines));
        }
    }

    // Sort largest files
    file_lines.sort_by(|a, b| b.1.cmp(&a.1));
    file_lines.truncate(5);

    let stats = FileStats {
        total_files,
        total_lines,
        source_files,
        largest_files: file_lines,
    };

    (lang_map, stats)
}

// ── Stack detection ───────────────────────────────────────────────────────────

fn detect_stack(
    dir:      &Path,
    lang_map: &HashMap<String, (usize, usize)>,
) -> TechStack {
    let mut frameworks    = vec![];
    let mut manifests     = vec![];
    let mut project_type  = ProjectType::Unknown;
    let mut key_deps      = vec![];

    // ── Rust ────────────────────────────────────────────────────────────────
    let cargo_toml = dir.join("Cargo.toml");
    if cargo_toml.exists() {
        manifests.push("Cargo.toml".to_string());
        let content = std::fs::read_to_string(&cargo_toml).unwrap_or_default();
        project_type = if content.contains("[workspace]") {
            ProjectType::RustWorkspace
        } else {
            ProjectType::RustCrate
        };
        key_deps.extend(extract_rust_deps(&content));
        // Detect frameworks from deps
        if content.contains("ratatui")  { frameworks.push("ratatui".into()); }
        if content.contains("actix")    { frameworks.push("actix-web".into()); }
        if content.contains("axum")     { frameworks.push("axum".into()); }
        if content.contains("tokio")    { frameworks.push("tokio".into()); }
        if content.contains("bevy")     { frameworks.push("bevy".into()); }
        if content.contains("diesel")   { frameworks.push("diesel".into()); }
        if content.contains("sqlx")     { frameworks.push("sqlx".into()); }
        if content.contains("serde")    { frameworks.push("serde".into()); }
        if content.contains("tauri")    { frameworks.push("tauri".into()); }
    }

    // ── Node / JS / TS ───────────────────────────────────────────────────────
    let pkg_json = dir.join("package.json");
    if pkg_json.exists() {
        manifests.push("package.json".to_string());
        if project_type == ProjectType::Unknown { project_type = ProjectType::NodeJs; }
        let content = std::fs::read_to_string(&pkg_json).unwrap_or_default();
        key_deps.extend(extract_node_deps(&content));
        if content.contains("\"react\"")     { frameworks.push("React".into()); }
        if content.contains("\"next\"")      { frameworks.push("Next.js".into()); }
        if content.contains("\"vue\"")       { frameworks.push("Vue".into()); }
        if content.contains("\"nuxt\"")      { frameworks.push("Nuxt.js".into()); }
        if content.contains("\"svelte\"")    { frameworks.push("Svelte".into()); }
        if content.contains("\"solid-js\"")  { frameworks.push("SolidJS".into()); }
        if content.contains("\"express\"")   { frameworks.push("Express".into()); }
        if content.contains("\"fastify\"")   { frameworks.push("Fastify".into()); }
        if content.contains("\"electron\"")  { frameworks.push("Electron".into()); }
        if content.contains("\"tauri\"")     { frameworks.push("Tauri".into()); }
        if content.contains("\"prisma\"")    { frameworks.push("Prisma".into()); }
        if content.contains("\"drizzle\"")   { frameworks.push("Drizzle".into()); }
        if dir.join("turbo.json").exists()   { frameworks.push("Turborepo".into()); project_type = ProjectType::Monorepo; }
    }

    // ── Python ───────────────────────────────────────────────────────────────
    for manifest in &["requirements.txt", "pyproject.toml", "setup.py", "Pipfile"] {
        if dir.join(manifest).exists() {
            manifests.push(manifest.to_string());
            if project_type == ProjectType::Unknown { project_type = ProjectType::Python; }
            let content = std::fs::read_to_string(dir.join(manifest)).unwrap_or_default();
            if content.contains("django")     { frameworks.push("Django".into()); }
            if content.contains("flask")      { frameworks.push("Flask".into()); }
            if content.contains("fastapi")    { frameworks.push("FastAPI".into()); }
            if content.contains("torch") || content.contains("pytorch") { frameworks.push("PyTorch".into()); project_type = ProjectType::DataScience; }
            if content.contains("tensorflow") { frameworks.push("TensorFlow".into()); project_type = ProjectType::DataScience; }
            if content.contains("pandas")     { frameworks.push("Pandas".into()); }
            if content.contains("numpy")      { frameworks.push("NumPy".into()); }
            if content.contains("jupyter")    { frameworks.push("Jupyter".into()); }
            break;
        }
    }

    // ── Go ───────────────────────────────────────────────────────────────────
    if dir.join("go.mod").exists() {
        manifests.push("go.mod".to_string());
        if project_type == ProjectType::Unknown { project_type = ProjectType::Go; }
        let content = std::fs::read_to_string(dir.join("go.mod")).unwrap_or_default();
        if content.contains("gin-gonic")    { frameworks.push("Gin".into()); }
        if content.contains("echo")         { frameworks.push("Echo".into()); }
        if content.contains("fiber")        { frameworks.push("Fiber".into()); }
        if content.contains("gorm")         { frameworks.push("GORM".into()); }
    }

    // ── Java / Kotlin ────────────────────────────────────────────────────────
    if dir.join("pom.xml").exists() {
        manifests.push("pom.xml".to_string());
        if project_type == ProjectType::Unknown { project_type = ProjectType::Java; }
        let content = std::fs::read_to_string(dir.join("pom.xml")).unwrap_or_default();
        if content.contains("spring")  { frameworks.push("Spring".into()); }
        if content.contains("quarkus") { frameworks.push("Quarkus".into()); }
    }
    if dir.join("build.gradle").exists() || dir.join("build.gradle.kts").exists() {
        manifests.push("build.gradle".to_string());
        if project_type == ProjectType::Unknown { project_type = ProjectType::Java; }
        let content = std::fs::read_to_string(dir.join("build.gradle")).unwrap_or_default();
        if content.contains("kotlin")  { project_type = ProjectType::Kotlin; }
        if content.contains("android") { frameworks.push("Android".into()); }
        if content.contains("spring")  { frameworks.push("Spring".into()); }
    }

    // Build language stats
    let total_files: usize = lang_map.values().map(|(f, _)| f).sum();
    let mut languages: Vec<LanguageStat> = lang_map.iter()
        .map(|(lang, (files, lines))| LanguageStat {
            language:   lang.clone(),
            file_count: *files,
            line_count: *lines,
            percentage: if total_files > 0 { (*files as f32 / total_files as f32) * 100.0 } else { 0.0 },
        })
        .collect();
    languages.sort_by(|a, b| b.file_count.cmp(&a.file_count));

    // Primary language
    let primary_language = languages.first().map(|l| l.language.clone());

    frameworks.dedup();
    key_deps.dedup();
    key_deps.truncate(20);

    TechStack {
        primary_language,
        languages,
        frameworks,
        manifest_files: manifests,
        project_type,
        key_dependencies: key_deps,
    }
}

fn extract_rust_deps(cargo_toml: &str) -> Vec<String> {
    let mut deps = vec![];
    let mut in_deps = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[dependencies]") || trimmed.starts_with("[dev-dependencies]") {
            in_deps = true; continue;
        }
        if trimmed.starts_with('[') { in_deps = false; continue; }
        if in_deps {
            if let Some(name) = trimmed.split('=').next() {
                let name = name.trim().trim_matches('"');
                if !name.is_empty() && !name.starts_with('#') {
                    deps.push(name.to_string());
                }
            }
        }
    }
    deps
}

fn extract_node_deps(package_json: &str) -> Vec<String> {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(package_json) {
        let mut deps = vec![];
        for key in &["dependencies", "devDependencies"] {
            if let Some(obj) = val.get(key).and_then(|v| v.as_object()) {
                deps.extend(obj.keys().cloned());
            }
        }
        return deps;
    }
    vec![]
}

fn read_readme(dir: &Path) -> Option<String> {
    for name in &["README.md", "readme.md", "README.txt", "README"] {
        let path = dir.join(name);
        if path.exists() {
            let content = std::fs::read_to_string(&path).ok()?;
            // Take first 500 chars to keep context compact
            let excerpt: String = content.chars().take(500).collect();
            return Some(excerpt);
        }
    }
    None
}

fn find_key_files(dir: &Path) -> Vec<String> {
    const KEY_NAMES: &[&str] = &[
        "main.rs", "lib.rs", "main.py", "app.py", "index.js", "index.ts",
        "main.go", "main.java", "App.tsx", "App.vue", "app.config.ts",
        "vite.config.ts", "webpack.config.js", "tailwind.config.js",
        "docker-compose.yml", "Dockerfile", ".env.example",
        "schema.prisma", "schema.sql", "migrations",
    ];

    KEY_NAMES.iter()
        .filter(|name| dir.join(name).exists())
        .map(|s| s.to_string())
        .collect()
}

// ── Cache ─────────────────────────────────────────────────────────────────────

/// Simple in-memory cache for project context.
/// Invalidated when git HEAD changes or directory changes.
pub struct ContextCache {
    cached_dir:  Option<PathBuf>,
    cached_hash: Option<String>,
    pub context: Option<ProjectContext>,
}

impl ContextCache {
    pub fn new() -> Self {
        Self { cached_dir: None, cached_hash: None, context: None }
    }

    /// Get cached context if still valid, otherwise scan and cache.
    pub fn get_or_scan(&mut self, dir: &Path) -> &ProjectContext {
        let current_hash = git_head_hash(dir);
        let needs_refresh = self.cached_dir.as_deref() != Some(dir)
            || self.cached_hash != current_hash
            || self.context.is_none();

        if needs_refresh {
            let ctx = scan(dir);
            self.cached_dir  = Some(dir.to_path_buf());
            self.cached_hash = current_hash;
            self.context     = Some(ctx);
        }

        self.context.as_ref().unwrap()
    }

    pub fn invalidate(&mut self) {
        self.context = None;
    }
}

impl Default for ContextCache {
    fn default() -> Self { Self::new() }
}
