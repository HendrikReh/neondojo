//! PostgreSQL advisory lock keys. Every fixed key and every seed of a hashed
//! per-entity key lives here so that two domains cannot share a key by
//! accident: a shared key makes one domain's transaction block or skip an
//! unrelated one. All keys are 64-bit (`bigint`) keys; the two-`int4` form
//! of the advisory lock functions is not used.
//!
//! A key protects only the writers that take it, so each constant names the
//! writers it serializes.

/// Serializes benchmark run claims (`benchmarks::runner`).
pub(crate) const BENCHMARK_CLAIM: i64 = 91_432_301;
/// Serializes engine-duel series claims (`engine_matches::runner`).
pub(crate) const ENGINE_MATCH_CLAIM: i64 = 91_432_302;
/// Serializes analysis job claims and lease recovery (`jobs::claim`).
pub(crate) const ANALYSIS_JOB_CLAIM: i64 = 91_432_303;
/// Serializes new saved position filters so that concurrent saves respect the
/// filter limit (`position_browser::filters::create_filter`).
pub(crate) const POSITION_FILTER_LIMIT: i64 = 74_619_781_644_280;
/// Serializes the allocation of collection-wide Anki review IDs
/// (`learning::anki_review_sync`).
pub(crate) const ANKI_REVIEW_EXPORT_IDS: i64 = 73_270_182_800_161;
/// Serializes configuration writes: settings, profiles and opening books
/// (`configuration::store::lock`).
/// The bytes spell `BGTPROFI`.
pub(crate) const CONFIGURATION_PROFILES: i64 = 0x4247_5450_524f_4649;
/// Serializes rendered Anki package imports
/// (`learning::anki_flashcards::import_rendered_package`). Equal to
/// `hashtextextended('neondojo-anki-import', 0)`, the key used before the
/// keys were collected here.
pub(crate) const ANKI_PACKAGE_IMPORT: i64 = -4_300_197_411_433_216_181;

/// Seed of `hashtextextended(match_id::text, seed)`: serializes the decision
/// index rebuild of one match (`recurring_mistakes::index`).
pub(crate) const RECURRING_MISTAKE_MATCH_SEED: i64 = 72;
/// Seed of `hashtextextended('{match_id}:{analysis_id}', seed)`: serializes
/// the review hit set build of one analysis (`review_decisions::hits`).
pub(crate) const REVIEW_HIT_SET_SEED: i64 = 73;

/// Takes a transaction-scoped advisory lock; PostgreSQL releases it at commit
/// or rollback.
pub(crate) async fn xact_lock(
    connection: &mut sqlx::PgConnection,
    key: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(key)
        .execute(connection)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED_KEYS: [i64; 7] = [
        BENCHMARK_CLAIM,
        ENGINE_MATCH_CLAIM,
        ANALYSIS_JOB_CLAIM,
        POSITION_FILTER_LIMIT,
        ANKI_REVIEW_EXPORT_IDS,
        CONFIGURATION_PROFILES,
        ANKI_PACKAGE_IMPORT,
    ];

    fn assert_distinct(values: &[i64]) {
        for (index, value) in values.iter().enumerate() {
            assert!(!values[index + 1..].contains(value), "duplicate {value}");
        }
    }

    #[test]
    fn fixed_keys_and_hash_seeds_are_distinct() {
        assert_distinct(&FIXED_KEYS);
        // Hashed keys of different domains differ by their seed; collisions
        // with a fixed key or between seeds are as unlikely as any 64-bit
        // hash collision.
        assert_distinct(&[RECURRING_MISTAKE_MATCH_SEED, REVIEW_HIT_SET_SEED]);
    }

    #[sqlx::test(migrations = false)]
    async fn anki_package_import_keeps_its_hashed_key(pool: sqlx::PgPool) {
        let key: i64 = sqlx::query_scalar("SELECT hashtextextended('neondojo-anki-import', 0)")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(key, ANKI_PACKAGE_IMPORT);
    }

    #[sqlx::test(migrations = false)]
    async fn xact_lock_holds_the_key_until_the_transaction_ends(pool: sqlx::PgPool) {
        let mut holder = pool.begin().await.unwrap();
        xact_lock(&mut holder, POSITION_FILTER_LIMIT).await.unwrap();
        let free = || async {
            sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_xact_lock($1)")
                .bind(POSITION_FILTER_LIMIT)
                .fetch_one(&pool)
                .await
                .unwrap()
        };
        assert!(!free().await);
        holder.commit().await.unwrap();
        assert!(free().await);
    }
}
