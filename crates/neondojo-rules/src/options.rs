//! Authoritative option tables embedded from `shared/configuration-options.json`
//! and `shared/neongammon-releases.json`.

use serde_json::Value;
use std::sync::LazyLock;

/// Raw embedded `shared/configuration-options.json` document.
pub static OPTIONS: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../shared/configuration-options.json"))
        .expect("valid embedded configuration options")
});

/// Raw embedded `shared/neongammon-releases.json` string.
pub const NEONGAMMON_RELEASES_JSON: &str = include_str!("../../../shared/neongammon-releases.json");

/// Inclusive range and default for simultaneously running analysis jobs.
pub fn parallel_analysis_jobs() -> (i32, i32, i32) {
    let option = |name: &str| {
        OPTIONS["parallel_analysis_jobs"][name]
            .as_i64()
            .and_then(|number| i32::try_from(number).ok())
            .expect("parallel analysis job option is a valid integer")
    };
    (option("min"), option("max"), option("default"))
}

pub fn engine_min_threads(engine: &str) -> i32 {
    OPTIONS["engine_threads"][engine]["min"]
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .expect("valid embedded minimum thread count")
}

pub fn engine_default_threads(engine: &str, purpose: &str) -> i32 {
    OPTIONS["engine_threads"][engine]["default"][purpose]
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .expect("valid embedded default thread count")
}

/// Verified provider profiles, shared with client normalization and catalogs.
pub fn engine_depths(engine: &str, purpose: &str) -> Vec<i32> {
    OPTIONS["engine_profiles"][engine][purpose]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_i64().and_then(|n| i32::try_from(n).ok()))
                .collect()
        })
        .unwrap_or_default()
}

/// Supported board theme identifiers in canonical gallery order.
pub fn board_themes() -> Vec<&'static str> {
    OPTIONS["board_themes"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Supported match file extensions (lowercase, with leading dot).
pub fn match_file_extensions() -> Vec<&'static str> {
    OPTIONS["match_file_extensions"]
        .as_array()
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Inclusive logical pixel width bounds for the Analysis board pane.
pub fn analysis_board_width_bounds() -> (i32, i32) {
    let bound = |name: &str| {
        OPTIONS["analysis_board_width"][name]
            .as_i64()
            .and_then(|number| i32::try_from(number).ok())
            .expect("valid embedded analysis board width bound")
    };
    (bound("min"), bound("max"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_shared_options_are_valid_and_consistent() {
        let (min_jobs, max_jobs, default_jobs) = parallel_analysis_jobs();
        assert!(min_jobs >= 1 && default_jobs >= min_jobs && default_jobs <= max_jobs);
        assert_eq!(engine_depths("gnubg", "analysis"), vec![1, 2, 3, 4, 5]);
        assert_eq!(engine_depths("bgsage", "analysis"), vec![1, 2, 3, 4]);
        assert_eq!(engine_min_threads("gnubg"), 1);
        assert_eq!(engine_min_threads("bgsage"), 2);
        assert_eq!(engine_default_threads("gnubg", "analysis"), 1);
        assert_eq!(engine_default_threads("bgsage", "analysis"), 4);
        assert_eq!(engine_default_threads("bgsage", "play"), 2);
        assert_eq!(analysis_board_width_bounds(), (320, 1680));
        assert!(board_themes().contains(&"forest"));
        assert!(match_file_extensions().contains(&".xg"));
        let releases: Value =
            serde_json::from_str(NEONGAMMON_RELEASES_JSON).expect("valid releases JSON");
        assert!(releases.get("3.2.2").is_some());
        assert!(releases.get("4.0.0").is_some());
    }
}
