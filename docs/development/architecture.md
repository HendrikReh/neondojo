# Architecture and data

## Repository layout

| Path                                                                                                               | Responsibility                                                      |
| ------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| [`backend/src/api.rs`](../../backend/src/api.rs)                                                                         | REST, SSE, and OpenAPI                                              |
| [`backend/src/import/`](../../backend/src/import/)                                                                       | Import, rule validation, and GNUbg identifiers                      |
| [`backend/src/engine.rs`](../../backend/src/engine.rs), [`backend/src/match_analysis.rs`](../../backend/src/match_analysis.rs) | Isolated GNUbg processes and native checker and cube analysis       |
| [`backend/src/jobs.rs`](../../backend/src/jobs.rs), [`backend/src/match_jobs.rs`](../../backend/src/match_jobs.rs)             | Job state, progress events, game checkpoints, and player statistics |
| [`backend/src/engines/`](../../backend/src/engines/) | Engine providers, capability checks, and NeonGammon integration |
| [`backend/src/learning/`](../../backend/src/learning/), [`backend/src/recurring_mistakes.rs`](../../backend/src/recurring_mistakes.rs), [`backend/src/practice.rs`](../../backend/src/practice.rs) | Flashcards and FSRS, recurring mistakes, and shared answer evaluation |
| [`backend/src/store.rs`](../../backend/src/store.rs), [`backend/migrations/`](../../backend/migrations/)                       | SQLx persistence, schema, and search indexes                        |
| [`web/src/`](../../web/src/)                                                                                             | React client, SVG board, and generated API types                    |
| [`web/e2e/`](../../web/e2e/), [`web/playwright.setup.ts`](../../web/playwright.setup.ts) | Browser workflows and isolated PostgreSQL test setup |

The Rust backend runs independently of the browser. A future Tauri shell could start it as a sidecar and reuse the HTTP API.

## How matches and positions are stored

Imports store the original file, canonical SGF and normalized positions together in PostgreSQL. A position records 26 signed checker counts including the bar, the player on roll, the dice, the game and move numbers, and the played move. The game catalog also records games without checker moves, with their opening score and Crawford status. On first access, older imports build the catalog from stored SGF.

The **80-bit GNUbg Position ID** encodes checker counts as 14 Base64 characters. Exact board searches use its B-tree index without reparsing the original file. The separate **72-bit GNUbg Match ID** records match length, score, cube, dice and rules. A match, game and move-order index gives direct access to a late game without loading earlier games.

## Private demo data

The Git-ignored folders `games/bgf`, `games/mat` and `games/xg` are for private matches. To build a demo library with the current schema, create **Demo BGF**, **Demo MAT** and **Demo XG** in **Match database** and import each folder's files into its corresponding database. The importer reports duplicates and rejected files individually.

The legacy helper `tools/rebuild_demo.py` is incompatible with the current schema. Staging compares every PostgreSQL database ID with the public REST list, which omits the internal saved-position collection. The activation SQL can select that collection and its matches for deletion.

The helper was designed to stage three databases through REST, verify a PostgreSQL dump and record each file's outcome in a manifest outside Git. Its `--allow-rejected` option allows staged matches from valid files to proceed after you review the individual rejections. Network and server errors still abort staging. With the backend stopped, activation checks the PostgreSQL cluster and database identity, all three staging databases, match IDs, their database assignments and SHA-256 hashes under a transaction lock. A failed check leaves the active library intact. Each manifest can be activated once.

The local rebuild on November 8, 2026 imported 5 BGF, 5 MAT and 23 XG matches, with 72,959 checker and cube analysis records and 98 additional source-statistics records. The private report lists nine XG files rejected for invalid or inconsistent moves. The previous library was backed up and the rebuild preserved the flashcards and review history. Public sources and product research are listed in the [BackgammonDB comparison](../research/2026-11-08-backgammondb-product-ux.md).
