# Unified Analysis Catalog Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development for bounded tasks and requesting-code-review for final review. Preserve unrelated uncommitted work. The implementation and verification phases did not authorize commits; the subsequent user request authorizes a selective catalog commit.

**Goal:** One persisted analysis selection per match drives checker/cube review, move coloring, statistics and card creation independently of calculation defaults. Preserve every saved evaluation.

**Architecture:** Add a read catalog over existing jobs/checkpoints/imported records. Stable keys are `job:<uuid>` and `imported:<match uuid>`. Store the selected key and source revision in PostgreSQL; never copy or overwrite original results. Explicit selection never silently falls back or mixes evaluations. Automatic selection prefers a current completed whole-match result, then available imported analysis or a scoped completed result, without ranking engines by ply.

**Tech Stack:** Rust/Axum/SQLx/PostgreSQL, React/TypeScript, generated OpenAPI types, Vitest/Playwright.

## Contracts and invariants

- GET `/api/matches/{id}/analyses` returns a bounded paginated catalog (including standalone calculations), selected entry and next cursor. Selection remains accessible when off-page.
- PUT `/api/matches/{id}/analysis-selection` accepts `{analysis_id: string|null}` and persists a validated same-match/current-source selection. Null resets to automatic. Returns `{match_id, analysis_id}`.
- GET `/api/positions/{id}/analysis-result?analysis_id=...` reads exactly the chosen result. It retains native position candidates, native match decisions, imported candidates/cube records and missing values; no engine execution.
- Existing review and move-assessment endpoints accept `analysis_id`; omitted selection resolves centrally. Native results never merge imported or individual hints. Selected partial scope remains partial.
- Catalog native entries expose their existing Job where available. Job monitoring and submission retain their own hooks; these no longer choose displayed results.
- Statistics and cards carry the selected analysis identity. Imported statistics are only offered where normalized validated records support them; incomplete coverage and absent native PR stay explicit.
- Library availability is independent of the configured engine. Automatic practice/card result reads use the same selection and preserve provenance.
- No deletion or rewriting of existing job/import result rows. A migration adds selection metadata, canonical read views and a versioned derived import-validation cache. Backup handling includes the metadata.

## Task 1: Catalog and persistence

Files: `backend/src/analysis_catalog/`, new migration, `backend/src/lib.rs`, `backend/src/api.rs`, backup metadata policy, `backend/tests/analysis_catalog.rs`.

- [x] Write integration tests for catalog visibility across engines/imports, stable IDs, pagination, current-source validation, persisted selection and strict missing results.
- [x] Verify failure before implementation using an isolated PostgreSQL server.
- [x] Implement catalog, selection resolver, exact position result reader and OpenAPI registration.
- [x] Regenerate API contracts; verify tests and backup schema coverage.

## Task 2: Shared backend consumers

Files: `backend/src/review_decisions.rs`, `move_assessments.rs`, `analysis_statistics.rs`, `learning/content.rs`, `databases.rs`, `position_browser.rs`, `recurring_mistakes/source.rs`, associated tests.

- [x] Reproduce inconsistent selection with competing engines/imports and different severities.
- [x] Route consumers through the resolver, keeping unit/perspective validation and stable cursors.
- [x] Remove the `local-checker-analysis` fallback endpoint and automatic profile-engine filtering of saved results.
- [x] Make explicit card creation retain selected provenance, including imported results when supported.
- [x] Verify no cross-source fallback, no manufactured equity/PR, and preserved stored data.

## Task 3: Unified interface

Files: new catalog hook/selector/messages, `AnalysisWorkspace.tsx`, `AnalysisPanel.tsx`, `MatchAnalysisPanel.tsx`, `MatchAnalysisComparison.tsx`, `LearningPanel.tsx`, `useJobs.ts`, related tests.

- [x] Add a central selection shared by all analysis subviews, persisted via API, with loading/error and scoped-result states.
- [x] Replace imported/local-engine tabs by one result view; keep opening books as a separate optional reference.
- [x] Use catalog identity for review, marks, statistics and cards; ensure late responses cannot cross selections.
- [x] Limit calculation controls/profile engines to creating jobs; discover saved standalone results from the server.
- [x] Remove obsolete hook/selection code and localStorage result discovery.
- [x] Translate new labels into all six supported locales.

## Task 4: Integration and verification

- [x] Add browser coverage for Match-28 pattern, imports, shared source changes, reload/profile changes, missing scopes, and no automatic jobs.
- [x] Run focused integration tests, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, frontend tests/build and affected E2E tests.
- [x] Review spec compliance and code quality; fix findings.
- [x] Update README/navigation docs and visually inspect relevant desktop layouts.
- [x] Build, safely restart the local service with its existing configuration, and verify preserved Match 28 analyses through the live catalog.

## Verification record

These results describe the integrated working tree before selective commit preparation; unrelated work was present during those runs.

- Catalog, shared review/statistics/marks, position/card creation, library, recurring mistakes, copying and retention were exercised against isolated PostgreSQL databases; affected native paths also ran with GNUbg.
- All 570 frontend tests and the production build passed. The 30 affected browser cases passed across targeted runs; the catalog was visually checked at 1800×1169, 2560×1440 and 3840×2160.
- Rust formatting and Clippy passed. The default workspace run finished successfully: 260 passed, 402 ignored; selected ignored PostgreSQL/GNUbg targets were run separately as described above. The final workspace run uses an exclusive test server after another concurrent test process collided on SQLx test-database names. Optional integration suites outside the affected targets were not all executed.
- Database-only EXPLAIN measurements use synthetic data in an isolated database: 1,000 matches, 10,000 saved positions, 6,000 completed jobs, 60,000 native decisions and 4,920 validated imported projections. Final single-run execution times were 205 ms for the position page and 99 ms for the library page (50 rows each); the missing-summary provenance case took 206 ms. Measurements ran alongside build work and do not establish end-to-end UI latency.
- A full pre-migration database dump was created and its archive catalog checked. Checksums of all 173,395 previously stored original-source, job, checkpoint and decision rows were unchanged after migration and restart; concurrent work added new rows.
- Live Match 28 exposes four retained catalog entries, automatically selects its complete 2-ply match analysis, and reports 21 games and 912 checker decisions. Browser verification submitted no calculation jobs.
- No commits or pushes were made. Existing unrelated workspace changes were preserved.

## Follow-up integration verification — 2026-11-15

The remaining catalog-related integration checks were run against three dedicated PostgreSQL servers, separate from the application database. Selected test targets use `--include-ignored`; native/SQLx library tests use `--ignored`. GNUbg was first verified with a real opening-position evaluation and its installed weights. Native Open Sage checks use both the installed optimized fork and the retained original interpreter where the test requires them.

The completed backend scope contains 277 distinct tests:

- Catalog and decision consumers: `analysis_catalog`, `analysis_statistics`, `move_assessments`, `review_decisions`, `match_analysis`, `learning`, `position_learning`, `position_browser`, `recurring_mistakes`, `recurring_mistakes_imported`, `recurring_mistakes_native`, `workflow`, `open_sage`, `open_sage_settings`.
- Data retention and import: `database_backup`, `imported_analysis`, `import_api`, `databases`, `saved_position_retention`, `position_bookmarks`, `position_decks`, `library_occurrences`, `library_sorting`, `study_reports`.
- Native/SQLx library groups: engine evaluation, match analysis, job checkpoints and recovery, XG review query plan, practice evaluation, both opening books, position retention and similar-position practice/history. The database-pool test runs in the binary test target.

Corrections preserve the catalog contract: empty jobs/import placeholders do not become analyses; in-progress jobs do not replace existing imported results; explicit selections keep their scope and provenance. The review query-plan test now binds the source-selection parameter and verifies non-empty results. The Open Sage benchmark rejection case uses unsupported 5-ply, preserving supported 4-ply.

The backup roundtrip now compares the exact selected source revision, native history/checkpoints/decisions, original import records and derived projections, including a NULL projection. Removing the selection row from the test archive makes this assertion fail; restoring the full archive passes. Similar-position practice verifies that scoring/revealing creates no implicit job or selection and that an explicit older analysis remains selected despite a newer result.

The wider browser coverage exposed two UI defects: the analysis submission error appeared twice because the same hook error was rendered through two aliases; a long database name overflowed the existing narrow-window heading. The duplicate rendering was removed, and the heading now wraps within its existing narrow breakpoint. The existing alert-count test and the viewport test with an explicit long-name fixture cover the fixes.

Browser fixtures now use the shared catalog selection, real checkpoint/import values, server-side position-job discovery and current labels. The full-match helper locates the exact match UUID across library pages, including older deduplicated imports; it does not substitute a match by player name. Backup-browser checks account for the test data already present before export.

Final Rust formatting and Clippy passed; the default workspace suite passed 260 tests (404 ignored by default, with the catalog-related cases exercised separately above). All 570 frontend tests and the production build passed after both UI fixes. The combined 154-case browser run passed with no failures or skipped cases, including both native Open Sage flows. Desktop checks cover 1800×1169, 2560×1440 and 3840×2160; the narrow-window regression uses 390px. The one earlier statistics-browser case using the modified pagination helper also passed separately, for 155 distinct browser cases in this follow-up. The final review found no blocking issues or weakened domain assertions. No user database was used for these tests, and the dedicated test servers were stopped afterward.

## Selective commit verification — 2026-11-15

The catalog changes were reconstructed over `8ea7157` in an isolated tree to
exclude unrelated open profile, playing, engine, analysis-deletion and Anki work.
Migration 0068 remains byte-for-byte unchanged; the unrelated migrations 0063–0067
are outside this commit. Existing working-tree files were not rewritten.

The isolated candidate passed Rust formatting, Clippy and the workspace suite
(243 tests; 394 ignored by default), plus 177 targeted PostgreSQL/GNUbg/Open Sage
integration tests on a dedicated test server. API contracts were regenerated
from this candidate. All 572 frontend tests and the production build passed.
Across the targeted browser runs, all 49 distinct cases passed after corrections;
the complete optional integration/browser suites were not rerun in this isolated
step. The catalog case checks 1800×1169, 2560×1440 and 3840×2160 layouts.

The isolation checks retained a review filter while catalog data reloads for
another position. Browser assertions now check engine provenance in its tooltip,
explicitly select a newly completed result, and delay the position request used
by navigation to verify that the latest click wins. The duplicate-alert and
narrow-heading regression cases also passed. A final bounded review found no
blocking issues or weakened domain assertions.

## Port to origin/main

The cherry-pick onto `e5b51b6` retains the target branch's command bar,
next-error navigation, and cube-decision dialog. That dialog uses the selected
native decision when opened from native review. Switching to another move in
the dialog keeps its board and notation without loading imported evaluations.
The dice distribution restores calculation jobs for the configured engine.
Displayed checker and match analyses continue to use the persisted catalog
selection across engines.

The upgrade from schema 0062 to 0068 preserved checksums for 87,920 synthetic
rows spanning matches, positions, jobs, checkpoints, and original evaluations.
Rust formatting, Clippy, 243 workspace tests, 185 targeted integration tests,
564 frontend tests, and the production build passed on the target branch.
All 83 affected browser cases passed across the targeted runs. After the
last fixes, 26 cases covering results, dialogs, dice, filters, and delayed
navigation passed again.
