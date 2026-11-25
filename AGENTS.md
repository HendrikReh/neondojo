# NeonDojo repository guidance

These instructions apply from the repository root. Paths and commands are relative to that root. Follow the directory-specific rules in [backend/AGENTS.md](backend/AGENTS.md) and [web/AGENTS.md](web/AGENTS.md) when working in those subtrees.

## Product and priorities

NeonDojo is a local backgammon application for importing matches, searching matches and positions, analyzing and playing with GNU Backgammon (GNUbg) and NeonGammon, and practicing specific decisions. The target is Apple Silicon macOS, especially the Mac mini M4 Pro with 64 GB RAM. The interface is designed for dense desktop use.

Prioritize backgammon correctness, preservation of user data, database performance, and maintainable module boundaries. Give each business rule one authoritative implementation. Complete the requested change and keep the scope focused.

## Working in this repository

- Start with `git status` and inspect both staged and unstaged changes in the affected files. Preserve existing work. Read the relevant README, reference, and development sections and current specifications. Check historical plans against the implementation and tests. [README](README.md) · [Reference](docs/reference/) · [Development](docs/development/) · [Documentation](docs/)
- Use `rg` to find existing modules, helpers, components, and tests before adding code. Make small, cohesive changes. Keep broad formatting, dependency upgrades, and unrelated cleanup out of the diff.
- Use the documented local restart procedure (`./tools/build_local_restart.sh`) to build before stopping the running service and to reuse its database and engine settings. After startup, verify the changed workflow on port 8787 when the task calls for a live check. [Local restart](README.md#restart-neondojo-locally-on-port-8787)
- Make reversible implementation decisions independently. Clarify domain ambiguities that would materially change the data model, calculations, or user behavior.
- Get explicit authorization for destructive Git commands, database resets, or deletion of user data. Commit, push, and publish within the requested scope.
- Do not run the legacy helper `tools/rebuild_demo.py` against the current schema — its activation step can delete the internal saved-position collection. [Private demo data](docs/development/architecture.md#private-demo-data)
- Keep `strip = "none"` in [Cargo.toml](Cargo.toml) and do not re-enable stripping via `RUSTFLAGS` or Cargo config; implicit stripping breaks procedural-macro loading on macOS 27 with Rust 1.94.1.
- Frontend `dev`, `build`, and `test` run `themes:prepare` automatically and abort if the untracked BGBlitz ZIP archives are missing from `themes/` (such as in fresh worktrees). Copy/link the archives or set `NEONDOJO_ALLOW_MISSING_THEMES=1` only when intentionally running without them; it never edits the tracked `shared/configuration-options.json`, which only `themes:prepare -- --update-options` rewrites (when adding or removing a theme). [Board artwork](docs/reference/settings.md#board-artwork)

## Architecture and module boundaries

- The headless backend uses Rust, Tokio, Axum, and PostgreSQL via SQLx. The React and TypeScript client uses Vite and communicates over REST and SSE. Keep this boundary intact for a future Tauri shell. Use the existing npm scripts and lockfile.
- Keep transport, domain logic, persistence, and engine integration separate. Use the existing domain module that owns the behavior and keep HTTP handlers and React components focused on their roles.
- Backend ownership:
  - `backend/src/api.rs`: REST, SSE, and OpenAPI
  - `backend/src/backgammon/`: boards, legal checker turns, and GNUbg identifiers; depends on no other module
  - `backend/src/import/`: import formats, replay, and match validation
  - `backend/src/engine.rs` and `backend/src/match_analysis.rs`: isolated GNUbg processes and native analysis
  - `backend/src/engines/`: engine providers, capability probes, and NeonGammon sidecar (`worker.py`)
  - `backend/src/jobs.rs` and `backend/src/match_jobs.rs`: job state, progress events, and game checkpoints
  - `backend/src/play/`: live play sessions, legal checker paths, and play rules
  - `backend/src/learning/`, `backend/src/recurring_mistakes.rs`, and `backend/src/practice.rs`: FSRS flashcards, recurring mistakes, and shared answer evaluation
  - `backend/src/player_directory.rs` and `backend/src/player_scope.rs`: player profiles, virtual player profiles, and PR scoping
  - `backend/src/schema_migrations.rs`, `backend/src/database_backup.rs`, and `backend/migrations/`: baseline adoption, backups, and schema changes
- Look for existing validation, normalization, configuration defaults, error thresholds, position recognition, equity calculations, search, and job-state rules. Consolidate rules that have the same meaning and change for the same reason; update all callers and remove obsolete implementations. Keep separate responsibilities separate even when their code looks similar.
- The backend owns authoritative validation and domain decisions; client-side validation supports usability. Share configuration values through [configuration options](shared/configuration-options.json) and test cross-language normalization with shared fixtures such as [configuration-normalization.json](tests/fixtures/configuration-normalization.json).
- Refactors preserve observable behavior, data formats, and error semantics unless the task changes them.

## API, jobs, and engines

- Rust handlers and schemas define the OpenAPI contract. After any contract change, run `npm run api:types --prefix web`, review the regenerated OpenAPI and TypeScript files, and update affected callers. [API reference](docs/development/api.md#api-reference)
- Preserve structured errors, HTTP status codes, revision conflicts, and stable pagination cursors.
- Run long analyses as persisted background jobs. Preserve atomic claims, deduplication, checkpoints, lease recovery, and protection from stale workers. Keep each started job's engine configuration and analysis profile immutable, and preserve the provenance, depth, and scope of results.
- **Unified ply convention:** NeonDojo stores, displays, and compares all depths in XG/NeonGammon numbering (`1-ply` is the raw neural-net evaluation; GNUbg up to 5-ply, NeonGammon up to 4-ply). Convert only at the GNUbg boundary (`engine::gnubg_plies`). [Ply convention](docs/reference/settings.md#ply-convention)
- **GNUbg:** Launch with controlled process arguments and keep user input out of shell interpolation. Bound concurrency, runtime, and output; handle cancellation, failures, and cleanup explicitly. Verify capabilities with a real position evaluation using neural-network weights, plus embedded Python support for play changes.
- **NeonGammon:** The user-facing name is **NeonGammon**, while the internal engine/DB identifier remains `bgsage`. It runs locally through the verified ARM64 Python 3.12 sidecar (`neongammon==3.1.2` for named opponent presets; `3.1.1`, `3.1.0` and `3.0.0` remain supported for existing installations and snapshots) with package/model integrity checks and a real position probe. Keep venvs and wheels under `local-data/` (gitignored), never in source control. There is no download path and **no silent fallback** to another engine on failure; surface missing or failed engine results as unavailable or failed. [Local NeonGammon setup](docs/open-sage-local.md)

## Backgammon correctness

- Distinguish the board position, its full match context, and its occurrence in a particular game. A Position ID identifies the board; analysis also requires the relevant Match ID context.
- Keep player perspective, move direction, bar entry, bearing off, hits, forced passes, and doubles on the dice consistent.
- Apply the supported rules for match and money play, including match length, score, Crawford, cube value, and cube ownership.
- Keep checker and cube decisions, equity and win probability, and imported and native analyses distinct. Use an explicit domain conversion when comparing values across sources or units.
- Preserve missing evaluations as missing and expose incomplete analysis as partial. Use complete analysis for full-match statistics. `player_match_sides` is the single definition of which match sides enter PR statistics (assisted and running bot matches are excluded; bot matches catalogue only the human side under platform `neondojo`).
- Count study cards by card identity. A card can belong to several decks, and a review updates its shared FSRS schedule. Study recommendations count due cards that belong to a deck. Preserve the distinction between checking an answer and submitting an FSRS rating. [Study](docs/reference/study.md#study) · [Flashcards and Anki](docs/reference/study.md#flashcards-and-anki)
- Dice Distribution covers 21 unordered pairs and 36 equally probable ordered rolls. Doubles have weight 1 and other pairs have weight 2. Include blocked rolls as forced passes.
- Verify changes to rules, parsers, player perspective, and evaluation math with reproducible reference cases, preferring expected values from an independent reference.

## PostgreSQL and performance

- Use PostgreSQL as the authoritative persistent store. Add another application store only when a requirement justifies it.
- Leave the consolidated baseline [0001_schema.sql](backend/migrations/0001_schema.sql) unchanged. Add each schema change as a new sequential migration after the latest migration, account for existing records, constraints, and backfills, and check backup compatibility (`backend/src/database_backup.rs`). [Database schema](docs/development/architecture.md#database-schema)
- Make imports, related writes, and job transitions transactional. Preserve idempotency and deduplication under concurrency.
- Filter and paginate large lists on the server with a stable order. Avoid loading whole libraries into the browser, N+1 queries, and unnecessary round trips.
- Design indexes around actual filter and sort patterns. For relevant search changes, run `EXPLAIN (ANALYZE, BUFFERS)` against representative data in a suitable test database. Report dataset size, selectivity, and measurement scope, distinguishing database execution time, API latency, and UI response time.
- Preserve user matches, notes, study progress, profiles, and backups. Use isolated test databases and keep test cleanup away from user data.

## Verification and handoff

Run focused tests first, then the checks relevant to the change (see [Verification](docs/development/testing.md#verification) for ignored tests, corpora, and opt-in E2E specs):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/postgres cargo test --workspace
npm run api:types --prefix web        # after REST/OpenAPI changes
npm test --prefix web
npm run typecheck:e2e --prefix web    # Playwright strips types; catches drifted E2E mocks
npm run test:themes --prefix web      # runs all Python tests under tools/
npm run build --prefix web
./tools/build_local_restart.sh        # before stopping a running port-8787 service
```

- Always export `DATABASE_URL` to a maintenance database (`/postgres`) before running `cargo test --workspace`; otherwise SQLx reads `.env` and targets the local library database.
- See [backend/AGENTS.md](backend/AGENTS.md) for the single Rust integration test binary, SQLx `#[ignore]` rules, and migration checks, and [web/AGENTS.md](web/AGENTS.md) for parallel Playwright DB isolation, UI navigation, and desktop layout checks.
- For documentation-only changes, verify content against the current implementation and check paths, links, and commands. Update relevant documentation when behavior, configuration, APIs, or workflows change.
- Distinguish existing failures from regressions and keep checks that enforce intended behavior. Review the final diff for unintended changes, duplicated logic, and sensitive data.
- Respond concisely in the language of the user's request (defaulting to German when unspecified). Report the result, checks actually run, missing prerequisites, and remaining limitations. Keep credentials out of code and reports, and support performance claims with measured results.
