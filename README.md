<img src="web/public/neondojo-logo.svg" alt="NeonDojo logo: a torii gate whose pillars are backgammon points, in front of a checker as the rising sun" width="128" height="128">

# NeonDojo

<!-- The repository is private: Shields.io cannot read it, so its badges are static, and GitHub shows the CI badge only to viewers with access. Update the version badge together with backend/Cargo.toml and web/package.json for each release. -->
[![Version 1.4.2](https://img.shields.io/badge/version-1.4.2-2ea44f)](CHANGELOG.md)
[![CI](https://github.com/HendrikReh/neondojo/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/HendrikReh/neondojo/actions/workflows/ci.yml?query=branch%3Amain)
[![NeonDojo manual](https://img.shields.io/badge/NeonDojo%20manual-0969DA)](https://hendrikreh.github.io/neondojo/)
[![NeonGammon manual](https://img.shields.io/badge/NeonGammon%20manual-0969DA)](https://hendrikreh.github.io/neongammon/)
[![Platform: macOS on Apple Silicon](https://img.shields.io/badge/platform-macOS%20%7C%20Apple%20Silicon-000000?logo=apple&logoColor=white)](#requirements)
[![Rust 1.93.1](https://img.shields.io/badge/Rust-1.93.1-CE422B?logo=rust&logoColor=white)](#requirements)
[![Node.js 22.12+ or 24+](https://img.shields.io/badge/Node.js-22.12%2B%20%7C%2024%2B-5FA04E?logo=nodedotjs&logoColor=white)](#requirements)
[![PostgreSQL 17](https://img.shields.io/badge/PostgreSQL-17-4169E1?logo=postgresql&logoColor=white)](#database-and-engine)
[![React 19](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)](web/package.json)
[![TypeScript 5.9](https://img.shields.io/badge/TypeScript-5.9-3178C6?logo=typescript&logoColor=white)](web/package.json)
[![Vite 7](https://img.shields.io/badge/Vite-7-646CFF?logo=vite&logoColor=white)](web/package.json)
[![OpenAPI 3.1](https://img.shields.io/badge/OpenAPI-3.1-6BA539?logo=openapiinitiative&logoColor=white)](docs/development/api.md)
[![GNUbg 1.08.003](https://img.shields.io/badge/GNUbg-1.08.003-6F42C1)](#requirements)
[![NeonGammon 3.2.1 optional](https://img.shields.io/badge/NeonGammon-3.2.1%20optional-8250DF)](docs/open-sage-local.md)
[![Languages: 2](https://img.shields.io/badge/languages-EN%20%7C%20DE-0969DA)](docs/reference/settings.md#interface-appearance-and-language)

NeonDojo is a local backgammon study app for Apple Silicon Macs. Import your matches, find positions in your library, and practice decisions you missed. You can review checker plays and cube decisions with GNU Backgammon (GNUbg) or install [NeonGammon](https://github.com/HendrikReh/neongammon) as a second engine. NeonDojo shows the engine’s analysis alongside your moves and lets you play matches against either engine.

We chose “Neon” as a nod to Arm’s Neon SIMD (single instruction, multiple data) instructions on Apple Silicon. They let the CPU apply the same operation to several values at once.

NeonDojo’s headless Rust backend uses Tokio, Axum, PostgreSQL, and SQLx. The React and TypeScript client connects through REST and server-sent events (SSE).

## Features

- **Match library.** Import MAT (also as plain-text `.txt` transcripts, preserving saved import notes for transcript irregularities), XG, SGF, BGF and OGXM files one by one or as a whole folder. Search by player, file name, metadata or notes, find duplicate matches, and export the original file, SGF or XG. [Match library](docs/reference/library.md#match-databases)
- **Positions.** Find an exact GNUbg position ID in the library, search all analyzed decisions, save positions and searches, and practice similar positions. [Saved positions](docs/reference/library.md#saved-positions-and-searches)
- **Analysis.** Analyze positions, games and matches as persistent background jobs with GNUbg up to 5-ply or NeonGammon up to 4-ply. Imported analyses stay separate from local ones. Review candidates and cube decisions, dice equity, error statistics, game plans and opening books. [Analysis](docs/reference/analysis.md#analysis-workspace)
- **Playing.** Play point matches and money games against GNUbg or NeonGammon in Competition, Training, Coach or Tutor feedback mode. Choose Beginner, Casual, or Intermediate to practice against a NeonGammon opponent that makes mistakes, or challenge it at Full Strength. NeonGammon also offers 2-ply + Top-2 (10 mEq). [Playing](docs/reference/playing.md#playing-against-an-engine)
- **Study.** Reports, optional AI assessment, recurring mistakes, and flashcards scheduled with FSRS that export to and import from Anki. [Study](docs/reference/study.md#study)
- **Players.** A player database with profiles, merging and virtual player profiles. [Player database](docs/reference/library.md#player-database)
- **Extras.** Version information, engine benchmark, engine duels and database backup. [Extras](docs/reference/extras.md)

[User guide](docs/user-guide/README.md) · [Features](#features) · [Quick start](#quick-start) · [Restart on port 8787](#restart-neondojo-locally-on-port-8787) · [Analyze a match](#analyze-your-first-match) · [Documentation](#documentation) · [API](docs/development/api.md) · [Verification](docs/development/testing.md) · [Repository guidance](AGENTS.md) · [Changelog](CHANGELOG.md)

## Quick start

### Requirements

- Rust, tested with 1.93.1
- Node.js 22.12 or later in the 22.x series, or Node.js 24 or later, with npm
- Docker Compose for the bundled PostgreSQL 17 database, or an existing PostgreSQL server
- Python 3.10 or later for the frontend build and development scripts
- A local GNUbg installation with its neural network weights. Playing also requires GNUbg's embedded Python module. Analysis and playing were tested with GNUbg 1.08.003 on macOS. Earlier analysis checks used 1.05.002.
- Optional: NeonGammon 3.2.1 as a second engine, running in a locked native ARM64 Python 3.12 environment on macOS 14 or later (3.2.0 and 3.1.0 through 3.1.2 also support named difficulty presets; 3.0.0 remains supported for analysis and ordinary play without presets). See [Local NeonGammon setup](docs/open-sage-local.md).
- The BGBlitz theme archives listed in `shared/configuration-options.json`, copied into `themes/`. They are local third-party files and not part of the repository. Without them, `npm run build`, `npm run dev` and `npm test` stop and name the missing ZIP files; set `NEONDOJO_ALLOW_MISSING_THEMES=1` only when intentionally building without them. Only `npm run themes:prepare --prefix web -- --update-options` rewrites the tracked theme list. See [Board artwork](docs/reference/settings.md#board-artwork) and [Board theme formats](themes/README.md).

### Database and engine

From the repository root, create `.env` if needed and start PostgreSQL. Use the Compose command below for the bundled database. To use another server, set `DATABASE_URL` in `.env` to the intended database. If you change the Compose port with `NEONDOJO_DB_PORT`, update `DATABASE_URL` to match.

```sh
[ -f .env ] || cp .env.example .env
docker compose up -d --wait db
```

The backend applies migrations at startup. Cargo can compile with PostgreSQL stopped. The backend reads `.env` only from its working directory, not from parent directories. See [.env.example](.env.example) and [Environment variables](#environment-variables).

Check that `gnubg --version` works. If the executable is outside the backend's `PATH`, set `GNUBG_BIN` in `.env` before the first launch or configure the executable under **Settings → Engines**. Use **Check installation** to verify that GNUbg can evaluate a position with its installed weights.

### Environment variables

The backend reads these settings from its environment or from `.env` in its working directory.

| Variable | Default | Purpose |
| --- | --- | --- |
| `DATABASE_URL` | `postgres://neondojo:neondojo_local@127.0.0.1:54329/neondojo` | PostgreSQL database |
| `NEONDOJO_BIND` | `127.0.0.1:8787` | Listen address for the API and the built frontend. Requests must address a loopback host name ([Local access](docs/development/api.md#local-access)) |
| `NEONDOJO_WEB_DIR` | unset | Serve the built frontend from this directory, usually `web/dist` |
| `NEONDOJO_DB_MAX_CONNECTIONS` | `32` | Largest number of PostgreSQL connections the backend opens (`1–1000`). Keep the sum over all backends on one server below its `max_connections` (PostgreSQL default `100`). A query waits up to 15 seconds for a free connection. |
| `GNUBG_BIN` | `gnubg` | GNUbg executable until a path is saved in **Settings** |
| `RUST_LOG` | `neondojo=info,tower_http=info` | Log filter |
| `NEONDOJO_ANKI_PYTHON` | `python3` | Python with the Anki package for importing third-party Anki decks. See [Flashcards and Anki](docs/reference/study.md#flashcards-and-anki). |
| `NEONDOJO_ANKI_CONNECT_KEY` | unset | API key for a local AnkiConnect add-on that requires one |
| `OPENAI_API_KEY`, `NEONDOJO_ASSESSMENT_MODEL` | unset, `gpt-6.1-terra` | Optional AI assessment of saved reports. See [AI assessment](docs/reference/study.md#ai-assessment). |

Docker Compose and the frontend tools also recognize three variables: `NEONDOJO_DB_PORT` (default `54329`, host port in [compose.yaml](compose.yaml)), `NEONDOJO_API_URL` (default `http://127.0.0.1:8787`, Vite `/api` proxy target), and `NEONDOJO_ALLOW_MISSING_THEMES` (default unset; set to `1` only to build the theme catalog without missing BGBlitz archives; `shared/configuration-options.json` stays unchanged unless preparation runs with `--update-options`).

### Local use with the built frontend

For regular local use, the backend serves the built frontend and the API on port `8787`. Keep PostgreSQL running and stop any NeonDojo backend already listening on that port before starting the release build with `NEONDOJO_WEB_DIR` set.

```sh
npm ci --prefix web
npm run build --prefix web
NEONDOJO_WEB_DIR=web/dist cargo run -p neondojo --release
```

Open <http://127.0.0.1:8787> to use NeonDojo. By default the services listen on loopback. NeonDojo has no built-in user accounts, so network access requires separate access control.

| Service     | Address                                                               |
| ----------- | --------------------------------------------------------------------- |
| API health  | <http://127.0.0.1:8787/api/health>                                     |
| OpenAPI 3.1 | <http://127.0.0.1:8787/api/openapi.json>                               |
| PostgreSQL  | `127.0.0.1:54329`, backed by the `neondojo_postgres_data` Docker volume |

### Frontend development with Vite

Start the backend from the repository root with `cargo run -p neondojo`. In a second terminal at the repository root, start the Vite development server.

```sh
npm ci --prefix web
npm run dev --prefix web
```

Open <http://127.0.0.1:5173>. Vite forwards `/api` to `http://127.0.0.1:8787` by default. Set `NEONDOJO_API_URL` in Vite's environment to point it at a different backend. If port `5173` is in use, Vite reports the conflict at startup.

### Restart NeonDojo locally on port 8787

1. From the repository root, keep the existing PostgreSQL database and `.env` configuration. Reuse any `DATABASE_URL` and `GNUBG_BIN` overrides from the previous launch. If you use the Compose database and it is stopped, start it with `docker compose up -d --wait db`. If the frontend dependencies are missing or the lockfile changed, run `npm ci --prefix web`.
2. Build the updated application before stopping the running server.

   ```sh
   ./tools/build_local_restart.sh
   ```

   The script builds from the current checkout, including uncommitted changes. It uses `target/local-restart` to keep the restart build separate from other Cargo builds and recompiles the NeonDojo package on each run so SQLx embeds the current migration files. Continue once both builds succeed and the script prints the path to the new binary.

   The workspace `Cargo.toml` explicitly sets `strip = "none"` for the `dev` and `release` profiles; `test` and `bench` inherit these settings. This also covers direct Cargo commands and fresh build directories. No `CARGO_PROFILE_RELEASE_STRIP` prefix is needed. Release optimization remains enabled, but the artifacts can be larger.

   Keep these explicit settings while supporting Rust 1.94.1 on macOS 27: Cargo can implicitly strip debug information when no `strip` setting is present, and the bundled LLVM can then produce procedural-macro libraries that fail to load with `mis-aligned LINKEDIT string pool`. Removing an explicit `strip` option is therefore not equivalent to disabling stripping. Environment variables, Cargo config overrides, and `RUSTFLAGS` must not re-enable stripping. After a toolchain or profile change, verify a release build in a fresh target directory without a strip override. [Rust issue](https://github.com/rust-lang/rust/issues/157750)

3. Stop NeonDojo with `Ctrl+C` in its terminal. If it runs in the background, use `lsof -nP -iTCP:8787 -sTCP:LISTEN` to find the listener. Confirm that it is the `neondojo` process, then replace `PID` with its process ID in `kill -TERM PID`. Wait until the listener has stopped.
4. From the repository root, start the binary produced by step 2. It applies pending migrations at startup and serves the built frontend on the same port.

   ```sh
   NEONDOJO_BIND=127.0.0.1:8787 NEONDOJO_WEB_DIR="$PWD/web/dist" ./target/local-restart/neondojo
   ```

5. Check the health endpoint from another terminal, then open or reload NeonDojo and verify the workflow you changed. A response with `"status":"ok"` confirms PostgreSQL responds and the configured GNUbg reports its version. If either check fails, the endpoint returns HTTP 503 with `"status":"degraded"`, and `curl --fail` exits with an error. Allow up to five seconds for the GNUbg version command. During a benchmark, the health check can reuse the recorded engine version. The health check does not evaluate a position; **Check installation** runs that probe. [NeonDojo on port 8787](http://127.0.0.1:8787/)

   ```sh
   curl --fail --silent --show-error http://127.0.0.1:8787/api/health
   ```

## Analyze your first match

1. In **Match database**, choose **Import match** and open [tutorial.mat](backend/tests/fixtures/tutorial.mat) or the reference match [WC2014.xg](backend/tests/fixtures/WC2014.xg). **Import folder** imports every supported file of a folder and its subfolders. You can also drop files or a folder on the welcome screen. MAT and XG transcripts with recorded irregularities still import with saved **Import notes**. Search by player or file name to find an imported match, or use **Duplicates** to group matches with the same played record. Use the numbered navigation to browse results or jump to the first or last page.
2. In **Analysis**, use **Select game** or its adjacent arrows to choose a game. The selector shows the game number, starting score, move count, and Crawford status when applicable. The move history stays within the selected game and loads more moves as needed.
3. Select a move. The board shows the decision position before the recorded play, together with the score and doubling cube. Point numbering and notation follow the player on roll. Pip counts include checkers on the bar.
4. In the Analysis header, open **Analysis options → Recalculate position …**, choose a depth, then select **Analyze position** to queue a persistent job. The default is 3-ply. Depths follow [one ply convention](docs/reference/settings.md#ply-convention) for both engines. The result includes ranked candidate moves, cubeful equity, and win probabilities.
5. Use **Analyze game** in the Analysis header to evaluate the selected game. In the **Match database**, select one or more matches and use **Analyze selected matches …** above the list. Change the engine or depth in **Analysis options** for a game, or in the confirmation dialog for the selected matches. The engine starts with the profile's selection. Game and match analysis go up to 5-ply with GNUbg and 4-ply with NeonGammon. **Displayed analysis** selects a saved result from the shared catalog. Games with no checker moves, including cube-only games, can also be selected and analyzed.
6. Open **Checker → Dice equity** to see saved values or calculate all 21 unordered dice pairs automatically if the position has no result. The matrix represents 36 equally likely ordered rolls, with weight 1 for doubles and 2 for other pairs. The weighted mean gives the expected equity. When a player has no legal play for a roll, the active engine evaluates it as a forced pass. **Game rolls** loads the recorded roll frequencies for the selected game on demand.

**Match database → Position ID** (or **Search this position** above the board in **Analysis**) searches for an exact GNUbg position ID, which represents the checker arrangement from the perspective of the player on roll. Analysis also uses the GNUbg match ID for context such as the score, cube, and dice. Each occurrence in the library has its own row with the database, game, and move. Opening a row selects that exact occurrence, even when the same position appears more than once in a match. To find related positions, save the position and choose **Find similar positions** in its preview under **Positions**. The list of similar positions shows each occurrence's score as away values, the cube, the played move and, where the match has a displayed native analysis, the move's equity loss.

## Documentation

Read the [NeonDojo documentation on GitHub Pages](https://hendrikreh.github.io/neondojo/).

| Audience | Document | Contents |
| --- | --- | --- |
| Players | [User guide](docs/user-guide/README.md) | Guided tour of every workspace, learning paths by level, keyboard reference, troubleshooting and glossary |
| Detailed behavior | [Analysis](docs/reference/analysis.md) | Analysis workspace, candidate and cube review, displayed and imported analysis, opening books, game plans, error statistics, notes, time estimates and analysis jobs |
| | [Match library, positions and players](docs/reference/library.md) | Match databases, duplicate match finder, import formats and notes, saved positions, analyzed-decision search, similar positions and their practice, player database and virtual player profiles |
| | [Playing](docs/reference/playing.md) | Matches against GNUbg or NeonGammon, NeonGammon difficulty presets and 2-ply + Top-2, resigning, game results, feedback modes and the play API |
| | [Study](docs/reference/study.md) | Overview, reports, AI assessment, recurring mistakes, flashcards and Anki |
| | [Settings, engines and profiles](docs/reference/settings.md) | Settings, engine installations and threads, configuration profiles, themes and languages |
| | [Extras](docs/reference/extras.md) | Version information, engine benchmark, engine duels and database backup |
| | [HedgeHog comparison](docs/reference/hedgehog-comparison.md) | Feature comparison with HedgeHog |
| Developers | [API reference](docs/development/api.md) | Common REST endpoints, other endpoint groups and OpenAPI generation |
| | [Verification and tests](docs/development/testing.md) | Quality checks, PostgreSQL and GNUbg integration tests, import corpus, browser tests and native play tests |
| | [Architecture and data](docs/development/architecture.md) | Repository layout, schema baseline, storage of matches and positions, private demo data |
| | [UI navigation](docs/ui-navigation.md), [typography](docs/typography.md) and [action icons](docs/design/action-icons-proposal.md) | Text navigation, keyboard operation, type scale, tokens and Lucide action icons for interface changes |
| | [Local NeonGammon setup](docs/open-sage-local.md) | Installing and checking NeonGammon 3.2.1, measurements and limits |
| | [Engine duel CPU budget](docs/engine-duel-cpu-budget.md) | Concept for comparing engines with the same computing budget |
| | [Import corpus results](docs/import-corpus-results.md) | Results of importing the local match corpus |
| | [Repository guidance](AGENTS.md) | Rules for changes to this repository |

Dated plans and design specifications under [`docs/superpowers/`](docs/superpowers/) and product research under [`docs/research/`](docs/research/) record how features were planned. They are historical; the reference documents above describe the current behavior.

## License

You may use, modify, and share NeonDojo for noncommercial purposes, including personal study and free, unpaid instruction. Commercial use and the use of NeonDojo to prepare or deliver paid courses or coaching are prohibited. See the [NeonDojo Noncommercial License](LICENSE) for the full terms.
