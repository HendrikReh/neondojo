# Architecture and data

## Repository layout

| Path                                                                                                               | Responsibility                                                      |
| ------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| [`backend/src/api.rs`](../../backend/src/api.rs)                                                                         | REST, SSE, and OpenAPI                                              |
| [`backend/src/import/`](../../backend/src/import/)                                                                       | Import, rule validation, and GNUbg identifiers                      |
| [`backend/src/engine.rs`](../../backend/src/engine.rs), [`backend/src/match_analysis.rs`](../../backend/src/match_analysis.rs) | Isolated GNUbg processes and native checker and cube analysis       |
| [`backend/src/engines/`](../../backend/src/engines/) | Engine providers, capability checks, and NeonGammon sidecar (`worker.py`) |
| [`backend/src/jobs.rs`](../../backend/src/jobs.rs), [`backend/src/match_jobs.rs`](../../backend/src/match_jobs.rs)             | Job state, progress events, game checkpoints, and player statistics |
| [`backend/src/play/`](../../backend/src/play/), [`backend/src/play.rs`](../../backend/src/play.rs) | Live play sessions, legal checker paths, rules process, and play rules |
| [`backend/src/learning/`](../../backend/src/learning/), [`backend/src/recurring_mistakes.rs`](../../backend/src/recurring_mistakes.rs), [`backend/src/practice.rs`](../../backend/src/practice.rs) | Flashcards and FSRS, recurring mistakes, and shared answer evaluation |
| [`backend/src/player_directory.rs`](../../backend/src/player_directory.rs), [`backend/src/player_scope.rs`](../../backend/src/player_scope.rs) | Player profiles, virtual player profiles, and PR scoping |
| [`backend/src/schema_migrations.rs`](../../backend/src/schema_migrations.rs), [`backend/src/database_backup.rs`](../../backend/src/database_backup.rs) | Baseline adoption, sequential migrations, and portable backups |
| [`backend/src/store.rs`](../../backend/src/store.rs), [`backend/migrations/`](../../backend/migrations/)                       | SQLx persistence, schema, and search indexes                        |
| [`web/src/`](../../web/src/)                                                                                             | React client, SVG board, and generated API types                    |
| [`web/e2e/`](../../web/e2e/), [`web/playwright.setup.ts`](../../web/playwright.setup.ts) | Browser workflows; one isolated PostgreSQL database, backend and Vite server per Playwright worker ([`web/e2e/stack.ts`](../../web/e2e/stack.ts)) |

The Rust backend runs independently of the browser. A future Tauri shell could start it as a sidecar and reuse the HTTP API.

## Database schema

[`0001_schema.sql`](../../backend/migrations/0001_schema.sql) is the schema baseline. It consolidates the former migrations 0001–0084 and seeds the system databases, the NeonGammon installation record and the learning preferences. Add each schema change as a new sequential migration after the latest file in [`backend/migrations/`](../../backend/migrations/) and leave the baseline unchanged. Sequential migrations currently run from `0085` through `0089` (`0085_bot_match_players.sql`, `0086_played_record_fingerprint.sql`, `0087_match_import_warnings.sql`, `0088_play_top_two.sql`, and `0089_decision_source_revisions.sql`).

Before migrating, [`schema_migrations.rs`](../../backend/src/schema_migrations.rs) adopts a database that was migrated with the former files. If its `_sqlx_migrations` records match versions 0001–0084 and their checksums exactly, it replaces them with the baseline record in one transaction and leaves tables and rows unchanged. A database from NeonDojo 1.0.0 records 0001–0083; it first receives the former 0084, embedded in `schema_migrations/`, in the same transaction. A database with any other incomplete or changed history fails to start; update it with a NeonDojo version from before the consolidation first.

The backup fingerprint in [`database_backup.rs`](../../backend/src/database_backup.rs) expands the baseline record back to the former 0001–0084 checksums, so backups made before and after the consolidation stay compatible. Migrations that change backed-up tables (`0085`–`0088`) change the backup fingerprint. Derived-only migrations listed in `DERIVED_ONLY_MIGRATIONS` (currently `0089_decision_source_revisions`, which only adds derived revision tables) are skipped in the fingerprint so existing archives remain restorable.

## How matches and positions are stored

Imports store the original file, canonical SGF, resolved import warnings (`import_warnings`), and normalized positions together in PostgreSQL. A position records 26 signed checker counts including the bar, the player on roll, the dice, the game and move numbers, and the played move. Each library match also stores a `played_record_fingerprint` derived from its positions and kept current by database triggers for duplicate match detection. The game catalog also records games without checker moves, with their opening score and Crawford status. On first access, older imports build the catalog from stored SGF.

The **80-bit GNUbg Position ID** encodes checker counts as 14 Base64 characters. Exact board searches use its B-tree index without reparsing the original file. The separate **72-bit GNUbg Match ID** records match length, score, cube, dice and rules. A match, game and move-order index gives direct access to a late game without loading earlier games. Derived read models (`decision_index`, `decision_index_matches`, `native_analysis_revisions`, `imported_analysis_revisions`, `review_hits`, `review_hit_sets`) are excluded from backups and rebuilt on demand.

## Private demo data

The Git-ignored folders `games/bgf`, `games/mat` and `games/xg` are for private matches. To build a demo library with the current schema, create **Demo BGF**, **Demo MAT** and **Demo XG** in **Match database** and import each folder's files into its corresponding database. The importer reports duplicates and rejected files individually.

Do not use the legacy helper `tools/rebuild_demo.py` with the current schema. Its staging step expects the public REST database list to contain every PostgreSQL database ID, but that list omits the internal saved-position collection. Its activation SQL deletes matches outside the three demo databases and could therefore delete that collection's matches.

The helper was designed to stage three databases through REST, verify a PostgreSQL dump and record each file's outcome in a manifest outside Git. Its `--allow-rejected` option allows staged matches from valid files to proceed after you review the individual rejections. Network and server errors still abort staging. With the backend stopped, activation checks the PostgreSQL cluster and database identity, all three staging databases, match IDs, their database assignments and SHA-256 hashes under a transaction lock. A failed check leaves the active library intact. Each manifest can be activated once.

The local rebuild on November 8, 2026 imported 5 BGF, 5 MAT and 23 XG matches, with 72,959 checker and cube analysis records and 98 additional source-statistics records. The private report lists nine XG files rejected for invalid or inconsistent moves. The previous library was backed up and the rebuild preserved the flashcards and review history. Public sources and product research are listed in the [BackgammonDB comparison](../research/2026-11-08-backgammondb-product-ux.md).
