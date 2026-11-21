# Frontend and E2E guidance (`web/`)

These rules extend the root [AGENTS.md](../AGENTS.md) for the React/TypeScript client and Playwright browser tests under `web/`.

## TypeScript, API contract, and React state

- Use generated API types from `web/src/generated-api.d.ts` (regenerated via `npm run api:types --prefix web`) and shared HTTP error handling. Validate external data at the boundary and avoid `any` or assertions that hide contract mismatches.
- Clean up effects and SSE subscriptions explicitly, and prevent delayed or stale responses from overwriting a newer selection.

## Desktop UI, navigation, and localization

- Prioritize desktop information density and mouse and keyboard operation. Keep additional mobile product work within explicit scope.
- Follow [typography](../docs/typography.md) and [UI navigation](../docs/ui-navigation.md). Reuse tokens and components for color, spacing, and typography.
- Keep header rendering and keyboard navigation driven by the same workspace order in `web/src/App.tsx`. When changing that order, update the [analysis reference](../docs/reference/analysis.md#analysis-workspace), [UI navigation](../docs/ui-navigation.md), and [text-navigation.spec.ts](e2e/text-navigation.spec.ts).
- Localize visible text and format numbers consistently across all six languages: English, German, Spanish, French, Italian, and Polish.
- Preserve focus, accessible labels, table controls, and unsaved drafts across loading, empty, error, conflict, and disabled states.
- Visually verify affected desktop layouts at relevant logical viewports such as `1800 × 1169`, `2560 × 1440`, and `3840 × 2160`.

## Workspace-specific UI invariants

- **Study:** Reuse the checker board, cube choices, and result components. Keep answer validation and equity evaluation in the shared Rust practice code (`backend/src/practice.rs`), and preserve the distinction between checking an answer and submitting an FSRS rating. [Study](../docs/reference/study.md#study)
- **Dice equity:** Start missing dice equity through the existing position-job flow once the visible Dice equity tab has loaded the selected position's saved and active jobs. Use the active analysis engine and profile depth for automatic requests. Preserve explicit retries after failures and verify result reuse, position changes, and duplicate suppression with [dice-equity.spec.ts](e2e/dice-equity.spec.ts).

## Themes and build scripts

- `predev`, `prebuild`, and `pretest` run `npm run themes:prepare` (`tools/prepare_themes.py`), which requires the untracked BGBlitz ZIP archives listed in [configuration-options.json](../shared/configuration-options.json) under `themes/`. In a fresh worktree without those archives, link or copy `themes/*.zip` before building; set `NEONDOJO_ALLOW_MISSING_THEMES=1` only when intentionally dropping them.
- `npm run test:themes --prefix web` runs all `tools/test_*.py` Python suites, including worker, probe, and sound tests.

## Playwright browser tests (`web/e2e/`)

- Always run `npm run typecheck:e2e --prefix web` after changing API contracts, components mounted by specs, or E2E mocks — Playwright strips types at runtime and will otherwise silently run drifted mocks.
- The Playwright harness ([web/e2e/stack.ts](e2e/stack.ts), [web/e2e/test.ts](e2e/test.ts)) runs four workers by default (`NEONDOJO_E2E_WORKERS` or `--workers`), each with its own disposable PostgreSQL database, backend, and Vite server. Because different spec files on the same worker share that worker's database:
  - Import fixtures from `./test` (`test` isolates personal config with a mocked profile backend; `backendTest` uses the worker backend's real config), never directly from `@playwright/test`.
  - Never assume an empty library or assert on global counts. Give created databases and imported matches unique names, select the spec's own database, and assert on its own rows.
- Scope assertions to the visible workspace and the control or region that owns the state (for example, match analysis progress belongs to the analysis workspace, while the selected job's rows belong to the error panel). Preserve checks for both when changing the analysis flow. [Analysis workspace](../docs/reference/analysis.md#analysis-workspace)
- Write screenshots for visual review with `test.info().outputPath(...)`. See [Browser tests](../docs/development/testing.md#browser-tests) for opt-in native specs, backend logs (`NEONDOJO_E2E_BACKEND_LOG`), and template databases.
