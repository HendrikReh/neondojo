//! Portable, data-only backups of the application's PostgreSQL tables.
mod archive;
mod restore;

use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use crate::json_errors::{Json, Query};
use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use std::{sync::Arc, time::Duration};
use tokio::{
    fs::File,
    sync::{OwnedRwLockReadGuard, OwnedSemaphorePermit, RwLock, Semaphore},
};
use utoipa::{
    IntoParams, OpenApi, PartialSchema, ToSchema,
    openapi::{KnownFormat, Object, RefOr, Schema, SchemaFormat, Type},
};

const MEDIA_TYPE: &str = "application/vnd.neondojo.backup";
const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DATA_BYTES: u64 = 64 * 1024 * 1024 * 1024;

/// Derived from the backed-up tables and rebuilt on demand. A restore clears
/// them with the data they describe; they are never written to an archive.
const DERIVED_TABLES: &[&str] = &[
    "decision_index",
    "decision_index_matches",
    "native_analysis_revisions",
    "imported_analysis_revisions",
    "review_hits",
    "review_hit_sets",
];
// These migrations leave the backed-up table layout and format compatible
// with earlier archives. They only affect derived data, notifications,
// indexes, views or how long job progress events are kept (0094).
const ARCHIVE_COMPATIBLE_MIGRATIONS: &[i64] = &[89, 90, 91, 92, 93, 94];

// Parent tables precede their children so foreign keys remain enforced. Host
// installations and the benchmark machine identity belong to the destination.
const TABLES: &[&str] = &[
    "app_settings",
    "opening_book_settings",
    "configuration_profiles",
    "configuration_state",
    "databases",
    "player_profiles",
    "player_aliases",
    "virtual_player_profiles",
    "virtual_player_members",
    "matches",
    "analysis_selections",
    "match_players",
    "match_games",
    "positions",
    "match_moves",
    "annotations",
    "position_game_plans",
    "saved_positions",
    "position_filters",
    "imported_analyses",
    "imported_analysis_projections",
    "decks",
    "learning_preferences",
    "mistake_practice_attempts",
    "learning_cards",
    "anki_media",
    "anki_card_faces",
    "anki_source_decks",
    "learning_card_game_plans",
    "deck_cards",
    "learning_card_exports",
    "score_practice_profiles",
    "review_sessions",
    "card_reviews",
    "anki_received_reviews",
    "anki_review_exports",
    "anki_auto_sync",
    "anki_sync_history",
    "card_practice_rounds",
    "card_practice_items",
    "card_topics",
    "transfer_practice_seen",
    "benchmark_runs",
    "jobs",
    "job_events",
    "match_analysis_games",
    "move_analyses",
    "match_analysis_timing",
    "play_sessions",
    "study_report_snapshots",
    "match_assessments",
    "similar_practice_summaries",
    "engine_match_runs",
    "engine_match_results",
];

/// How long a restore waits for background work it cannot stop (see
/// [`BackgroundWork`]) to finish before it answers that the database is busy.
/// It only covers work between two short polls; no API request waits for it.
const RESTORE_PREFLIGHT: Duration = Duration::from_secs(2);
/// How long a restore waits for running requests and the analysis jobs it
/// stopped to release the database. New API requests get `503` meanwhile.
const RESTORE_DRAIN: Duration = Duration::from_secs(30);

/// Long-running background work that holds database access and that a restore
/// does not stop. A restore refuses to start while any of it runs, instead of
/// blocking every API request until it finishes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackgroundWork {
    Benchmark,
    EngineDuel,
    AiAssessment,
    AnkiSync,
}

impl BackgroundWork {
    fn label(self) -> &'static str {
        match self {
            Self::Benchmark => "benchmark",
            Self::EngineDuel => "engine duel",
            Self::AiAssessment => "AI assessment",
            Self::AnkiSync => "Anki sync",
        }
    }
}

pub(crate) struct Access {
    /// Shared so background tasks can hold an owned read guard while they run.
    pub(crate) requests: Arc<RwLock<()>>,
    transfer: Arc<Semaphore>,
    background: Arc<Background>,
    preflight: Duration,
    drain: Duration,
}

/// Admission of background work and analysis jobs. The same mutex closes it
/// for a restore and registers new holders, so no holder starts unseen.
#[derive(Default)]
struct Background {
    state: std::sync::Mutex<BackgroundState>,
    released: tokio::sync::Notify,
}

#[derive(Default)]
struct BackgroundState {
    restoring: bool,
    active: Vec<BackgroundWork>,
}

impl Background {
    fn lock(&self) -> std::sync::MutexGuard<'_, BackgroundState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Database access of one piece of [`BackgroundWork`] while it runs.
pub(crate) struct BackgroundAccess {
    _access: OwnedRwLockReadGuard<()>,
    work: BackgroundWork,
    background: Arc<Background>,
}

impl Drop for BackgroundAccess {
    fn drop(&mut self) {
        let mut state = self.background.lock();
        if let Some(index) = state.active.iter().position(|&work| work == self.work) {
            state.active.swap_remove(index);
        }
        drop(state);
        self.background.released.notify_waiters();
    }
}

/// Keeps new background work and analysis jobs out until a restore finishes
/// or fails.
struct RestoreAdmission(Arc<Background>);

impl Drop for RestoreAdmission {
    fn drop(&mut self) {
        self.0.lock().restoring = false;
    }
}

impl Default for Access {
    fn default() -> Self {
        Self::with_waits(RESTORE_PREFLIGHT, RESTORE_DRAIN)
    }
}

impl Access {
    pub(crate) fn with_waits(preflight: Duration, drain: Duration) -> Self {
        Self {
            requests: Arc::new(RwLock::new(())),
            transfer: Arc::new(Semaphore::new(1)),
            background: Arc::default(),
            preflight,
            drain,
        }
    }

    /// Database access for background work, or `None` while the database is
    /// being restored or prepared for a restore.
    pub(crate) fn try_background(&self, work: BackgroundWork) -> Option<BackgroundAccess> {
        let mut state = self.background.lock();
        if state.restoring {
            return None;
        }
        let access = self.requests.clone().try_read_owned().ok()?;
        state.active.push(work);
        Some(BackgroundAccess {
            _access: access,
            work,
            background: self.background.clone(),
        })
    }

    /// Database access for an analysis job, which a restore stops, or `None`
    /// while the database is being restored or prepared for a restore.
    pub(crate) fn try_job(&self) -> Option<OwnedRwLockReadGuard<()>> {
        let state = self.background.lock();
        if state.restoring {
            return None;
        }
        self.requests.clone().try_read_owned().ok()
    }

    /// Close admission for a restore once no background work runs. Work that
    /// is still running after the preflight wait is returned as the blocker.
    async fn close_for_restore(&self) -> Result<RestoreAdmission, Vec<BackgroundWork>> {
        self.background.lock().restoring = true;
        let admission = RestoreAdmission(self.background.clone());
        let deadline = tokio::time::Instant::now() + self.preflight;
        loop {
            let released = self.background.released.notified();
            let mut released = std::pin::pin!(released);
            released.as_mut().enable();
            let active = self.background.lock().active.clone();
            if active.is_empty() {
                return Ok(admission);
            }
            if tokio::time::timeout_at(deadline, released).await.is_err() {
                let mut blockers = self.background.lock().active.clone();
                if blockers.is_empty() {
                    return Ok(admission);
                }
                blockers.sort_by_key(|work| work.label());
                blockers.dedup();
                return Err(blockers);
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    schema: String,
    sequences: Vec<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct Sequence {
    name: String,
    table_name: String,
    column_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchiveLayout {
    Current,
    BeforeGameCubeRules,
}

#[derive(Debug, PartialEq, Eq)]
struct ArchiveSchema {
    current: String,
    before_game_cube_rules: Option<String>,
}

impl ArchiveSchema {
    fn layout(&self, fingerprint: &str) -> Option<ArchiveLayout> {
        if fingerprint == self.current {
            Some(ArchiveLayout::Current)
        } else if self.before_game_cube_rules.as_deref() == Some(fingerprint) {
            Some(ArchiveLayout::BeforeGameCubeRules)
        } else {
            None
        }
    }
}

async fn schema(connection: &mut PgConnection) -> Result<ArchiveSchema> {
    let migrations: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version, checksum FROM public._sqlx_migrations WHERE success ORDER BY version",
    )
    .fetch_all(connection)
    .await?;
    let mut digest = Sha256::new();
    let mut before_game_cube_rules = None;
    for (version, checksum) in crate::schema_migrations::expanded_history(migrations) {
        if ARCHIVE_COMPATIBLE_MIGRATIONS.contains(&version) {
            continue;
        }
        // This adapter covers exactly one layout transition. A later change
        // to a backed-up table invalidates it until compatibility is reviewed.
        before_game_cube_rules = (version == 95
            && crate::schema_migrations::MIGRATOR
                .iter()
                .any(|m| m.version == version && m.checksum.as_ref() == checksum))
        .then(|| format!("{:x}", digest.clone().finalize()));
        digest.update(version.to_be_bytes());
        digest.update(checksum);
    }
    Ok(ArchiveSchema {
        current: format!("{:x}", digest.finalize()),
        before_game_cube_rules,
    })
}

fn identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn relation(name: &str) -> String {
    format!("public.{}", identifier(name))
}

async fn columns(
    connection: &mut PgConnection,
    table: &str,
    layout: ArchiveLayout,
) -> Result<String> {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT attname::text FROM pg_attribute
         WHERE attrelid=$1::regclass AND attnum>0 AND NOT attisdropped AND attgenerated=''
         ORDER BY attnum",
    )
    .bind(relation(table))
    .fetch_all(connection)
    .await?;
    anyhow::ensure!(!columns.is_empty(), "Backup table has no columns");
    let legacy_games = layout == ArchiveLayout::BeforeGameCubeRules && table == "match_games";
    Ok(columns
        .iter()
        .filter(|name| !legacy_games || name.as_str() != "cube_enabled")
        .map(|name| identifier(name))
        .collect::<Vec<_>>()
        .join(","))
}

async fn sequences(connection: &mut PgConnection) -> Result<Vec<Sequence>> {
    Ok(sqlx::query_as(
        "SELECT s.relname::text AS name, t.relname::text AS table_name, a.attname::text AS column_name
         FROM pg_depend d
         JOIN pg_class s ON s.oid=d.objid AND s.relkind='S'
         JOIN pg_class t ON t.oid=d.refobjid
         JOIN pg_namespace n ON n.oid=t.relnamespace AND n.nspname='public' AND n.oid=s.relnamespace
         JOIN pg_attribute a ON a.attrelid=t.oid AND a.attnum=d.refobjsubid
         WHERE d.classid='pg_class'::regclass AND d.refclassid='pg_class'::regclass
           AND d.deptype IN ('a','i') AND t.relname::text=ANY($1)
         ORDER BY s.relname::text COLLATE \"C\"",
    )
    .bind(TABLES)
    .fetch_all(connection)
    .await?)
}

async fn transfer_settings(connection: &mut PgConnection) -> Result<()> {
    for statement in [
        "SET LOCAL DateStyle TO 'ISO, YMD'",
        "SET LOCAL IntervalStyle TO postgres",
        "SET LOCAL TimeZone TO 'UTC'",
        "SET LOCAL bytea_output TO hex",
        "SET LOCAL extra_float_digits TO 3",
        "SET LOCAL search_path TO pg_catalog, public",
    ] {
        sqlx::query(statement).execute(&mut *connection).await?;
    }
    Ok(())
}

fn busy() -> ApiError {
    ApiError(
        StatusCode::CONFLICT,
        "Database backup or restore is already in progress".into(),
    )
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/database-backup", get(export).post(import))
        .route("/api/database-backup/check", post(check))
}

// The archive is the raw request/response body.
struct BackupFile;
impl PartialSchema for BackupFile {
    fn schema() -> RefOr<Schema> {
        Object::builder()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
            .into()
    }
}
impl ToSchema for BackupFile {}

#[utoipa::path(get, path="/api/database-backup", operation_id="export_database_backup", responses(
    (status=200, body=inline(BackupFile), content_type="application/vnd.neondojo.backup"),
    (status=409, body=ErrorBody), (status=500, body=ErrorBody)
), tag="Database backup")]
async fn export(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let permit = state
        .backup
        .transfer
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    let created_at = Utc::now();
    let (file, length) = archive::export(&state.pool, created_at).await?;
    let filename = format!(
        "neondojo-{}.neondojo-backup",
        created_at.format("%Y-%m-%d-%H%M%S")
    );
    let content = tokio_util::io::ReaderStream::with_capacity(file, 128 * 1024);
    let content =
        futures_util::stream::unfold((content, permit), |(mut stream, permit)| async move {
            use futures_util::StreamExt;
            stream.next().await.map(|chunk| (chunk, (stream, permit)))
        });
    Ok((
        [
            (header::CONTENT_TYPE, MEDIA_TYPE.to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
            (header::CONTENT_LENGTH, length.to_string()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
        ],
        Body::from_stream(content),
    ))
}

fn schema_mismatch() -> ApiError {
    ApiError::invalid("Backup schema differs from this NeonDojo installation")
}

fn validate_sequences(manifest: &Manifest, count: usize) -> ApiResult<()> {
    if manifest.sequences.len() != count
        || manifest
            .sequences
            .iter()
            .any(|&value| !(1..i64::MAX).contains(&value))
    {
        return Err(ApiError::invalid(
            "Backup contains data that cannot be restored",
        ));
    }
    Ok(())
}

async fn receive_backup(
    state: &AppState,
    headers: &HeaderMap,
    body: Body,
) -> ApiResult<(File, Arc<OwnedSemaphorePermit>)> {
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        != Some(MEDIA_TYPE)
    {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Select a NeonDojo backup file".into(),
        ));
    }
    let permit = state
        .backup
        .transfer
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    let file = archive::receive(headers, body).await?;
    Ok((file, Arc::new(permit)))
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum BackupCompatibility {
    Compatible,
    IncompatibleSchema,
}

#[derive(Serialize, ToSchema)]
struct BackupCheck {
    status: BackupCompatibility,
    #[serde(skip_serializing_if = "Option::is_none")]
    contents: Option<BackupContents>,
}

#[derive(Serialize, ToSchema)]
struct BackupContents {
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<DateTime<Utc>>,
    matches: u64,
    saved_positions: u64,
    learning_cards: u64,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
struct BackupTargetCounts {
    matches: i64,
    saved_positions: i64,
    learning_cards: i64,
}

#[utoipa::path(get, path="/api/database-backup/target-counts", operation_id="database_backup_target_counts",
    responses((status=200, body=BackupTargetCounts), (status=503, body=ErrorBody),
              (status=500, body=ErrorBody)),
    tag="Database backup"
)]
pub(crate) async fn target_counts(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    // One statement keeps all three counts on the same PostgreSQL snapshot.
    let counts: BackupTargetCounts = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM matches WHERE NOT collection_only) AS matches,
            (SELECT count(*) FROM saved_positions) AS saved_positions,
            (SELECT count(*) FROM learning_cards) AS learning_cards",
    )
    .fetch_one(&state.pool)
    .await
    .context("Counting backup destination contents")?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(counts)))
}

#[utoipa::path(post, path="/api/database-backup/check", operation_id="check_database_backup",
    request_body(content=inline(BackupFile), content_type="application/vnd.neondojo.backup"),
    responses((status=200, body=BackupCheck), (status=408, body=ErrorBody), (status=409, body=ErrorBody),
              (status=413, body=ErrorBody), (status=415, body=ErrorBody),
              (status=422, body=ErrorBody), (status=500, body=ErrorBody)),
    tag="Database backup"
)]
async fn check(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Json<BackupCheck>> {
    let (file, permit) = receive_backup(&state, &headers, body).await?;
    let mut connection = state
        .pool
        .acquire()
        .await
        .context("Accessing backup schema")?;
    let fingerprint = schema(&mut connection).await?;
    let sequence_count = sequences(&mut connection).await?.len();
    drop(connection);
    let checked = archive::check(file, fingerprint, permit).await?;
    let status = if checked.compatible {
        validate_sequences(&checked.manifest, sequence_count)?;
        BackupCompatibility::Compatible
    } else {
        BackupCompatibility::IncompatibleSchema
    };
    Ok(Json(BackupCheck {
        status,
        contents: checked.contents,
    }))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
struct RestoreQuery {
    /// Explicitly replace the target database's application data.
    replace: bool,
}

#[utoipa::path(post, path="/api/database-backup", operation_id="restore_database_backup", params(RestoreQuery),
    request_body(content=inline(BackupFile), content_type="application/vnd.neondojo.backup"),
    responses((status=204), (status=408, body=ErrorBody), (status=409, body=ErrorBody), (status=413, body=ErrorBody),
              (status=415, body=ErrorBody), (status=422, body=ErrorBody), (status=500, body=ErrorBody)),
    tag="Database backup"
)]
async fn import(
    State(state): State<AppState>,
    Query(query): Query<RestoreQuery>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<StatusCode> {
    if !query.replace {
        return Err(ApiError::invalid(
            "Replacing the target database must be confirmed",
        ));
    }
    let (file, permit) = receive_backup(&state, &headers, body).await?;
    let mut connection = state
        .pool
        .acquire()
        .await
        .context("Accessing backup schema")?;
    let fingerprint = schema(&mut connection).await?;
    drop(connection);
    let input = archive::prepare(file, fingerprint, permit.clone()).await?;

    // Benchmarks, engine duels, AI assessments and Anki syncs are not stopped.
    // Refuse at once while one runs: waiting for the write lock below would
    // turn every API request away with 503 until the work finished.
    let _admission = state.backup.close_for_restore().await.map_err(|blockers| {
        let running = blockers
            .iter()
            .map(|work| work.label())
            .collect::<Vec<_>>()
            .join(", ");
        ApiError(
            StatusCode::CONFLICT,
            format!(
                "Database is busy; retry the restore when current work finishes ({running} running)"
            )
            .into(),
        )
    })?;
    // Analysis jobs can hold their read access for hours, and the restore
    // replaces their data anyway: stop them instead of waiting. The drain also
    // stops a job claimed just before admission closed once it registers.
    let drain = state
        .running_jobs
        .drain(crate::jobs::RESTORE_INTERRUPTED_ERROR);
    if drain.interrupted > 0 {
        tracing::info!(
            interrupted = drain.interrupted,
            "Database restore stops running analysis jobs"
        );
    }
    // Drain requests already using the destination and reject new ones until
    // the transaction finishes. The table locks also cover other DB clients.
    let _maintenance = tokio::time::timeout(state.backup.drain, state.backup.requests.write())
        .await
        .map_err(|_| {
            ApiError(
                StatusCode::CONFLICT,
                "Database is busy; retry the restore when current work finishes".into(),
            )
        })?;
    restore::replace(&state, input).await?;
    drop(drain);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(OpenApi)]
#[openapi(paths(export, check, import, target_counts))]
pub(crate) struct BackupDoc;

#[cfg(test)]
mod tests {
    use super::TABLES;

    #[sqlx::test(migrations = "./migrations")]
    async fn derived_and_notification_migrations_keep_backup_fingerprint(pool: sqlx::PgPool) {
        let mut tx = pool.begin().await.unwrap();
        let after = super::schema(&mut tx).await.unwrap();
        let removed = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ANY($1)")
            .bind(super::ARCHIVE_COMPATIBLE_MIGRATIONS)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            removed.rows_affected(),
            super::ARCHIVE_COMPATIBLE_MIGRATIONS.len() as u64
        );
        let before = super::schema(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(before, after);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn restore_refuses_at_once_while_unstoppable_work_runs(pool: sqlx::PgPool) {
        use super::BackgroundWork;
        use crate::api::AppState;
        use axum::{
            body::{Body, to_bytes},
            http::{Request, StatusCode, header},
        };
        use std::time::{Duration, Instant};
        use tower::ServiceExt;
        let state = AppState::new(pool, crate::engine::Engine::new("/unused".into()))
            .with_restore_waits(Duration::from_millis(300), Duration::from_secs(60));
        let app = crate::api::router(state.clone());
        let archive = app
            .clone()
            .oneshot(
                Request::get("/api/database-backup")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(archive.status(), StatusCode::OK);
        let archive = to_bytes(archive.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap();
        let restore = |archive: axum::body::Bytes| {
            app.clone().oneshot(
                Request::post("/api/database-backup?replace=true")
                    .header(header::CONTENT_TYPE, "application/vnd.neondojo.backup")
                    .body(Body::from(archive))
                    .unwrap(),
            )
        };
        let duel = state
            .backup
            .try_background(BackgroundWork::EngineDuel)
            .expect("database access");
        // A polling worker that releases its access during the preflight wait
        // does not block the restore's decision.
        let poll = state
            .backup
            .try_background(BackgroundWork::AiAssessment)
            .expect("database access");
        let started = Instant::now();
        let pending = tokio::spawn(restore(archive.clone()));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !state.backup.background.lock().restoring {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the restore must close admission");
        drop(poll);
        // Neither new background work nor analysis jobs start meanwhile, but
        // API requests are still served.
        assert!(
            state
                .backup
                .try_background(BackgroundWork::Benchmark)
                .is_none()
        );
        assert!(state.backup.try_job().is_none());
        let version = app
            .clone()
            .oneshot(Request::get("/api/version").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(version.status(), StatusCode::OK);
        let response = tokio::time::timeout(Duration::from_secs(5), pending)
            .await
            .expect("the restore must not wait for the running duel")
            .unwrap()
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(
            body["error"],
            "Database is busy; retry the restore when current work finishes (engine duel running)"
        );
        // The refused restore reopens admission and leaves the API available.
        let benchmark = state
            .backup
            .try_background(BackgroundWork::Benchmark)
            .expect("admission reopens after the refused restore");
        drop((duel, benchmark));
        let response = restore(archive).await.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[test]
    fn every_migrated_table_has_an_explicit_backup_policy() {
        let migrator = sqlx::migrate!("./migrations");
        let table_name =
            regex::Regex::new(r"(?im)^CREATE TABLE (?:public\.)?([a-z_]+)\s*\(").unwrap();
        let actual = migrator
            .iter()
            .flat_map(|migration| table_name.captures_iter(&migration.sql))
            .map(|capture| capture[1].to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        let mut covered = TABLES
            .iter()
            .map(|table| (*table).to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(covered.len(), TABLES.len());
        covered.extend(["engine_installations".into(), "benchmark_machine".into()]);
        covered.extend(
            super::DERIVED_TABLES
                .iter()
                .map(|table| (*table).to_owned()),
        );
        assert_eq!(actual, covered);
    }
}
