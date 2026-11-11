# Global unfiltered position search

User superseded the previous scope policy on 2026-11-10: the analysis action searches all databases with only the current position as a filter. Existing sorting can remain because it does not restrict results.

- [x] Update the previous preserve-filter regression to require global scope and cleared filters; add a real two-database case with stale basic/advanced filters and verify both occurrences. Confirm failure before changes.
- [x] Set App database selection to the existing all-databases sentinel on explicit source search, and replace library filters with emptyFilters plus position_id when consuming that intent. Preserve manual-library filter behavior.
- [x] Verify action/manual-search browser regressions, frontend tests/build and formatting; update current documentation.


Verification on 2026-11-10:

- Updated action regression failed before implementation because database_id remained in the outgoing request.
- Analysis search now selects the existing global database sentinel and constructs filters from emptyFilters plus position_id; ordinary manual library searches retain their scoped/filter-preserving behavior.
- Seven action checks pass. The added real two-database scenario sets prior text, player, match name, tournament, round, tag, comment, date-range/type, match-type/result and match-length filters, then verifies no restrictive query parameters remain except the position ID. Both source and second-database occurrences appear, All databases is visibly selected, and filter inputs are cleared.
- Five existing manual-position-search regressions pass. Total 12 focused browser cases pass against the updated production app; all 341 frontend tests pass. Production build, changed-file formatting and git diff --check pass (existing Vite bundle-size advisory remains).
- No backend changes, restart, migration, push or merge; unrelated workspace changes preserved.
