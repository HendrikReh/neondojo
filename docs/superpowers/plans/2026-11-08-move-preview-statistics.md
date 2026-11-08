# Move Preview and Statistics Implementation Plan

> Execute independent board and backend domains under dispatching-parallel-agents; root integrates candidate selection, workspace navigation and statistics UI. Apply TDD and verification-before-completion.

**Goal:** Show legal played/candidate moves before, with arrows and after; make native game/match statistics navigate filtered decisions without regressions.

**Architecture:** Pure TypeScript move projection and reusable SVG overlay feed the existing Board. Read-only per-job statistics use durable game checkpoints and share the existing paginated decisions API. UI state fences candidate, filter and navigation changes by position/job identity.

**Tech Stack:** Rust/Axum/SQLx/PostgreSQL, React/TypeScript/SVG, Vitest, Playwright.

- [x] Verify and commit current repository state, excluding ignored private data.
- [x] Add failing move-projection oracles for hits, bar, multi-die paths, doubles, oversize bear-off and forced pass; run red, implement pure projection, run green.
- [x] Add a shared arrow overlay using builtin/theme geometries; expose Board preview props without altering stored Position or analysis IDs; test both movers and all themes.
- [x] Add failing SQLx statistics/filter tests; implement migration0016, per-game checkpoint summaries and GET /api/jobs/{id}/statistics. Add severity validation, filter intersection and cursor binding to /moves; test missing/legacy/partial/scoped jobs.
- [x] Wire explicit played/candidate preview selection and before/arrows/after controls into AnalysisWorkspace, AnalysisPanel and SourceAnalysisView. Clear stale choices on source/position changes; add six-language labels and selected candidate semantics.
- [x] Add the statistics inspector, summaries and game table. Metric clicks set visible player/kind/severity/game filters and open the errors list. Game links select the exact source game; source-move links preserve cube notes. Extend next-error filtering.
- [x] Regenerate OpenAPI/TypeScript, add meaningful unit/browser regressions and verify red/green. Run native-engine/PostgreSQL tests, frontend/browser suite and formatting/build/lint. Use a disposable browser-test database, restore the original backend and verify the three demo counts and learning state.
- [x] Independently review integration, inspect desktop/mobile screenshots including imported and native candidate previews, document final behavior and validation.

Contracts: Board preview data and mode are optional so database previews and study cards retain existing behavior. Statistics response contains job_id, optional existing MatchAnalysisSummary and ordered source-game entries with game_no, scores, crawford, analyzed, optional per-game summary. Filters add optional severity (none/doubtful/bad/very_bad); game_no is already supported. Statistics never launch engine work.


## Verification and outcome

- Previous work committed as **941cff0** before implementation. The new feature changes are grouped in a separate feature commit.
- Legal projection tested against all **47,936** positions in the unchanged private demo library. **46,686** consecutive boards matched exactly; no unsupported played notation or projected mismatch occurred. Unit oracles additionally cover illegal partial rolls, higher-die rules, opposing-bar safety, compound hits, ambiguous ordering, doubles and oversize bear-off.
- **183 Rust tests** pass including PostgreSQL, native GNUbg, installed-book oracles, per-game summaries, checkpoint recovery, legacy catalog fallback and severity cursor validation. The optional all-valid private import test is excluded because the corpus intentionally retains nine rejected files from the previous task.
- **81 frontend tests** pass; all **47 Playwright workflows** pass on the production build served by a separate backend and disposable PostgreSQL database. Existing import, search, note scopes, SSE, learning/Anki, theme/language, pagination and exact-game navigation workflows remain covered. The disposable database was removed after testing.
- **18 Python tests**, fmt, all-target Clippy with warnings denied, Prettier, TypeScript and production build pass. Vite retains its bundle-size advisory.
- Independent review found the game filter missing from Next error; a red/green browser test now covers it. A second red/green test prevents delayed next-error responses from overriding manual navigation. Accessibility regression tests preserve original dice/pip labels alongside preview context; existing analysis regions and unique decision-list markers remain intact.
- Desktop/mobile screenshots inspected for arrows, after state and native statistics. All16 themes expose identical legal arrow steps in browser tests. No runtime errors or page overflow at390px; desktop checked at1440px. Tables can scroll within their own container.
- Main backend restarted successfully with additive migration0016. Main library fingerprint is unchanged: three demo collections (5BGF,5MAT,23XG),33 match IDs/digests,69 learning cards and62 reviews. No private source/book/screenshot artifacts enter Git.
