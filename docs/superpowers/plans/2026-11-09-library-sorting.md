# Match library sorting and stable numeric IDs

**Goal:** Give every match entry a persistent numeric ID and make all data columns clearly sortable without losing the compact desktop layout.

**Design:** User selected permanent numeric match IDs. Add `matches.match_no` as a unique generated identity; existing UUIDs remain the API identity. Copies receive a fresh number, moves and metadata edits retain it. Numbers never represent current row order. Expose the number in the library API and add an ID column immediately after selection.

All data headers show sort buttons and direction indicators. Combined Format/Positions and Tags/Comment headers offer separate buttons. Numeric IDs, position counts, scores and PR sort numerically; text sorts case-insensitively; missing values remain last. Sorting applies server-side before pagination, and changing it resets results/cursors and batch selection. Keep the old `result` status sort API and introduce `score` for the displayed score pair.

Long comments must not create oversized URL cursors. Use compact, scope-bound anchor cursors for tags/comments, resolving the current anchor value server-side and reporting a stale/deleted anchor as a validation error. Existing cursor formats keep working for other sorts. Stable UUID tie-breakers avoid duplicates for equal values.

## Tasks

- [x] Backend: regression tests first for permanent IDs, copies/moves, all new sort fields in both directions, numeric score/ID order, missing values, ties, cursor scope and long annotations. Add migration 0021 and extend library query/OpenAPI; preserve all prior work and data.
- [x] Frontend: tests first for visible IDs, all sort controls/directions and API submissions. Add ID, independent combined-column buttons, clear indicators and accessible labels while preserving 35px rows and usable notes on all target desktops.
- [x] Regenerate API types; update existing column-index assertions and mock library records. Build and run frontend, backend database integration and affected browser checks against disposable PostgreSQL data.
- [x] Review changes, back up the main database, activate migration/backend only when user jobs are idle, verify old IDs/data/settings plus new numbers, and remove disposable services.

No commit was requested. Private data and test artifacts remain outside Git.

Verification: 71 database integration tests passed (annotations, databases, game/move annotations, library sorting, move assessments and position browser); 109 frontend tests and 63 affected browser tests passed. The unchanged two deep native browser cases were excluded from this UI/database regression run. Build, Prettier, rustfmt, Clippy and whitespace checks passed. New browser coverage submits all 15 sort fields in both directions, loads 55 matches across pages, and verifies selection reset and ID stability.

Migration 0021 was activated after a private backup and confirming no active user jobs. All 33 existing main-library entries have unique positive numbers; saved settings, source bytes/SGF, metadata, database and completed-job fingerprints remained unchanged. A read-only main-service desktop smoke check verified 15 sort controls, numeric ID ordering and 35px rows. Temporary test backend/database/assets were removed.

UI follow-up: the user questioned the usefulness of source format in the match list. The visible column now contains only the numeric, sortable position count; format remains stored and supported by import/export and the existing API. The narrower column leaves more width for notes. Existing UI expectations were updated; all 109 frontend tests, three desktop layout/action browser tests, build and formatting checks passed.
