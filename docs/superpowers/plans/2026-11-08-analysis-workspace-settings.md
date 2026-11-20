# Analysis Workspace and Settings Implementation Plan

> **For agentic workers:** Use independent backend/settings-dialog tasks with explicit file ownership; root integrates navigation and workspace. Preserve all uncommitted import/game-selection work. Execute in the current feature branch because it contains the required existing implementation; do not reset, stash, or commit unrelated changes.

**Goal:** Deliver the approved first stage: analysis-first tabs, compact board workspace, separate existing library and learning views, and a real persisted settings dialog.

**Architecture:** Keep match/game/position selection in App while mounted views preserve drafts, job subscriptions and searches. Extract AnalysisWorkspace and MatchDatabase from the current App. Use the existing REST/SSE/board/annotation/learning components. Backend app settings and per-job configuration snapshots make engine changes durable without changing running jobs.

**Tech Stack:** Existing Rust/Tokio/Axum/SQLx/PostgreSQL/GNUbg and React/TypeScript/Vite/Playwright.

---

## Task 1 — Regression contracts before implementation

Files: `web/e2e/workspace.spec.ts`, `web/src/i18n.test.ts`, `backend/tests/settings.rs`.

- [x] Add a browser regression that imports a unique two-game SGF and checks Analysis / Match database / Learn tabs, one active main tab, no permanent library sidebar in analysis, board and move list together, local Evaluation / Errors / Notes tabs, note draft retained across main-tab switches, and direct database-to-analysis navigation.

```ts
await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'de'));
await page.goto('/');
await expect(page.getByRole('tab', { name: 'Analyse', exact: true })).toHaveAttribute('aria-selected', 'true');
```

- [x] Run `npx playwright test e2e/workspace.spec.ts` before navigation edits; expect failure because the main tab does not exist.
- [x] Settings tests cover default English vs retained saved language, alphabetic language labels, optimistic settings persistence, invalid path rollback and revision conflict, and native engine check. Run targeted tests before implementation and confirm the missing endpoint/default failures.

## Task 2 — Durable engine settings (backend-owned)

Files: `backend/src/settings.rs`, migration `0008_app_settings.sql`, existing api/main/jobs/store/engine modules and tests.

- [x] Add singleton settings with GET/PUT revision contract:

```ts
type AppSettings = {
  version: number; player_name: string;
  analysis_engine: string; analysis_plies: number;
  play_engine: string; play_plies: number; gnubg_path: string;
};
```

- [x] GET `/api/engines` returns capabilities and availability; only GNUbg is usable. POST `/api/engines/gnubg/check` accepts `{path}` and returns `{version}` after real version/evaluation check with bounded time.
- [x] PUT validates path/engine/profile/name before activation, uses the expected revision and returns 409 on stale writes. Existing values survive failed validation.
- [x] Capture immutable engine configuration for each new job, including queued/recovered jobs. Changes affect subsequent jobs and native MAT imports. Preserve scope/plies/config deduplication and existing API behavior for explicit plies.
- [x] Run settings integration tests, existing workflow/match/learning regressions, fmt and clippy. Do not restart the dev backend until root integration is ready.

## Task 3 — Settings dialog (UI-owned)

Files: new SettingsDialog, settings API/messages/CSS, preferences, i18n-core and tests.

- [x] Implement agreed component contract:

```ts
type AppliedSettings = { theme: Theme; locale: Locale; settings: AppSettings };
type Props = { open: boolean; theme: Theme; onClose(): void; onApply(value: AppliedSettings): void };
```

- [x] Categories General / Analysis engine / Play engine / Board: real API values, name, language, engine/profile/path test, independently stored play profile, all local ZIP/builtin themes with genuine board preview. Disabled Sage option clearly marked planned.
- [x] Native accessible modal, focus restore, cancel discards drafts, save applies only after backend success, stale requests aborted, revision/errors retain edits. English is the fallback for new installations; explicit old languages persist. Local preferences tolerate unavailable storage.
- [x] All new text translated into six languages. Check key and placeholder parity and write focused dialog browser tests for root to run after integration.

## Task 4 — Navigation, analysis workspace and library (root-owned)

Files: `web/src/App.tsx`, new AnalysisWorkspace/MatchDatabase/useWorkspaceNavigation/workspace-messages/workspace.css; existing analysis/job hooks and panels as necessary.

- [x] Three real main tabs, Analysis first, database and learning separate. No nonfunctional play tab. Settings button in global header. Existing import formats and all entry points remain accessible; database retains current match/text/annotation/exact-position searches and SGF export.
- [x] Split analysis into match/job toolbar, left moves, center board, right Evaluation / Errors / Notes; Cards under learning subview in the same detail area. Reuse and reposition existing components, keeping hidden panels mounted for subscriptions/drafts.
- [x] Compact scoped analysis controls above board; summaries/errors appear only in Errors detail, with position analysis/candidates/distribution in Evaluation. All controls retain actual engine/plies context and errors. Selected-game remains default; full-match option persists.
- [x] Preserve active match/game/position across main tabs and reload via a validated saved position UUID (empty-game selection saved separately). Restore does not start analysis. Newer user selections supersede restore. Keyboard move navigation ignores text fields/dialogs.
- [x] Reuse persistent settings defaults for position/distribution/game/match jobs. Keep actual displayed-result depth independent from next-run selection. Settings modal does not unmount workspaces or stop jobs. A global background job indicator returns to the current analysis.
- [x] CSS supports 1440/1024 desktop and 390/320 mobile. Internal lists/tables may scroll; the page must not widen. Real ZIP assets remain unchanged.

## Task 5 — Integrate and verify

Files: generated OpenAPI/TS, existing E2E suites updated to real navigation and settings entry points, README/spec status.

- [x] Regenerate with `npm run api:types`; build Rust and apply migration by controlled dev-backend restart. Keep PostgreSQL data intact.
- [x] Run `cargo test -p neondojo -- --include-ignored` with PostgreSQL and local GNUbg, including optional local corpus; inspect all results.
- [x] Run frontend Vitest, themes tests, production build, fmt/clippy/Prettier and diff whitespace checks.
- [x] Run all browser tests after source edits have settled, adapting old tests to the new tabs/dialog/default language while preserving their behavioral assertions. New tests exercise save/cancel/conflict/default profile, engine path validation, genuine theme preview and drafts/subscriptions across views.
- [x] Review the completed integration independently; fix concrete findings and rerun affected checks. Visually inspect screenshots of analysis, database and settings on desktop/mobile. Record actual results and any intentionally later database-management features. No commit requested in this turn.


## Completed verification — 8 November 2026

- Full PostgreSQL/GNUbg Rust run: **99 tests passed**, none ignored. Includes ten settings/snapshot tests, version fencing of queued/recovered jobs and existing match checkpoints, all 37 local XG/MAT/BGF files (17 imports, 20 duplicates), search plans, annotations, learning and SSE.
- Frontend: **26 Vitest tests**, **six theme-tool tests**, generated OpenAPI/types, TypeScript and production build passed.
- Chrome: **31 Playwright tests passed** in the final complete run. Preserved import, exact search, scope navigation, complete native analysis, all ZIP themes, six languages, annotation drafts/conflicts, learning/FSRS and APKG flows. New regressions cover navigation/settings, late settings responses, accepted jobs across source switches, roving inspector tabs, single error reporting, player-filtered error jumps and exact last-position restoration without automatic analysis.
- Independent backend and frontend reviews found version fencing, an old initial settings response, accepted-job tracking, inspector keyboard handling and duplicate error rendering. These were fixed and rechecked; no remaining reported findings.
- Manual Chrome inspection: analysis, database and settings screenshots; board navigation and inspector visible at1366×900, no horizontal document overflow at1024/390/320; real theme preview works and no browser runtime errors observed.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, Prettier and `git diff --check` passed. Migration0008 is applied in the running local backend; Vite and the production build are current. No commit requested or made.
- Later stage intentionally retained: metadata/backfill, advanced database filters and bulk management/rename/delete/original+ZIP export. Playing and BGSage are clearly unavailable future integrations; existing GNUbg, learning and SGF export remain functional.
