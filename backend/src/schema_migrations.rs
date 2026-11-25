//! Schema migrations and adoption of databases built before the baseline.
//!
//! `migrations/0001_schema.sql` consolidates the former migrations 0001-0084.
//! A database migrated with those files records them in `_sqlx_migrations`,
//! which SQLx would reject as missing or changed. Before migrating, such a
//! database is adopted: once its history matches the former files exactly,
//! the records are replaced by the baseline record in one transaction. The
//! schema itself is already identical, so no table or row changes.
//!
//! NeonDojo 1.0.0 shipped migrations 0001-0083. A database from that release
//! first receives the former 0084 in the same transaction, then the baseline
//! record.

use anyhow::{Context, Result, bail};
use sqlx::{PgPool, migrate::Migrator};

/// The embedded migrations: the baseline followed by later changes.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const BASELINE_VERSION: i64 = 1;
/// The last former migration contained in the baseline.
const LAST_CONSOLIDATED_VERSION: i64 = 84;
const LEGACY_CHECKSUMS: &str = include_str!("schema_migrations/legacy_checksums.txt");
/// The last former migration in NeonDojo 1.0.0.
const RELEASED_1_0_VERSION: i64 = 83;
/// The former migrations after 1.0.0, applied when adopting its databases.
const AFTER_RELEASE_1_0: &[(i64, &str)] = &[(
    84,
    include_str!("schema_migrations/0084_virtual_player_profiles.sql"),
)];

/// Adopts a database migrated with the former files, then applies pending
/// migrations.
pub async fn run(pool: &PgPool) -> Result<()> {
    adopt_consolidated_history(pool)
        .await
        .context("Adopting the consolidated migration history")?;
    MIGRATOR.run(pool).await.context("Applying migrations")?;
    let mut tx = pool.begin().await?;
    resolve_game_cube_rules(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Only candidate sources marked by migration 0095 need to be replayed.
/// Repeated startup and concurrent backfills keep all analysis rows intact.
pub(super) async fn resolve_game_cube_rules(connection: &mut sqlx::PgConnection) -> Result<()> {
    let matches: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT DISTINCT g.match_uuid FROM match_games g JOIN matches m ON m.id=g.match_uuid LEFT JOIN matches origin ON m.collection_only AND origin.id=m.source_match_id WHERE g.cube_enabled IS NULL AND coalesce(origin.sgf,m.sgf)<>'' ORDER BY g.match_uuid")
        .fetch_all(&mut *connection).await?;
    for id in matches {
        let source: Option<String> =
            sqlx::query_scalar("SELECT coalesce(origin.sgf,m.sgf) FROM matches m LEFT JOIN matches origin ON m.collection_only AND origin.id=m.source_match_id WHERE m.id=$1 FOR UPDATE OF m")
                .bind(id)
                .fetch_optional(&mut *connection)
                .await?;
        let Some(source) = source else { continue };
        let parsed = tokio::task::spawn_blocking(move || {
            crate::import::sgf::parse_sgf_with_context(&source).map(|(_, games)| {
                games
                    .into_iter()
                    .map(|g| g.cube_enabled)
                    .collect::<Vec<_>>()
            })
        })
        .await?;
        let rules = match parsed {
            Ok(rules) => rules,
            Err(error) => {
                tracing::warn!(match_id = %id, %error, "Could not recover game cube rules from stored source");
                continue;
            }
        };
        sqlx::query("UPDATE match_games g SET cube_enabled=r.enabled FROM unnest($2::boolean[]) WITH ORDINALITY r(enabled,n) WHERE g.match_uuid=$1 AND g.game_no=r.n AND g.cube_enabled IS NULL")
            .bind(id).bind(rules).execute(&mut *connection).await?;
    }
    Ok(())
}

fn baseline_checksum() -> &'static [u8] {
    MIGRATOR
        .iter()
        .find(|migration| migration.version == BASELINE_VERSION)
        .map(|migration| migration.checksum.as_ref())
        .unwrap_or_default()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(text.get(index..index + 2)?, 16).ok())
        .collect()
}

/// Version and checksum of each former migration in the baseline.
fn consolidated_history() -> Vec<(i64, Vec<u8>)> {
    LEGACY_CHECKSUMS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (version, checksum) = line.split_once(' ')?;
            Some((version.parse().ok()?, decode_hex(checksum)?))
        })
        .collect()
}

/// The applied history with the baseline record expanded to the former
/// migrations it consolidates. Backup fingerprints use this form, so backups
/// written before and after the consolidation stay restorable.
pub(crate) fn expanded_history(applied: Vec<(i64, Vec<u8>)>) -> Vec<(i64, Vec<u8>)> {
    let baseline = baseline_checksum();
    let mut history = Vec::with_capacity(applied.len() + LAST_CONSOLIDATED_VERSION as usize);
    for (version, checksum) in applied {
        if version == BASELINE_VERSION && checksum == baseline {
            history.extend(consolidated_history());
        } else {
            history.push((version, checksum));
        }
    }
    history
}

async fn adopt_consolidated_history(pool: &PgPool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(&mut *tx)
            .await?;
    if !exists {
        return Ok(());
    }
    // Serialize concurrent startups; the loser sees the adopted history.
    sqlx::query("LOCK TABLE public._sqlx_migrations IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let applied: Vec<(i64, Vec<u8>, bool)> = sqlx::query_as(
        "SELECT version, checksum, success FROM public._sqlx_migrations
         WHERE version <= $1 ORDER BY version",
    )
    .bind(LAST_CONSOLIDATED_VERSION)
    .fetch_all(&mut *tx)
    .await?;
    let baseline = baseline_checksum();
    match applied.as_slice() {
        // A new database, or one already on the baseline: SQLx takes over.
        [] => return Ok(()),
        [(BASELINE_VERSION, checksum, _)] if checksum.as_slice() == baseline => return Ok(()),
        _ => {}
    }
    let expected = consolidated_history();
    let records = |through: i64| {
        let expected = expected.iter().filter(|(version, _)| *version <= through);
        applied.len() == expected.clone().count()
            && applied.iter().zip(expected).all(
                |((version, checksum, success), (want_version, want_checksum))| {
                    *success && version == want_version && checksum == want_checksum
                },
            )
    };
    if records(RELEASED_1_0_VERSION) {
        for (version, sql) in AFTER_RELEASE_1_0 {
            sqlx::raw_sql(sql)
                .execute(&mut *tx)
                .await
                .with_context(|| format!("Applying the former migration {version:04}"))?;
        }
    } else if !records(LAST_CONSOLIDATED_VERSION) {
        let latest = applied.last().map_or(0, |(version, _, _)| *version);
        bail!(
            "The database records migrations up to {latest:04} that differ from the former \
             migrations 0001-{LAST_CONSOLIDATED_VERSION:04} and from NeonDojo 1.0.0. Update it \
             with a NeonDojo version from before the schema consolidation first, then start \
             this version again."
        );
    }
    sqlx::query("DELETE FROM public._sqlx_migrations WHERE version <= $1")
        .bind(LAST_CONSOLIDATED_VERSION)
        .execute(&mut *tx)
        .await?;
    let description = MIGRATOR
        .iter()
        .find(|migration| migration.version == BASELINE_VERSION)
        .map(|migration| migration.description.to_string())
        .unwrap_or_default();
    sqlx::query(
        "INSERT INTO public._sqlx_migrations(version, description, success, checksum, execution_time)
         VALUES($1, $2, true, $3, 0)",
    )
    .bind(BASELINE_VERSION)
    .bind(description)
    .bind(baseline)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!("Adopted the consolidated migration history");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consolidated_history_lists_every_former_migration() {
        let history = consolidated_history();
        let versions: Vec<i64> = history.iter().map(|(version, _)| *version).collect();
        assert_eq!(
            versions,
            (1..=LAST_CONSOLIDATED_VERSION).collect::<Vec<_>>()
        );
        assert!(history.iter().all(|(_, checksum)| checksum.len() == 48));
    }

    #[test]
    fn baseline_follows_the_consolidated_versions() {
        assert_eq!(baseline_checksum().len(), 48);
        assert!(MIGRATOR.iter().all(|migration| {
            migration.version == BASELINE_VERSION || migration.version > LAST_CONSOLIDATED_VERSION
        }));
    }

    #[test]
    fn migrations_after_1_0_are_the_recorded_former_files() {
        use sha2::{Digest, Sha384};
        let history = consolidated_history();
        let after: Vec<i64> = history
            .iter()
            .map(|(version, _)| *version)
            .filter(|version| *version > RELEASED_1_0_VERSION)
            .collect();
        assert_eq!(
            after,
            AFTER_RELEASE_1_0
                .iter()
                .map(|(version, _)| *version)
                .collect::<Vec<_>>()
        );
        for (version, sql) in AFTER_RELEASE_1_0 {
            let recorded = history.iter().find(|(v, _)| v == version).unwrap();
            assert_eq!(Sha384::digest(sql.as_bytes()).as_slice(), recorded.1);
        }
    }

    #[test]
    fn expanded_history_replaces_only_the_baseline_record() {
        let later = (85, vec![7; 48]);
        let expanded = expanded_history(vec![(1, baseline_checksum().to_vec()), later.clone()]);
        let mut expected = consolidated_history();
        expected.push(later.clone());
        assert_eq!(expanded, expected);
        let unknown = vec![(1, vec![0; 48]), later];
        assert_eq!(expanded_history(unknown.clone()), unknown);
    }
}
