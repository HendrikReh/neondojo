use super::{
    ArchiveLayout, DERIVED_TABLES, TABLES, archive::Prepared, columns, relation, schema,
    schema_mismatch, sequences, transfer_settings, validate_sequences,
};
use crate::api::{ApiError, ApiResult, AppState};
use anyhow::{Context, Result};
use axum::http::StatusCode;
use sqlx::{Acquire, PgConnection};
use tokio::{fs::File, io::AsyncReadExt};
use uuid::Uuid;

fn rejected() -> ApiError {
    ApiError::invalid("Backup contains data that cannot be restored")
}

/// Insert triggers that are disabled while their table is copied. Each one
/// derives data the archive already holds (the player catalog in
/// `match_players`, `matches.played_record_fingerprint`), derives a revision
/// table that [`rebuild_analysis_revisions`] fills once after the copy, or
/// announces job events, which are history in a backup. The statements run
/// in the restore transaction, so a failed restore rolls them back with the
/// data. Foreign key and other constraint triggers stay enabled throughout.
const COPY_SUSPENDED_TRIGGERS: &[(&str, &str)] = &[
    ("matches", "catalog_match_players"),
    ("positions", "positions_played_record_insert"),
    ("imported_analyses", "imported_analyses_revision_insert"),
    ("job_events", "job_events_notify"),
    (
        "match_analysis_games",
        "match_analysis_games_revision_insert",
    ),
    ("move_analyses", "move_analyses_revision_insert"),
];

async fn set_copy_triggers(connection: &mut PgConnection, table: &str, enable: bool) -> Result<()> {
    let action = if enable { "ENABLE" } else { "DISABLE" };
    for (_, trigger) in COPY_SUSPENDED_TRIGGERS
        .iter()
        .filter(|(owner, _)| *owner == table)
    {
        sqlx::query(&format!(
            "ALTER TABLE {} {action} TRIGGER {}",
            relation(table),
            super::identifier(trigger)
        ))
        .execute(&mut *connection)
        .await
        .with_context(|| format!("Changing restore trigger {trigger} ({action})"))?;
    }
    Ok(())
}

/// Write the revision rows the suspended insert triggers would have written
/// for the copied analyses: one per job with checkpoints or decisions and one
/// per match with imported analyses, each carrying this transaction's token.
/// The revision tables were cleared with the data, and recurring-mistake
/// publishing locks these rows.
async fn rebuild_analysis_revisions(connection: &mut PgConnection) -> Result<()> {
    sqlx::query(
        "SELECT public.mark_native_analysis_revisions(ARRAY(
             SELECT j.id FROM public.jobs j
             WHERE EXISTS (SELECT FROM public.match_analysis_games g WHERE g.job_id=j.id)
                OR EXISTS (SELECT FROM public.move_analyses a WHERE a.job_id=j.id)))",
    )
    .execute(&mut *connection)
    .await
    .context("Rebuilding native analysis revisions")?;
    // The rows and conflict rule `mark_imported_analysis_rows` (migration 0089)
    // writes, so a row another trigger wrote during the copy cannot fail the restore.
    sqlx::query(
        "INSERT INTO public.imported_analysis_revisions AS revisions(match_id, revision)
         SELECT m.id, 'tx:' || pg_current_xact_id()::text FROM public.matches m
         WHERE EXISTS (SELECT FROM public.imported_analyses a WHERE a.match_uuid=m.id)
         ORDER BY m.id
         ON CONFLICT (match_id) DO UPDATE SET revision = EXCLUDED.revision
         WHERE revisions.revision <> EXCLUDED.revision",
    )
    .execute(connection)
    .await
    .context("Rebuilding imported analysis revisions")?;
    Ok(())
}

async fn copy_tables(
    connection: &mut PgConnection,
    file: &mut File,
    lengths: Vec<u64>,
    layout: ArchiveLayout,
) -> ApiResult<()> {
    let mut buffer = vec![0; 128 * 1024];
    for (table, mut remaining) in layout.tables().iter().zip(lengths) {
        let columns = columns(connection, table, layout).await?;
        let name = relation(table);
        set_copy_triggers(connection, table, false).await?;
        let mut writer = connection
            .copy_in_raw(&format!(
                "COPY {name} ({columns}) FROM STDIN WITH (ENCODING 'UTF8')",
            ))
            .await
            .context("Starting restore COPY")?;
        while remaining != 0 {
            let limit = remaining.min(buffer.len() as u64) as usize;
            if let Err(error) = file.read_exact(&mut buffer[..limit]).await {
                let _ = writer.abort("Backup data read failed").await;
                return Err(anyhow::Error::from(error)
                    .context("Reading staged backup data")
                    .into());
            }
            if let Err(error) = writer.send(&buffer[..limit]).await {
                let _ = writer.abort("Backup COPY failed").await;
                return Err(anyhow::Error::from(error)
                    .context("Writing backup data")
                    .into());
            }
            remaining -= limit as u64;
        }
        if let Err(error) = writer.finish().await {
            if let Some(code) = error.as_database_error().and_then(|e| e.code())
                && (code.starts_with("22") || code.starts_with("23") || code == "P0001")
            {
                tracing::warn!(table, %code, "Backup data rejected by PostgreSQL");
                return Err(rejected());
            }
            return Err(anyhow::Error::from(error)
                .context("Finishing restore COPY")
                .into());
        }
        set_copy_triggers(connection, table, true).await?;
    }
    rebuild_analysis_revisions(connection).await?;
    Ok(())
}

async fn preserve_host_settings(connection: &mut PgConnection, binary: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO public.app_settings(singleton,gnubg_path)
         SELECT true,gnubg_path FROM backup_local_app ON CONFLICT DO NOTHING",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO public.opening_book_settings(singleton,path,xg_path)
         SELECT true,path,xg_path FROM backup_local_book ON CONFLICT DO NOTHING",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE public.app_settings a SET
             gnubg_path=COALESCE((SELECT gnubg_path FROM backup_local_app),$1),
             gnubg_version=(SELECT gnubg_version FROM backup_local_app),
             gnubg_threads=(SELECT gnubg_threads FROM backup_local_app),
             open_sage_threads=(SELECT open_sage_threads FROM backup_local_app),
             gnubg_play_threads=(SELECT gnubg_play_threads FROM backup_local_app),
             open_sage_play_threads=(SELECT open_sage_play_threads FROM backup_local_app),
             parallel_analysis_jobs=(SELECT parallel_analysis_jobs FROM backup_local_app)",
    )
    .bind(binary)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE public.opening_book_settings SET
             path=COALESCE((SELECT path FROM backup_local_book),$1),
             xg_path=COALESCE(NULLIF((SELECT xg_path FROM backup_local_book),''),$2)",
    )
    .bind(crate::opening_book::default_path())
    .bind(crate::opening_book::xg_book::default_path())
    .execute(&mut *connection)
    .await?;
    // The configuration owns the book choice and the settings table repeats
    // it, so both get the target's paths. Profile paths are also rebound so
    // applying a moved profile keeps the target's book locations.
    for (table, column) in [
        ("configuration_state", "current_configuration"),
        ("configuration_profiles", "configuration"),
    ] {
        sqlx::query(&format!(
            "UPDATE public.{table} SET {column} = jsonb_set({column}, '{{opening_book}}',
                 ({column}->'opening_book') || jsonb_build_object('path', b.path, 'xg_path', b.xg_path))
             FROM public.opening_book_settings b
             WHERE jsonb_typeof({column}->'opening_book')='object'"
        )).execute(&mut *connection).await?;
    }
    // Checkpoints keep the saved engine version and model. Bind executable
    // paths to the destination so a backup can never select a program to start.
    sqlx::query(
        "UPDATE public.play_sessions SET gnubg_path=COALESCE((SELECT gnubg_path FROM backup_local_app), $1)"
    ).bind(binary).execute(&mut *connection).await?;
    for part in ["rules", "bot", "analysis"] {
        sqlx::query(&format!(
            "UPDATE public.play_sessions SET state = jsonb_set(state, '{{engine_snapshots,{part},gnubg_path}}', to_jsonb(gnubg_path))
             WHERE jsonb_typeof(state#>'{{engine_snapshots,{part}}}')='object'"
        )).execute(&mut *connection).await?;
        sqlx::query(&format!(
            "UPDATE public.play_sessions SET state = jsonb_set(state, '{{engine_snapshots,{part},open_sage,python}}',
                 to_jsonb(COALESCE((SELECT python_path FROM public.engine_installations WHERE engine_id='bgsage' AND snapshot IS NOT NULL), '')))
             WHERE jsonb_typeof(state#>'{{engine_snapshots,{part},open_sage}}')='object'"
        )).execute(&mut *connection).await?;
    }
    Ok(())
}

async fn retire_source_work(connection: &mut PgConnection) -> Result<()> {
    use crate::jobs::RESTORE_INTERRUPTED_ERROR;
    // A restored deck requires a new opt-in for the active Anki collection.
    sqlx::query("DELETE FROM public.anki_auto_sync")
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM public.score_practice_profiles")
        .execute(&mut *connection)
        .await?;
    sqlx::query(
        "WITH interrupted AS (
             UPDATE public.jobs SET status='failed',error=$1,
                 lease_token=NULL,lease_until=NULL,updated_at=now()
             WHERE status IN ('queued','running') RETURNING id,status,error,progress,completed_games,total_games
         )
         INSERT INTO public.job_events(job_id,event,data)
         SELECT id,status,jsonb_build_object('status',status,'error',error,'progress',progress,
             'completed_games',completed_games,'total_games',total_games) FROM interrupted"
    ).bind(RESTORE_INTERRUPTED_ERROR).execute(&mut *connection).await?;
    sqlx::query(
        "UPDATE public.match_analysis_timing SET active_started_at=NULL,active_elapsed_ms=0
         WHERE active_started_at IS NOT NULL",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE public.match_assessments SET status='failed',error_code='interrupted',
            lease_token=NULL,lease_until=NULL WHERE status IN ('queued','running')",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE public.benchmark_runs SET status='failed',finished_at=now(),
             error=$1,worker_token=NULL,lease_until=NULL
         WHERE status IN ('queued','running')",
    )
    .bind(RESTORE_INTERRUPTED_ERROR)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "UPDATE public.engine_match_runs SET status='cancelled',error=$1,
             lease_token=NULL,lease_until=NULL WHERE status IN ('queued','running')",
    )
    .bind(RESTORE_INTERRUPTED_ERROR)
    .execute(connection)
    .await?;
    Ok(())
}

pub(super) async fn replace(state: &AppState, mut input: Prepared) -> ApiResult<()> {
    let mut connection = state
        .pool
        .acquire()
        .await
        .context("Connecting to restore target")?;
    connection.close_on_drop();
    let mut tx = connection
        .begin()
        .await
        .context("Opening restore transaction")?;
    transfer_settings(&mut tx).await?;
    let layout = schema(&mut tx)
        .await?
        .layout(&input.manifest.schema)
        .ok_or_else(schema_mismatch)?;
    let sequences = sequences(&mut tx).await?;
    validate_sequences(&input.manifest, sequences.len())?;

    sqlx::query("SET LOCAL lock_timeout TO '3s'")
        .execute(&mut *tx)
        .await
        .context("Setting restore lock timeout")?;
    let tables = TABLES
        .iter()
        .chain(DERIVED_TABLES)
        .map(|table| relation(table))
        .collect::<Vec<_>>()
        .join(",");
    // Explicit RESTRICT prevents a future foreign key from clearing a table
    // outside the backup. Lock before reading any host-specific settings.
    sqlx::query(&format!("LOCK TABLE {tables} IN ACCESS EXCLUSIVE MODE"))
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            if error.as_database_error().and_then(|e| e.code()).as_deref() == Some("55P03") {
                ApiError(
                    StatusCode::CONFLICT,
                    "Database is busy; retry when current work finishes".into(),
                )
            } else {
                anyhow::Error::from(error)
                    .context("Locking restore target")
                    .into()
            }
        })?;
    sqlx::query(
        "CREATE TEMP TABLE backup_local_app ON COMMIT DROP AS
        SELECT gnubg_path,gnubg_version,gnubg_threads,open_sage_threads,
            gnubg_play_threads,open_sage_play_threads,parallel_analysis_jobs FROM public.app_settings",
    )
    .execute(&mut *tx)
    .await
    .context("Preserving local engine settings")?;
    sqlx::query(
        "CREATE TEMP TABLE backup_local_book ON COMMIT DROP AS
        SELECT path,xg_path FROM public.opening_book_settings",
    )
    .execute(&mut *tx)
    .await
    .context("Preserving local book settings")?;
    sqlx::query(&format!("TRUNCATE {tables} CONTINUE IDENTITY RESTRICT"))
        .execute(&mut *tx)
        .await
        .context("Preparing target backup tables")?;
    copy_tables(&mut tx, &mut input.file, input.lengths, layout).await?;
    if layout == ArchiveLayout::BeforeGameCubeRules {
        sqlx::query("SELECT mark_legacy_game_cube_rules()")
            .execute(&mut *tx)
            .await
            .context("Marking restored games for cube-rule replay")?;
    }
    crate::schema_migrations::resolve_game_cube_rules(&mut tx).await?;
    sqlx::query("SELECT repair_legacy_retained_checker_labels(NULL)")
        .execute(&mut *tx)
        .await
        .context("Correcting restored NeonGammon checker labels")?;

    let default: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM public.databases WHERE is_default")
            .fetch_optional(&mut *tx)
            .await
            .context("Checking restored default database")?;
    let default = default.ok_or_else(rejected)?;
    let system: i64 = sqlx::query_scalar("SELECT count(*) FROM public.databases WHERE id=ANY($1)")
        .bind(
            &[
                crate::position_retention::COLLECTION_DATABASE_ID,
                crate::play::BOT_DATABASE_ID,
                crate::play::PAUSED_MATCHES_DATABASE_ID,
                crate::engine_matches::ENGINE_DUELS_DATABASE_ID,
            ][..],
        )
        .fetch_one(&mut *tx)
        .await
        .context("Checking restored system databases")?;
    if system != 4 {
        return Err(rejected());
    }
    // Independent installations can have different IDs for their seeded default.
    sqlx::query(&format!(
        "ALTER TABLE public.matches ALTER COLUMN database_id SET DEFAULT '{default}'::uuid"
    ))
    .execute(&mut *tx)
    .await
    .context("Rebinding restored default database")?;

    for (sequence, next) in sequences.into_iter().zip(input.manifest.sequences) {
        let max: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT max({})::bigint FROM {}",
            super::identifier(&sequence.column_name),
            relation(&sequence.table_name)
        ))
        .fetch_one(&mut *tx)
        .await
        .context("Checking restored sequence values")?;
        if max.is_some_and(|value| value >= next) || next == i64::MAX {
            return Err(rejected());
        }
        sqlx::query(&format!(
            "ALTER SEQUENCE {} RESTART WITH {next}",
            relation(&sequence.name)
        ))
        .execute(&mut *tx)
        .await
        .context("Restoring sequence position")?;
    }
    preserve_host_settings(&mut tx, state.engine.binary()).await?;
    retire_source_work(&mut tx).await?;
    tx.commit().await.context("Committing restored database")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn suspended_triggers_belong_to_copied_tables() {
        for (table, _) in super::COPY_SUSPENDED_TRIGGERS {
            assert!(super::TABLES.contains(table), "{table}");
        }
    }
}
