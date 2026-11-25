# Verification and tests

## Verification

Run the checks relevant to the change from the repository root. Install the frontend dependencies with `npm ci --prefix web`. `cargo test --workspace` includes every SQLx test that needs only PostgreSQL, so export `DATABASE_URL` as described under [PostgreSQL test server](#postgresql-test-server) first. Use `cargo test --workspace --lib` to run the Rust library tests without the SQLx integration tests.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix web
npm run typecheck:e2e --prefix web
npm run test:themes --prefix web
npm run build --prefix web
```

After changing an API contract, run `npm run api:types --prefix web` before the frontend checks. It regenerates [OpenAPI](../openapi.json) and [the TypeScript types](../../web/src/generated-api.d.ts) from the Rust API. `typecheck:e2e` then checks the browser specs, their API mocks, and the components they mount against the regenerated types. Playwright itself only strips types, and its raw `route.fulfill({ json })` accepts any payload. Use [the E2E response helpers](../../web/e2e/api-fixtures.ts) to check mocks against the generated schema, for example `fulfillJson<'LibraryPage'>(route, { items: [] })` or `fulfillJsonList<'EngineInfo'>(route, engines)`. Settings and benchmark specs opt into `mockEngineCatalog(page)` when they need deterministic availability.

The `test:themes` script discovers the Python tests under `tools/`, including tests for tools beyond theme preparation. Frontend `dev`, `build`, and `test` run `themes:prepare` automatically and abort if the untracked BGBlitz ZIP archives listed in `shared/configuration-options.json` are missing from `themes/` (such as in a fresh worktree); link or copy `themes/*.zip`, or set `NEONDOJO_ALLOW_MISSING_THEMES=1` only when intentionally running without them. The variable leaves the tracked `shared/configuration-options.json` unchanged; only `--update-options` rewrites it.

All integration tests in [`backend/tests/`](../../backend/tests/) link into one test binary, `integration`. Each file is a module of [`backend/tests/integration.rs`](../../backend/tests/integration.rs), and Cargo's automatic test discovery is off, so add a `mod` line there for a new test file; the test `every_test_file_is_a_module` fails until you do. Select a file's tests by its module path:

```sh
cargo test -p neondojo --test integration position_browser::
```

The filter matches substrings, so `play::` also selects `open_sage_play::` and `settings::` also selects `open_sage_settings::`. Exclude those with `--skip open_sage_play::` or `--skip open_sage_settings::` after `--`. Tests of all modules share one process. Use `--test-threads=1` for tests that need exclusive use of a process-wide resource, such as the NeonGammon worker described in [Local NeonGammon setup](../open-sage-local.md).

[cargo-nextest](https://nexte.st/) runs each test in its own process and in parallel across test binaries. With nextest installed, `cargo nextest run --workspace` replaces `cargo test --workspace` and does not run doctests; `-E 'binary(integration) & test(/^play::/)'` selects exactly the `play` module of the integration tests. On November 21, 2026, on the Mac mini M4 Pro, the default suite of 604 tests took 56 s with `cargo test` and 54 s with nextest, after the build. With one binary per test file, `cargo test` had needed 100 s. After a change in the library, rebuilding the tests took 4.0 s instead of 6.3 s.

### Production web startup

On 2026-11-25, the production entry built with all 13 BGBlitz archives measured 432,028 bytes raw and 137,441 bytes gzip-9. The entry on `237d2ec` measured 1,091,818 and 350,574 bytes respectively.

The table shows median results from three cold Chrome 158 runs per build on macOS. Each run used an empty disposable library, German, BG Quantum, a local HTTP server with precompressed gzip-9 assets and a fresh browser context with caching disabled. Boot covers JavaScript requested before clicking **Spielen**. The first switch ends when the idle board appears. Transfer counts compressed JavaScript response bodies. Parse CPU sums the thread durations of Chrome’s `v8.parseOnBackground` events for those JavaScript assets.

| Phase | JS requests, before → after | Transferred bytes, before → after | Background parse CPU, before → after | Visible frame, before → after |
| --- | --- | --- | --- | --- |
| Cold welcome screen | 1 → 4 | 350,574 → 172,291 | 10.1 → 6.3 ms | navigation to German tabs, 70 → 84 ms |
| First switch to Playing | 2 → 12 | 23,362 → 37,899 | 0.9 → 1.6 ms | click to BG Quantum board, 50 → 51 ms |

The two phases together transferred 210,190 bytes after the change and 373,936 before. With Forest, the changed build transferred 169,368 bytes before the click and requested zero BGBlitz catalog bytes. The split reduces cold transfer and background parsing while moving some work to the first workspace visit. The local frame timings include network scheduling, application work and browser rendering.

To repeat the observation, serve `web/dist` from the production build with a disposable backend, save German and BG Quantum, start a Chrome Performance recording with the network cache disabled, reload the welcome screen, then switch to **Spielen**. Inspect the JavaScript requests and V8 background parse events. The Playwright test in [themes-languages.spec.ts](../../web/e2e/themes-languages.spec.ts) also holds both initial downloads open and delays a runtime language switch.

### Continuous integration

[GitHub Actions](../../.github/workflows/ci.yml) runs the default tier every night at 02:17 UTC on `main` and on pushes to `release/**`. Pull requests and pushes to `main` do not trigger a run, so run the relevant checks locally before merging; `gh workflow run ci.yml --ref <branch>` starts a run for a pushed branch. The `Rust` job runs `cargo fmt`, Clippy, and `cargo test --workspace` against a PostgreSQL 17 service. The `Web` job runs Vitest, `typecheck:e2e`, the Python tests under `tools/`, and the production build. Both jobs run on Ubuntu. The Rust toolchain is pinned in the workflow, so new Clippy lints only arrive when someone updates that pin.

CI has neither GNUbg, NeonGammon, import corpora, nor the BGBlitz theme archives. The `Web` job therefore prepares themes with `NEONDOJO_ALLOW_MISSING_THEMES=1`. That empties the archive catalog for the run, and the test that renders every archive theme is skipped. The Playwright suite, the ignored Rust tests, and the macOS-specific code paths still need a local run before a release.

### Which Rust tests run by default

| Requirement | Default `cargo test --workspace` | Marked |
| --- | --- | --- |
| Nothing external | Runs | No attribute |
| PostgreSQL only | Runs; needs `DATABASE_URL` | `#[sqlx::test]` |
| GNUbg, NeonGammon, Anki, opening books, local corpora, or large synthetic datasets | Skipped | `#[ignore = "requires …"]` naming the resource |

Mark a test `#[ignore]` only for a resource beyond PostgreSQL and name that resource in the reason. Keep long waits out of the default suite by injecting shorter intervals, as `AppState::with_job_heartbeat` does for the lease heartbeat. Pure checks of OpenAPI or of fixed reference data need no database; use `#[test]` and `api::ApiDoc::openapi()` for them. Shared HTTP helpers for the integration tests are in [`backend/tests/support/mod.rs`](../../backend/tests/support/mod.rs); a test module uses them with `use crate::support;`.

### PostgreSQL test server

Start PostgreSQL with `docker compose up -d --wait db` or use a dedicated test server. Export `DATABASE_URL` for a maintenance database on that server using a role with `CREATEDB`. For the default local Compose instance, use the connection settings from [.env.example](../../.env.example) with `/postgres` as the database name:

```sh
export DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/postgres
```

SQLx creates and drops a separate database for each test and keeps its bookkeeping in a `_sqlx_test` schema of the database that `DATABASE_URL` names. Without an exported value, the tests read `DATABASE_URL` from `.env`, which names the local library, so export the maintenance URL before running them. A failed test leaves its `_sqlx_test_*` database behind for inspection. SQLx derives the database name from the test's path, so two processes that run the same test against the same server at the same time, for example two checkouts, fail with a duplicate key in `_sqlx_test.databases`; rerun one of them afterwards.

### PostgreSQL and GNUbg integration tests

The ignored tests need a native engine or other local resources in addition to the [PostgreSQL test server](#postgresql-test-server). Set `GNUBG_BIN` if GNUbg is outside `PATH`. Native analysis needs the neural-network weights, and native play tests also need GNUbg's embedded Python support. `--ignored` runs only the ignored tests; `--include-ignored` runs them together with the default suite.

```sh
cargo test --workspace -- --ignored --nocapture --skip all_local_game_files_import_atomically_and_deduplicate
```

Use `cargo test -p neondojo --test integration <module>:: -- --ignored --nocapture` for the tests of an affected file. Integration coverage includes import and deduplication, game catalogs and older matches, scope-bound cursors, SSE replay, worker claims and lease recovery, Dice Distribution, and native checker and cube analysis. The reference match exercises 10 games and 460 moves. Native depth tests check actual 4- and 5-ply GNUbg engine output (`settings::`) and 3- and 5-ply opening candidates (`match_analysis::`), plus game and match exports. Parser tests compare XG positions with the same match imported from MAT through GNUbg.

The search performance test (`search_performance::`) builds 50,000 synthetic matches and 250,000 positions in an isolated database. It uses `EXPLAIN (ANALYZE, BUFFERS)` to verify a GIN trigram index for a selective player-name search and a B-tree index for an exact Position ID search. Each query returns one row. On November 8, 2026, the warm test queries took 0.279 ms and 0.036 ms respectively in PostgreSQL. These measurements cover database execution only.

The saved-position browser performance test (`cargo test -p neondojo --lib position_browser::browse_performance -- --ignored --nocapture`) lives beside the private query builder in `backend/src/position_browser/browse_performance.rs`. It builds 10,000 matches in 10 databases (200 of them collection-only copies), 450,000 positions, 7,000 native match analyses with 315,000 checker decisions, 180,000 imported analyses with cached projections and 22,500 saved positions (5%), 296 of them (1.3%) in the database used for the scoped filter. It runs `EXPLAIN (ANALYZE, BUFFERS)` on the page queries with `jit = off`, as the application pool does. It checks that sorts and filters without analysis read analysis rows only for the page, that the database filter uses `matches_effective_database_idx`, and that the card content of 100 saved positions does not build the analysis catalog of the whole library. On November 22, 2026, warm database execution of a 25-row page took, before and after the change in the same run: default sort 685 → 44 ms, page 400 1,078 → 55 ms, database filter 119 → 6 ms, cursor page 69 → 42 ms, loss sort 1,511 → 860 ms, loss sort in the scoped database 120 → 43 ms, and card content 386 → 16 ms. The **Ready** filter went from 604 to 720 ms, because it needs the analysis selection of every saved match in scope. These measurements cover database execution only.

`search_performance::decision_search_stays_index_bound_on_a_large_library` loads [`decision_search_library.sql`](../../backend/tests/fixtures/decision_search_library.sql): 10,000 matches in four databases, 600,000 positions and 399,000 decision-index rows whose fingerprints are current. The player Hero has 5,000 matches (3,500 with a native match analysis, 1,000 with imported analysis, 500 unanalyzed) and 94,500 indexed decisions (23.7 %); 31,571 of them (7.9 %) meet the default loss of 0.05, and 215 (0.05 %) are also cube offers at cube 4. For each warm request it explains the production statements with the handler's parameters and checks the plans: the legacy-catalog step probes the empty `matches_pending_imported_catalog` index, the freshness check finds Hero's matches through the `lower(btrim(...))` name indexes and reads position analyses only for the 500 unanalyzed matches, and the count and page use `decision_index_player_loss` without scanning the table, joining `matches` only for a database filter and reading only the 50 returned hits. On November 22, 2026, with JIT off as in the application, PostgreSQL execution times before → after migration `0093` on the same data were: legacy-catalog check 9.1 + 7.6 → 0.03 + 0.02 ms; freshness check 231 → 83 ms (2,500 matches of one database: 115 → 36 ms); first page 53 → 0.2 ms; selective page 39 → 1.0 ms; page in one database 44 → 0.4 ms; page at offset 90,000 131 → 33 ms; count 7.5 → 4.0 ms, selective count 39 → 3.0 ms, count in one database 5.5 → 6.5 ms. Planning the freshness check takes 6 ms instead of 4 ms. The earlier values come from the previous statements run with `PREPARE`/`EXPLAIN EXECUTE` on an identical dataset. They cover database execution only, not API or UI latency.

On November 25, 2026, the two 10,000-match fixtures above were retained in disposable databases on PostgreSQL 17.12. The production queries ran with JIT off, alternating the schema at `d93796a` and [migration 0096](../../backend/migrations/0096_analysis_selection_probes.sql) three times each on the same data. Each value below is the median warm database execution time or the number of shared buffer hits plus reads in `EXPLAIN (ANALYZE, BUFFERS)`. The readiness and loss queries fetch 25 rows and one lookahead row. Both fixtures returned identical effective selections in a bidirectional `EXCEPT ALL` comparison.

| Query and selectivity | Time before | Time after | Buffers before | Buffers after |
| --- | ---: | ---: | ---: | ---: |
| Decision Search freshness, Hero's 5,000 matches | 86.35 ms | 80.88 ms | 180,639 | 177,959 |
| Decision Search freshness, Hero's 2,500 matches in one database | 36.44 ms | 34.36 ms | 58,990 | 52,426 |
| Saved Positions, Ready, 12,960 of 22,500 saved positions | 484.60 ms | 410.69 ms | 477,239 | 438,882 |
| Saved Positions, loss sort, 22,500 saved positions | 470.71 ms | 410.90 ms | 477,284 | 438,927 |

The loss-sort runs varied from 468 to 516 ms before the change and 405 to 542 ms after it. The buffer totals for each query were identical across all three runs of each schema.

The baseline planner already removed the catalog's decision counts and imported engine/depth aggregation from selection. For Hero, the native eligibility check read one decision for each of 3,500 jobs. The completeness check counted three game checkpoints per job. The import check probed 1,500 matches, visiting 42,000 index entries for 1,000 matching imports. Its new partial index returns at most one qualifying entry per probe and takes 3,001 buffers versus 5,681 for the previous bitmap/heap scan. Automatic position fallback still makes 30,000 indexed job probes for the 500 unanalyzed matches. This accounts for 60,000 buffers in both plans.

The Ready query looked up the explicit selection once for each of 10,000 matches. The covering index reduced those probes from 29,925 to 20,001 buffers. On the 200 explicitly selected imports, the analysis ID prefix also avoided 9,000 native job probes. The full ID and source revision still determine which selection is valid. On this fixture the import index occupied 1,296 KiB for 180,000 imported analyses and the covering index occupied 40 KiB for 200 selections. Inserting the same 10,000 fixture analyses into temporary copies with all other production indexes took 24.4 ms before and 32.9 ms with the partial index, median of three warm pairs. This isolates row and index insertion. The temporary copies omit foreign keys, triggers and WAL, and the measurement excludes parsing and the rest of the import workflow.

`search_performance::duplicate_finder_and_active_job_checks_use_indexes_on_a_large_library` builds 10,000 library matches in two databases with 100,000 positions. 1,000 pairs (2,000 matches, 20 %) share a played record, 250 of the pairs in the second database (2,500 matches). The jobs table holds 14,000 finished jobs and 10 active ones (0.07 %), none in the second database. With JIT off, it checks that the first page of `GET /api/library/duplicates` reads `matches_played_record_idx` without scanning `matches`, that the active-job check before deleting matches reads only the active jobs through `jobs_active_claim`, and that lease recovery and the job claim use `jobs_active_claim`. On November 23, 2026, warm PostgreSQL execution before → after migration `0094` on the same data was: duplicate groups 12.8 → 8.0 ms (1,385 → 247 shared buffers), in one database 3.9 → 2.0 ms (1,382 → 244 buffers); the active-job check 0.04 → 0.03 ms for one match and 0.09 → 0.08 ms for the 2,500 matches of a database (23 → 22 buffers); lease recovery 0.06 → 0.04 ms and the claim 0.9 → 0.7 ms (50 → 48 buffers). Planning took under 0.6 ms in every case. These measurements cover database execution only, not API or UI latency.

In the default PostgreSQL suite, `review_decisions::review_query_plans_use_the_current_page_parameters` also checks that paginated match review queries keep parameter-specific query plans rather than caching a generic prepared statement across filters and page modes.

### GNUbg executable verification

On macOS, point `GNUBG_BIN` at a native GNUbg binary with its evaluation weights available. This opt-in benchmark measures the executable identity check on its own and the complete version and 1-ply position probe with and without a saved engine version.

```sh
cargo test --release -p neondojo --lib benchmark_native_installation_checks -- --ignored --nocapture
```

On November 25, 2026, a Mac mini M4 Pro ran GNUbg 1.08.003 (1,927,608-byte ARM64 executable) in four paired release-build runs against NeonDojo 1.4.1. Each build went first in two pairs. Each run rotated through the three operations, discarded three warmups per operation and timed 30 samples per operation against a warm filesystem cache. Each value is the median of four run medians, in milliseconds.

| Operation | 1.4.1 | Single blocking task and ARM SHA-256 |
|---|---:|---:|
| Resolve and hash the executable before and after a run | 8.05 | 1.88 |
| Full probe with a saved engine version | 164.95 | 158.33 |
| Full probe without a saved engine version | 159.55 | 156.21 |

The pinned path launches the executable it verified and resolves the configured path again when the process exits. Wrappers also report their version before and after execution. The unpinned probe skips the executable hashes and shows the variation in GNUbg startup and position evaluation. Timings cover backend work, excluding HTTP and database access.

### Optional local import corpus

The corpus test reads `games/xg`, `games/mat`, and `games/bgf` by default. Set `NEONDOJO_GAMES_DIR` to use a different parent directory. It sends the files through the multipart API in its own SQLx test database and skips when the local files are absent.

```sh
NEONDOJO_CORPUS_REPORT=/tmp/neondojo-import-results.json \
  cargo test -p neondojo --test integration import_corpus:: -- --ignored --nocapture
```

The `mat_corpus` tests replay each file in `games/mat` through GNUbg and check that it normalizes to strictly replayable SGF. They need GNUbg but no database.

```sh
cargo test -p neondojo --test integration mat_corpus:: -- --ignored --nocapture
```

The November 8, 2026 rerun imported 33 of 42 files and rejected nine XG tournament transcripts. On November 20, 2026, after the importer started resolving their irregularities, 41 of 41 local XG files imported. The earlier 37-of-37 result used a different corpus with 17 distinct contents and 20 recognized duplicates. The corpus test requires every supplied file to pass. See the [import corpus report](../import-corpus-results.md).

### Browser tests

Playwright requires Google Chrome, `psql`, PostgreSQL, and the frontend dependencies. GNUbg workflows require a working local engine. Global setup builds the backend once. Each Playwright worker then starts its own Vite server on a local port and keeps it for all its spec files. For every spec file the worker creates a new database, starts a backend on it, verifies that the API is using it, and removes both before the next file. Global teardown drops any database that a crashed worker left behind. Vite serves HTTP and its development WebSocket on the same loopback port. The harness lives in [`web/e2e/stack.ts`](../../web/e2e/stack.ts) and [`web/e2e/test.ts`](../../web/e2e/test.ts).

```sh
npm run test:e2e --prefix web
```

The suite runs four workers by default. Set `NEONDOJO_E2E_WORKERS` or pass `--workers` to change that; every worker runs its own GNUbg processes, so more workers mainly help the mocked specs. On November 21, 2026, on the Mac mini M4 Pro, 614 tests ran in 6.3 minutes with four workers and 24 opt-in tests were skipped. The previous single-worker harness had needed 7.2 minutes for its first 200 tests.

Every spec file starts with an empty library, so it cannot see the matches, analyses or settings of another file, whatever ran before it on the same worker. Replacing the database and backend takes about half a second per file. Tests of one spec file run in order and share that library. Keep them independent all the same, because after a failure Playwright continues the file on a new worker with an empty library, and `--grep` skips the earlier tests: give imported matches and created databases unique names, assert on their own rows rather than on global counts, and note that importing the same fixture twice opens the same match with the analyses it already has. Import specs from `./test`. Its `test` fixture also isolates the personal configuration of each test with a mocked profile backend, and `backendTest` uses the real configuration of the spec file's backend. `NEONDOJO_E2E_DATABASE_URL` and the `databaseUrl` fixture name the spec file's database for direct SQL checks.

Write screenshots for visual review with `test.info().outputPath(...)`. Playwright stores them with the other artifacts of the test under `web/test-results/`, or under the directory passed with `--output`. To keep the backend output for diagnosis, set `NEONDOJO_E2E_BACKEND_LOG` to a file; the worker with index 1 and higher appends `-1`, `-2` and so on to its name.

Pass a spec path to run only the affected workflow. These specs cover native
match analysis, automatic dice equity, and the Study overview with its two
direct practice actions.

```sh
npm run test:e2e --prefix web -- e2e/workflow.spec.ts
npm run test:e2e --prefix web -- e2e/dice-equity.spec.ts
npm run test:e2e --prefix web -- e2e/study-overview.spec.ts
```

The bulk analysis regression uses GNUbg to analyze a short match and the
ten-game WC2014 fixture. It reloads while an analysis is running and checks the
restored progress and persisted job IDs in the disposable database.

```sh
npm run test:e2e --prefix web -- e2e/analysis-calculation-controls.spec.ts --grep "reload resumes native bulk"
```

The default PostgreSQL connection targets the Compose instance. For another server or port, set `NEONDOJO_E2E_POSTGRES_URL` to a connection with `CREATEDB`. For tests that need prepared data, set `NEONDOJO_E2E_POSTGRES_TEMPLATE` to a dedicated fixture database named `neondojo_e2e_template_*`. The harness copies the template for each spec file. Unset the obsolete `NEONDOJO_TEST_URL` before running Playwright.

Coverage includes import and analysis, game selection and move pagination, search and filters, saved notes and revision conflicts, deck creation, review and Anki exports, themes and all six locales, and selection changes while responses are in flight.

The opt-in analysis estimate test covers all five ply depths for both game and match scope. It can reuse compatible benchmark runs from the test database template. When measurements are missing, it runs native benchmarks, which can take substantially longer than the standard browser checks.

```sh
NEONDOJO_NATIVE_ESTIMATE=1 npm run test:e2e --prefix web -- e2e/analysis-time-estimate-native.spec.ts
```

#### Opt-in browser specs

These specs skip unless their environment variable is set. Each still uses the disposable database and backend described above.

| Spec | Enable with | Requires |
| --- | --- | --- |
| `e2e/analysis-time-estimate-native.spec.ts` | `NEONDOJO_NATIVE_ESTIMATE=1` | GNUbg; may run native benchmarks |
| `e2e/benchmark-native.spec.ts` | `NEONDOJO_NATIVE_BENCHMARK=1` | GNUbg with embedded Python for play-turn latency |
| `e2e/play-native.spec.ts` | `NEONDOJO_NATIVE_PLAY_TEST=1` | GNUbg with embedded Python |
| `e2e/play-full-match.spec.ts` | `NEONDOJO_FULL_MATCH_TEST=1` and `GNUBG_BIN=/absolute/path/to/gnubg` | GNUbg with embedded Python, see [Native play tests](#native-play-tests) |
| `e2e/play-feedback-native.spec.ts` | `NEONDOJO_FEEDBACK_NATIVE_TEST=1` | GNUbg with embedded Python, see [Native feedback tests](#native-feedback-tests) |
| `e2e/profile-native.spec.ts` | `NEONDOJO_PROFILE_NATIVE=1` | GNUbg with embedded Python for play sessions |
| `e2e/open-sage-native.spec.ts` | `NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python` | NeonGammon and GNUbg, see [Local NeonGammon setup](../open-sage-local.md) |
| `e2e/play-sage-difficulty.spec.ts` | `NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python` | NeonGammon 3.1.0 and GNUbg with embedded Python, see [Local NeonGammon setup](../open-sage-local.md) |
| `e2e/anki-import.spec.ts` | `NEONDOJO_ANKI_PYTHON=/path/to/python` | Python with the Anki package |
| `e2e/user-guide-screenshots.spec.ts` | `NEONDOJO_GUIDE_SCREENSHOTS=1` and `NEONDOJO_SAGE_PYTHON` | GNUbg with embedded Python, NeonGammon and `cwebp`, see [User guide screenshots](#user-guide-screenshots) |

Individual archive-dependent checks in `e2e/themes-languages.spec.ts`, `e2e/ui-themes.spec.ts`, and `e2e/play-checker-colour.spec.ts` also skip automatically when the corresponding local BGBlitz board archives are not installed.

### User guide screenshots

The screenshots in [`docs/user-guide/images/`](../user-guide/images/) are generated by an opt-in browser spec. It runs against Playwright's disposable database and backend, never against a personal library. It seeds the database with the public fixtures from [`backend/tests/fixtures/`](../../backend/tests/fixtures/), analyzes them with GNUbg and NeonGammon, plays three short bot matches through the play API, creates decks and saved positions, runs benchmarks and a short engine duel, and answers a few practice questions. It then captures all 35 images at 1800 × 1169 with the player name "Sam", by default in light mode with the default interface theme and board, and adds the numbered markers used by the guide's legends. `cwebp` converts the PNG screenshots to WebP.

```sh
NEONDOJO_GUIDE_SCREENSHOTS=1 \
NEONDOJO_SAGE_PYTHON=/path/to/neongammon-venv/bin/python \
  npm run test:e2e --prefix web -- e2e/user-guide-screenshots.spec.ts
```

The captures use the default appearance unless you choose another one. The values are saved into the test profile before the seed starts, and the backend rejects invalid ones:

| Variable | Values |
| --- | --- |
| `NEONDOJO_GUIDE_INTERFACE_THEME` | `graphite`, `club` or `paper` (**Settings → Appearance → Interface theme**) |
| `NEONDOJO_GUIDE_DISPLAY_MODE` | `light`, `dark` or `system`; the browser runs in dark mode only for `dark`, so `system` renders light |
| `NEONDOJO_GUIDE_BOARD_THEME` | A board theme ID from `board_themes` in `shared/configuration-options.json`, such as `forest` or `bgblitz:bg-moon`; BGBlitz themes also accept the archive name without the prefix, such as `bg-moon` |

```sh
NEONDOJO_GUIDE_SCREENSHOTS=1 \
NEONDOJO_SAGE_PYTHON=/path/to/neongammon-venv/bin/python \
NEONDOJO_GUIDE_INTERFACE_THEME=graphite \
NEONDOJO_GUIDE_DISPLAY_MODE=light \
NEONDOJO_GUIDE_BOARD_THEME=bg-moon \
NEONDOJO_GUIDE_SCREENSHOTS_DIR=/tmp/neondojo-guide-moon \
  npm run test:e2e --prefix web -- e2e/user-guide-screenshots.spec.ts
```

The user guide describes the default appearance, so write other variants to a separate directory.

The frontend build needs the local BGBlitz archives listed in `shared/configuration-options.json`; in a fresh checkout or worktree, copy `themes/*.zip` first, as described under [Board artwork](../reference/settings.md#board-artwork). They also fill the board gallery in **Settings → Board**. A run takes about ten minutes because the engine work is real. It overwrites the files in `docs/user-guide/images/`; set `NEONDOJO_GUIDE_SCREENSHOTS_DIR` to write them elsewhere. Review every changed image before committing it, and update the user guide text and the marker legends when the interface changes. When a view gains or loses elements, adjust the spec's selectors, crops or markers.

### Native play tests

With PostgreSQL running and `DATABASE_URL` set as described under [Verification](#verification), run the native integration suite. SQLx creates isolated test databases. The tests cover complete matches, both colors, saved engine settings, forced passes including rolls on which both players dance, cube offers and responses, gammons and backgammons, Crawford and post-Crawford games, SGF replay, concurrent actions, and failure recovery.

```sh
GNUBG_BIN=/absolute/path/to/gnubg \
cargo test -p neondojo --test integration play:: -- --ignored --skip open_sage_play::
```

The opt-in browser suite uses Playwright's test backend, native GNUbg with seeded randomness, and browser clicks for play actions. Its two full three-point matches use white/bear off right and black/bear off left. They check the hover destination and move for each die, swap dice, use the roll and cube controls, and reload the saved match. Three additional scenarios verify opening rolls and ties across reloads. Three cube scenarios verify real bot offers, taking or passing, and starting the next game. Native SGF evaluations verify 1-, 2-, and 3-ply play (GNUbg's native `0C`, `1C`, and `2C` markers in SGF) after resuming and changing games.

```sh
cd web
NEONDOJO_FULL_MATCH_TEST=1 \
GNUBG_BIN=/absolute/path/to/gnubg \
npx playwright test e2e/play-full-match.spec.ts --output=/tmp/neondojo-full-match-results --reporter=line
```

### Engine Duel process reuse

The native regression plays a three-point match using dice seed 13, NeonGammon at 2-ply versus 3-ply, and four threads per side. It counts Python worker starts separately for each depth and GNUbg rules processes per game. It checks the saved checker evaluations and per-side move counts, then prints hashes of the played decisions and imported evaluations for comparison across builds.

```sh
NEONDOJO_SAGE_PYTHON=/path/to/venv/bin/python \
cargo test -p neondojo --lib two_neongammon_depths_reuse_workers -- --ignored --nocapture
```

On November 25, 2026, four local runs per version on an Apple M4 Pro used NeonGammon 3.1.2 and GNUbg 1.08.003 (20261117). Wall time covers one completed duel, including worker startup and SGF retention. The initial installation probes run before timing begins.

| Measurement | Before reuse | With reuse |
|---|---:|---:|
| Python workers for the 2-ply / 3-ply sides | 56 / 55 | 1 / 1 |
| GNUbg processes in games 1 / 2 / 3 | 20 / 158 / 76 | 1 / 1 / 1 |
| Median duel wall time (range) | 57.7 s (53.1–65.6 s) | 7.9 s (7.8–8.3 s) |

Both versions produced the same 3–1 result. All 134 played events, 117 retained NeonGammon checker evaluations and three sets of GNUbg game statistics matched. Process counts exclude GNUbg version checks. The timings describe this seeded match under local host load.

### Native feedback tests

To verify mode switching and feedback with the configured GNUbg and an isolated test backend and database, run the native browser suite.

```sh
cd web
NEONDOJO_FEEDBACK_NATIVE_TEST=1 \
  npx playwright test e2e/play-feedback-native.spec.ts
```
