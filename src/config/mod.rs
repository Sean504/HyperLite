pub mod schema;
pub use schema::*;

use anyhow::Result;
use std::path::PathBuf;

/// Returns the global config directory: ~/.config/hyperlite/
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hyperlite")
}

/// Returns the data directory for the DB: ~/.local/share/hyperlite/ on Linux,
/// ~/Library/Application Support/hyperlite/ on macOS, %APPDATA%/hyperlite/ on Windows
pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("hyperlite")
}

/// Load and merge config from all sources (lowest → highest priority):
///   1. Built-in defaults
///   2. Global: ~/.config/hyperlite/settings.toml
///   3. Project: ./.hyperlite/settings.toml
pub fn load(project_dir: Option<&str>) -> Result<Config> {
    let mut config = Config {
        theme:    "cyberpunk".into(),
        model:    String::new(),
        provider: String::new(),
        agent:    "default".into(),
        ..Default::default()
    };

    // Load global config
    let global_path = config_dir().join("settings.toml");
    if global_path.exists() {
        let raw = std::fs::read_to_string(&global_path)?;
        let global: Config = toml::from_str(&raw)?;
        merge_into(&mut config, global);
    }

    // Load project config
    let project_path = project_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join(".hyperlite")
        .join("settings.toml");

    if project_path.exists() {
        let raw = std::fs::read_to_string(&project_path)?;
        let project: Config = toml::from_str(&raw)?;
        merge_into(&mut config, project);
    }

    // Resolve env var substitutions in api keys
    resolve_env_vars(&mut config);

    Ok(config)
}

/// Persist the current config to the global settings file.
/// Only writes fields that differ from defaults (theme, model, provider, agent).
pub fn save(config: &Config) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("settings.toml");
    let raw = toml::to_string_pretty(config)?;
    std::fs::write(&path, raw)?;
    Ok(())
}

fn merge_into(base: &mut Config, overlay: Config) {
    if !overlay.theme.is_empty()    { base.theme    = overlay.theme; }
    if !overlay.model.is_empty()    { base.model    = overlay.model; }
    if !overlay.provider.is_empty() { base.provider = overlay.provider; }
    if !overlay.agent.is_empty()    { base.agent    = overlay.agent; }
    if overlay.animations.is_some()     { base.animations     = overlay.animations; }
    if overlay.scroll_speed.is_some()   { base.scroll_speed   = overlay.scroll_speed; }
    if overlay.terminal_title.is_some() { base.terminal_title = overlay.terminal_title; }
    if overlay.thinking.is_some()       { base.thinking       = overlay.thinking; }
    if overlay.tool_details.is_some()   { base.tool_details   = overlay.tool_details; }

    for (k, v) in overlay.keybinds   { base.keybinds.insert(k, v); }
    for (k, v) in overlay.providers  { base.providers.insert(k, v); }
    for (k, v) in overlay.agents     { base.agents.insert(k, v); }
    base.permissions.rules.extend(overlay.permissions.rules);
}

/// Replace ${ENV_VAR} in api_key / base_url fields
fn resolve_env_vars(config: &mut Config) {
    for prov in config.providers.values_mut() {
        if let Some(ref k) = prov.api_key.clone() {
            prov.api_key = Some(expand_env(k));
        }
        if let Some(ref u) = prov.base_url.clone() {
            prov.base_url = Some(expand_env(u));
        }
    }
}

fn expand_env(s: &str) -> String {
    // Replace ${VAR} with the environment variable value
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    re.replace_all(s, |caps: &regex::Captures| {
        std::env::var(&caps[1]).unwrap_or_default()
    })
    .into_owned()
}
