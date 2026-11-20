# Match Analysis Time Estimate Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development or superpowers:executing-plans. Preserve the copied uncommitted baseline and transfer only feature changes back to the original checkout.

**Goal:** Display a benchmark-based estimate before starting the selected game or whole-match analysis.

**Architecture:** A read-only backend endpoint resolves the current engine installation and local machine, selects the newest complete compatible benchmark, counts the actual analysis scope and multiplies count by recorded average. A scoped frontend request renders approximate localized duration and provenance next to analysis controls without blocking submission.

**Tech Stack:** Rust/Axum/sqlx/PostgreSQL, React/TypeScript, Vitest/Playwright.

## Task 1: Backend estimate endpoint

Files: `backend/src/benchmarks/estimate.rs`, `backend/src/benchmarks.rs`, `backend/tests/analysis_time_estimate.rs`.

- [x] Add failing PostgreSQL route tests for `GET /api/matches/{id}/analysis/estimate?plies=2&game_no=1` and missing/newest/mismatched benchmark cases; verify 404 before adding routes.
- [x] Define estimate response with match_id, game_no, engine_id, engine_version, plies, position_count, estimated_ms, unavailable_reason and optional benchmark basis (id, finished_at, reference_name, average_move_ms, protocol_id). Query accepts optional game_no, plies and engine_id; omitted depth uses saved default.
- [x] Apply existing match-source and Competition review access rules. Count positions only in the selected match/game, not UI error/player filters. Invalid Ply/scope returns422, missing source404/422 using existing conventions; zero checker moves returns an unavailable reason, never an invented0-second estimate.
- [x] Resolve canonical executable and actual version using the running benchmark snapshot or a short version-only probe. Capture current machine outside measured work. No native evaluation, match job or benchmark is submitted; unavailable executable returns engine_unavailable with null estimated_ms.
- [x] Select latest finished completed full-coverage benchmark matching installation/version/Ply, machine identity/hardware, reference and analysis parameters. Accept completed v1/v2 protocol snapshots differing only in time limit. Reject partial/failed/cancelled/running and nonfinite/invalid timing. Use `estimated_ms = position_count * average_move_ms`.
- [x] Test single game and whole match counts, latest selection, version/path/hardware/Ply exclusions, zero/missing/source restrictions, unsupported/invalid queries and no extra work insertion. Run `DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/neondojo cargo test -p neondojo --test analysis_time_estimate -- --ignored --test-threads=1`.

## Task 2: Frontend estimate display

Files: `web/src/AnalysisTimeEstimate.tsx`, `web/src/analysis-time-estimate-api.ts`, `web/src/analysis-time-estimate-messages.ts`, `web/src/analysis-time-estimate.css`, `web/src/MatchAnalysisPanel.tsx`, minimal context props in `AnalysisWorkspace.tsx`/`App.tsx` only if needed, formatter unit tests and `web/e2e/analysis-time-estimate.spec.ts`.

- [x] Write failing tests for formatting duration and browser controls changing Ply/scope/match, no matching basis, server errors and stale responses. Avoid real long benchmark work.
- [x] Fetch selected match/game estimate while configured and no active/submitting job. Key/abort requests by match, game, scope, Ply and applied engine/settings/profile revision; never show a previous choice's estimate in the new render.
- [x] Render seconds, minutes/seconds or hours/minutes with ca./approximately. Include engine/Ply/move count and benchmark date/version, with short approximation/queue caveat. Use readable unknown/loading/error states without disabling start.
- [x] Use existing locale conventions and visible keyboard/screen reader state. Fit the command layout at desktop and narrow widths; no new confirmation dialog or runtime ETA.
- [x] Run scoped Vitest, Playwright and frontend production build.

## Task 3: Integration and verification

- [x] Independent spec compliance review, followed by code quality review; fix material issues and rerun affected checks.
- [x] Generate OpenAPI/types, document formula/basis/limits in README, update approved spec status.
- [x] Compare against saved baseline hashes; preserve concurrent original edits and regenerate generated API artifacts from the original checkout. Keep Rust target directories independent across worktrees to avoid stale binaries.
- [x] Verify original checkout build/tests and live endpoint using already saved real 0–4-Ply measurements. Restart the local service when no active engine work exists and verify visible estimate before start, with no extra analysis submission.

## Abschlussprüfung

- Schätzungsendpunkt: acht PostgreSQL/GNUbg-Integrationstests mit `--ignored` bestanden, einschließlich Legacy-Daten ohne Schreibzugriffe, Kompatibilitätsfiltern, Zugriffsregeln und unveränderter Jobanzahl.
- Regression: acht bestehende Benchmark-Integrationstests und 101 Backend-Library-Tests bestanden; 29 umgebungsabhängige Library-Tests bleiben standardmäßig ignoriert.
- Frontend: 28 neue Tests für Dauerformatierung und Antwortvalidierung; vollständiger Lauf im Originalcheckout mit 446 bestandenen Tests.
- Browser: 13 neue Fälle auf Vite bestanden. Ein zusätzlicher Live-Test gegen den Produktionsbuild prüft die echten 0–4-Ply-Schätzungen ohne neue Analyseaufträge.
- Live-API: Für 460 Züge entsprechen die Prognosen den gespeicherten Laufzeiten von 1,61 s, 2,44 s, 18,60 s, 104,59 s und 1.933,87 s. Desktop, 390-Pixel-Ansicht und Produktionsbuild geprüft.
- Eigenständige Entwurfs- und Codeprüfungen ohne offene wesentliche Befunde. Browsertests löschen ihre exakten importierten Fixture-UUIDs; frühere eigene Testimporte wurden nach Prüfung des unveränderten Original-SGF ebenfalls bereinigt.
- Änderungen im Originalcheckout, dort regenerierte OpenAPI-Typen und lokal neu gestarteter Dienst. Es wurde kein neuer Referenzbenchmark benötigt.
