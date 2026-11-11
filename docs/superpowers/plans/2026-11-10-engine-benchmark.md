# Engine Benchmark Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development or superpowers:executing-plans. Preserve the existing uncommitted working state; do not stage unrelated changes.

**Goal:** Implement the approved Extras → Benchmark screen, durable serial GNUbg/Ply measurements, reference and hardware snapshots, cancellation and history.

**Architecture:** Dedicated benchmark tables and module use the existing background worker and native full-game analysis. A process-wide engine reader/writer gate grants benchmarks exclusive engine access, with a task-local scope preventing recursive acquisition. Database updates condition on status and worker ownership. Benchmark measurements never resume after interruption.

**Tech Stack:** Rust/Axum/Tokio/sqlx/PostgreSQL, React/TypeScript, Vitest/Playwright.

## Task 1: Persistence and API

Files: `backend/migrations/0029_benchmarks.sql`, `backend/src/benchmarks.rs`, `backend/src/benchmarks/machine.rs`, `backend/src/benchmarks/reference.sgf`, `backend/tests/benchmarks.rs`, `backend/src/api.rs`, `backend/src/lib.rs`.

- [x] Write API integration tests before implementation: reference metadata with 10 games/460 moves, invalid/missing/unavailable engine rejection, distinct append-only runs, immutable machine/engine/reference snapshots, cancel queued run, history surviving another AppState, OpenAPI schema.
- [x] Run `cargo test -p bgtutor --test benchmarks -- --ignored`; before implementation expect missing routes (404).
- [x] Add dedicated tables with status/check constraints, machine UUID singleton and immutable snapshots; expose `GET /api/benchmarks`, `GET /api/benchmarks/reference`, `POST /api/benchmarks`, `POST /api/benchmarks/{id}/cancel`. Request `{engine_id:"gnubg", plies:[0,2]}`; list response `{runs:BenchmarkRun[],active:bool}`, create response array of new runs (202).
- [x] Validate nonempty distinct 0–4 depths, engine availability and version; resolve executable before storing. Embed a copied unanalysed WC2014 SGF with stable reference ID/version/SHA. Snapshot backend hardware without serial numbers.
- [x] Run tests and regenerate `docs/openapi.json` / `web/src/generated-api.d.ts` after integration.

## Task 2: Engine exclusivity and worker

Files: `backend/src/engine.rs`, `backend/src/benchmarks/runner.rs`, `backend/src/jobs.rs`, `backend/src/main.rs`, benchmark tests.

- [x] Add failing tests proving an ordinary engine invocation waits during exclusive benchmark access, cancellation drops/kills its child, and ownership guards prevent late writes after cancellation.
- [x] Add global Tokio RwLock reader gate around process execution, including version checks. Avoid recursive readers by using a private unlocked version helper. An exclusive benchmark scope bypasses the reader gate only inside the holder's task-local scope.
- [x] Check benchmark queue from the existing worker; serialize using database claim predicates. Recover interrupted running rows at server startup without combining partial timing. Use Instant around each full analyze_game call, exclude DB progress writes, validate full game and checker coverage, require original installed version throughout. Keep the existing 600-second per-game timeout explicit in protocol metadata.
- [x] Persist progress per game; poll cancellation while work runs and drop active native process. Fence all progress/completion writes by run ID, status and ownership. On failure/cancellation leave total and average null; retain progress.
- [x] Run backend tests, PostgreSQL API tests and real native 0-Ply completion for all 460 moves.

## Task 3: Extras view and browser behavior

Files: `web/src/BenchmarkWorkspace.tsx`, `web/src/benchmark-api.ts`, `web/src/benchmark-messages.ts`, `web/src/benchmark.css`, `web/src/App.tsx`, `web/src/SettingsDialog.tsx`, `web/e2e/benchmark.spec.ts`.

- [x] Add failing Playwright coverage for Extras → Benchmark, default selected depth, run creation, progress/cancel, history after navigation/reload and error rendering. Use API route fixtures for deterministic UI states.
- [x] Add an Extras workspace tab with Benchmark child text navigation; link Settings → Analysis to the same view, respecting dialog drafts. Screen polls only while visible; backend jobs survive closing it. Use enabled engines from existing catalog and multi-select Ply checkboxes, default current analysis depth only.
- [x] Show reference details, machine/backend information, measurement definition (total includes cube and startup / 460 checker moves), queue/running state and abort. History includes timestamp, machine, engine/version, Ply, reference/version, total, average and errors, sorted newest first. Null timings display dash.
- [x] Add translations through established locale fallback behavior, accessible labels/focus states and responsive overflow for the results table.
- [x] Run `npm test`, `npm run build`, and targeted Playwright tests at desktop and narrow viewports.

## Task 4: Review and delivery

- [x] Review specification compliance first, then code quality/cancellation/races independently. Fix material findings and rerun affected tests.
- [x] Document navigation, timing semantics, reference source and endpoints in README; update approved spec status.
- [x] Compare all changed files against the copied baseline and copy only benchmark changes back to the user's original checkout, detecting concurrent edits before overwriting.
- [x] Run final relevant verification in the user's checkout. Leave changes reviewable without an unsolicited merge or commit of pre-existing work.

## Verification results

- Backend: full non-ignored test suite passed using the local PostgreSQL test URL.
- Benchmark: eight PostgreSQL/native integration cases passed, including process termination, responsive catalog/health, expired owner recovery, reference coverage and stored average.
- Existing settings/native engine regression suite: fourteen ignored integration cases explicitly passed, including 3/4-ply real searches.
- Frontend: 332 Vitest tests, five deterministic Playwright benchmark cases and production build passed.
- Original checkout: feature-only transfer preserved concurrent README/API changes; API schema regenerated and local service restarted with migration.
- Real browser/worker/database/engine smoke test passed, creating retained 0-ply results and checking history after reload (explicit opt-in `BGTUTOR_NATIVE_BENCHMARK=1`).
- Independent spec compliance and code quality reviews passed after health/cancellation/recovery fixes.

## User change: unlimited benchmark execution (2026-11-10)

New protocol `gnubg-match-v2-unlimited` stores a null game timeout. The benchmark calls the optional-limit native pipeline with no deadline; ordinary match analysis still uses its existing bound. Legacy run snapshots preserve their numeric timeout. Added and passed red/green API metadata and bounded-vs-unbounded process tests; native benchmark cancellation integration and five browser tests passed. Full production build regenerated from the current original checkout, preserving newer study/profile changes. The prior active 4-ply attempt was explicitly cancelled and will be remeasured from the start; completed0–3-ply results are retained for comparison.
