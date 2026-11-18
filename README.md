# BG Tutor

BG Tutor is a local backgammon application for importing matches, finding positions, analyzing decisions with GNU Backgammon (GNUbg) or Open Sage, playing against an engine, and practicing your own mistakes. Its headless Rust backend uses Tokio, Axum, PostgreSQL, and SQLx. The React and TypeScript client connects through REST and server-sent events (SSE).

[User guide](docs/user-guide/README.md) · [Quick start](#quick-start) · [Restart on port 8787](#restart-bg-tutor-locally-on-port-8787) · [Analyze a match](#analyze-your-first-match) · [Documentation](#documentation) · [API](docs/development/api.md) · [Verification](docs/development/testing.md) · [Repository guidance](AGENTS.md)

## Quick start

- Rust, tested with 1.94.1
- Node.js 22.12 or later in the 22.x series, or Node.js 24 or later, with npm
- Docker Compose for the bundled PostgreSQL 17 database, or an existing PostgreSQL server
- Python 3.10 or later for the frontend build and development scripts
- A local GNUbg installation with its neural network weights. Playing also requires GNUbg's embedded Python module. Analysis and playing were tested with GNUbg 1.08.003 on macOS. Earlier analysis checks used 1.05.002.

From the repository root, create `.env` if needed and start PostgreSQL. Use the Compose command below for the bundled database. To use another server, set `DATABASE_URL` in `.env` to the intended database. If you change the Compose port with `BGTUTOR_DB_PORT`, update `DATABASE_URL` to match.

```sh
[ -f .env ] || cp .env.example .env
docker compose up -d --wait db
```

The backend applies migrations at startup. Cargo can compile with PostgreSQL stopped. The backend reads `.env` from its working directory. See [.env.example](.env.example) for available settings.

Check that `gnubg --version` works. If the executable is outside the backend's `PATH`, set `GNUBG_BIN` in `.env` before the first launch or configure the executable under **Settings → Analysis → Engines on this computer**. Use **Check installation** to verify that GNUbg can evaluate a position with its installed weights.

### Local use with the built frontend

For regular local use, the backend serves the built frontend and the API on port `8787`. Keep PostgreSQL running and stop any BG Tutor backend already listening on that port before starting the release build with `BGTUTOR_WEB_DIR` set.

```sh
npm ci --prefix web
npm run build --prefix web
BGTUTOR_WEB_DIR=web/dist cargo run -p bgtutor --release
```

Open <http://127.0.0.1:8787> to use BG Tutor. By default the services listen on loopback. BG Tutor has no built-in user accounts, so network access requires separate access control.

| Service     | Address                                                               |
| ----------- | --------------------------------------------------------------------- |
| API health  | <http://127.0.0.1:8787/api/health>                                     |
| OpenAPI 3.1 | <http://127.0.0.1:8787/api/openapi.json>                               |
| PostgreSQL  | `127.0.0.1:54329`, backed by the `bgtutor_postgres_data` Docker volume |

### Frontend development with Vite

Start the backend from the repository root with `cargo run -p bgtutor`. In a second terminal at the repository root, start the Vite development server.

```sh
npm ci --prefix web
npm run dev --prefix web
```

Open <http://127.0.0.1:5173>. Vite forwards `/api` to `http://127.0.0.1:8787` by default. Set `BGTUTOR_API_URL` in Vite's environment to point it at a different backend. If port `5173` is in use, Vite reports the conflict at startup.

### Restart BG Tutor locally on port 8787

1. From the repository root, keep the existing PostgreSQL database and `.env` configuration. Reuse any `DATABASE_URL` and `GNUBG_BIN` overrides from the previous launch. If you use the Compose database and it is stopped, start it with `docker compose up -d --wait db`. If the frontend dependencies are missing or the lockfile changed, run `npm ci --prefix web`.
2. Build the updated application before stopping the running server.

   ```sh
   ./tools/build_local_restart.sh
   ```

   The script builds from the current checkout, including uncommitted changes. It uses `target/local-restart` to keep the restart build separate from other Cargo builds and recompiles the BG Tutor package on each run so SQLx embeds the current migration files. Continue once both builds succeed and the script prints the path to the new binary.

   The workspace `Cargo.toml` explicitly sets `strip = "none"` for the `dev` and `release` profiles; `test` and `bench` inherit these settings. This also covers direct Cargo commands and fresh build directories. No `CARGO_PROFILE_RELEASE_STRIP` prefix is needed. Release optimization remains enabled, but the artifacts can be larger.

   Keep these explicit settings while supporting Rust 1.94.1 on macOS 27: Cargo can implicitly strip debug information when no `strip` setting is present, and the bundled LLVM can then produce procedural-macro libraries that fail to load with `mis-aligned LINKEDIT string pool`. Removing an explicit `strip` option is therefore not equivalent to disabling stripping. Environment variables, Cargo config overrides, and `RUSTFLAGS` must not re-enable stripping. After a toolchain or profile change, verify a release build in a fresh target directory without a strip override. [Rust issue](https://github.com/rust-lang/rust/issues/157750)

3. Stop BG Tutor with `Ctrl+C` in its terminal. If it runs in the background, use `lsof -nP -iTCP:8787 -sTCP:LISTEN` to find the listener. Confirm that it is the `bgtutor` process, then replace `PID` with its process ID in `kill -TERM PID`. Wait until the listener has stopped.
4. From the repository root, start the binary produced by step 2. It applies pending migrations at startup and serves the built frontend on the same port.

   ```sh
   BGTUTOR_BIND=127.0.0.1:8787 BGTUTOR_WEB_DIR="$PWD/web/dist" ./target/local-restart/bgtutor
   ```

5. Check the health endpoint from another terminal, then open or reload BG Tutor and verify the workflow you changed. A response with `"status":"ok"` confirms PostgreSQL responds and the configured GNUbg reports its version. Allow up to five seconds for the GNUbg version command. During a benchmark, the health check can reuse the recorded engine version. **Check installation** runs the evaluation probe. [BG Tutor on port 8787](http://127.0.0.1:8787/)

   ```sh
   curl --fail --silent --show-error http://127.0.0.1:8787/api/health
   ```

## Analyze your first match

1. In **Match database**, choose **Import match** and open [tutorial.mat](backend/tests/fixtures/tutorial.mat) or the reference match [WC2014.xg](backend/tests/fixtures/WC2014.xg). You can also drop a file on the welcome screen. Search by player or file name to find an imported match. Use the numbered navigation to browse results or jump to the first or last page.
2. In **Analysis**, use **Select game** or its adjacent arrows to choose a game. The selector shows the game number, starting score, move count, and Crawford status when applicable. The move history stays within the selected game and loads more moves as needed.
3. Select a move. The board shows the decision position before the recorded play, together with the score and doubling cube. Point numbering and notation follow the player on roll. Pip counts include checkers on the bar.
4. In the Analysis header, open **Analysis options → Recalculate position …**, choose a depth, then select **Analyze position** to queue a persistent job. The default is 2-ply. The result includes up to five candidate moves, cubeful equity, and win probabilities.
5. Use **Analyze game** in the Analysis header to evaluate the selected game. In the **Match database**, select one or more matches and use **Analyze selected matches …** above the list. Change the engine or depth in **Analysis options** for a game, or in the confirmation dialog for the selected matches. The engine starts with the profile's selection. GNUbg and Open Sage both offer 4-ply for game and match analysis. **Displayed analysis** selects a saved result from the shared catalog. Games with no checker moves, including cube-only games, can also be selected and analyzed.
6. Open **Checker → Dice equity** to see saved values or calculate all 21 unordered dice pairs automatically if the position has no result. The matrix represents 36 equally likely ordered rolls, with weight 1 for doubles and 2 for other pairs. The weighted mean gives the expected equity. When a player has no legal play for a roll, GNUbg evaluates the forced pass. **Game rolls** shows the rolls recorded in the selected game.

**Match database → Find a position in the library** searches for an exact GNUbg position ID, which represents the checker arrangement from the perspective of the player on roll. Analysis also uses the GNUbg match ID for context such as the score, cube, and dice. Each occurrence in the library has its own row with the database, game, and move. Opening a row selects that exact occurrence, even when the same position appears more than once in a match. To find related positions, save the position and choose **Find similar positions** in its preview under **Positions**. The list of similar positions shows each occurrence's score as away values, the cube, the played move and, where the match has a displayed native analysis, the move's equity loss.

## Documentation

| Audience | Document | Contents |
| --- | --- | --- |
| Players | [User guide](docs/user-guide/README.md) | Guided tour of every workspace, learning paths by level, keyboard reference, troubleshooting and glossary |
| Detailed behavior | [Analysis](docs/reference/analysis.md) | Analysis workspace, candidate and cube review, displayed and imported analysis, opening books, game plans, error statistics, notes, time estimates and analysis jobs |
| | [Match library, positions and players](docs/reference/library.md) | Match databases, import formats, saved positions and searches, similar positions and their practice, player database |
| | [Playing](docs/reference/playing.md) | Matches against GNUbg or Open Sage, resigning, game results, feedback modes and the play API |
| | [Study](docs/reference/study.md) | Overview, reports, AI assessment, recurring mistakes, flashcards and Anki |
| | [Settings, engines and profiles](docs/reference/settings.md) | Settings, engine installations and threads, configuration profiles, themes and languages |
| | [Extras](docs/reference/extras.md) | Version information, engine benchmark, engine duels and database backup |
| | [HedgeHog comparison](docs/reference/hedgehog-comparison.md) | Feature comparison with HedgeHog |
| Developers | [API reference](docs/development/api.md) | Common REST endpoints and OpenAPI generation |
| | [Verification and tests](docs/development/testing.md) | Quality checks, PostgreSQL and GNUbg integration tests, import corpus, browser tests and native play tests |
| | [Architecture and data](docs/development/architecture.md) | Repository layout, storage of matches and positions, private demo data |
| | [Local Open Sage setup](docs/open-sage-local.md) | Installing and checking the Open Sage fork |
| | [Repository guidance](AGENTS.md) | Rules for changes to this repository |
