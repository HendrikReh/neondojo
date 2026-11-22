//! Fixed PostgreSQL advisory lock keys for worker claims. Each claim domain
//! needs its own key: a shared key makes one worker's claim transaction block
//! or skip an unrelated worker's claim.

/// Serializes benchmark run claims (`benchmarks::runner`).
pub(crate) const BENCHMARK_CLAIM: i64 = 91_432_301;
/// Serializes engine-duel series claims (`engine_matches::runner`).
pub(crate) const ENGINE_MATCH_CLAIM: i64 = 91_432_302;
/// Serializes analysis job claims and lease recovery (`jobs::claim`).
pub(crate) const ANALYSIS_JOB_CLAIM: i64 = 91_432_303;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_domains_use_distinct_keys() {
        let keys = [BENCHMARK_CLAIM, ENGINE_MATCH_CLAIM, ANALYSIS_JOB_CLAIM];
        for (index, key) in keys.iter().enumerate() {
            assert!(!keys[index + 1..].contains(key), "duplicate key {key}");
        }
    }
}
