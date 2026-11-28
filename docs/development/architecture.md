# Architecture and data

## Repository layout

| Path                                                                                                               | Responsibility                                                      |
| ------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| [`backend/src/api.rs`](../../backend/src/api.rs)                                                                         | REST, SSE, and OpenAPI (thin HTTP wrapper over `CoreRuntime` / `BackendFacade`) |
| [`backend/src/facade/`](../../backend/src/facade/) | Typed in-process `BackendFacade` (8 domain modules + broadcast push channels) used by the native GPUI desktop client |
| [`crates/neondojo-rules/`](../../crates/neondojo-rules/) | Pure-Rust backgammon board rules, legal checker turns, GNUbg IDs, notation, and embedded configuration options (zero async/DB deps) |
| [`crates/neondojo-i18n/`](../../crates/neondojo-i18n/) | Native EN/DE localization catalog (2,088 keys), ICU plural/variable formatter, live-retranslatable `AppError`, and locale-aware formatters |
| [`crates/neondojo-board/`](../../crates/neondojo-board/) | GPU-accelerated GPUI `BoardView`, 9 built-in themes + direct Rust BGBlitz ZIP loader, `ThemeAtlasCache`, 60–120fps animation state, and `rodio` play audio |
| [`crates/neondojo-desktop/`](../../crates/neondojo-desktop/) | Native macOS GPUI desktop shell (`AppShell`), `TokioBridge`, `NeonTheme` (6 palettes), `Settings`, shared `review/`, all 7 workspaces, and scoped `WKWebView` for third-party HTML/JS Anki cards |
| [`backend/src/backgammon/`](../../backend/src/backgammon/) | Re-exports `neondojo_rules::backgammon` for backend callers |
| [`backend/src/import/`](../../backend/src/import/) | Import formats, replay, and match validation. [`format.rs`](../../backend/src/import/format.rs) is the one list of match formats and of what differs between them |
| [`backend/src/engine.rs`](../../backend/src/engine.rs), [`backend/src/match_analysis.rs`](../../backend/src/match_analysis.rs) | Isolated GNUbg processes and native checker and cube analysis       |
| [`backend/src/engines/`](../../backend/src/engines/) | Engine providers, capability checks, and NeonGammon sidecar (`worker.py`) |
| [`backend/src/jobs.rs`](../../backend/src/jobs.rs), [`backend/src/match_jobs.rs`](../../backend/src/match_jobs.rs)             | Job state, progress events, game checkpoints, and player statistics |
| [`backend/src/play/`](../../backend/src/play/), [`backend/src/play.rs`](../../backend/src/play.rs) | Live play sessions, legal checker paths, rules process, and play rules |
| [`backend/src/learning/`](../../backend/src/learning/), [`backend/src/recurring_mistakes.rs`](../../backend/src/recurring_mistakes.rs), [`backend/src/practice.rs`](../../backend/src/practice.rs) | Flashcards and FSRS, recurring mistakes, and shared answer evaluation |
| [`backend/src/player_directory.rs`](../../backend/src/player_directory.rs), [`backend/src/player_scope.rs`](../../backend/src/player_scope.rs) | Player profiles, virtual player profiles, and PR scoping |
| [`backend/src/schema_migrations.rs`](../../backend/src/schema_migrations.rs), [`backend/src/database_backup.rs`](../../backend/src/database_backup.rs) | Baseline adoption, sequential migrations, and portable backups |
| [`backend/src/store.rs`](../../backend/src/store.rs), [`backend/migrations/`](../../backend/migrations/)                       | SQLx persistence, schema, and search indexes                        |
| [`web/src/`](../../web/src/)                                                                                             | React client, SVG board, and generated API types                    |
| [`web/e2e/`](../../web/e2e/), [`web/playwright.setup.ts`](../../web/playwright.setup.ts) | Browser workflows; one isolated PostgreSQL database, backend and Vite server per Playwright worker ([`web/e2e/helpers/stack.ts`](../../web/e2e/helpers/stack.ts)) |

The Rust backend runs independently of the browser and desktop UI. [`neondojo::runtime::CoreRuntime::start`](../../backend/src/runtime.rs) starts the PostgreSQL pool, migrations, backfills, and background workers and exposes a typed in-process [`BackendFacade`](../../backend/src/facade/mod.rs) for the native GPUI app (`crates/neondojo-desktop`). The CLI HTTP server wraps `CoreRuntime` via [`neondojo::runtime::start`](../../backend/src/runtime.rs), binding an Axum listener (including port zero for tests) and draining HTTP requests and background workers on `RunningServer::shutdown().await`. Each running instance owns its opening-book caches and its Study, Play, and Anki operation limits.

### Engine providers

Stored snapshots, settings and HTTP contracts keep the engine ID strings `gnubg` and `bgsage` (NeonGammon). [`engines/provider.rs`](../../backend/src/engines/provider.rs) parses them into `EngineId`, which owns the display and product names, the verified depths and thread minimums from [configuration options](../../shared/configuration-options.json), and every capability that differs between engines: GNUbg as the rules engine, use of the GNUbg thread settings, verified sidecar profiles, difficulty presets, top-2 play and practice depth caps. Unknown IDs parse to nothing and fail; nothing falls back to another engine. `AnalysisEngine` in [`engines/mod.rs`](../../backend/src/engines/mod.rs) dispatches the operations both engines share (analysis, cube equities, installation checks, benchmark binary identity, cold-worker restart). Feature modules that need an engine-specific implementation, such as play feedback, duel turns, the play benchmark and practice checker scoring, match `AnalysisEngine` exhaustively, so a new engine is a compile error at each of them.

### Web client layout

`web/src/` groups the client by feature. Each workspace folder owns its components, API calls, messages, styles and tests. A module that several features use lives in `shared/`.

| Path | Contents |
| --- | --- |
| `main.tsx`, `generated-api.d.ts`, `generated-themes.json`, `assets/` | Entry point, generated API types and theme catalogue, bundled sounds |
| `app/` | `App.tsx` with the workspace order (`views`) and header; hooks for importing matches (`useMatchImporter`), restoring the last match (`useSelectionRestore`), installation settings, global shortcuts and background jobs |
| `styles/` | Global stylesheets, the cascade order in `stylesheets.ts` (imported first by `main.tsx`) and the palette tokens linked from `index.html` |
| `settings/` | Settings dialog and its sections; `configuration/`, `profiles/` and `appearance/` |
| `shared/` | `http/` (request helpers), `jobs/`, `i18n/`, `format/`, `engine/`, `ui/` (buttons, dialogs, tooltips, search tables), `hooks/`, `board/` (board, checker play state, themes, sounds) and `review/` (decision details shared by Analysis, Play and Study) |
| `workspaces/play/` | Playing |
| `workspaces/analysis/` | Analysis; `match/` (match analysis and statistics), `candidates/` (candidate tables and source analyses), `notes/` and `game-plans/` |
| `workspaces/study/` | Study; `flashcards/`, `reports/`, `mistakes/` and `practice/` |
| `workspaces/positions/` | Positions, Decisions and Similar positions |
| `workspaces/library/` | Match database, its database list (`useDatabases`) and `import/` |
| `workspaces/players/`, `workspaces/extras/` | Player database; Extras (benchmark, engine matches, backups, version) |

**Loading.** The initial shell includes the welcome screen and the Match database. The database clears a deleted import destination at startup. Analysis loads when the first match opens or restores. Playing, Study, Positions, the Player database, Extras and Settings load on their first visit. Visited workspaces stay mounted to preserve their state. Playing, Study, Saved positions and Extras mount hidden for one commit before becoming visible (`MountHidden`). A pending chunk shows a localized loading line, and a failed download offers a reload. The entry point waits for the saved language before rendering. A saved BGBlitz board loads its artwork catalog in parallel with the language. Built-in boards load without the archive catalog. Opening the board gallery or displaying an archive board loads the catalog on demand.

**Analysis.** `AnalysisWorkspace` holds the state that several of its areas share and renders their controls in place. `useMatchReview` owns the filters, pages, error navigation and statistics of the displayed analysis, and `usePositionCalculation` the evaluation tab and the recalculation of the displayed position. `MatchAnalysisHeader` owns the subscription to the running game analysis, so it stays mounted on every inspector tab and while another workspace is open.

**Playing.** `PlayWorkspace` combines hooks with one responsibility each: `usePlaySession` restores and loads the match and runs one request at a time, `usePlayTurn` holds the unconfirmed checker turn, and `usePlayDialogs` the dialogs the player opens. `playInterruption` decides which dialog stands between the player and the board. `PlayBoardStage` alone reads the animation frames, so the rest of the workspace does not render again while the dice roll.

**Messages.** Each language has a [locale module](../../web/src/shared/i18n/locales/) with one dictionary per feature and an `app` dictionary for `useI18n().t`. The English module defines the key schema. The per-language `workspace-messages.ts` files own the shell’s tab and status labels. A feature exposes typed formatters through `defineMessages('feature-name')` in its own `*-messages.ts` module. A language switch keeps the current language visible until the new module has loaded. [Message tests](../../web/src/shared/i18n/messages.test.ts) exercise every key, placeholder and plural form in both languages.

**Stylesheet order.** [`styles/stylesheets.ts`](../../web/src/styles/stylesheets.ts) imports every stylesheet in cascade order, and `main.tsx` imports it before any component. Components also import their own stylesheet, but those imports add nothing, so neither module order nor lazy loading changes which of two equally specific rules wins. The manifest places `style.css` before component styles and loads desktop layout, typography and text navigation after them. `stylesheets.test.ts` checks that each stylesheet appears once and that the base stylesheet loads first. Compare computed styles in all six palettes before and after changing the order. Keep `/api/` out of module paths: the Vite dev server serves modules under `/src/…`, and browser tests intercept `**/api/**` as backend requests.

## Database schema

[`0001_schema.sql`](../../backend/migrations/0001_schema.sql) is the schema baseline. It consolidates the former migrations 0001–0084 and seeds the system databases, the NeonGammon installation record and the learning preferences. Add each schema change as a new sequential migration after the latest file in [`backend/migrations/`](../../backend/migrations/) and leave the baseline unchanged. Sequential migrations start at `0085_bot_match_players.sql`; the directory lists them all.

Before migrating, [`schema_migrations.rs`](../../backend/src/schema_migrations.rs) adopts a database that was migrated with the former files. If its `_sqlx_migrations` records match versions 0001–0084 and their checksums exactly, it replaces them with the baseline record in one transaction and leaves tables and rows unchanged. A database from NeonDojo 1.0.0 records 0001–0083; it first receives the former 0084, embedded in `schema_migrations/`, in the same transaction. A database with any other incomplete or changed history fails to start; update it with a NeonDojo version from before the consolidation first.

The backup fingerprint in [`database_backup.rs`](../../backend/src/database_backup.rs) expands the baseline record back to the former 0001–0084 checksums, so backups made before and after the consolidation stay compatible. The fingerprint skips migrations listed in `ARCHIVE_COMPATIBLE_MIGRATIONS`, currently 0089–0094 and 0096. These migrations preserve the archived column layout. Migration 0094 also prunes finished jobs’ progress events. Older archives can still restore those rows. Migration 0095 introduced a new layout for game cube rules. When restoring the previous layout, NeonDojo recovers each game’s cube rule from its saved SGF.

The backup tests apply each allowlisted migration and compare the fingerprint and the names, types and order of the columns used by PostgreSQL COPY. The restore fixtures exercise both the 0094 game-rule conversion and a 0095 archive with an explicitly selected imported analysis restored on 0096. [Restore integration tests](../../backend/tests/database_backup.rs)

### Settings

`configuration_state.current_configuration` owns every profile setting. [`configuration/store.rs`](../../backend/src/configuration/store.rs) is the only writer: `write_state` saves the configuration and, in the same transaction, repeats the player name, engines, depths, play options and bear-off direction in `app_settings` and the book choice in `opening_book_settings`. Engine, job and book code reads these tables. `PUT /api/settings` and `PUT /api/opening-book` go through the same function; nothing copies from the tables back, except `initialize`, which imports them once when the configuration does not exist yet. The GNUbg path and version, the thread overrides and the number of parallel analysis jobs exist only in `app_settings`, belong to the installation, and are written by `PUT /api/settings`. Profiles do not contain them. A restore keeps them, and points the book paths of the configuration and of every profile at the target's files.

Host-only settings edits and repeated saves of the same opening-book choice preserve the current profile configuration revision. The settings and opening-book endpoints still check and advance their own versions on each accepted save.

### Locking

Request handlers serialize with row locks (`SELECT … FOR UPDATE` on the edited record), unique constraints and transaction-scoped advisory locks, not table locks. [`advisory_locks.rs`](../../backend/src/advisory_locks.rs) holds every fixed advisory key and every seed of a hashed per-entity key (`hashtextextended(id, seed)`), names the writers each key serializes, and tests that the keys are distinct. An advisory lock protects only the writers that take it: limits such as 200 card topics or saved position filters, the shared transfer-practice seen list and the choice of My player profile are each written only by the handler that takes its key. The remaining table locks belong to migrations (`schema_migrations.rs`) and to restore, which locks the restored tables in `ACCESS EXCLUSIVE` mode.

## How matches and positions are stored

Imports store the original file, canonical SGF, resolved import warnings (`import_warnings`), and normalized positions together in PostgreSQL. A position records 26 signed checker counts including the bar, the player on roll, the dice, the game and move numbers, and the played move. Each library match also stores a `played_record_fingerprint` derived from its positions and kept current by database triggers for duplicate match detection. The game catalog also records games without checker moves, with their opening score and Crawford status. On first access, older imports build the catalog from stored SGF.

The **80-bit GNUbg Position ID** encodes checker counts as 14 Base64 characters. Exact board searches use its B-tree index without reparsing the original file. The separate **72-bit GNUbg Match ID** records match length, score, cube, dice and rules. A match, game and move-order index gives direct access to a late game without loading earlier games. Derived read models (`decision_index`, `decision_index_matches`, `native_analysis_revisions`, `imported_analysis_revisions`, `review_hits`, `review_hit_sets`) are excluded from backups and rebuilt on demand.

## Private demo data

The Git-ignored folders `games/bgf`, `games/mat` and `games/xg` are for private matches. To build a demo library with the current schema, create **Demo BGF**, **Demo MAT** and **Demo XG** in **Match database** and import each folder's files into its corresponding database. The importer reports duplicates and rejected files individually.

Do not use the legacy helper `tools/rebuild_demo.py` with the current schema. Its staging step expects the public REST database list to contain every PostgreSQL database ID, but that list omits the internal saved-position collection. Its activation SQL deletes matches outside the three demo databases and could therefore delete that collection's matches.

The helper was designed to stage three databases through REST, verify a PostgreSQL dump and record each file's outcome in a manifest outside Git. Its `--allow-rejected` option allows staged matches from valid files to proceed after you review the individual rejections. Network and server errors still abort staging. With the backend stopped, activation checks the PostgreSQL cluster and database identity, all three staging databases, match IDs, their database assignments and SHA-256 hashes under a transaction lock. A failed check leaves the active library intact. Each manifest can be activated once.

The local rebuild on November 8, 2026 imported 5 BGF, 5 MAT and 23 XG matches, with 72,959 checker and cube analysis records and 98 additional source-statistics records. The private report lists nine XG files rejected for invalid or inconsistent moves. The previous library was backed up and the rebuild preserved the flashcards and review history. Public sources and product research are listed in the [BackgammonDB comparison](../research/2026-11-08-backgammondb-product-ux.md).
