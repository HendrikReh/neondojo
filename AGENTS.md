# BG Tutor repository guidance

These instructions apply from the repository root. Paths and commands are relative to that root. Follow any more specific `AGENTS.md` in the directories you change.

## Product and priorities

BG Tutor is a local backgammon application for importing matches, searching matches and positions, analyzing and playing with GNU Backgammon (GNUbg), and practicing specific decisions. The target is Apple Silicon macOS, especially the Mac mini M4 Pro with 64 GB RAM. The interface is designed for dense desktop use.

Prioritize backgammon correctness, preservation of user data, database performance, and maintainable module boundaries. Give each business rule one authoritative implementation. Complete the requested change and keep the scope focused.

## Working in this repository

- Start with `git status` and inspect the relevant files and local changes. Preserve existing work. Read the relevant [README](README.md) sections and specifications in [docs](docs/). Check historical plans against current code and tests.
- For a local restart, follow [Restart BG Tutor locally on port 8787](README.md#restart-bg-tutor-locally-on-port-8787).
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

## Rust and TypeScript

- Use descriptive Rust types and enums. Propagate errors with context and map them deliberately at the API boundary. Avoid `unwrap()` and `expect()` on untrusted input or fallible runtime resources.
- Avoid unnecessary clones, unbounded collections, and long-held locks. Run blocking and CPU-intensive work off Tokio's async worker threads.
- Keep TypeScript strict. Validate external data at the boundary and avoid `any` or assertions that hide contract mismatches.
- Give React state a clear owner and derive values where possible. Clean up effects and SSE subscriptions. Prevent delayed responses from overwriting a newer selection.
- Prefer existing libraries and standard tooling. Add a dependency when it has a clear benefit.

## API, jobs, and GNUbg

- The Rust handlers and schemas define the OpenAPI contract. After a contract change, run `npm run api:types --prefix web` and review both `docs/openapi.json` and `web/src/generated-api.d.ts`. Regenerate both files from source and update affected callers.
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
- Localize visible text and format numbers consistently in English, German, Spanish, French, Italian, and Polish.
- Handle loading, empty, error, conflict, and disabled states. Preserve focus, accessible labels, table controls, and unsaved drafts.
- Visually verify affected desktop layouts at relevant logical viewport sizes such as 1800 × 1169, 2560 × 1440, and 3840 × 2160.

## Verification and handoff

For documentation-only changes, verify content, paths, links, and commands. For code changes, run focused tests first, then the checks relevant to the change.

```sh
# Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Frontend
npm test --prefix web
npm run build --prefix web

# Themes or Python tools
npm run test:themes --prefix web

# API contract changes, followed by frontend checks
npm run api:types --prefix web

# Affected browser workflows
npm run test:e2e --prefix web
```

- Follow the [README test setup](README.md#verification) for affected PostgreSQL, GNUbg, and browser workflows. The full Rust workspace suite includes SQLx integration tests. Additional real-engine and database tests require `--ignored`. Verify the target server, database isolation, and required engine capabilities before running them.
- Cover domain edge cases, invalid input, concurrency, recovery, and stale responses where relevant. Test observable behavior. Distinguish existing failures from regressions and keep the checks that enforce intended behavior.
- Update relevant documentation when behavior, configuration, APIs, or workflows change. Review the final diff for unintended changes, duplicated logic, and sensitive data.
- Report the result, checks actually run, missing prerequisites, and remaining limitations concisely in German unless the user requests another language. Keep credentials out of code and reports. Support performance claims with measured results.
