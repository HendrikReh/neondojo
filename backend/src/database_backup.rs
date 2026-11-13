//! Portable, data-only backups of the application's PostgreSQL tables.
mod archive;
mod restore;

use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use tokio::sync::{RwLock, Semaphore};
use utoipa::{
    IntoParams, OpenApi, PartialSchema, ToSchema,
    openapi::{KnownFormat, Object, RefOr, Schema, SchemaFormat, Type},
};

const MEDIA_TYPE: &str = "application/vnd.bgtutor.backup";
const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DATA_BYTES: u64 = 64 * 1024 * 1024 * 1024;

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
    "matches",
    "match_players",
    "match_games",
    "positions",
    "match_moves",
    "annotations",
    "position_game_plans",
    "saved_positions",
    "position_filters",
    "imported_analyses",
    "decks",
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
    "similar_practice_summaries",
    "engine_match_runs",
    "engine_match_results",
];

pub(crate) struct Access {
    pub(crate) requests: RwLock<()>,
    transfer: std::sync::Arc<Semaphore>,
}

impl Default for Access {
    fn default() -> Self {
        Self {
            requests: RwLock::new(()),
            transfer: std::sync::Arc::new(Semaphore::new(1)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    schema: String,
    sequences: Vec<i64>,
}

#[derive(sqlx::FromRow)]
struct Sequence {
    name: String,
    table_name: String,
    column_name: String,
}

async fn schema(connection: &mut PgConnection) -> Result<String> {
    let migrations: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version, checksum FROM public._sqlx_migrations WHERE success ORDER BY version",
    )
    .fetch_all(connection)
    .await?;
    let mut digest = Sha256::new();
    for (version, checksum) in migrations {
        digest.update(version.to_be_bytes());
        digest.update(checksum);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn relation(name: &str) -> String {
    format!("public.{}", identifier(name))
}

async fn columns(connection: &mut PgConnection, table: &str) -> Result<String> {
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT attname::text FROM pg_attribute
         WHERE attrelid=$1::regclass AND attnum>0 AND NOT attisdropped AND attgenerated=''
         ORDER BY attnum",
    )
    .bind(relation(table))
    .fetch_all(connection)
    .await?;
    anyhow::ensure!(!columns.is_empty(), "Backup table has no columns");
    Ok(columns
        .iter()
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
    Router::new().route("/api/database-backup", get(export).post(import))
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
    (status=200, body=inline(BackupFile), content_type="application/vnd.bgtutor.backup"),
    (status=409, body=ErrorBody), (status=500, body=ErrorBody)
), tag="Database backup")]
async fn export(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let permit = state
        .backup
        .transfer
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    let (file, length) = archive::export(&state.pool).await?;
    let filename = format!(
        "bgtutor-{}.bgtutor-backup",
        chrono::Utc::now().format("%Y-%m-%d-%H%M%S")
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in=Query)]
struct RestoreQuery {
    /// Explicitly replace the target database's application data.
    replace: bool,
}

#[utoipa::path(post, path="/api/database-backup", operation_id="restore_database_backup", params(RestoreQuery),
    request_body(content=inline(BackupFile), content_type="application/vnd.bgtutor.backup"),
    responses((status=204), (status=408, body=ErrorBody), (status=409, body=ErrorBody), (status=413, body=ErrorBody),
              (status=415, body=ErrorBody), (status=422, body=ErrorBody), (status=500, body=ErrorBody)),
    tag="Database backup"
)]
async fn import(
    State(state): State<AppState>,
    query: Result<Query<RestoreQuery>, axum::extract::rejection::QueryRejection>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<StatusCode> {
    if !crate::databases::query(query)?.replace {
        return Err(ApiError::invalid(
            "Replacing the target database must be confirmed",
        ));
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        != Some(MEDIA_TYPE)
    {
        return Err(ApiError(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Select a BG Tutor backup file".into(),
        ));
    }
    let _permit = state.backup.transfer.try_acquire().map_err(|_| busy())?;
    let file = archive::receive(&headers, body).await?;
    let mut connection = state
        .pool
        .acquire()
        .await
        .context("Accessing backup schema")?;
    let fingerprint = schema(&mut connection).await?;
    drop(connection);
    let input = archive::prepare(file, fingerprint).await?;

    // Drain requests already using the destination and reject new ones until
    // the transaction finishes. The table locks also cover other DB clients.
    let _maintenance = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state.backup.requests.write(),
    )
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::CONFLICT,
            "Database is busy; retry the restore when current work finishes".into(),
        )
    })?;
    restore::replace(&state, input).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(OpenApi)]
#[openapi(paths(export, import))]
pub(crate) struct BackupDoc;

#[cfg(test)]
mod tests {
    use super::TABLES;

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
        assert_eq!(actual, covered);
    }
}
