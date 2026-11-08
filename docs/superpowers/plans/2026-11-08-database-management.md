# Database Management Implementation Plan

> **For agentic workers:** Use the executing-plans workflow, with independent backend domains assigned according to dispatching-parallel-agents. Root integrates and verifies.

**Goal:** Multiple independent Backgammon collections, useful match filters and batch actions, preview, reliable PR and comments at every level.

**Architecture:** Add logical collection ownership without changing existing public match/position responses. New library API supplies richer rows and bound cursors. Stable source-action IDs support cube notes separately from generated analysis decisions.

**Tech Stack:** Rust, Tokio, Axum, PostgreSQL/SQLx, GNUbg, React/TypeScript, Vitest, Playwright.

## 1. Database storage and API

- [x] Write failing `backend/tests/databases.rs` cases for independent copies, scoped deduplication, filter cursors, batch conflicts and exports.
- [x] Add migration `0009_databases.sql`, `backend/src/databases.rs` and database-aware import in `store.rs`; keep original endpoints compatible.
- [x] Run `DATABASE_URL=postgres://bgtutor:bgtutor_local@127.0.0.1:54329/bgtutor cargo test -p bgtutor --test databases`; require all cases pass.

## 2. Game and source move notes

- [x] Write failing `backend/tests/game_move_annotations.rs` for empty-game and cube notes, correct snapshots, stable lazy catalogs and search.
- [x] Add migration0010 and `move_catalog.rs`; extend existing annotations with game/move targets and versions.
- [x] Run targeted integration tests and old annotation tests.

## 3. Genuine GNUbg PR

- [x] Write native-reference tests for normalized losses, relevant decisions, weighted aggregation and missing/legacy results.
- [x] Extend analysis summary/checkpoints using native GNUbg statistics; publish explicit method and nullable PR.
- [x] Run focused PR and recovery tests.

## 4. Frontend library

- [x] Add red browser cases for collections, filters, copying independent notes and source move comments; confirm failure on missing controls.
- [x] Create `library-api.ts`, `library-messages.ts`, `DatabaseSidebar.tsx`, `LibraryFilters.tsx`, `MatchPreview.tsx`, and `LibraryDialogs.tsx`; replace thin MatchDatabase list with orchestration.
- [x] Add selected import database and multiple-file outcomes to App; preserve navigation and background jobs.
- [x] Run targeted Playwright cases against real backend after migrations.

## 5. Notes integration

- [x] Add game scope to NotesPanel and source move notes dialog using exact snapshots; extend annotation search navigation and translations in all six locales.
- [x] Verify checker notes use existing position target; cube notes never submit synthetic position jobs.

## 6. Integration and validation

- [x] Register modules/routes/OpenAPI, generate TypeScript contract, build UI and restart local backend.
- [x] Run fmt, Clippy, complete Rust tests, Vitest, theme tests, Prettier and Playwright; reproduce and repair failures with targeted tests.
- [x] Run private import corpus without copying private fixtures; inspect desktop/mobile UI screenshots.
- [x] Review independent domains and document behavior/limits in README. Preserve dirty work; no commit requested.

## Validation evidence

- `cargo test -p bgtutor -- --include-ignored --skip all_local_game_files_import_atomically_and_deduplicate`: **127 passed**, including PostgreSQL, native GNUbg, scoped imports, copy independence, source notes, search performance and recovery. Log: `/tmp/bgtutor-databases-core-tests.log`.
- `npx playwright test` in `web/`: **37 passed**. Includes collection CRUD, actual-result filters, copy, per-file import errors, tags, move, ZIP download, delete, native PR, empty-game notes, cube snapshots, shared drafts and late-search fencing. Log: `/tmp/bgtutor-databases-browser-tests.log`.
- Vitest: **27 passed**. Python theme suite: **6 passed**.
- Rust fmt, Clippy `-D warnings`, TypeScript/Vite build, generated OpenAPI/types, Prettier and `git diff --check`: passed.
- Desktop1600px and mobile390px visually checked. The mobile document has no horizontal overflow (scrollWidth390). Match list scrolling is bounded to600px; previews preserve game/move state across same-scope refresh.
- Independent reviews found and verified fixes for latest-PR fallback, position-job/deletion locking, stale annotation navigation, duplicate editor drafts, cube-offer display, deletion of an active workspace and preview refresh. Spanish/Italian match/game labels are distinct.
- **Private corpus qualification:** the optional full-corpus test was run, not silently skipped. User-owned input changed during work from37 to42 files. Current result:33 imports,9 strict XG rejections; independent format/CRC/history inspection confirms invalid markers, a missing footer or inconsistent board history. No parser relaxation or private fixture copying. See `docs/import-corpus-results.md`. The optional corpus test remains red for these inputs.
- Actual-result filters additionally support completed white wins, black wins and draws; incomplete leads and unknown scores do not match them.
- No commit requested for this iteration; all pre-existing dirty work preserved.
