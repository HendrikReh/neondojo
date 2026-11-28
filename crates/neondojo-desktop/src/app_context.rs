//! Finder-safe macOS desktop bootstrap and shared [`DesktopAppContext`].
//!
//! Resolves PostgreSQL connection details, `gnubg` executable path (probing
//! standard macOS Homebrew / system directories when launched from Finder with
//! `CWD = /`), and custom BGBlitz theme directories without requiring a terminal
//! shell environment.

use crate::local_state::LocalUiState;
use gpui_kit::{Context, Global};
use neondojo::facade::BackendFacade;
use neondojo::runtime::{CoreConfig, CoreRuntime};
use neondojo_board::BoardView;
use neondojo_board::themes::{BoardThemeConfig, ThemeAtlasCache};
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

fn dir_has_zip_archives(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        path.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    })
}

/// Candidate BGBlitz `themes/` directories in priority order:
/// 1. User Application Support override (`~/Library/Application Support/NeonDojo/themes`)
/// 2. Bundled `.app` resources (`<NeonDojo.app>/Contents/Resources/themes`)
/// 3. Current working directory (`./themes`)
/// 4. Ancestors of the running executable (finds `<repo>/themes` when launched from
///    `target/local-desktop/NeonDojo.app/Contents/MacOS/neondojo-desktop` or `target/debug/...`).
fn candidate_themes_dirs(
    app_support: &Path,
    cwd: Option<&Path>,
    exe: Option<&Path>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    candidates.push(app_support.join("themes"));

    if let Some(exe_path) = exe
        && let Some(macos_dir) = exe_path.parent()
        && let Some(contents_dir) = macos_dir.parent()
    {
        let bundle_themes = contents_dir.join("Resources").join("themes");
        if !candidates.contains(&bundle_themes) {
            candidates.push(bundle_themes);
        }
    }

    if let Some(cwd_path) = cwd {
        let cwd_themes = cwd_path.join("themes");
        if !candidates.contains(&cwd_themes) {
            candidates.push(cwd_themes);
        }
    }

    if let Some(exe_path) = exe {
        for ancestor in exe_path.ancestors().skip(1).take(7) {
            let repo_themes = ancestor.join("themes");
            if !candidates.contains(&repo_themes) {
                candidates.push(repo_themes);
            }
        }
    }

    candidates
}

pub fn resolve_themes_dir_from(
    app_support: &Path,
    cwd: Option<&Path>,
    exe: Option<&Path>,
) -> Option<PathBuf> {
    let candidates = candidate_themes_dirs(app_support, cwd, exe);
    if let Some(with_zips) = candidates.iter().find(|dir| dir_has_zip_archives(dir)) {
        return Some(with_zips.clone());
    }
    candidates.into_iter().find(|dir| dir.is_dir())
}

fn resolve_default_themes_dir() -> Option<PathBuf> {
    let app_support = LocalUiState::app_support_dir();
    let cwd = std::env::current_dir().ok();
    let exe = std::env::current_exe().ok();
    resolve_themes_dir_from(&app_support, cwd.as_deref(), exe.as_deref())
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
    pub board_config: BoardThemeConfig,
    pub reset_generation: u64,
}

impl Global for DesktopAppContext {}

impl DesktopAppContext {
    pub fn new(bootstrap: ResolvedBootstrap) -> Self {
        let themes_dir = bootstrap.custom_themes_dir.clone();
        Self {
            bootstrap,
            connection: BackendConnectionState::Idle,
            board_atlas: Arc::new(Mutex::new(ThemeAtlasCache::new(themes_dir, 4))),
            board_config: BoardThemeConfig::default(),
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

/// Creates a board that follows the applied desktop artwork, including in dialogs.
pub(crate) fn themed_board(id: &'static str, cx: &mut Context<BoardView>) -> BoardView {
    let desktop = cx.try_global::<DesktopAppContext>();
    let atlas = desktop
        .map(|ctx| Arc::clone(&ctx.board_atlas))
        .unwrap_or_else(|| Arc::new(Mutex::new(ThemeAtlasCache::default())));
    let mut config = desktop
        .map(|ctx| ctx.board_config.clone())
        .unwrap_or_else(|| BoardThemeConfig {
            board_theme: cx.try_global::<LocalUiState>().map_or_else(
                || BoardThemeConfig::default().board_theme,
                |state| state.board_theme.clone(),
            ),
            ..BoardThemeConfig::default()
        });
    let mut board = BoardView::new(id, atlas);
    board.set_theme_config(config.clone(), cx);
    cx.observe_global::<DesktopAppContext>(move |board, cx| {
        if let Some(desktop) = cx.try_global::<DesktopAppContext>()
            && desktop.board_config != config
        {
            config = desktop.board_config.clone();
            board.set_theme_config(config.clone(), cx);
        }
    })
    .detach();
    board
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

    #[test]
    fn resolves_bgblitz_themes_from_bundle_resources_or_repo_ancestors_when_cwd_is_root() {
        let dir = tempdir().expect("temp dir");
        let app_support = dir.path().join("AppSupport");
        let repo = dir.path().join("bgtutor");
        let repo_themes = repo.join("themes");
        let bundle_macos = repo
            .join("target")
            .join("local-desktop")
            .join("NeonDojo.app")
            .join("Contents")
            .join("MacOS");
        let bundle_resources_themes = repo
            .join("target")
            .join("local-desktop")
            .join("NeonDojo.app")
            .join("Contents")
            .join("Resources")
            .join("themes");
        fs::create_dir_all(&repo_themes).unwrap();
        fs::create_dir_all(&bundle_macos).unwrap();
        let fake_exe = bundle_macos.join("neondojo-desktop");
        fs::write(&fake_exe, b"").unwrap();
        fs::write(repo_themes.join("bg_classic.zip"), b"PK").unwrap();

        // Launched from Finder (cwd = /) without bundled themes -> finds <repo>/themes via exe ancestors.
        let found_repo =
            resolve_themes_dir_from(&app_support, Some(Path::new("/")), Some(&fake_exe));
        assert_eq!(found_repo.as_deref(), Some(repo_themes.as_path()));

        // When Contents/Resources/themes has ZIP archives, bundle Resources take priority over repo walk.
        fs::create_dir_all(&bundle_resources_themes).unwrap();
        fs::write(bundle_resources_themes.join("bg_classic.zip"), b"PK").unwrap();
        let found_bundle =
            resolve_themes_dir_from(&app_support, Some(Path::new("/")), Some(&fake_exe));
        assert_eq!(
            found_bundle.as_deref(),
            Some(bundle_resources_themes.as_path())
        );

        // User Application Support override takes highest priority when populated with ZIPs.
        let user_themes = app_support.join("themes");
        fs::create_dir_all(&user_themes).unwrap();
        fs::write(user_themes.join("custom.zip"), b"PK").unwrap();
        let found_user =
            resolve_themes_dir_from(&app_support, Some(Path::new("/")), Some(&fake_exe));
        assert_eq!(found_user.as_deref(), Some(user_themes.as_path()));
    }
}
