# BG Tutor repository guidance

These instructions apply from the repository root. Paths and commands are relative to that root. Follow any more specific `AGENTS.md` in the directories you change.

## Product and priorities

BG Tutor is a local backgammon application for importing matches, searching matches and positions, analyzing and playing with GNU Backgammon (GNUbg), and practicing specific decisions. The target is Apple Silicon macOS, especially the Mac mini M4 Pro with 64 GB RAM. The interface is designed for dense desktop use.

Prioritize backgammon correctness, preservation of user data, database performance, and maintainable module boundaries. Give each business rule one authoritative implementation. Complete the requested change and keep the scope focused.

## Working in this repository

- Start with `git status` and inspect both staged and unstaged changes in the affected files. Preserve existing work. Read the relevant README, reference, and development sections and current specifications. Check historical plans against the implementation and tests. [README](README.md) · [Reference](docs/reference/) · [Development](docs/development/) · [Documentation](docs/)
- Use the documented local restart procedure to build before stopping the running service and to reuse its database and engine settings. After startup, verify the changed workflow on port 8787 when the task calls for a live check. [Local restart](README.md#restart-bg-tutor-locally-on-port-8787)
- Use `rg` to find existing modules, helpers, components, and tests before adding code. Make small, cohesive changes. Keep broad formatting, dependency upgrades, and unrelated cleanup out of the diff.
- Make reversible implementation decisions independently. Clarify domain ambiguities that would materially change the data model, calculations, or user behavior.
- Get explicit authorization for destructive Git commands, database resets, or deletion of user data. Commit, push, and publish within the requested scope.

## Architecture and reuse

- The headless backend uses Rust, Tokio, Axum, and PostgreSQL via SQLx. The React and TypeScript client uses Vite and communicates over REST and SSE. Keep this boundary for any future Tauri shell. Use the existing npm scripts and lockfile.
- Keep transport, domain logic, persistence, and engine integration separate. Use the existing domain module that owns the behavior and keep HTTP handlers and React components focused on their roles.
- `backend/src/api.rs` owns REST, SSE, and OpenAPI. `backend/src/import/` owns import and rule validation. `backend/src/engine.rs` and `backend/src/match_analysis.rs` handle GNUbg and native analysis. `backend/src/jobs.rs` and `backend/src/match_jobs.rs` handle jobs. `backend/migrations/` owns schema changes.
- Look for existing validation, normalization, configuration defaults, error thresholds, position recognition, equity calculations, search, and job-state rules. Consolidate rules that have the same meaning and change for the same reason. Update all callers and remove obsolete implementations. Keep separate responsibilities separate even when their code looks similar.
- The backend owns authoritative validation and domain decisions. Client-side validation supports usability. Share configuration values through [configuration options](shared/configuration-options.json) and test cross-language normalization with shared fixtures such as [configuration-normalization.json](tests/fixtures/configuration-normalization.json).
- Use generated API types, shared HTTP error handling, and existing components, hooks, and formatters. Refactors preserve observable behavior, data formats, and error semantics unless the task changes them.
- For study changes, reuse the checker board, cube choices, and result components. Keep answer validation and equity evaluation in the shared Rust practice code. Preserve the distinction between checking an answer and submitting an FSRS rating. [Study](docs/reference/study.md#study) · [Shared practice code](backend/src/practice.rs)

## Rust and TypeScript

- Use descriptive Rust types and enums. Propagate errors with context and map them deliberately at the API boundary. Avoid `unwrap()` and `expect()` on untrusted input or fallible runtime resources.
- Avoid unnecessary clones, unbounded collections, and long-held locks. Run blocking and CPU-intensive work off Tokio's async worker threads.
- Keep TypeScript strict. Validate external data at the boundary and avoid `any` or assertions that hide contract mismatches.
- Give React state a clear owner and derive values where possible. Clean up effects and SSE subscriptions. Prevent delayed responses from overwriting a newer selection.
- Prefer existing libraries and standard tooling. Add a dependency when it has a clear benefit.

## API, jobs, and GNUbg

- The Rust handlers and schemas define the OpenAPI contract. After a contract change, run `npm run api:types --prefix web`, review the regenerated OpenAPI and TypeScript files, and update affected callers. [API reference](docs/development/api.md#api-reference)
- Preserve structured errors, HTTP status codes, revision conflicts, and stable pagination cursors.
- Run long analyses as persisted background jobs. Preserve atomic claims, deduplication, checkpoints, lease recovery, and protection from stale workers. Keep each started job's engine configuration and analysis profile immutable. Preserve the provenance, depth, and scope of results.
- Launch GNUbg with controlled process arguments and keep user input out of shell interpolation. Bound concurrency, runtime, and output. Handle cancellation, process failures, and cleanup explicitly. Surface missing or failed engine results as unavailable or failed.
- Verify engine capabilities with a real position evaluation that uses the neural-network weights. For play changes, also verify GNUbg's embedded Python support.

## Backgammon correctness

- Distinguish the board position, its full match context, and its occurrence in a particular game. A Position ID identifies the board. Analysis also needs the relevant match context.
- Keep player perspective, move direction, bar entry, bearing off, hits, forced passes, and doubles on the dice consistent.
- Apply the supported rules for match and money play, including match length, score, Crawford, cube value, and cube ownership.
- Keep checker and cube decisions, equity and win probability, and imported and native analyses distinct. Use an explicit domain conversion when comparing values across sources or units.
- Preserve missing evaluations as missing and expose incomplete analysis as partial. Use complete analysis for full-match statistics.
- Count study cards by card identity. A card can belong to several decks, and a review updates the card's shared schedule. Study recommendations count due cards that belong to a deck. [Flashcards and Anki](docs/reference/study.md#flashcards-and-anki)
- Dice Distribution covers 21 unordered pairs and 36 equally probable ordered rolls. Doubles have weight 1 and other pairs have weight 2. Include blocked rolls as forced passes.
- Verify changes to rules, parsers, player perspective, and evaluation math with reproducible reference cases. Prefer expected values from an independent reference.

## PostgreSQL and performance

- Use PostgreSQL as the authoritative persistent store. Add another application store only when a requirement justifies it.
- Add a new migration for schema changes. Account for existing records, constraints, and backfills while preserving already-applied migrations.
- Make imports, related writes, and job transitions transactional. Preserve idempotency and deduplication under concurrency.
- Filter and paginate large lists on the server with a stable order. Avoid loading whole libraries into the browser, N+1 queries, and unnecessary round trips.
- Design indexes around actual filter and sort patterns. For relevant search changes, run `EXPLAIN (ANALYZE, BUFFERS)` against representative data in a suitable test database. Report dataset size, selectivity, and measurement scope. Distinguish database execution time, API latency, and UI response time.
- Preserve user matches, notes, study progress, profiles, and backups. Use isolated test databases and keep test cleanup away from user data.

## Desktop UI

- Prioritize desktop information density and mouse and keyboard operation. Keep additional mobile product work within explicit scope.
- Follow [typography](docs/typography.md) and [UI navigation](docs/ui-navigation.md). Reuse tokens and components for color, spacing, and type.
- Keep header rendering and keyboard navigation driven by the same workspace order in `web/src/App.tsx`. When changing that order, update the [analysis reference](docs/reference/analysis.md#analysis-workspace), the UI navigation guide, and the header test in `web/e2e/text-navigation.spec.ts`.
- Start missing dice equity through the existing position-job flow once the visible Dice equity tab has loaded the selected position's saved and active jobs. Use the active analysis engine and profile depth for automatic requests. Preserve explicit retries after failures and use [the dice equity browser tests](web/e2e/dice-equity.spec.ts) to check result reuse, position changes, and duplicate submissions.
- Localize visible text and format numbers consistently in English, German, Spanish, French, Italian, and Polish.
- Handle loading, empty, error, conflict, and disabled states. Preserve focus, accessible labels, table controls, and unsaved drafts.
- Visually verify affected desktop layouts at relevant logical viewport sizes such as 1800 × 1169, 2560 × 1440, and 3840 × 2160.

## Verification and handoff

- For documentation-only changes, verify the content against the current implementation and check paths, links, and commands. For code changes, run focused tests first, then the applicable quality checks. The README covers prerequisites and startup, and the verification guide covers test commands. [Verification](docs/development/testing.md#verification)
- The full Rust workspace suite includes SQLx integration tests. Run affected ignored tests explicitly with an isolated database and the required engine capabilities. Use Playwright's own disposable database and backend for automated browser tests. [PostgreSQL and GNUbg integration tests](docs/development/testing.md#postgresql-and-gnubg-integration-tests) · [Browser tests](docs/development/testing.md#browser-tests)
- In browser tests, scope assertions to the visible workspace and the control or region that owns the state. For example, match analysis progress belongs to the analysis workspace, while the selected job's rows belong to the error panel. Preserve checks for both when changing the analysis flow. [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- Cover domain edge cases, invalid input, concurrency, recovery, and stale responses where relevant. Test observable behavior. Distinguish existing failures from regressions and keep the checks that enforce intended behavior.
- Update relevant documentation when behavior, configuration, APIs, or workflows change. Review the final diff for unintended changes, duplicated logic, and sensitive data.
- Report the result, checks actually run, missing prerequisites, and remaining limitations concisely in German unless the user requests another language. Keep credentials out of code and reports. Support performance claims with measured results.
