# Verification and tests

## Verification

Run the checks relevant to the change from the repository root. Install the frontend dependencies with `npm ci --prefix web`. For the full Rust suite, configure `DATABASE_URL` as described below. Use `cargo test --workspace --lib` to run the Rust library tests without the SQLx integration targets.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix web
npm run test:themes --prefix web
npm run build --prefix web
```

After changing an API contract, run `npm run api:types --prefix web` before the frontend checks. It regenerates [OpenAPI](../openapi.json) and [the TypeScript types](../../web/src/generated-api.d.ts) from the Rust API. The `test:themes` script discovers the Python tests under `tools/`, including tests for tools beyond theme preparation.

### PostgreSQL and GNUbg integration tests

Start PostgreSQL with `docker compose up -d --wait db` or use a dedicated test server. Export `DATABASE_URL` for a maintenance database on that server using a role with `CREATEDB`. For the default local Compose instance, use the connection settings from [.env.example](../../.env.example) with `/postgres` as the database name. SQLx creates and drops separate test databases. Set `GNUBG_BIN` if GNUbg is outside `PATH`. Native analysis needs the neural-network weights, and native play tests also need GNUbg's embedded Python support.

```sh
cargo test --workspace -- --ignored --nocapture --skip all_local_game_files_import_atomically_and_deduplicate
```

Use `cargo test -p neondojo --test <target> -- --ignored --nocapture` for an affected integration test target. Integration coverage includes import and deduplication, game catalogs and older matches, scope-bound cursors, SSE replay, worker claims and lease recovery, Dice Distribution, and native checker and cube analysis. The reference match exercises 10 games and 460 moves. Native depth tests check actual 3- and 4-ply engine output and game and match exports. Parser tests compare XG positions with the same match imported from MAT through GNUbg.

The search performance test builds 50,000 synthetic matches and 250,000 positions in an isolated database. It uses `EXPLAIN (ANALYZE, BUFFERS)` to verify a GIN trigram index for a selective player-name search and a B-tree index for an exact Position ID search. Each query returns one row. On November 8, 2026, the warm test queries took 0.279 ms and 0.036 ms respectively in PostgreSQL. These measurements cover database execution only.

### Optional local import corpus

The corpus test reads `games/xg`, `games/mat`, and `games/bgf` by default. Set `NEONDOJO_GAMES_DIR` to use a different parent directory. It sends the files through the multipart API in its own SQLx test database and skips when the local files are absent.

```sh
NEONDOJO_CORPUS_REPORT=/tmp/neondojo-import-results.json \
  cargo test -p neondojo --test import_corpus -- --ignored --nocapture
```

The November 8, 2026 rerun imported 33 of 42 files and rejected nine XG tournament transcripts. On November 20, 2026, after the importer started resolving their irregularities, 41 of 41 local XG files imported. The earlier 37-of-37 result used a different corpus with 17 distinct contents and 20 recognized duplicates. The corpus test requires every supplied file to pass. See the [import corpus report](../import-corpus-results.md).

### Browser tests

Playwright requires Google Chrome, `psql`, PostgreSQL, and the frontend dependencies. GNUbg workflows require a working local engine. The harness creates a fresh database, verifies that the API is using it, starts a dedicated backend and Vite on local ports, and removes the test database after the run. Vite serves HTTP and its development WebSocket on the same loopback port. Screenshots and other test artifacts are written to `web/test-results/`.

```sh
npm run test:e2e --prefix web
```

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

The default PostgreSQL connection targets the Compose instance. For another server or port, set `NEONDOJO_E2E_POSTGRES_URL` to a connection with `CREATEDB`. For tests that need prepared data, set `NEONDOJO_E2E_POSTGRES_TEMPLATE` to a dedicated fixture database named `neondojo_e2e_template_*`. The harness copies the template for each run. Unset the obsolete `NEONDOJO_TEST_URL` before running Playwright.

Coverage includes import and analysis, game selection and move pagination, search and filters, saved notes and revision conflicts, deck creation, review and Anki exports, themes and all six locales, and selection changes while responses are in flight.

The opt-in analysis estimate test covers all five ply depths for both game and match scope. It can reuse compatible benchmark runs from the test database template. When measurements are missing, it runs native benchmarks, which can take substantially longer than the standard browser checks.

```sh
NEONDOJO_NATIVE_ESTIMATE=1 npm run test:e2e --prefix web -- e2e/analysis-time-estimate-native.spec.ts
```

### Native play tests

With PostgreSQL running and `DATABASE_URL` set as described under [Verification](#verification), run the native integration suite. SQLx creates isolated test databases. The tests cover complete matches, both colors, saved engine settings, forced passes, cube offers and responses, gammons and backgammons, Crawford and post-Crawford games, SGF replay, concurrent actions, and failure recovery.

```sh
GNUBG_BIN=/absolute/path/to/gnubg \
cargo test -p neondojo --test play -- --ignored
```

The opt-in browser suite uses Playwright's test backend, native GNUbg with seeded randomness, and browser clicks for play actions. Its two full three-point matches use white/bear off right and black/bear off left. They check the hover destination and move for each die, swap dice, use the roll and cube controls, and reload the saved match. Three additional scenarios verify opening rolls and ties across reloads. Three cube scenarios verify real bot offers, taking or passing, and starting the next game. Native SGF evaluations verify 0-, 1-, and 2-ply play after resuming and changing games.

```sh
cd web
NEONDOJO_FULL_MATCH_TEST=1 \
GNUBG_BIN=/absolute/path/to/gnubg \
npx playwright test e2e/play-full-match.spec.ts --output=/tmp/neondojo-full-match-results --reporter=line
```

### Native feedback tests

To verify mode switching and feedback with the configured GNUbg and an isolated test backend and database, run the native browser suite.

```sh
cd web
NEONDOJO_FEEDBACK_NATIVE_TEST=1 \
  npx playwright test e2e/play-feedback-native.spec.ts
```
