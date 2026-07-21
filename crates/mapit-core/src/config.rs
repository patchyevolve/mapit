//! Config file reading/writing — matches docs/05-backend-schema.md §1–4 exactly.
//! Full implementation in Phase 4 when the CLI commands are wired up.
//! This module defines the structs and stubs the I/O operations.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Global config (~/.config/mapit/global_config.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub schema_version: u32,
    pub default_provider: String,
    pub default_model: String,
    pub ollama_base_url: String,
    pub ui_preferences: UiPreferences,
    pub default_ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiPreferences {
    pub preferred_port: u16,
    pub theme: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            default_provider: "ollama".to_owned(),
            default_model: "qwen2.5-coder:7b".to_owned(),
            ollama_base_url: "http://localhost:11434".to_owned(),
            ui_preferences: UiPreferences {
                preferred_port: 7780,
                theme: "system".to_owned(),
            },
            default_ignore_patterns: crate::walker::DEFAULT_IGNORE_PATTERNS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Project-local config (<root>/.mapit/config.json)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub schema_version: u32,
    pub extra_ignore_patterns: Vec<String>,
    pub provider_override: Option<String>,
    pub model_override: Option<String>,
    pub last_full_map_at: Option<String>,
    pub last_incremental_map_at: Option<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            extra_ignore_patterns: vec![],
            provider_override: None,
            model_override: None,
            last_full_map_at: None,
            last_incremental_map_at: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Credentials file (~/.config/mapit/credentials.json)
// See docs/05-backend-schema.md §2.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub schema_version: u32,
    pub providers: std::collections::HashMap<String, ProviderCredential>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCredential {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for Credentials {
    fn default() -> Self {
        Self {
            schema_version: 1,
            providers: std::collections::HashMap::new(),
        }
    }
}

pub fn load_credentials(config_dir: &Path) -> Result<Credentials> {
    let path = config_dir.join("credentials.json");
    if !path.exists() {
        return Ok(Credentials::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let creds: Credentials = serde_json::from_str(&text)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(creds)
}

pub fn save_credentials(config_dir: &Path, creds: &Credentials) -> Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let path = config_dir.join("credentials.json");
    let text = serde_json::to_string_pretty(creds)?;
    std::fs::write(&path, text)
        .with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

pub fn load_global_config(config_dir: &Path) -> Result<GlobalConfig> {
    let path = config_dir.join("global_config.json");
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {}", path.display()))?;
    let cfg: GlobalConfig = serde_json::from_str(&text)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(cfg)
}

pub fn save_global_config(config_dir: &Path, cfg: &GlobalConfig) -> Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let path = config_dir.join("global_config.json");
    let text = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&path, text)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn load_project_config(mapit_dir: &Path) -> Result<ProjectConfig> {
    let path = mapit_dir.join("config.json");
    if !path.exists() {
        return Ok(ProjectConfig::default());
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn save_project_config(mapit_dir: &Path, cfg: &ProjectConfig) -> Result<()> {
    std::fs::create_dir_all(mapit_dir)?;
    let path = mapit_dir.join("config.json");
    let text = serde_json::to_string_pretty(cfg)?;
    std::fs::write(path, text)?;
    Ok(())
}

/// Returns the OS-appropriate global config directory.
/// On Linux follows XDG: `$XDG_CONFIG_HOME/mapit` or `~/.config/mapit`.
pub fn global_config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("mapit");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    PathBuf::from(home).join(".config").join("mapit")
}
