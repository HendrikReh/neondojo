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

/// Numbered pages are one-based. Cursor callers retain their existing order and contract.
pub(crate) fn page_offset(
    page: Option<i64>,
    has_cursor: bool,
    limit: i64,
) -> crate::api::ApiResult<Option<i64>> {
    page.map(|page| {
        if has_cursor {
            return Err(crate::api::ApiError::invalid("Use either page or after"));
        }
        page.checked_sub(1)
            .filter(|index| *index >= 0)
            .and_then(|index| index.checked_mul(limit))
            .ok_or_else(|| crate::api::ApiError::invalid("Invalid page number"))
    })
    .transpose()
}
