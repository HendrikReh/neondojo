//! BackgammonDB's immutable, score-aware SQLite opening book.
//! The wire format is documented at https://github.com/chalkstreamdev/bgdb-opening-book/blob/main/book-format.md.

use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::engine::Candidate;
use crate::model::Position;
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path as FsPath};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

mod keys;
mod lookup;
mod routes;
mod sqlite_book;
pub(crate) use keys::canonical_keys;
use lookup::*;
pub use lookup::{lookup, lookup_with_rules};
pub use routes::OpeningBookDoc;
pub(crate) use routes::routes;
use sqlite_book::*;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct BookConfig {
    pub version: i64,
    pub provider: String,
    pub path: String,
    pub xg_path: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BookConfigWrite {
    pub version: i64,
    pub provider: String,
    pub path: String,
    /// Omission preserves a previously configured XG path.
    pub xg_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BookContext {
    pub name: String,
    pub entries: i64,
    /// Decisions into the game; this is not evaluation ply.
    pub depth: Option<i32>,
    pub complete: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BookStatus {
    pub available: bool,
    pub path: String,
    pub engine_version: Option<String>,
    pub entries: i64,
    pub contexts: Vec<BookContext>,
    pub compression: Option<String>,
    pub license: Option<String>,
    pub source_url: Option<String>,
    pub attribution: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BookSettings {
    pub config: BookConfig,
    pub status: BookStatus,
    pub xg_status: crate::xg_book::XgBookStatus,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BookProvenance {
    pub provider: String,
    pub engine: String,
    pub evaluation_kind: String,
    /// Cubeful equity normalized to the current cube (match play uses GNUbg NEMG).
    pub equity_kind: String,
    pub context: String,
    pub context_depth: Option<i32>,
    pub source_url: Option<String>,
    pub license: Option<String>,
    pub attribution: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BookChecker {
    pub candidates: Vec<Candidate>,
    pub plies: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BookCubeEquities {
    pub no_double: f64,
    pub double_take: f64,
    pub double_pass: f64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BookCube {
    pub probabilities: Vec<f64>,
    pub equities: BookCubeEquities,
    pub verdict: String,
    pub plies: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BookLookup {
    pub provider: String,
    pub hit: bool,
    pub reason: Option<String>,
    pub checker_xgid: Option<String>,
    pub cube_xgid: Option<String>,
    pub checker: Option<BookChecker>,
    pub cube: Option<BookCube>,
    pub provenance: Option<BookProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xg_position: Option<crate::xg_book::XgPositionResult>,
}

#[derive(Deserialize, ToSchema)]
pub struct BookPathRequest {
    pub path: String,
}

pub(crate) async fn load(pool: &sqlx::PgPool) -> Result<BookConfig> {
    sqlx::query("INSERT INTO opening_book_settings(singleton,path) VALUES(true,$1) ON CONFLICT(singleton) DO NOTHING")
        .bind(default_path()).execute(pool).await?;
    sqlx::query("UPDATE opening_book_settings SET xg_path=$1 WHERE singleton=true AND xg_path=''")
        .bind(crate::xg_book::default_path())
        .execute(pool)
        .await?;
    Ok(sqlx::query_as(
        "SELECT version,provider,path,xg_path FROM opening_book_settings WHERE singleton=true",
    )
    .fetch_one(pool)
    .await?)
}

const MAX_PAYLOAD: usize = 2 * 1024 * 1024;

/// User data stays outside the Git checkout and is never bundled into the app.
pub fn default_path() -> String {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    let directory = home.join("Library/Application Support/NeonDojo");
    #[cfg(not(target_os = "macos"))]
    let directory = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("neondojo");
    directory
        .join("opening-book.sqlite")
        .to_string_lossy()
        .into_owned()
}

pub async fn status(config: &BookConfig) -> BookStatus {
    let path = if config.path.is_empty() {
        default_path()
    } else {
        config.path.clone()
    };
    let task_path = path.clone();
    match tokio::task::spawn_blocking(move || Book::open(&task_path).map(|book| book.status)).await
    {
        Ok(Ok(status)) => status,
        result => BookStatus {
            available: false,
            path,
            engine_version: None,
            entries: 0,
            contexts: vec![],
            compression: None,
            license: None,
            source_url: None,
            attribution: None,
            error: Some(match result {
                Ok(Err(error)) => format!("{error:#}"),
                Err(error) => error.to_string(),
                _ => unreachable!(),
            }),
        },
    }
}

impl BookLookup {
    fn miss(reason: impl Into<String>) -> Self {
        Self {
            provider: String::new(),
            hit: false,
            reason: Some(reason.into()),
            checker_xgid: None,
            cube_xgid: None,
            checker: None,
            cube: None,
            provenance: None,
            xg_position: None,
        }
    }
}

/// Validate a local serving database before replacing the installed file. The
/// temporary file is on the destination filesystem, so the rename is atomic.
pub async fn install_from_path(config: &BookConfig, source_path: &str) -> Result<BookStatus> {
    let source = source_path.to_owned();
    let destination = if config.path.is_empty() {
        default_path()
    } else {
        config.path.clone()
    };
    tokio::task::spawn_blocking(move || {
        let source = FsPath::new(&source);
        let destination = FsPath::new(&destination);
        Book::open(source.to_str().context("Invalid book path")?)?;
        if fs::canonicalize(source).ok() == fs::canonicalize(destination).ok()
            && destination.exists()
        {
            return Ok(
                Book::open(destination.to_str().context("Invalid destination path")?)?.status,
            );
        }
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .context("Opening book needs an absolute destination path")?;
        install_validated_copy(source, destination, parent, |path| {
            Book::open(path.to_str().context("Invalid temporary book path")?)?;
            Ok(())
        })?;
        Ok(Book::open(destination.to_str().context("Invalid destination path")?)?.status)
    })
    .await
    .context("Opening book installer task failed")?
}

/// Validate the copied bytes in the destination directory before replacing the book.
pub(crate) fn install_validated_copy(
    source: &FsPath,
    destination: &FsPath,
    parent: &FsPath,
    validate: impl FnOnce(&FsPath) -> Result<()>,
) -> Result<()> {
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut fs::File::open(source)?, temporary.as_file_mut())?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    validate(temporary.path())?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests;
