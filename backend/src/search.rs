//! Shared text matching for PostgreSQL search queries.

/// Build a lowercased substring pattern with literal LIKE metacharacters.
pub(crate) fn literal_pattern(value: &str) -> String {
    format!(
        "%{}%",
        value
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    )
}
