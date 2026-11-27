//! Finder-safe macOS desktop bootstrap and shared [`DesktopAppContext`].
//!
//! Resolves PostgreSQL connection details, `gnubg` executable path (probing
//! standard macOS Homebrew / system directories when launched from Finder with
//! `CWD = /`), and custom BGBlitz theme directories without requiring a terminal
//! shell environment.

use crate::local_state::LocalUiState;
use gpui_kit::Global;
use neondojo::facade::BackendFacade;
use neondojo::runtime::{CoreConfig, CoreRuntime};
use neondojo_board::themes::ThemeAtlasCache;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgConnectOptions;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// Default local PostgreSQL connection string used only when no `DATABASE_URL`,
/// `bootstrap.json`, `LocalUiState`, or `.env` entry is configured.
pub const DEFAULT_LOCAL_DATABASE_URL: &str =
    "postgres://neondojo:neondojo_local@127.0.0.1:54329/neondojo";

/// Standard macOS binary search directories probed when launching from Finder
/// (where `PATH` is minimal `/usr/bin:/bin:/usr/sbin:/sbin`).
pub const STANDARD_MACOS_BIN_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

/// Persisted Finder-safe desktop bootstrap configuration stored at
/// `~/Library/Application Support/NeonDojo/bootstrap.json`.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gnubg_binary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_themes_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
}

impl BootstrapConfig {
    /// Default location: `~/Library/Application Support/NeonDojo/bootstrap.json`.
    pub fn default_path() -> PathBuf {
        LocalUiState::app_support_dir().join("bootstrap.json")
    }

    pub fn load_or_default(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Self>(&raw).ok())
            .unwrap_or_default()
    }

    /// Writes `bootstrap.json` atomically (`tmp` → `rename`).
    pub fn save_atomic(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_vec_pretty(self)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        let tmp_name = format!(
            ".{}.{}.tmp",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("bootstrap.json"),
            std::process::id()
        );
        let tmp_path = path.with_file_name(tmp_name);
        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(&payload)?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, path)
    }
}

/// Resolved concrete startup settings ready for [`CoreRuntime::start`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBootstrap {
    pub database_url: String,
    pub gnubg_binary: String,
    pub custom_themes_dir: Option<PathBuf>,
    pub max_connections: Option<u32>,
    pub bootstrap_file: PathBuf,
}

impl ResolvedBootstrap {
    /// Builds a [`CoreConfig`] from this resolved bootstrap specification.
    pub fn to_core_config(&self) -> anyhow::Result<CoreConfig> {
        let options = PgConnectOptions::from_str(&self.database_url)?;
        let mut config = CoreConfig::new(options);
        config.gnubg_binary = self.gnubg_binary.clone();
        if let Some(max_conns) = self.max_connections {
            config = config.with_max_connections(&max_conns.to_string())?;
        }
        Ok(config)
    }

    /// Returns a display-safe URL with any password replaced by `***`.
    pub fn redacted_database_url(&self) -> String {
        redact_database_url(&self.database_url)
    }
}

/// Redacts any password segment in a PostgreSQL URI before surfacing it in UI banners.
pub fn redact_database_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let rest = &url[scheme_end + 3..];
    let Some(at_pos) = rest.find('@') else {
        return url.to_string();
    };
    let userinfo = &rest[..at_pos];
    let host_and_path = &rest[at_pos..];
    match userinfo.find(':') {
        Some(colon) => format!(
            "{}://{}:***{}",
            &url[..scheme_end],
            &userinfo[..colon],
            host_and_path
        ),
        None => url.to_string(),
    }
}

/// Parses a single `KEY=VALUE` line from `.env` content (handling optional quotes).
pub fn parse_dotenv_value(content: &str, key: &str) -> Option<String> {
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map(str::trim).unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let value = v.trim();
        let unquoted = if (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
            || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
        {
            &value[1..value.len() - 1]
        } else {
            value.split('#').next().unwrap_or(value).trim()
        };
        if !unquoted.is_empty() {
            return Some(unquoted.to_string());
        }
    }
    None
}

fn find_dotenv_value(key: &str) -> Option<String> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".env"));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join(".env"));
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join(".env"));
    }
    candidates.push(LocalUiState::app_support_dir().join(".env"));

    for path in candidates {
        if let Ok(content) = fs::read_to_string(&path)
            && let Some(value) = parse_dotenv_value(&content, key)
        {
            return Some(value);
        }
    }
    None
}

/// Probes candidate directories (e.g. `/opt/homebrew/bin`, `/usr/local/bin`,
/// `$HOME/.local/bin`) for an executable named `gnubg`.
pub fn probe_gnubg_in_dirs(dirs: &[PathBuf]) -> Option<String> {
    for dir in dirs {
        let candidate = dir.join("gnubg");
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

fn default_gnubg_probe_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = STANDARD_MACOS_BIN_DIRS.iter().map(PathBuf::from).collect();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local").join("bin"));
    }
    dirs
}

fn resolve_default_themes_dir() -> Option<PathBuf> {
    let app_themes = LocalUiState::app_support_dir().join("themes");
    if app_themes.is_dir() {
        return Some(app_themes);
    }
    if let Ok(cwd) = std::env::current_dir() {
        let repo_themes = cwd.join("themes");
        if repo_themes.is_dir() {
            return Some(repo_themes);
        }
    }
    None
}

/// Resolves Finder-safe desktop bootstrap settings from env vars, `bootstrap.json`,
/// `LocalUiState`, `.env` fallbacks, and standard macOS binary directories.
pub fn resolve_bootstrap_config(local_state: &LocalUiState) -> ResolvedBootstrap {
    let bootstrap_file = BootstrapConfig::default_path();
    resolve_bootstrap_from_path(local_state, &bootstrap_file, &default_gnubg_probe_dirs())
}

pub fn resolve_bootstrap_from_path(
    local_state: &LocalUiState,
    bootstrap_file: &Path,
    probe_dirs: &[PathBuf],
) -> ResolvedBootstrap {
    let file_cfg = BootstrapConfig::load_or_default(bootstrap_file);

    let database_url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            file_cfg
                .database_url
                .clone()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| {
            local_state
                .bootstrap
                .database_url
                .clone()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| find_dotenv_value("DATABASE_URL"))
        .unwrap_or_else(|| DEFAULT_LOCAL_DATABASE_URL.to_string());

    let gnubg_binary = std::env::var("GNUBG_BINARY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            file_cfg
                .gnubg_binary
                .clone()
                .filter(|s| !s.trim().is_empty())
        })
        .or_else(|| find_dotenv_value("GNUBG_BINARY"))
        .or_else(|| probe_gnubg_in_dirs(probe_dirs))
        .unwrap_or_else(|| "gnubg".to_string());

    let custom_themes_dir = file_cfg
        .custom_themes_dir
        .or_else(|| local_state.bootstrap.custom_themes_dir.clone())
        .or_else(resolve_default_themes_dir);

    ResolvedBootstrap {
        database_url,
        gnubg_binary,
        custom_themes_dir,
        max_connections: file_cfg.max_connections,
        bootstrap_file: bootstrap_file.to_path_buf(),
    }
}

/// Lifecycle state of the in-process PostgreSQL / [`CoreRuntime`] backend.
#[derive(Clone)]
pub enum BackendConnectionState {
    /// Not started (e.g. unit/headless UI tests that do not open PostgreSQL).
    Idle,
    /// Connecting to PostgreSQL and running baseline checks/migrations.
    Connecting,
    /// Ready with a live in-process [`BackendFacade`] and supervised [`CoreRuntime`].
    Connected {
        runtime: Arc<CoreRuntime>,
        facade: Box<BackendFacade>,
    },
    /// Database unreachable or initialization failed; surfaces a retry/configure UI.
    Failed {
        database_url_redacted: String,
        message: String,
    },
}

impl BackendConnectionState {
    pub fn facade(&self) -> Option<&BackendFacade> {
        match self {
            Self::Connected { facade, .. } => Some(facade.as_ref()),
            _ => None,
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self, Self::Connecting)
    }

    pub fn failure(&self) -> Option<(&str, &str)> {
        match self {
            Self::Failed {
                database_url_redacted,
                message,
            } => Some((database_url_redacted.as_str(), message.as_str())),
            _ => None,
        }
    }
}

/// Shared desktop application services stored as a GPUI [`Global`].
#[derive(Clone)]
pub struct DesktopAppContext {
    pub bootstrap: ResolvedBootstrap,
    pub connection: BackendConnectionState,
    pub board_atlas: Arc<Mutex<ThemeAtlasCache>>,
    pub reset_generation: u64,
}

impl Global for DesktopAppContext {}

impl DesktopAppContext {
    pub fn new(bootstrap: ResolvedBootstrap) -> Self {
        Self {
            bootstrap,
            connection: BackendConnectionState::Idle,
            board_atlas: Arc::new(Mutex::new(ThemeAtlasCache::default())),
            reset_generation: 0,
        }
    }

    pub fn with_facade(mut self, runtime: Arc<CoreRuntime>, facade: BackendFacade) -> Self {
        self.connection = BackendConnectionState::Connected {
            runtime,
            facade: Box::new(facade),
        };
        self
    }

    pub fn facade(&self) -> Option<BackendFacade> {
        self.connection.facade().cloned()
    }

    pub fn set_connecting(&mut self) {
        self.connection = BackendConnectionState::Connecting;
    }

    pub fn set_connected(&mut self, runtime: Arc<CoreRuntime>, facade: BackendFacade) {
        self.connection = BackendConnectionState::Connected {
            runtime,
            facade: Box::new(facade),
        };
    }

    pub fn set_failed(&mut self, message: impl Into<String>) {
        self.connection = BackendConnectionState::Failed {
            database_url_redacted: self.bootstrap.redacted_database_url(),
            message: message.into(),
        };
    }

    pub fn update_database_url(&mut self, url: String) -> std::io::Result<()> {
        let mut cfg = BootstrapConfig::load_or_default(&self.bootstrap.bootstrap_file);
        cfg.database_url = Some(url.clone());
        cfg.save_atomic(&self.bootstrap.bootstrap_file)?;
        self.bootstrap.database_url = url;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn redacts_password_from_postgres_uri() {
        assert_eq!(
            redact_database_url("postgres://neondojo:secret_pw@127.0.0.1:54329/neondojo"),
            "postgres://neondojo:***@127.0.0.1:54329/neondojo"
        );
        assert_eq!(
            redact_database_url("postgres://neondojo@127.0.0.1:54329/neondojo"),
            "postgres://neondojo@127.0.0.1:54329/neondojo"
        );
    }

    #[test]
    fn parses_dotenv_lines_with_quotes_and_comments() {
        let env = r#"
            # Comment line
            export DATABASE_URL="postgres://u:p@localhost:5432/db"
            GNUBG_BINARY='/opt/homebrew/bin/gnubg'
            PLAIN=hello # trailing comment
        "#;
        assert_eq!(
            parse_dotenv_value(env, "DATABASE_URL").as_deref(),
            Some("postgres://u:p@localhost:5432/db")
        );
        assert_eq!(
            parse_dotenv_value(env, "GNUBG_BINARY").as_deref(),
            Some("/opt/homebrew/bin/gnubg")
        );
        assert_eq!(parse_dotenv_value(env, "PLAIN").as_deref(), Some("hello"));
    }

    #[test]
    fn probes_gnubg_and_persists_bootstrap_config_atomically() {
        let dir = tempdir().expect("temp dir");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let fake_gnubg = bin_dir.join("gnubg");
        fs::write(&fake_gnubg, b"#!/bin/sh\n").unwrap();

        let found = probe_gnubg_in_dirs(&[dir.path().join("missing"), bin_dir]);
        assert_eq!(
            found.as_deref(),
            Some(fake_gnubg.to_string_lossy().as_ref())
        );

        let cfg_path = dir.path().join("bootstrap.json");
        let cfg = BootstrapConfig {
            database_url: Some("postgres://u:p@127.0.0.1:54329/custom".into()),
            gnubg_binary: Some("/opt/homebrew/bin/gnubg".into()),
            custom_themes_dir: Some(dir.path().join("themes")),
            max_connections: Some(12),
        };
        cfg.save_atomic(&cfg_path).unwrap();
        let loaded = BootstrapConfig::load_or_default(&cfg_path);
        assert_eq!(loaded, cfg);
    }
}
