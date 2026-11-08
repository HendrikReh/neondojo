# Desktop Analysis Implementation Plan

> Execute coordinated React integration and independent CSS, move-assessment and match-result tasks in this session. Apply subagent-driven-development / dispatching-parallel-agents with strictly owned files; root reviews and verifies the combined result.

**Goal:** A dense desktop overview keeps board, moves, candidates, errors and statistics visible and incorporates the user's follow-up corrections without losing existing workflows.

**Architecture:** Reuse mounted analysis panels, their existing source selection and single native job subscription. An overview detail state exposes all core panels through CSS grid, while prior detail states preserve focused workflows. Game-bounded stored move assessments supply traffic-light annotations. Repair match-result derivation at the source-to-metadata boundary.

**Tech Stack:** React/TypeScript, CSS Grid, Vitest, Playwright, Rust/Axum/SQLx/PostgreSQL.

- [x] Add browser regression coverage before implementation: import into a disposable backend, assert the default Overview tab and simultaneous candidate/error/statistics panes at the three target desktop sizes; observe the missing-overview failure.
- [x] Extend `AnalysisDetail` with `overview`, default to it in `App.tsx`, and keep the existing evaluation/errors/statistics/notes/cards views. Move the detail navigation above the grid; use `.analysis-overview`, `.overview-evaluation`, `.overview-errors`, `.overview-statistics` and `.workspace-match-results` to expose a single instance of each panel.
- [x] Make `MatchAnalysisPanel` accept `overview` and `selectedPositionId`, suppress redundant summary/progress in overview results, render progress once in the command area, keep statistic drills in the visible errors pane and indicate the selected position. Retain the standalone component defaults.
- [x] Add narrowly scoped `desktop-analysis.css`: full analysis width, four columns at laptop/large desktop sizes, independent list scrolling, bounded board, dense statistics/candidate/error rows and useful smaller-desktop fallback. Existing non-analysis pages keep their layout. Validate visual geometry, not just class names.
- [x] Use failing label/markup tests to rename board controls to Initial/Arrows/Final, omit the redundant played row, preserve a selected candidate's reset action and remove the analysis header's database shortcut.
- [x] Delegate stored move assessment endpoint/hook with SQLx red/green tests. Scope requests to the selected game/job/checkpoint, retain source/depth, neutral missing values, and integrate accessible traffic lights without fabricating good moves.
- [x] Delegate result diagnosis, source-derived result repair and the redundant table filename removal. Prove the last-game defect with fixtures and preserve user-edited metadata. Coordinate any additive migration and verify it before touching main data.
- [x] Update six-language catalogs, relevant existing keyboard/label tests and documentation. Run targeted red/green tests, then the full frontend/native suites and isolated browser regressions; inspect desktop screenshots and retain the original demo fingerprint except any explicitly verified source-result repair.
- [x] Review spec compliance and code quality, address findings, clean up the disposable backend/database, rebuild the normal client and report actual verification and limits.

## Review and regression evidence

- Default-overview browser checks were first observed failing because Overview was absent. Final checks cover the five visible areas, exact preview controls, persistent notes and statistic-driven error filtering at the three requested desktop sizes.
- Board label/default-row and once-only overview progress tests were observed failing before their fixes.
- Independent review caught a tab-dependent default filter, missing current/background individual-hint invalidation, desktop CSS cascade order, and a high-cube BGF result overflow. Each behavioral issue has a reproduced failure followed by a passing regression. CSS is loaded after the existing styles in main.tsx.
- Completed individual or match jobs invalidate game assessments through the existing background job tracker. This works after navigating to another move and does not move the selected board. Unknown/unavailable assessments remain neutral.
- The overview initially filters errors only. Legacy pagination tests now explicitly select all decisions; database PR tests follow the removed filename column. Their original behavioral assertions remain intact.
- Full Rust integration/native/book suite: 227 passed, zero failures, excluding the opt-in all-private-files-must-import test because the nine intentionally retained rejected XGs are unchanged. After the final high-cube guard, all 43 import tests passed, including the additional overflow regression. Frontend: 102 tests passed in 22 files. TypeScript/Vite production build, rustfmt and Clippy passed. The existing Vite main-bundle advisory remains.
- Browser suites use only the disposable PostgreSQL database/backend on 8788. The normal backend's user-started 200-game analysis is allowed to finish before its restart and metadata repair.


## Final activation

- All 58 browser tests passed against the disposable backend; all 102 frontend tests, TypeScript/build/format and Clippy passed. Screenshots were visually inspected at 1800×1169 and 3840×2160; the 2560×1440 case also passed viewport geometry checks.
- The normal backend was restarted only after the user's 200-game job completed. A second private PostgreSQL backup includes its finished result. Its result checksum remains identical after activation.
- Migration 0018 succeeded. Source-derived results were corrected in 21 existing matches; WC2014.xg is 11–9, test4.bgf 9–11 and test5.bgf 11–9. All canonical SGF hashes, original-source/library fingerprint, annotations, imported analysis, decks, cards and reviews match the pre-test snapshot. User metadata revisions were preserved.
- Read-only main-library browser smoke: 41 move-history rows, 41 native assessments, 50 loaded error decisions, 200 statistics game rows, no page errors and no write requests. At 2560px the document width remains 2560px. The game assessment endpoint returned 41 native rows in 60.0/51.6/56.0ms local samples; no universal latency guarantee is inferred.
- The disposable backend and test database were removed. The regular backend remains healthy on 8787 with GNUbg 1.08.003. Private screenshots, backups and verification manifests remain outside Git.

## Follow-up: analysis depth and redundant global banner

User requests standard search depth2 and optional3-ply, plus removal of the duplicate global analysis-run indicator. Reuse the persisted default for position/dice/game/match requests, expose0–3 consistently in REST/OpenAPI/UI, and retain the separate future playing profile0–2. Migration 0019 widens constraints and changes only untouched legacy defaults; explicit saved profiles survive. Apply this user's requested2-ply profile through the versioned settings API after activation. Old jobs/results keep their submitted depth and snapshot. The global banner is removed while background tracking and scoped progress/errors remain.

- [x] Reproduce missing3-ply/default2 and visible duplicate banner in targeted tests.
- [x] Extend native backend/ranges/storage/schema; prove actual native 3-ply output on a close-choice position and native game/dice path.
- [x] Update client default/options/settings parsing and remove only the global banner plus obsolete CSS/messages. Preserve background completion refresh.
- [x] Verify UI settings persistence, native job payloads, background polling and relevant full regressions; safely activate and preserve library data.


Follow-up verification: 231 Rust tests passed with the established GNUbg 1.05.002 regression baseline, 106 frontend tests passed and 59 browser tests passed. Actual native 3-ply output was additionally checked on 1.08.003 for position, all 21 rolls, game, whole-match and scoped jobs. Two unchanged exact-value native reference assertions differ under 1.08.003; they pass with their original 1.05.002 baseline, so those assertions were retained. TypeScript/build, Prettier, rustfmt and Clippy passed. Independent frontend/backend review found no remaining issue.

Migration 0019 is active in the normal library. This user's explicitly saved 0-ply profile (revision 203) was changed to 2 through the existing versioned settings API, preserving player/path/play fields. Original library and completed 200-game result fingerprints remain identical. The private backup predates activation; the disposable depth-test backend/database was removed.


## Follow-up: zero-loss decisions in the error list

- [x] Trace/reproduce: normal `/moves?errors_only=true` returns no Good/zero-loss decisions; false returns them correctly. UI Reset and statistics drills could switch to false while retaining the Errors heading; focused entry also defaulted to false.
- [x] Observe failures before implementation: focused default checkbox unit test fails; native browser regression fails after Reset because errors-only becomes unchecked.
- [x] Use one stable errors-only default for overview/focused entry and scope/Reset; preserve deliberate all-decision/statistics drill filters and label them Decisions by move in all six locales. Preserve server queries, stored losses, job identities and pagination.
- [x] Targeted green: 107 frontend tests and 11 browser tests, including real native bad and perfect checker decisions, resets, explicit all-decision toggle and statistics drill. Independent read-only review approved.
- [x] Complete full browser regressions, read-only main-library smoke and remove the disposable test library. No backend/schema/result mutations are needed.

Error-filter final verification: all 60 browser tests and 107 frontend tests passed; TypeScript/Vite build, formatting and diff checks passed. Read-only main-library smoke confirms 50 real error rows with no zero losses or Good ratings after Reset; explicit all-decision drill-down uses the correct heading. No write requests or page errors occurred. The isolated backend/database was removed; the normal backend stayed running throughout.
