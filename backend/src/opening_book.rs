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
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

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

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/opening-book", get(get_settings).put(put_settings))
        .route("/api/opening-book/check", post(check_book))
        .route("/api/opening-book/install", post(install_book))
        .route("/api/opening-book/xg/check", post(check_xg_book))
        .route("/api/opening-book/xg/install", post(install_xg_book))
        .route("/api/positions/{id}/opening-book", get(position_book))
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

fn validate_path(path: &str) -> ApiResult<()> {
    if path.is_empty()
        || path.len() > 4096
        || path.chars().any(char::is_control)
        || !std::path::Path::new(path).is_absolute()
    {
        return Err(ApiError::invalid(
            "Opening book path must be absolute, nonempty, at most 4096 bytes and contain no control characters",
        ));
    }
    Ok(())
}

#[utoipa::path(get,path="/api/opening-book",responses((status=200,body=BookSettings)),tag="Opening book")]
async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<BookSettings>> {
    let config = load(&state.pool).await?;
    let status = status(&config).await;
    let xg_status = crate::xg_book::status(&config.xg_path).await;
    Ok(Json(BookSettings {
        config,
        status,
        xg_status,
    }))
}

#[utoipa::path(put,path="/api/opening-book",request_body=BookConfigWrite,responses((status=200,body=BookSettings),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Opening book")]
async fn put_settings(
    State(state): State<AppState>,
    payload: Result<Json<BookConfigWrite>, JsonRejection>,
) -> ApiResult<Json<BookSettings>> {
    let Json(config) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    if !(0..i64::MAX).contains(&config.version)
        || !matches!(
            config.provider.as_str(),
            "off" | "backgammondb" | "automatic" | "xg"
        )
    {
        return Err(ApiError::invalid(
            "Invalid opening book version or provider",
        ));
    }
    validate_path(&config.path)?;
    if let Some(path) = &config.xg_path {
        validate_path(path)?;
    }
    load(&state.pool).await?;
    let saved:Option<BookConfig>=sqlx::query_as("UPDATE opening_book_settings SET version=version+1,provider=$2,path=$3,xg_path=coalesce($4,xg_path) WHERE singleton=true AND version=$1 RETURNING version,provider,path,xg_path")
        .bind(config.version).bind(config.provider).bind(config.path).bind(config.xg_path).fetch_optional(&state.pool).await.map_err(anyhow::Error::from)?;
    let config = saved.ok_or_else(|| {
        ApiError(
            StatusCode::CONFLICT,
            "Opening book settings changed; reload before saving".into(),
        )
    })?;
    let status = status(&config).await;
    let xg_status = crate::xg_book::status(&config.xg_path).await;
    Ok(Json(BookSettings {
        config,
        status,
        xg_status,
    }))
}

#[utoipa::path(post,path="/api/opening-book/check",request_body=BookPathRequest,responses((status=200,body=BookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
async fn check_book(
    payload: Result<Json<BookPathRequest>, JsonRejection>,
) -> ApiResult<Json<BookStatus>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    validate_path(&request.path)?;
    Ok(Json(
        status(&BookConfig {
            version: 0,
            provider: "backgammondb".into(),
            path: request.path,
            xg_path: crate::xg_book::default_path(),
        })
        .await,
    ))
}

#[utoipa::path(post,path="/api/opening-book/install",request_body=BookPathRequest,responses((status=200,body=BookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
async fn install_book(
    State(state): State<AppState>,
    payload: Result<Json<BookPathRequest>, JsonRejection>,
) -> ApiResult<Json<BookStatus>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    validate_path(&request.path)?;
    let config = load(&state.pool).await?;
    Ok(Json(
        install_from_path(&config, &request.path)
            .await
            .map_err(|error| {
                ApiError::invalid(format!("Cannot install opening book: {error:#}"))
            })?,
    ))
}

#[utoipa::path(post,path="/api/opening-book/xg/check",request_body=BookPathRequest,responses((status=200,body=crate::xg_book::XgBookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
async fn check_xg_book(
    payload: Result<Json<BookPathRequest>, JsonRejection>,
) -> ApiResult<Json<crate::xg_book::XgBookStatus>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    validate_path(&request.path)?;
    Ok(Json(crate::xg_book::status(&request.path).await))
}

#[utoipa::path(post,path="/api/opening-book/xg/install",request_body=BookPathRequest,responses((status=200,body=crate::xg_book::XgBookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
async fn install_xg_book(
    State(state): State<AppState>,
    payload: Result<Json<BookPathRequest>, JsonRejection>,
) -> ApiResult<Json<crate::xg_book::XgBookStatus>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    validate_path(&request.path)?;
    let config = load(&state.pool).await?;
    Ok(Json(
        crate::xg_book::install_from_path(&config.xg_path, &request.path)
            .await
            .map_err(|error| {
                ApiError::invalid(format!("Cannot install XG opening book: {error:#}"))
            })?,
    ))
}

#[utoipa::path(get,path="/api/positions/{id}/opening-book",params(("id"=Uuid,Path)),responses((status=200,body=BookLookup),(status=404,body=ErrorBody)),tag="Opening book")]
async fn position_book(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BookLookup>> {
    let position = crate::store::get_position(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    let config = load(&state.pool).await?;
    let beaver = source_beaver_rule(&state.pool, &position).await?;
    Ok(Json(
        lookup_with_rules(&config, &position, None, beaver).await?,
    ))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_settings,
        put_settings,
        check_book,
        install_book,
        check_xg_book,
        install_xg_book,
        position_book
    ),
    components(schemas(
        BookConfig,
        BookConfigWrite,
        BookContext,
        BookStatus,
        BookSettings,
        BookPathRequest,
        BookLookup,
        BookChecker,
        BookCube,
        BookCubeEquities,
        BookProvenance,
        crate::xg_book::XgBookStatus,
        crate::xg_book::XgPositionResult,
        crate::xg_book::XgEvaluation,
        crate::xg_book::XgRollout
    ))
)]
pub struct OpeningBookDoc;

const MAX_PAYLOAD: usize = 2 * 1024 * 1024;

/// User data stays outside the Git checkout and is never bundled into the app.
pub fn default_path() -> String {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    let directory = home.join("Library/Application Support/BG Tutor");
    #[cfg(not(target_os = "macos"))]
    let directory = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"))
        .join("bgtutor");
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

/// None means explicitly browse this book's own engine answers. A cache in front
/// of native analysis MUST pass the installed engine version; mismatches miss.
/// All SQLite, file IO and decompression run off Tokio's executor threads.
async fn lookup_bgdb(
    config: &BookConfig,
    position: &Position,
    expected_engine_version: Option<&str>,
) -> Result<BookLookup> {
    if config.provider == "off" {
        return Ok(BookLookup::miss("Opening book is off"));
    }
    if !matches!(config.provider.as_str(), "automatic" | "backgammondb") {
        return Ok(BookLookup::miss("Unsupported opening book provider"));
    }
    let (checker_key, cube_key) = match canonical_keys(position) {
        Ok(keys) => keys,
        Err(error) => {
            return Ok(BookLookup::miss(format!(
                "Unsupported position context: {error:#}"
            )));
        }
    };
    let config = config.clone();
    let expected = expected_engine_version.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        let mut result = BookLookup::miss("No entry for this position, roll and match context");
        result.checker_xgid = Some(checker_key.clone());
        result.cube_xgid = cube_key.clone();
        let work = || -> Result<(Option<BookChecker>, Option<BookCube>, BookProvenance)> {
            let book = Book::open(
                if config.path.is_empty() {
                    default_path()
                } else {
                    config.path
                }
                .as_str(),
            )?;
            if let Some(version) = expected {
                let version = version
                    .strip_prefix("GNU Backgammon ")
                    .unwrap_or(&version)
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                ensure!(
                    book.status.engine_version.as_deref() == Some(version),
                    "Opening book engine version mismatch (book {}, requested {version})",
                    book.status.engine_version.as_deref().unwrap_or("unknown")
                );
            }
            let checker = book
                .get(&checker_key)?
                .map(|payload| parse_checker(payload, &checker_key, book.top_ply))
                .transpose()?;
            let cube = cube_key
                .as_deref()
                .map(|key| book.get(key))
                .transpose()?
                .flatten()
                .map(|payload| parse_cube(payload, book.cube_ply))
                .transpose()?;
            let context = context_name(&checker_key)?;
            let depth = book
                .status
                .contexts
                .iter()
                .find(|item| item.name == context)
                .and_then(|item| item.depth);
            let provenance = BookProvenance {
                provider: "backgammondb".into(),
                engine: format!(
                    "GNU Backgammon {}",
                    book.status.engine_version.as_deref().unwrap_or("unknown")
                ),
                evaluation_kind: "evaluation".into(),
                equity_kind: "normalized_cubeful".into(),
                context,
                context_depth: depth,
                source_url: book.status.source_url,
                license: book.status.license,
                attribution: book.status.attribution,
            };
            Ok((checker, cube, provenance))
        };
        match work() {
            Ok((checker, cube, provenance)) => {
                result.hit = checker.is_some() || cube.is_some();
                result.checker = checker;
                result.cube = cube;
                if result.hit {
                    result.reason = None;
                    result.provenance = Some(provenance);
                }
            }
            Err(error) => result.reason = Some(format!("Opening book unavailable: {error:#}")),
        }
        result
    })
    .await
    .context("Opening book reader task failed")
}

/// Canonical BG Tutor positions use the standard no-beaver contract. REST
/// callers resolve the immutable source's rule and use lookup_with_rules.
pub async fn lookup(
    config: &BookConfig,
    position: &Position,
    expected_engine_version: Option<&str>,
) -> Result<BookLookup> {
    lookup_with_rules(config, position, expected_engine_version, Some(false)).await
}
pub async fn lookup_with_rules(
    config: &BookConfig,
    position: &Position,
    expected_engine_version: Option<&str>,
    beaver: Option<bool>,
) -> Result<BookLookup> {
    let finish = |mut result: BookLookup| {
        result.provider = config.provider.clone();
        result
    };
    if config.provider == "off" {
        return Ok(finish(BookLookup::miss("Opening book is off")));
    }
    if !matches!(
        config.provider.as_str(),
        "automatic" | "backgammondb" | "xg"
    ) {
        return Ok(finish(BookLookup::miss(
            "Unsupported opening book provider",
        )));
    }
    let canonical = match canonical_keys(position) {
        Ok((key, _)) => key,
        Err(error) => {
            return Ok(finish(BookLookup::miss(format!(
                "Unsupported position context: {error:#}"
            ))));
        }
    };
    let money = canonical.split(':').nth(8) == Some("0");
    if money && beaver.is_none() {
        return Ok(finish(BookLookup::miss(
            "Money-game beaver rule is not recorded; no book context assumed",
        )));
    }
    let mut result = if matches!(config.provider.as_str(), "automatic" | "backgammondb")
        && (!money || beaver == Some(false))
    {
        lookup_bgdb(config, position, expected_engine_version).await?
    } else {
        BookLookup::miss(if config.provider == "xg" {
            "No XG position reference for this board and rule context"
        } else {
            "BackgammonDB has no beaver-rule entries"
        })
    };
    // Native jobs must never be fulfilled with a different engine's reference.
    if matches!(config.provider.as_str(), "automatic" | "xg") && expected_engine_version.is_none() {
        match crate::xg_book::lookup(&config.xg_path, position, beaver).await {
            Ok(Some(reference)) => {
                result.hit = true;
                result.reason = None;
                result.xg_position = Some(reference);
            }
            Ok(None) => {}
            Err(error) if config.provider == "xg" => {
                result.reason = Some(format!("XG opening book unavailable: {error:#}"))
            }
            Err(error) => {
                tracing::debug!(error=?error,"Optional XG position reference unavailable")
            }
        }
    }
    Ok(finish(result))
}

/// Beaver rules are not encoded by a GNUbg Match ID. Resolve them once from
/// immutable source metadata rather than treating different money rules alike.
async fn source_beaver_rule(pool: &sqlx::PgPool, position: &Position) -> Result<Option<bool>> {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    let bytes = STANDARD_NO_PAD.decode(&position.gnubg_match_id)?;
    ensure!(bytes.len() == 9, "Invalid GNUbg Match ID");
    let length: i32 = (0..15)
        .map(|i| i32::from((bytes[(21 + i) / 8] >> ((21 + i) % 8)) & 1) << i)
        .sum();
    if length > 0 {
        return Ok(Some(false));
    }
    let row: Option<(String, String, i32)> =
        sqlx::query_as("SELECT format,sha256,octet_length(original) FROM matches WHERE id=$1")
            .bind(position.match_uuid)
            .fetch_optional(pool)
            .await?;
    let Some((format, sha, length)) = row else {
        return Ok(None);
    };
    match format.as_str() {
        // These public importers reject the beaver rule, including its RU form.
        "sgf" | "bgf" => Ok(Some(false)),
        "xg" => {
            type Rules = std::collections::HashMap<(Uuid, String), Option<bool>>;
            static RULES: std::sync::OnceLock<std::sync::Mutex<Rules>> = std::sync::OnceLock::new();
            let cache = RULES.get_or_init(|| std::sync::Mutex::new(Rules::new()));
            let key = (position.match_uuid, sha);
            if let Some(rule) = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("Book rule cache unavailable"))?
                .get(&key)
                .copied()
            {
                return Ok(rule);
            }
            // Bound the legacy source read separately from the 64 MiB XG
            // decompression limit. Larger sources miss without an assumption.
            if length > 16 * 1024 * 1024 {
                return Ok(None);
            }
            let original: Vec<u8> = sqlx::query_scalar("SELECT original FROM matches WHERE id=$1")
                .bind(position.match_uuid)
                .fetch_one(pool)
                .await?;
            let rule =
                tokio::task::spawn_blocking(move || crate::import::xg_beaver_rule(&original))
                    .await?
                    .ok();
            let mut cache = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("Book rule cache unavailable"))?;
            if cache.len() >= 128 {
                cache.clear();
            }
            cache.insert(key, rule);
            Ok(rule)
        }
        // MAT has no reliable money-session beaver declaration.
        _ => Ok(None),
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
        use std::{fs, io::Write, path::Path};
        let source = Path::new(&source);
        let destination = Path::new(&destination);
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
        fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        std::io::copy(&mut fs::File::open(source)?, temporary.as_file_mut())?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        Book::open(
            temporary
                .path()
                .to_str()
                .context("Invalid temporary book path")?,
        )?;
        temporary
            .persist(destination)
            .map_err(|error| error.error)?;
        Ok(Book::open(destination.to_str().context("Invalid destination path")?)?.status)
    })
    .await
    .context("Opening book installer task failed")?
}

struct Book {
    connection: rusqlite::Connection,
    status: BookStatus,
    dictionary: Option<Vec<u8>>,
    top_ply: Option<i32>,
    cube_ply: Option<i32>,
}
impl Book {
    fn open(path: &str) -> Result<Self> {
        use rusqlite::types::ValueRef;
        let connection = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("Cannot open opening book at {path}"))?;
        connection.busy_timeout(std::time::Duration::from_millis(250))?;
        let mut metadata = std::collections::HashMap::new();
        let mut dictionary = None;
        {
            let mut statement = connection.prepare("SELECT key,value FROM meta")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let key: String = row.get(0)?;
                let value = row.get_ref(1)?;
                if key == "zdict" {
                    if let ValueRef::Blob(bytes) = value {
                        dictionary = Some(bytes.to_vec());
                    }
                } else {
                    let value = match value {
                        ValueRef::Text(bytes) => String::from_utf8(bytes.to_vec())?,
                        ValueRef::Integer(number) => number.to_string(),
                        _ => continue,
                    };
                    metadata.insert(key, value);
                }
            }
        }
        ensure!(
            metadata.get("formatVersion").map(String::as_str) == Some("1"),
            "Unsupported opening book formatVersion"
        );
        let version = metadata
            .get("engineVersion")
            .filter(|value| !value.is_empty())
            .context("Opening book lacks engineVersion")?
            .clone();
        let compression = metadata
            .get("compression")
            .context("Opening book lacks compression")?
            .clone();
        ensure!(
            matches!(compression.as_str(), "none" | "zlib"),
            "Unsupported opening book compression"
        );
        if compression == "zlib" {
            ensure!(
                dictionary
                    .as_ref()
                    .is_some_and(|bytes| !bytes.is_empty() && bytes.len() <= 32768),
                "Opening book lacks a valid zlib dictionary"
            );
        }
        let contexts: Vec<BookContext> = connection
            .prepare("SELECT name,entries,depth,complete FROM contexts ORDER BY name")?
            .query_map([], |row| {
                Ok(BookContext {
                    name: row.get(0)?,
                    entries: row.get(1)?,
                    depth: row.get(2)?,
                    complete: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        ensure!(
            contexts.iter().all(|context| context.entries >= 0),
            "Invalid book context row counts"
        );
        // Prepare an indexed lookup to validate the table without scanning millions of entries.
        connection.prepare("SELECT payload FROM entries WHERE xgid=?1")?;
        let plies = |key: &str| -> Result<Option<i32>> {
            metadata
                .get(key)
                .map(|value| {
                    let value = value.parse::<i32>()?;
                    ensure!((0..=7).contains(&value), "Invalid evaluation ply metadata");
                    Ok(value)
                })
                .transpose()
        };
        let top_ply = plies("topPly")?;
        let cube_ply = plies("cubePly")?;
        let status = BookStatus {
            available: true,
            path: path.to_owned(),
            engine_version: Some(version.clone()),
            entries: contexts.iter().map(|context| context.entries).sum(),
            contexts,
            compression: Some(compression),
            license: metadata.get("license").cloned(),
            source_url: metadata.get("source").cloned(),
            attribution: Some(format!(
                "BackgammonDB opening book, generated with GNU Backgammon {version}"
            )),
            error: None,
        };
        Ok(Self {
            connection,
            status,
            dictionary,
            top_ply,
            cube_ply,
        })
    }
    fn get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        use rusqlite::{OptionalExtension, types::ValueRef};
        let payload = self
            .connection
            .query_row(
                "SELECT payload FROM entries WHERE xgid=?1",
                [key],
                |row| match row.get_ref(0)? {
                    ValueRef::Text(value) | ValueRef::Blob(value) => Ok(value.to_vec()),
                    _ => Err(rusqlite::Error::InvalidColumnType(
                        0,
                        "payload".into(),
                        row.get_ref(0)?.data_type(),
                    )),
                },
            )
            .optional()?;
        payload
            .map(|payload| {
                ensure!(
                    payload.len() <= MAX_PAYLOAD,
                    "Opening book payload is too large"
                );
                let bytes = if self.status.compression.as_deref() == Some("zlib") {
                    use flate2::{Decompress, FlushDecompress, Status};
                    let mut decoder = Decompress::new(true);
                    let mut output = vec![0; MAX_PAYLOAD];
                    let first = decoder.decompress(&payload, &mut output, FlushDecompress::Finish);
                    let state = match first {
                        Err(error) if error.needs_dictionary().is_some() => {
                            decoder.set_dictionary(
                                self.dictionary
                                    .as_deref()
                                    .context("Missing zlib dictionary")?,
                            )?;
                            decoder.decompress(
                                &payload[decoder.total_in() as usize..],
                                &mut output,
                                FlushDecompress::Finish,
                            )?
                        }
                        other => other?,
                    };
                    ensure!(
                        state == Status::StreamEnd && decoder.total_in() as usize == payload.len(),
                        "Invalid or oversized opening book payload"
                    );
                    output.truncate(decoder.total_out() as usize);
                    output
                } else {
                    payload
                };
                Ok(serde_json::from_slice(&bytes)?)
            })
            .transpose()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Probabilities {
    win: f64,
    win_gammon: f64,
    win_backgammon: f64,
    lose_gammon: f64,
    lose_backgammon: f64,
}
impl Probabilities {
    fn vector(&self) -> Result<Vec<f64>> {
        let values = vec![
            self.win,
            self.win_gammon,
            self.win_backgammon,
            1.0 - self.win,
            self.lose_gammon,
            self.lose_backgammon,
        ];
        ensure!(
            values
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
                && self.win_backgammon <= self.win_gammon
                && self.win_gammon <= self.win
                && self.lose_backgammon <= self.lose_gammon
                && self.lose_gammon <= 1.0 - self.win + 0.000001,
            "Invalid book probabilities"
        );
        Ok(values)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Play {
    r#move: Vec<[usize; 2]>,
    probabilities: Probabilities,
    cubeful_equity: f64,
}

fn parse_checker(payload: serde_json::Value, key: &str, plies: Option<i32>) -> Result<BookChecker> {
    #[derive(Deserialize)]
    struct Entry {
        plays: Vec<Play>,
    }
    let entry: Entry = serde_json::from_value(payload)?;
    ensure!(
        !entry.plays.is_empty() && entry.plays.len() <= 8,
        "Invalid book candidate count"
    );
    let mut candidates = Vec::new();
    for (index, play) in entry.plays.into_iter().enumerate() {
        ensure!(play.cubeful_equity.is_finite(), "Invalid book equity");
        let mut board: Vec<i32> = key
            .split(':')
            .next()
            .context("Missing board")?
            .bytes()
            .map(|byte| match byte {
                b'A'..=b'O' => i32::from(byte - b'A' + 1),
                b'a'..=b'o' => -i32::from(byte - b'a' + 1),
                _ => 0,
            })
            .collect();
        let mut notation = Vec::new();
        ensure!(
            !play.r#move.is_empty() && play.r#move.len() <= 4,
            "Invalid book move"
        );
        for [from, to] in play.r#move {
            ensure!(
                (1..=25).contains(&from) && to < from && board[from] > 0,
                "Invalid book move step"
            );
            let hit = to > 0 && board[to] == -1;
            board[from] -= 1;
            if to > 0 {
                ensure!(board[to] >= -1, "Book move is blocked");
                board[to] = board[to].max(0) + 1;
            }
            notation.push(format!(
                "{}/{}{}",
                if from == 25 {
                    "bar".into()
                } else {
                    from.to_string()
                },
                if to == 0 {
                    "off".into()
                } else {
                    to.to_string()
                },
                if hit { "*" } else { "" }
            ));
        }
        candidates.push(Candidate {
            rank: index as i32 + 1,
            notation: notation.join(" "),
            equity: play.cubeful_equity,
            probabilities: play.probabilities.vector()?,
        });
    }
    Ok(BookChecker { candidates, plies })
}
fn parse_cube(payload: serde_json::Value, plies: Option<i32>) -> Result<BookCube> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Equities {
        no_double: f64,
        double_take: f64,
        double_pass: f64,
    }
    #[derive(Deserialize)]
    struct Entry {
        probabilities: Probabilities,
        equities: Equities,
        verdict: String,
    }
    let entry: Entry = serde_json::from_value(payload)?;
    ensure!(
        matches!(
            entry.verdict.as_str(),
            "no-double" | "double-take" | "double-pass" | "too-good"
        ),
        "Invalid book cube verdict"
    );
    ensure!(
        [
            entry.equities.no_double,
            entry.equities.double_take,
            entry.equities.double_pass
        ]
        .iter()
        .all(|value| value.is_finite()),
        "Invalid book cube equity"
    );
    Ok(BookCube {
        probabilities: entry.probabilities.vector()?,
        equities: BookCubeEquities {
            no_double: entry.equities.no_double,
            double_take: entry.equities.double_take,
            double_pass: entry.equities.double_pass,
        },
        verdict: entry.verdict,
        plies,
    })
}

fn context_name(key: &str) -> Result<String> {
    let parts: Vec<_> = key.split(':').collect();
    let length = parts[8].parse::<i32>()?;
    if length == 0 {
        return Ok(if parts[7] == "1" { "money" } else { "money-nj" }.into());
    }
    let bottom = length - parts[5].parse::<i32>()?;
    let top = length - parts[6].parse::<i32>()?;
    Ok(format!(
        "{}a{}a{}",
        bottom.max(top),
        bottom.min(top),
        if parts[7] == "1" { "c" } else { "" }
    ))
}

pub(crate) fn canonical_keys(position: &Position) -> Result<(String, Option<String>)> {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    ensure!(matches!(position.player, 0 | 1), "Invalid player");
    ensure!(
        position.board.len() == 26 && position.board[0] <= 0 && position.board[25] >= 0,
        "Invalid board orientation"
    );
    ensure!(
        position.board.iter().all(|n| (-15..=15).contains(n)),
        "Invalid checker counts"
    );
    for side in [1, -1] {
        let count: i32 = position.board.iter().map(|n| (n * side).max(0)).sum();
        ensure!(
            (1..=15).contains(&count),
            "Unsupported board variant or finished game"
        );
    }
    let bytes = STANDARD_NO_PAD
        .decode(&position.gnubg_match_id)
        .context("Invalid GNUbg Match ID")?;
    ensure!(bytes.len() == 9, "GNUbg Match ID must encode 72 bits");
    let bits = |offset: usize, width: usize| -> i32 {
        (0..width)
            .map(|i| i32::from((bytes[(offset + i) / 8] >> ((offset + i) % 8)) & 1) << i)
            .sum()
    };
    ensure!(bits(67, 5) == 0, "Unsupported GNUbg Match ID extension");
    ensure!(
        bits(8, 3) == 1 && bits(12, 3) == 0,
        "Game is not an ordinary checker decision"
    );
    ensure!(
        bits(6, 1) == position.player && bits(11, 1) == position.player,
        "Mover does not match GNUbg Match ID"
    );
    let mut dice = [bits(15, 3), bits(18, 3)];
    ensure!(
        dice.iter().all(|die| (1..=6).contains(die)),
        "Checker dice are not rolled"
    );
    let mut stored_dice = position.dice.clone();
    stored_dice.sort_unstable();
    dice.sort_unstable();
    ensure!(stored_dice == dice, "Dice do not match GNUbg Match ID");
    let roll = format!("{}{}", dice[1], dice[0]);
    let mut length = bits(21, 15);
    let mut scores = [bits(36, 15), bits(51, 15)];
    let crawford = bits(7, 1) == 1;
    ensure!(
        !crawford
            || (length > 0 && scores.iter().filter(|score| **score == length - 1).count() == 1),
        "Invalid Crawford state"
    );
    if length > 0 {
        ensure!(
            scores.iter().all(|score| *score < length),
            "Match has already ended"
        );
        let away = [length - scores[0], length - scores[1]];
        ensure!(
            away.iter().all(|n| *n <= 25),
            "Away score exceeds released book coverage"
        );
        length = away[0].max(away[1]);
        scores = [length - away[0], length - away[1]];
    } else {
        scores = [0, 0];
    }
    let flags = if length > 0 {
        i32::from(crawford)
    } else {
        1 - bits(66, 1)
    };
    let owner = bits(4, 2);
    ensure!(matches!(owner, 0 | 1 | 3), "Invalid cube owner");
    let mut cube_exp = bits(0, 4);
    ensure!(
        !crawford || (cube_exp == 0 && owner == 3),
        "Cube is turned in Crawford game"
    );
    ensure!(cube_exp > 0 || owner == 3, "Owned cube at value one");
    if length == 0 && owner == 3 {
        cube_exp = 0;
    }
    // Check the redundant Position ID too; never silently normalize inconsistent imports.
    let mut packed = [0u8; 10];
    let mut bit = 0;
    for side in [1 - position.player, position.player] {
        for point in 1..=25 {
            let absolute = if side == 0 { point } else { 25 - point };
            let count = (position.board[absolute] * if side == 0 { 1 } else { -1 }).max(0);
            for _ in 0..count {
                ensure!(bit < 80, "Invalid board packing");
                packed[bit / 8] |= 1 << (bit % 8);
                bit += 1;
            }
            bit += 1;
        }
    }
    ensure!(
        STANDARD_NO_PAD.encode(packed) == position.position_id,
        "Board does not match GNUbg Position ID"
    );
    let mover = position.player as usize;
    let board = if mover == 0 {
        position.board.clone()
    } else {
        position.board.iter().rev().map(|n| -n).collect()
    };
    let board: String = board
        .iter()
        .map(|n| match *n {
            0 => '-',
            n if n > 0 => (b'A' + (n - 1) as u8) as char,
            n => (b'a' + (-n - 1) as u8) as char,
        })
        .collect();
    let xg_owner = if owner == 3 {
        0
    } else if owner == position.player {
        1
    } else {
        -1
    };
    let key = format!(
        "{board}:{cube_exp}:{xg_owner}:1:{roll}:{}:{}:{flags}:{length}:10",
        scores[mover],
        scores[1 - mover]
    );
    let cube = (!crawford && (owner == 3 || owner == position.player))
        .then(|| key.replacen(&format!(":{roll}:"), ":00:", 1));
    Ok((key, cube))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn position(sgf: &str) -> Position {
        let imported = crate::import::parse_sgf(sgf).unwrap();
        let p = &imported.positions[0];
        Position {
            id: Uuid::nil(),
            match_uuid: Uuid::nil(),
            ordinal: 1,
            game_no: p.game_no,
            ply: p.ply,
            player: p.player,
            dice: p.dice.to_vec(),
            board: p.board.clone(),
            position_id: p.position_id.clone(),
            gnubg_match_id: p.match_id.clone(),
            notation: p.notation.clone(),
        }
    }

    #[test]
    fn canonical_key_collapses_away_pair_and_orders_dice() {
        let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
        let (key, cube) = canonical_keys(&p).unwrap();
        assert_eq!(key, "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:0:3:10");
        assert_eq!(cube.unwrap(), key.replace(":31:", ":00:"));
    }

    #[test]
    fn canonical_key_mirrors_mover_scores_and_cube_owner() {
        let p = position("(;GM[6]MI[length:9][ws:2][bs:4]CV[2]CO[w];B[52lqqs])");
        let (key, cube) = canonical_keys(&p).unwrap();
        assert_eq!(key, "-b----E-C---eE---c-e----B-:1:-1:1:52:2:0:0:7:10");
        assert_eq!(cube, None, "Opponent owns cube, so the mover cannot double");
    }

    #[test]
    fn crawford_and_money_jacoby_have_distinct_keys() {
        let p = position("(;GM[6]MI[length:7][ws:6][bs:2]RU[Crawford:CrawfordGame];W[31hefe])");
        let (key, cube) = canonical_keys(&p).unwrap();
        assert_eq!(key, "-b----E-C---eE---c-e----B-:0:0:1:31:4:0:1:5:10");
        assert!(cube.is_none());
        let jacoby = position("(;GM[6]RU[Jacoby];W[31hefe])");
        let no_jacoby = position("(;GM[6];W[31hefe])");
        assert_eq!(
            canonical_keys(&jacoby).unwrap().0,
            "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:1:0:10"
        );
        assert_eq!(
            canonical_keys(&no_jacoby).unwrap().0,
            "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:0:0:10"
        );
    }

    #[test]
    fn contradictory_or_unsupported_context_is_rejected() {
        let mut p = position("(;GM[6]MI[length:7];W[31hefe])");
        p.player = 1;
        assert!(canonical_keys(&p).is_err());
        p.player = 0;
        p.dice = vec![6, 6];
        assert!(canonical_keys(&p).is_err());
        p.dice = vec![3, 1];
        p.board[25] = -1;
        assert!(canonical_keys(&p).is_err());
    }

    #[test]
    fn only_centered_money_cubes_are_collapsed() {
        let money = position("(;GM[6]CV[4]CO[c];W[31hefe])");
        assert_eq!(
            canonical_keys(&money).unwrap().0,
            "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:0:0:10"
        );
        let owned = position("(;GM[6]CV[4]CO[w];W[31hefe])");
        let (key, cube) = canonical_keys(&owned).unwrap();
        assert_eq!(key, "-b----E-C---eE---c-e----B-:2:1:1:31:0:0:0:0:10");
        assert!(cube.is_some());
        let match_play = position("(;GM[6]MI[length:7]CV[4]CO[c];W[31hefe])");
        assert_eq!(
            canonical_keys(&match_play).unwrap().0,
            "-b----E-C---eE---c-e----B-:2:0:1:31:0:0:0:7:10"
        );
    }

    #[test]
    fn invalid_scores_pending_cube_and_reserved_rules_are_rejected() {
        use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
        let source = position("(;GM[6]MI[length:7];W[31hefe])");
        for (offset, width, value) in [
            (12, 1, 1),
            (13, 2, 2),
            (67, 1, 1),
            (36, 15, 7),
            (7, 1, 1),
            (4, 2, 2),
        ] {
            let mut p = source.clone();
            let mut bytes = STANDARD_NO_PAD.decode(&p.gnubg_match_id).unwrap();
            for index in 0..width {
                bytes[(offset + index) / 8] &= !(1 << ((offset + index) % 8));
                bytes[(offset + index) / 8] |=
                    (((value >> index) & 1) as u8) << ((offset + index) % 8);
            }
            p.gnubg_match_id = STANDARD_NO_PAD.encode(bytes);
            assert!(
                canonical_keys(&p).is_err(),
                "offset {offset}, value {value}"
            );
        }
        let long = position("(;GM[6]MI[length:26];W[31hefe])");
        assert!(canonical_keys(&long).is_err());
    }

    fn book(compressed: bool) -> (tempfile::TempDir, BookConfig) {
        use flate2::{Compress, Compression, FlushCompress};
        use rusqlite::params;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.sqlite");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value); CREATE TABLE entries(xgid TEXT PRIMARY KEY,payload NOT NULL) WITHOUT ROWID; CREATE TABLE contexts(name TEXT PRIMARY KEY,entries INTEGER,depth INTEGER,complete INTEGER);").unwrap();
        for (key, value) in [
            ("formatVersion", "1"),
            ("engineVersion", "1.08.003"),
            ("compression", if compressed { "zlib" } else { "none" }),
            ("topPly", "3"),
            ("cubePly", "3"),
            ("license", "CC-BY-4.0"),
            ("source", "https://backgammondb.com"),
            (
                "credits",
                "BackgammonDB opening book, generated with GNU Backgammon 1.08.003",
            ),
        ] {
            connection
                .execute("INSERT INTO meta VALUES(?1,?2)", params![key, value])
                .unwrap();
        }
        let dict = br#"plays move probabilities win winGammon winBackgammon loseGammon loseBackgammon cubelessEquity cubefulEquity"#;
        if compressed {
            connection
                .execute("INSERT INTO meta VALUES('zdict',?1)", params![dict])
                .unwrap();
        }
        let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
        let checker_key = "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:0:3:10";
        let checker = serde_json::json!({"plays":[{"move":[[8,5],[6,5]],"probabilities":{"win":0.55,"winGammon":0.17,"winBackgammon":0.01,"loseGammon":0.12,"loseBackgammon":0.005},"cubelessEquity":0.159,"cubefulEquity":0.219}]});
        let cube = serde_json::json!({"probabilities":{"win":0.55,"winGammon":0.17,"winBackgammon":0.01,"loseGammon":0.12,"loseBackgammon":0.005},"equities":{"noDouble":0.2,"doubleTake":0.1,"doublePass":1.0},"verdict":"no-double"});
        for (key, payload) in [
            (checker_key.to_string(), checker),
            (checker_key.replace(":31:", ":00:"), cube),
        ] {
            if compressed {
                let mut encoder = Compress::new(Compression::best(), true);
                encoder.set_dictionary(dict).unwrap();
                let mut bytes = Vec::with_capacity(8192);
                encoder
                    .compress_vec(
                        payload.to_string().as_bytes(),
                        &mut bytes,
                        FlushCompress::Finish,
                    )
                    .unwrap();
                connection
                    .execute("INSERT INTO entries VALUES(?1,?2)", params![key, bytes])
                    .unwrap();
            } else {
                connection
                    .execute(
                        "INSERT INTO entries VALUES(?1,?2)",
                        params![key, payload.to_string()],
                    )
                    .unwrap();
            }
        }
        connection
            .execute("INSERT INTO contexts VALUES('3a3a',2,3,1)", [])
            .unwrap();
        drop(p);
        (
            dir,
            BookConfig {
                version: 0,
                provider: "backgammondb".into(),
                path: path.to_string_lossy().into_owned(),
                xg_path: crate::xg_book::default_path(),
            },
        )
    }

    #[tokio::test]
    async fn sqlite_lookup_reads_compressed_and_plain_candidates_and_cube() {
        for compressed in [false, true] {
            let (_dir, config) = book(compressed);
            let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
            let result = lookup(&config, &p, None).await.unwrap();
            assert!(result.hit, "{:?}", result.reason);
            let checker = result.checker.unwrap();
            assert_eq!(checker.plies, Some(3));
            assert_eq!(checker.candidates[0].notation, "8/5 6/5");
            assert_eq!(checker.candidates[0].equity, 0.219);
            assert_eq!(
                checker.candidates[0].probabilities,
                vec![0.55, 0.17, 0.01, 0.44999999999999996, 0.12, 0.005]
            );
            assert_eq!(result.cube.unwrap().verdict, "no-double");
            let provenance = result.provenance.unwrap();
            assert_eq!(provenance.evaluation_kind, "evaluation");
            assert_eq!(provenance.context_depth, Some(3));
            assert_eq!(provenance.engine, "GNU Backgammon 1.08.003");
            assert_eq!(
                serde_json::to_value(&provenance).unwrap()["equity_kind"],
                "normalized_cubeful"
            );
            let state = status(&config).await;
            assert!(state.available);
            assert_eq!(state.entries, 2);
        }
    }

    #[tokio::test]
    async fn off_missing_wrong_score_and_wrong_engine_return_misses() {
        let (_dir, mut config) = book(true);
        let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
        let wrong_engine = lookup(&config, &p, Some("GNU Backgammon 1.05.002  Oct  8 2025"))
            .await
            .unwrap();
        assert!(!wrong_engine.hit);
        assert!(wrong_engine.reason.unwrap().contains("version"));
        assert!(
            lookup(&config, &p, Some("GNU Backgammon 1.08.003"))
                .await
                .unwrap()
                .hit
        );
        let wrong_score = position("(;GM[6]MI[length:7][ws:3][bs:4];W[31hefe])");
        assert!(!lookup(&config, &wrong_score, None).await.unwrap().hit);
        config.provider = "off".into();
        assert!(!lookup(&config, &p, None).await.unwrap().hit);
        config.provider = "automatic".into();
        config.path.push_str(".missing");
        assert!(!lookup(&config, &p, None).await.unwrap().hit);
        assert!(!status(&config).await.available);
    }

    #[tokio::test]
    async fn unsupported_money_rules_never_match_bgdb_no_beaver_data() {
        let (_dir, config) = book(false);
        // A match-play book does not depend on the money-game beaver rule.
        let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
        let result = lookup_with_rules(&config, &p, None, Some(true))
            .await
            .unwrap();
        assert_eq!(result.provider, "backgammondb");
        assert!(result.hit);
        let source = position("(;GM[6]RU[Jacoby];W[31hefe])");
        // Put an exact money checker key into the real SQLite fixture.
        let connection = rusqlite::Connection::open(&config.path).unwrap();
        connection
            .execute(
                "UPDATE entries SET xgid=replace(xgid,':0:0:0:3:10',':0:0:1:0:10')",
                [],
            )
            .unwrap();
        assert!(
            lookup_with_rules(&config, &source, None, Some(false))
                .await
                .unwrap()
                .hit
        );
        assert!(
            !lookup_with_rules(&config, &source, None, Some(true))
                .await
                .unwrap()
                .hit
        );
        assert!(
            !lookup_with_rules(&config, &source, None, None)
                .await
                .unwrap()
                .hit
        );
    }

    #[tokio::test]
    async fn installs_valid_book_atomically_and_rejects_invalid_source() {
        let (_source, config) = book(true);
        let dest = tempfile::tempdir().unwrap();
        let destination = BookConfig {
            version: 0,
            xg_path: crate::xg_book::default_path(),
            provider: "automatic".into(),
            path: dest
                .path()
                .join("book.sqlite")
                .to_string_lossy()
                .into_owned(),
        };
        let installed = install_from_path(&destination, &config.path).await.unwrap();
        assert!(installed.available);
        let invalid = dest.path().join("invalid");
        std::fs::write(&invalid, b"not sqlite").unwrap();
        assert!(
            install_from_path(&destination, invalid.to_str().unwrap())
                .await
                .is_err()
        );
        assert!(status(&destination).await.available);
    }

    #[tokio::test]
    #[ignore = "requires installed official BackgammonDB SQLite"]
    async fn released_full_book_serves_real_scores_both_movers_and_cube() {
        let config = BookConfig {
            version: 0,
            provider: "backgammondb".into(),
            path: std::env::var("BGTUTOR_OPENING_BOOK").unwrap_or_else(|_| default_path()),
            xg_path: crate::xg_book::default_path(),
        };
        let state = status(&config).await;
        assert!(state.available, "{:?}", state.error);
        assert_eq!(state.entries, 4_230_443);
        assert_eq!(state.contexts.len(), 351);
        for length in [0, 1, 2, 3, 5, 7, 9, 15, 25] {
            for (mover, movement) in [("W", "hefe"), ("B", "qtst")] {
                let p = position(&format!(
                    "(;GM[6]MI[length:{length}];{mover}[31{movement}])"
                ));
                let result = lookup(&config, &p, None).await.unwrap();
                assert!(
                    result.hit,
                    "length={length}, mover={mover}: {:?}",
                    result.reason
                );
                assert_eq!(result.checker.unwrap().candidates[0].notation, "8/5 6/5");
                assert!(
                    result.cube.is_none(),
                    "The initial opening roll has no cube decision"
                );
            }
        }
        for metadata in [
            "MI[length:7][ws:4][bs:4]",
            "MI[length:9][ws:2][bs:4]",
            "MI[length:7][ws:6][bs:2]RU[Crawford:CrawfordGame]",
            "RU[Jacoby]",
        ] {
            let p = position(&format!("(;GM[6]{metadata};W[31hefe])"));
            assert!(lookup(&config, &p, None).await.unwrap().hit, "{metadata}");
        }
        let source = crate::import::parse_sgf("(;GM[6]MI[length:7];W[31hefe];B[42qusu])").unwrap();
        let p = &source.positions[1];
        let reply = Position {
            id: Uuid::nil(),
            match_uuid: Uuid::nil(),
            ordinal: 2,
            game_no: p.game_no,
            ply: p.ply,
            player: p.player,
            dice: p.dice.to_vec(),
            board: p.board.clone(),
            position_id: p.position_id.clone(),
            gnubg_match_id: p.match_id.clone(),
            notation: p.notation.clone(),
        };
        let result = lookup(&config, &reply, None).await.unwrap();
        assert!(result.checker.is_some(), "{:?}", result.reason);
        assert!(result.cube.is_some());
        assert_eq!(result.provenance.unwrap().evaluation_kind, "evaluation");
        assert!(
            !lookup(&config, &reply, Some("GNU Backgammon 1.05.002"))
                .await
                .unwrap()
                .hit
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn settings_routes_preserve_cas_and_position_lookup(pool: sqlx::PgPool) {
        use axum::{body::Body, http::Request};
        use http_body_util::BodyExt;
        use serde_json::{Value, json};
        use tower::ServiceExt;
        async fn request(app: Router, method: &str, url: &str, body: Value) -> (u16, Value) {
            let response = app
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(url)
                        .header("content-type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status().as_u16();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice(&bytes).unwrap())
        }
        let app = routes().with_state(AppState::new(
            pool.clone(),
            crate::engine::Engine::new("/missing/gnubg".into()),
        ));
        let (_, settings) = request(app.clone(), "GET", "/api/opening-book", Value::Null).await;
        assert_eq!(settings["config"]["provider"], "automatic");
        let (_dir, fixture) = book(true);
        let config = json!({"version":0,"provider":"backgammondb","path":fixture.path});
        let (status, saved) =
            request(app.clone(), "PUT", "/api/opening-book", config.clone()).await;
        assert_eq!(status, 200, "{saved}");
        assert_eq!(saved["config"]["version"], 1);
        assert_eq!(saved["status"]["entries"], 2);
        assert_eq!(
            request(app.clone(), "PUT", "/api/opening-book", config)
                .await
                .0,
            409
        );
        assert_eq!(
            request(
                app.clone(),
                "PUT",
                "/api/opening-book",
                json!({"version":1,"provider":"unsupported","path":"/book"})
            )
            .await
            .0,
            422
        );
        sqlx::query(
            "UPDATE opening_book_settings SET xg_path='/custom/XG.ob' WHERE singleton=true",
        )
        .execute(&pool)
        .await
        .unwrap();
        let (code, preserved) = request(
            app.clone(),
            "PUT",
            "/api/opening-book",
            json!({"version":1,"provider":"backgammondb","path":fixture.path}),
        )
        .await;
        assert_eq!(code, 200, "{preserved}");
        assert_eq!(
            preserved["config"]["xg_path"], "/custom/XG.ob",
            "Old clients must preserve a custom XG path"
        );
        let sgf = "(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])";
        let imported = crate::store::import_match(
            &pool,
            "book.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &crate::import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let positions = crate::store::positions(&pool, Some(imported.r#match.id), None, None, 10)
            .await
            .unwrap();
        let (status, result) = request(
            app.clone(),
            "GET",
            &format!("/api/positions/{}/opening-book", positions.items[0].id),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["checker"]["candidates"][0]["notation"], "8/5 6/5");
        assert_eq!(result["provenance"]["engine"], "GNU Backgammon 1.08.003");
        assert_eq!(
            request(
                app,
                "GET",
                &format!("/api/positions/{}/opening-book", Uuid::new_v4()),
                Value::Null
            )
            .await
            .0,
            404
        );
    }

    #[tokio::test]
    #[ignore = "requires official installed XG opening book"]
    async fn xg_provider_and_automatic_keep_references_separate_from_candidates() {
        let source = crate::import::parse_sgf("(;GM[6]MI[length:7];W[31hefe];B[42qusu])").unwrap();
        let p = &source.positions[1];
        let position = Position {
            id: Uuid::nil(),
            match_uuid: Uuid::nil(),
            ordinal: 1,
            game_no: p.game_no,
            ply: p.ply,
            player: p.player,
            dice: p.dice.to_vec(),
            board: p.board.clone(),
            position_id: p.position_id.clone(),
            gnubg_match_id: p.match_id.clone(),
            notation: p.notation.clone(),
        };
        let mut config = BookConfig {
            version: 0,
            provider: "xg".into(),
            path: default_path(),
            xg_path: crate::xg_book::default_path(),
        };
        let result = lookup(&config, &position, None).await.unwrap();
        assert!(result.hit, "{:?}", result.reason);
        assert_eq!(result.provider, "xg");
        assert!(result.checker.is_none() && result.cube.is_none() && result.provenance.is_none());
        assert!(result.xg_position.is_some());
        assert!(
            !lookup(&config, &position, Some("GNU Backgammon 1.08.003"))
                .await
                .unwrap()
                .hit,
            "An XG reference cannot fulfill a GNUbg cache request"
        );
        config.provider = "automatic".into();
        let result = lookup(&config, &position, None).await.unwrap();
        assert!(result.checker.is_some() && result.xg_position.is_some());
        let money_source =
            crate::import::parse_sgf("(;GM[6]RU[Jacoby];W[31hefe];B[42qusu])").unwrap();
        let p = &money_source.positions[1];
        let money = Position {
            id: Uuid::nil(),
            match_uuid: Uuid::nil(),
            ordinal: 1,
            game_no: p.game_no,
            ply: p.ply,
            player: p.player,
            dice: p.dice.to_vec(),
            board: p.board.clone(),
            position_id: p.position_id.clone(),
            gnubg_match_id: p.match_id.clone(),
            notation: p.notation.clone(),
        };
        let beavers = lookup_with_rules(&config, &money, None, Some(true))
            .await
            .unwrap();
        assert!(beavers.xg_position.is_some());
        assert!(
            beavers.checker.is_none() && beavers.cube.is_none(),
            "BGDB cannot answer the beaver-rule context"
        );
        assert!(
            !lookup_with_rules(&config, &money, None, None)
                .await
                .unwrap()
                .hit
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn source_money_rules_are_explicit_and_unknown_mat_rules_miss(pool: sqlx::PgPool) {
        let sgf = "(;GM[6]RU[Jacoby];W[31hefe])";
        let parsed = crate::import::parse_sgf(sgf).unwrap();
        for (format, expected) in [("sgf", Some(false)), ("bgf", Some(false)), ("mat", None)] {
            // Separate content hashes produce independent source rows.
            let content = format!("{format}:{sgf}");
            let imported = crate::store::import_match(
                &pool,
                "rules",
                format,
                content.as_bytes(),
                sgf,
                &parsed,
            )
            .await
            .unwrap();
            let page = crate::store::positions(&pool, Some(imported.r#match.id), None, None, 10)
                .await
                .unwrap();
            assert_eq!(
                source_beaver_rule(&pool, &page.items[0]).await.unwrap(),
                expected
            );
        }
    }
}
