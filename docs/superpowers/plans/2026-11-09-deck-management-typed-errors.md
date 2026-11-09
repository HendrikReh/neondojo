# Deck management and typed error statistics

> For agentic workers: use the parallel-agents workflow for the independent deck backend, deck frontend and statistics domains; root integrates, reviews and verifies. Preserve current behavior and private data.

**Goal:** After committing the approved themes/game-plan state (b2bf4b9), add deck rename/delete and distinguish checker/cube errors throughout the native match/game statistics and their clickable filters.

**Approved design:** Extend existing Learning & Anki deck controls rather than adding a new workspace. Rename retains deck identity, membership, card identity and FSRS state. Delete removes only the named deck/memberships, after an explicit name/count confirmation; learning cards, reviews, saved positions, other decks and their content remain. Re-adding the same decision recovers its existing shared card/history. A deck version detects stale rename/delete requests; membership additions advance that version. Local drafts survive errors/conflicts. Successful mutations refresh all deck selectors and position-membership displays. Existing card preview, pagination, study/reveal/review and Anki export remain available. Deck selectors recover gracefully when the current deck vanishes; no late request may restore a removed deck or stale study card.

Native statistics use existing durable move_analyses, grouped by absolute player, checker/cube kind and severity in the same repeatable-read snapshot as job/game coverage. No engine rerun or PR recalculation is needed, including for earlier completed jobs. Add nullable error_breakdown to job and per-game statistics responses: an array of two player objects `{player,checker:{doubtful,bad,very_bad},cube:{doubtful,bad,very_bad}}`. Zero is shown only for available durable data; unavailable/inconsistent legacy coverage remains null. Preserve all existing aggregate summary fields and persisted job/checkpoint JSON. In the UI, replace the four severity rows with a compact Total/Checker/Cube matrix; preserve old total actions and add kind-aware drilldown. Single-game statistics pin the correct source game. Good/zero-loss and forced/unavailable rows must not inflate error counts or filtered results. Six languages, semantic theme tokens, desktop information density and keyboard/dialog behavior are required.

## Task 1 — Deck backend

Files: backend/migrations/0024_deck_versions.sql; backend/src/learning/mod.rs (optionally focused deck module); backend/tests/deck_management.rs.

- [x] Write HTTP/PostgreSQL regression tests first: trim/validation/unknown body fields, rename retaining identity/cards/reviews, fresh/stale version handling, deletion retaining shared/sole cards/history and other decks, missing deck, membership-version advancement/concurrent mutation, OpenAPI.
- [x] Implement `version bigint NOT NULL DEFAULT 0 CHECK(version>=0)` and return it from all deck creation/list reads.
- [x] `PUT /api/decks/{id}` accepts `{name,expected_version}` and returns Deck; `DELETE /api/decks/{id}?expected_version=n` returns204. Missing404, stale409, invalid422 JSON errors. Use existing deck row lock order; no learning-card deletion.
- [x] New memberships increment version inside existing write_cards transaction; duplicates/no-op transfers do not. Publish contract promptly to frontend/root. Run focused tests on SQLx-isolated temporary databases, not main data.

## Task 2 — Deck frontend

Files: web/src/LearningPanel.tsx, new DeckManagementDialog.tsx / deck-management-api.ts / messages/CSS/tests, hook integration only where necessary; web/e2e/deck-management.spec.ts.

- [x] Add tests first for API versions/204, cancel/conflicts, stale responses/deck switches and refresh behavior; browser RED on old staged app if available.
- [x] Place Rename/Delete beside selected deck. Dialogs name their target, preserve errors/drafts and trap/restore focus. Delete confirmation explains position/history preservation and shows count. Disable mutation while pending, recover 404/409 by refreshing without losing rename draft.
- [x] Notify shared deck revision after success; invalidate deleted study session/preview/pagination immediately, select remaining deck or empty state, clear/recover persisted selection and pending external requested-deck token. Rename preserves current study/progress/selection.
- [x] Localize six languages and use existing appearance tokens; verify Anki/new labels reflect renamed deck. No unrelated styling or feature changes.

## Task 3 — Typed statistics (backend and frontend)

Files: backend/src/analysis_statistics.rs, backend/tests/analysis_statistics.rs (or focused new file); web/src/match-statistics-api.ts, MatchStatisticsView.tsx, review-statistics-messages.ts, match-statistics.css, unit tests and web/e2e/typed-error-statistics.spec.ts.

- [x] Write mixed checker/cube/player/game fixtures first, including good zero-loss, forced/unavailable, game scopes, old JSON and partial checkpoints. Verify failure before implementation.
- [x] Add optional breakdown fields to endpoint-only schemas; aggregate checkpointed decision rows once per response and verify coverage. Retain legacy totals/PR semantics and no job/engine/database writes.
- [x] Extend StatisticsAction with optional checker/cube kind; combine it with severity/player/game without inherited filters. Render compact Total/Checker/Cube matrix and preserve total actions/labels and good-zero filtering. Available zeros are actionable; unavailable data is an em dash with explanation.
- [x] Verify both source-game and whole-match drilldowns, locale/theming, refreshed statistics after analysis and no layout regression.

## Task 4 — Integration, review and activation (root)

- [x] Generate docs/openapi.json and web/src/generated-api.d.ts after backend contracts settle. Root owns generated files, README and this plan.
- [x] Run relevant PostgreSQL integration suites including shared learning/retained positions/statistics, whole frontend units, format checks, Rust/frontend builds, focused and existing browser regressions on a disposable service/database. Check exact callback/filter compatibility, desktop1800×1169 and4K, old and fresh analysis data.
- [x] Read-only independent code reviews for both domains; address concrete findings and update README status/usage.
- [x] Back up main PostgreSQL privately, verify no active jobs before restart/migration, build/activate verified artifacts, read-only main smoke (allow only pure game-plan POST), compare user settings/data and retained learning state. Clean only task-owned disposable service/database/artifacts. Keep demo data out of Git.
- [x] Report initial commit and final new functionality/tests. Do not silently amend the initial commit; additional feature commit only if explicitly requested.


## Verification and delivered state

- Initial requested snapshot committed as `b2bf4b9` before new implementation. New deck/statistics changes deliberately remain separate and uncommitted.
- Final frontend: 277 tests across47files pass; TypeScript/production build, whole-web Prettier, cargo fmt and git diff whitespace checks pass. Backend unit92pass/26existingignored;44 distinct PostgreSQL integration tests pass across deck management8, statistics8, learning6, position learning11, deck membership2 and saved-position retention9. SQLx integration databases are isolated from main.
- Focused new browser cases were RED on the old service and GREEN on the new service. Whole browser regression ran114cases (two existing expensive3/4-ply engine cases excluded),113passed and exposed accepted Anki download cancellation after a deck switch. After preserving accepted downloads, all12 affected deck/learning/notes browser cases passed. No unresolved regression remains.
- Independent domain reviews completed. Fixed premature requested-deck token consumption during pending list refresh, lost invalidation when rename/review revisions batch, hidden per-game severity cells in desktop overview, and explicit keyboard focus wrapping. Rename retains later-page preview/pending study answer; version conflicts require refreshed details and explicit resubmission.
- Verified real shared-card FSRS history, renamed position memberships/selectors and Anki filename, delete-one-of-two-decks, empty/deleted requested deck recovery, checker/cube severity drilldown for whole-match and single-game scope, positive and zero counts, and desktop1800×1169/3840×2160 in light/dark.
- Backed up main PostgreSQL privately, verified idle jobs and unchanged backup snapshot, activated additive migration24 and tested artifacts on8787. Final read-only main smoke recorded no unexpected write requests or page errors. Main setting equality and content digests for decks(excluding newly added version), memberships, cards, reviews, review sessions, saved positions, positions, annotations, jobs and game checkpoints all match the pre-migration snapshot. Counts preserved:33matches,47936positions,2savedpositions,65decks,72memberships,71cards,64reviews,26jobs. Native summaries/PR/checkpoints were not rewritten.
- Task-owned disposable service/database and staging/rollback bundles removed. Latest private backup retained outside Git. No private/demo data added to Git.
