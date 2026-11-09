# Deck membership in position search results

**Goal:** Show the learning decks that already contain each saved position, in the compact result table and position preview.

**Design:** Add a required `decks: [{id, name}]` field to each position-browser hit. Resolve memberships in one bounded page-level query through the existing indexed card signature (checker Position-ID + GNUbg Match-ID + decision kind), so equivalent occurrences share membership while different dice/score/cube contexts remain distinct. Preserve saved positions after source deletion. No database migration is required.

The desktop table gains a resizable, sortable Decks column. Names stay on one line; the existing row summary and preview expose the complete list. Empty membership displays an em dash, with a localized explanation in the preview. Refresh results after deck changes, retaining filters, selection, preview and loaded pages. Extend column-layout reading with an explicit default for the newly introduced column so existing custom widths survive.

- [x] Write backend membership/context/pagination tests and frontend rendering/legacy-width tests; verify failures before implementation.
- [x] Implement bounded membership lookup, deck-name sorting and the OpenAPI response; regenerate frontend API types.
- [x] Add the compact Decks column, full preview/summary, six-language labels and deck-change refresh; preserve existing widths.
- [x] Run relevant backend/frontend/browser regressions on a disposable database, review, then activate verified assets/backend after a private main-database backup. Clean test resources. Do not commit or add demo/private data.

Validation: `cargo test -p bgtutor --test position_browser -- --ignored`, learning and saved-position retention integration suites; frontend unit tests, production build and format check; real-browser membership addition/duplicate/context/refresh/pagination and existing desktop position/learning flows.

TDD evidence: both backend cases failed before implementation (missing decks field / unsupported decks sort); three frontend cases failed for the missing column, empty-state text and width migration. After implementation, 57 relevant PostgreSQL integration tests, 94 Rust unit/contract tests and 198 frontend tests pass. The production build and both format checks pass. The focused browser case passes with 28 saved positions across two pages, two deck transfers, duplicate transfer, persistent membership and legacy widths. It also tolerates preexisting shared learning cards when rerun against the same disposable database. Independent read-only review found no actionable issue.

All 88 broad browser regressions pass (excluding the two expensive analysis-depth suites). A final layout check found that the extra column compressed default notes to 76px in a 1440px window. A browser regression reproduced that failure, then passed after increasing the table's minimum widths by the new column's 144px. All 10 desktop-position and position-browser tests passed again on the final bundle, covering 1440/1800/2560/3840px windows, legacy widths, membership refresh, preview, analysis, study and Anki.

Activated the verified backend and final frontend after a private database backup and graceful restart with no queued/running jobs. Read-only smoke tests verified existing saved-position memberships against SQL, table/preview labels and deck sorting without write requests. Existing match/position/deck/card/membership counts and settings were unchanged. Disposable test service/database and temporary bundles were removed. No migrations, commits or private/demo files were added for this feature.
