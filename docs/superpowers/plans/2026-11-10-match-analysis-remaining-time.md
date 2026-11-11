# Match Analysis Remaining Time Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development. Preserve the copied dirty baseline. Rust build targets are independent worktree clones; do not share the original target directory.

**Goal:** Show an adaptive, persistent remaining-time estimate during native game/match analysis.

**Architecture:** Dedicated per-job timing persistence is updated within the existing lease-fenced game checkpoints. A read-only job timing endpoint computes remaining time from benchmark calibration, then measured throughput; frontend uses job updates plus a monotonic local countdown.

**Tech Stack:** Rust/Axum/Tokio/sqlx/PostgreSQL, React/TypeScript, Vitest/Playwright.

## Task 1: Backend timing and checkpoints

Files: `backend/migrations/0034_match_analysis_timing.sql`, new `backend/src/analysis_timing.rs`, edits to `backend/src/{lib,api,jobs,match_jobs,engine}.rs`, extraction of reusable benchmark lookup in `backend/src/benchmarks/estimate.rs`, `backend/tests/analysis_remaining_time.rs` and focused module tests.

- [x] Add failing tests for benchmark fallback, observed rate, game-size weighting, elapsed subtraction, overdue/unknown, queued/terminal, legacy resume, lease fencing and persisted reload state.
- [x] Persist total/completed moves, separately timed moves for legacy resumed work, elapsed confirmed pipeline milliseconds, optional benchmark ID/average snapshot, active game timestamp and worker ownership. Do not use created_at as active work time.
- [x] Initialize using the actual job engine snapshot, Ply and scope; select the same compatible completed benchmark as the start estimate. Native analysis is not launched for calibration. Keep the existing read-only estimate endpoint behavior.
- [x] Before each native game obtain the ordinary engine permit outside the active clock (task-local scoped exemption may reuse existing gate machinery), record active start, measure game pipeline with Instant, and persist confirmed timing in the existing checkpoint transaction under its lease. Preserve confirmed work on retry and reset interrupted active time. No stale-owner writes.
- [x] Add GET `/api/jobs/{id}/remaining-time`, with existing source/review guards, returning `{job_id,status,state,basis,remaining_ms,total_moves,completed_moves,timed_moves,measured_elapsed_ms,current_game_elapsed_ms,sampled_at,attempt}`. State is queued/waiting/estimated/reestimating/unavailable/finishing/completed/failed; basis benchmark/observed/null. Queue and engine waiting do not count down. Use r=T/timed_moves after a measurable completed section, otherwise benchmark average; R=r*(N-C)-active_elapsed. Overdue is reestimating, not zero. Terminal states have no remaining value. The endpoint is read-only and never consults mutable current profile for a running job.
- [x] Emit/touch ordinary job progress when timing becomes ready/active, so existing SSE and polling refresh the UI. Retain normal job retry and heartbeat semantics.
- [x] Run targeted tests with local PostgreSQL (`--ignored` where annotated), one small real native analysis, and previous estimate/benchmark regressions. Do not run a full 4-Ply benchmark.

## Task 2: Frontend countdown

Files: new `web/src/AnalysisRemainingTime.tsx`, `analysis-remaining-time-{api,messages}.ts`, `analysis-remaining-time.test.ts`, `web/e2e/analysis-remaining-time.spec.ts`; small progress-block integration in `MatchAnalysisPanel.tsx` and scoped CSS.

- [x] Write failing tests for persisted initial timing, benchmark→observed transition, weighted moves, monotonic countdown under browser clock skew, reestimating, absent data, queued, terminal and late-response/attempt changes.
- [x] Fetch the job timing endpoint on actual job changes (ID/status/attempt/updated_at/checkpoint). Retry unknown/initial states and network failures without engine work; existing status updates plus local 1-second timer drive the normal countdown.
- [x] Anchor elapsed subtraction to performance.now at receipt, not Date.now. Never show0/negative as completed while the job is running. Render approximate localized duration, basis label, waiting/unknown/overdue/finalizing states; hide after failure/completion. Preserve existing pre-start estimate behavior.
- [x] Keep screen-reader countdown updates quiet between meaningful checkpoints and clear intervals/requests on job switch or unmount. Do not refetch based on current profile while a job is running.
- [x] Run focused/unit suite, browser behavior, responsive view and production build. Any test-created imports/jobs use isolated fixtures or exact-ID cleanup.

## Task 3: Review and integration

- [x] Independent specification review, then quality review; resolve all material findings and rerun appropriate checks.
- [x] Update OpenAPI/types and document checkpoints, limitations, queue/retry semantics in README and approved spec.
- [x] Transfer feature-only changes after baseline comparison; preserve concurrent original edits and regenerate API artifacts from the original source tree.
- [x] Verify original build/tests, deploy locally only when no active engine work exists, and smoke-test a short native analysis plus reload and completed cleanup. Leave user changes reviewable without unrelated commits.


## Verified completion

2026-11-10: Specification review and independent quality review passed. The
original checkout preserves concurrent source and README changes; OpenAPI and
TypeScript contracts were regenerated there. Original verification:117 Rust unit
tests passed(31 optional ignored),7 dedicated timing integration tests passed,
482 frontend tests passed, frontend and backend production builds and formatting
passed. Additional checkpoint/lease/gate/native fixtures passed in the isolated
worktree. Nine deterministic browser cases passed.

A production smoke run used a uniquely named disposable reference copy with real
GNUbg at2Ply:10 games/460 moves,19.315s confirmed analysis. It verified benchmark
initialization, observed speed after the first game, persistence through reload,
390px layout and terminal cleanup. The exact fixture was deleted. No new benchmark
was needed. Local service restarted after verifying no active jobs or benchmarks.
