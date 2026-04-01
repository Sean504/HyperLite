use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level config structure mirroring ~/.config/hyperlite/settings.toml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub theme:             String,
    pub model:             String,
    pub provider:          String,
    pub agent:             String,
    pub animations:        Option<bool>,
    pub scroll_speed:      Option<u8>,
    pub sidebar:           SidebarMode,
    pub terminal_title:    Option<bool>,
    pub thinking:          Option<bool>,
    pub tool_details:      Option<bool>,

    #[serde(default)]
    pub keybinds:          HashMap<String, String>,

    #[serde(default)]
    pub providers:         HashMap<String, ProviderConfig>,

    #[serde(default)]
    pub permissions:       PermissionsConfig,

    #[serde(default)]
    pub agents:            HashMap<String, AgentConfig>,
}

impl Config {
    pub fn animations_enabled(&self) -> bool {
        self.animations.unwrap_or(true)
    }
    pub fn scroll_speed(&self) -> u8 {
        self.scroll_speed.unwrap_or(3)
    }
    pub fn terminal_title_enabled(&self) -> bool {
        self.terminal_title.unwrap_or(true)
    }
    pub fn show_thinking(&self) -> bool {
        self.thinking.unwrap_or(false)
    }
    pub fn show_tool_details(&self) -> bool {
        self.tool_details.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SidebarMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderConfig {
    pub api_key:  Option<String>,
    pub base_url: Option<String>,
    pub org_id:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PermissionsConfig {
    pub rules: Vec<PermissionRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tool:    String,
    pub pattern: String,
    pub action:  PermissionAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AgentConfig {
    pub name:          String,
    pub description:   Option<String>,
    pub model:         Option<String>,
    pub provider:      Option<String>,
    pub system:        Option<String>,
    /// If set, only these tools are available in this agent's system prompt.
    /// Plan mode uses ["read_file","list_dir","grep","glob","search"].
    pub allowed_tools: Option<Vec<String>>,
}
