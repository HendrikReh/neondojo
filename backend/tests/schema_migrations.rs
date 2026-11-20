//! Databases migrated with the former migrations 0001-0084 adopt the
//! consolidated baseline without schema or data changes.
use neondojo::schema_migrations::{self, MIGRATOR};
use sqlx::PgPool;

const LEGACY: &str = include_str!("../src/schema_migrations/legacy_checksums.txt");

fn legacy() -> Vec<(i64, Vec<u8>)> {
    LEGACY
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (version, checksum) = line.split_once(' ').unwrap();
            let bytes = (0..checksum.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&checksum[i..i + 2], 16).unwrap())
                .collect();
            (version.parse().unwrap(), bytes)
        })
        .collect()
}

/// Rewrites a baseline database's records to what the former files left.
async fn record_legacy_history(pool: &PgPool, through: i64) {
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version <= 84")
        .execute(pool)
        .await
        .unwrap();
    for (version, checksum) in legacy().into_iter().filter(|(v, _)| *v <= through) {
        sqlx::query(
            "INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time)
             VALUES($1,'former',true,$2,1)",
        )
        .bind(version)
        .bind(checksum)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn history(pool: &PgPool) -> Vec<(i64, Vec<u8>)> {
    sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap()
}

fn current() -> Vec<(i64, Vec<u8>)> {
    MIGRATOR
        .iter()
        .map(|m| (m.version, m.checksum.to_vec()))
        .collect()
}

#[sqlx::test(migrations = false)]
#[ignore = "requires PostgreSQL"]
async fn new_databases_apply_the_baseline_and_later_migrations(pool: PgPool) {
    schema_migrations::run(&pool).await.unwrap();
    schema_migrations::run(&pool).await.unwrap();
    assert_eq!(history(&pool).await, current());
    let databases: i64 = sqlx::query_scalar("SELECT count(*) FROM databases")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(databases, 6);
    // New matches land in the seeded default database.
    let default_target: bool = sqlx::query_scalar(
        "SELECT (SELECT pg_get_expr(adbin, adrelid) FROM pg_attrdef
                 WHERE adrelid='matches'::regclass
                   AND adnum=(SELECT attnum FROM pg_attribute
                              WHERE attrelid='matches'::regclass AND attname='database_id'))
                = format('%L::uuid', (SELECT id FROM databases WHERE is_default))",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(default_target);
}

#[sqlx::test(migrations = false)]
#[ignore = "requires PostgreSQL"]
async fn former_history_is_adopted_and_keeps_user_rows(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    record_legacy_history(&pool, 84).await;
    sqlx::query("INSERT INTO databases(id,name) VALUES(gen_random_uuid(),'Kept')")
        .execute(&pool)
        .await
        .unwrap();

    schema_migrations::run(&pool).await.unwrap();

    assert_eq!(history(&pool).await, current());
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM databases WHERE name='Kept'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kept, 1);
}

#[sqlx::test(migrations = false)]
#[ignore = "requires PostgreSQL"]
async fn former_history_before_later_migrations_is_adopted_and_migrated(pool: PgPool) {
    let baseline_only = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|m| m.version == 1)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    baseline_only.run(&pool).await.unwrap();
    record_legacy_history(&pool, 84).await;

    schema_migrations::run(&pool).await.unwrap();

    assert_eq!(history(&pool).await, current());
}

#[sqlx::test(migrations = false)]
#[ignore = "requires PostgreSQL"]
async fn incomplete_or_changed_former_history_is_rejected_unchanged(pool: PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    record_legacy_history(&pool, 60).await;
    let incomplete = history(&pool).await;
    let error = schema_migrations::run(&pool).await.unwrap_err();
    assert!(format!("{error:#}").contains("up to 0060"), "{error:#}");
    assert_eq!(history(&pool).await, incomplete);

    record_legacy_history(&pool, 84).await;
    sqlx::query("UPDATE _sqlx_migrations SET checksum='\\x00' WHERE version=42")
        .execute(&pool)
        .await
        .unwrap();
    let changed = history(&pool).await;
    assert!(schema_migrations::run(&pool).await.is_err());
    assert_eq!(history(&pool).await, changed);
}
