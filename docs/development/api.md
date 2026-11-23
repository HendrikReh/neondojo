# API reference

These endpoints cover common library and analysis operations. The complete contract, including parameter names and result schemas, is in [OpenAPI](../openapi.json) and served live at `GET /api/openapi.json`. Generate it and the TypeScript types used by the client directly from the Rust handlers. The generator runs without a server.

```sh
npm run api:types --prefix web
```

### Local access

The API has no authentication and is meant to be reached from the same machine only. Besides binding to loopback (`NEONDOJO_BIND`), every request passes one guard (`backend/src/local_origin.rs`) that answers HTTP 403 with the usual `{ "error": ... }` body when:

- `Host` (or the HTTP/2 authority) is not `localhost`, `127.0.0.1` or `[::1]`, with any port. This blocks DNS rebinding, so the API cannot be reached through another host name, including a LAN address.
- A `POST`, `PUT`, `PATCH` or `DELETE` carries an `Origin` other than `http://localhost`, `http://127.0.0.1` or `http://[::1]` (any port), `tauri://localhost` or `http(s)://tauri.localhost`. `Origin: null` is refused.
- Such a request has no `Origin` but `Sec-Fetch-Site: cross-site`.

Clients that send neither `Origin` nor `Sec-Fetch-Site`, such as `curl` or scripts, are not affected. The Vite dev server and the Playwright harness forward the browser's loopback `Host` and `Origin` unchanged.

### Library and position search

The library match (`/api/library/matches`), player, saved position, annotation, flashcard, duplicate-flashcard (`/api/cards/duplicates`), match review-decision (`/api/matches/{id}/review-decisions`), and Study decision search endpoints accept a one-based `page` and return the filtered `total` (`page` and `after` are mutually exclusive). Existing cursor clients can continue to use `after` on its own. `GET /api/library/duplicates` uses keyset pagination (`after` and `limit`) and returns `total_groups` and `total_matches`.

| Method       | Path                                                             | Purpose                                                                                                                                                                       |
| ------------ | ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| POST         | `/api/imports?database_id=`                                      | Multipart field `file` (`.mat`, `.txt` in MAT layout, `.sgf`, `.xg`, `.bgf`, `.ogxm`). HTTP 201 for a new match, 200 for a duplicate in the target database. Omitting `database_id` uses **Unsorted**. `warnings` lists the source irregularities the import resolved. |
| GET / POST   | `/api/databases`                                                 | List databases with match counts or create a database.                                                                                                                        |
| PUT / DELETE | `/api/databases/{id}`                                            | Edit with `version` or delete a user-created database. Deletion requires `version` and `delete_matches=true` when it contains matches.                                        |
| DELETE       | `/api/databases/{id}/matches?version=`                           | Empty a system database while retaining it. Returns HTTP 204 on success and 409 for a stale version, an ineligible database or active analysis.                               |
| GET          | `/api/library/matches`                                           | Filter and sort matches or occurrences before pagination. Limit 1–200. The cursor is bound to the filters and sort.                                                           |
| GET          | `/api/library/duplicates`                                        | Groups of matches with the same played record in `database_id` or all library databases (excluding **Paused Matches** and saved-position context), 1–100 groups per page with a keyset cursor (`after`) and up to 50 members per group. `kind` is `identical_files` or `same_moves`. |
| GET / PUT    | `/api/matches/{id}/metadata`                                     | Read or update match details with version checks.                                                                                                                             |
| POST         | `/api/library/actions`                                           | Bulk action with `action`, `match_ids` and optional source or destination database and tags. Returns individual outcomes.                                                     |
| POST         | `/api/library/export`                                            | Export `match_ids` with `format` set to `original` or `sgf`. Returns a file or ZIP.                                                                                           |
| GET          | `/api/matches?q=&after=&limit=`                                  | Match search. Limit 1–100.                                                                                                                                                    |
| GET          | `/api/matches/{id}`, `/api/matches/{id}/source`                  | Match summary or canonical SGF download.                                                                                                                                      |
| GET          | `/api/matches/{id}/analyses` | Paginated catalog, effective selection and explicit selection identity. |
| DELETE       | `/api/matches/{id}/analyses/{analysis_id}?source_revision=` | Remove one saved analysis. Returns 409 for a changed match source or active calculation. |
| PUT          | `/api/matches/{id}/analysis-selection` | Persist `analysis_id`; null restores the shared automatic policy. |
| GET          | `/api/positions/{id}/analysis-result` | Exact selected result for this occurrence; null for missing scope. |
| GET          | `/api/matches/{id}/analysis-statistics` | Selected native or imported statistics; missing depth/coverage remain unknown. |
| GET          | `/api/positions/{id}/jobs` | Discover persisted position calculation jobs without browser storage. |
| DELETE       | `/api/positions/{id}/jobs/{job_id}` | Delete one finished dice equity job of the position. Queued or running jobs return 409; checker analyses return 422 and are deleted through the analysis catalog. |
| GET          | `/api/matches/{id}/games`                                        | Game catalog with starting scores, Crawford status and move counts.                                                                                                           |
| GET          | `/api/matches/{id}/import-warnings`                              | Source irregularities the import resolved, saved with the match in game order. Library rows carry their `import_warning_count`.                                               |
| GET          | `/api/matches/{id}/dice?game_no=`                                | Observed roll frequencies, optionally for one game (loaded on demand when **Game rolls** is opened).                                                                          |
| GET          | `/api/matches/{id}/games/{game_no}/decisions`, `/api/moves/{id}` | Stable game and move IDs and recorded actions with their pre-action positions.                                                                                                |
| GET          | `/api/positions`, `/api/positions/{id}`                          | List match positions or find exact board matches. Filters include `database_id`, `match_id`, `game_no` and `position_id`. A direct lookup includes the match and game number. |
| GET          | `/api/position-browser?filters=&after=&limit=`                   | Search checker decisions in context. `filters` is URL-encoded JSON with `schema_version` set to 1.                                                                            |
| POST         | `/api/position-browser/analysis-readiness`                       | Count saved positions that still need local analysis and IDs that are no longer available. Returns the first position needing analysis in selection order, if any. Accepts 1–200 `position_ids`. |
| GET / POST   | `/api/position-filters`                                          | List or create saved searches.                                                                                                                                                |
| PUT / DELETE | `/api/position-filters/{id}`                                     | Update or delete a saved search with `expected_version`.                                                                                                                      |

```sh
curl -F file=@backend/tests/fixtures/tutorial.mat http://127.0.0.1:8787/api/imports
curl 'http://127.0.0.1:8787/api/matches?q=Tutor'
```

### Engines and analysis

| Method     | Path                                                                         | Purpose                                                                                                                                                                                                                    |
| ---------- | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET        | `/api/health`, `/api/engines`                                                | PostgreSQL and GNUbg health, or available engines, versions, fork identity, depths, NeonGammon `difficulty_presets` support, and `max_parallel_threads`. Health returns HTTP 200 with `status` `ok`, or 503 with `degraded` when the database or the GNUbg version check fails. |
| GET        | `/api/database-backup` | Export a compressed NeonDojo backup as a file. |
| POST       | `/api/database-backup/check` | Check a raw backup archive and report schema compatibility and archive contents. |
| GET        | `/api/database-backup/target-counts` | Read the current match, saved position, and flashcard counts in the destination. |
| POST       | `/api/database-backup?replace=true` | Restore a NeonDojo backup with media type `application/vnd.neondojo.backup`. |
| GET / PUT  | `/api/settings`                                                              | Persistent engine, playing (`play_top_two`, depths, thread overrides, parallel analysis jobs), and player settings with revision checks.                                                                                                                                      |
| POST       | `/api/engines/gnubg/check`                                                   | Check the installation using `{ "path": "..." }` (with optional `threads`).                                                                                                                                                                                                   |
| GET / PUT  | `/api/opening-book`                                                          | Provider, local BGDB and XG paths, availability and version checks.                                                                                                                                                        |
| POST       | `/api/opening-book/check`, `/api/opening-book/install`                       | Validate or atomically install a local BGDB file.                                                                                                                                                                          |
| POST       | `/api/opening-book/xg/check`, `/api/opening-book/xg/install`                 | Validate or install an official XG book file.                                                                                                                                                                              |
| GET        | `/api/positions/{id}/opening-book`                                           | Context-specific book result or explicit miss, with a separate XG position reference.                                                                                                                                      |
| GET        | `/api/matches/{id}/imported-analysis`                                        | Source analysis provenance and coverage.                                                                                                                                                                                   |
| GET        | `/api/positions/{id}/imported-analysis`, `/api/moves/{id}/imported-analysis` | Original checker or cube analysis.                                                                                                                                                                                         |
| POST       | `/api/jobs`                                                                  | Submit a position analysis or dice equity job with `{ "position_id": "<position UUID>", "kind": "analysis", "plies": 1 }` (`kind` is `analysis` or `dice`; optional `engine_id` and `plies` in NeonDojo's 1-ply numbering). |
| GET / POST | `/api/matches/{id}/analysis`                                                 | Get the latest job or submit a new one. Pass `engine_id` in the query or request body to override the saved engine. Use `game_no` to select an individual game; omit it for the whole match. `plies` uses NeonDojo's unified 1-ply numbering (1–5 for GNUbg, 1–4 for NeonGammon), for example `{ "plies": 1, "game_no": 2 }` for game 2. |
| GET        | `/api/matches/{id}/analysis/jobs?game_no=`                                   | Current analysis jobs for the selected game or whole match, newest first, including whole-match checkpoints for game scope. |
| GET        | `/api/jobs/{id}`, `/api/jobs/{id}/events`                                    | Job state, progress, result and error, or persistent SSE updates.                                                                                                                                                          |
| POST       | `/api/jobs/{id}/cancel`                                                      | Cancel a queued or running job. It ends as `failed` with the error `Cancelled by user`, its engine process is stopped, and games a match analysis completed before stay saved. HTTP 409 when the job has already finished. |
| POST       | `/api/jobs/status`                                                           | Read up to 100 job IDs in one request with `{ "ids": ["<job UUID>"] }`. Each distinct ID is represented in `jobs`, `missing`, or `restricted`. Restricted jobs are unavailable during competition play. |
| GET        | `/api/jobs/{id}/moves`                                                       | Cursor-paginated evaluations filtered by game, player, checker or cube, and severity.                                                                                                                                      |
| GET        | `/api/jobs/{id}/statistics`                                                  | Native match and game statistics with coverage, PR where available, and `error_breakdown`.                                                                                                                                 |
| GET        | `/api/matches/{id}/move-assessments?game_no=&job_id=`                        | Stored checker evaluations for a game with source and depth. Optionally restrict to one native job.                                                                                                                        |
| GET        | `/api/matches/{id}/review-decisions`                                         | Bounded review of pinned native checkpoints or validated imported evaluations, filtered by game, player, kind, and severity; supports `after` cursor and one-based `page` pagination. |

### Checker turns

- A play session in the `move` phase includes `legal_turns`. Each path contains `steps`, the corresponding `dice` in execution order and a `move_index` into `legal_moves` for that session revision. Submit the move index with the saved revision as usual. An empty list means a forced pass. A missing field in the move phase means paths could not be projected from the saved checkpoint.
- `POST /api/play/checker-turns` takes the 26-element `board` of signed checker counts including the bars, `player` and two `dice` for a training position. It returns the complete checker paths and their dice in execution order. It returns an empty list for a forced pass and HTTP 422 for invalid input. It uses the same step rules as practice answer validation and can run without a database or an engine subprocess.

### Notes and flashcards

| Method       | Path                                                                                                                         | Purpose                                                                                                                                                                                           |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET / PUT    | `/api/matches/{id}/annotation`, `/api/positions/{id}/annotation`, `/api/games/{id}/annotation`, `/api/moves/{id}/annotation` | Read or save versioned annotations.                                                                                                                                                               |
| GET          | `/api/annotations/search`, `/api/tags`                                                                                       | Search notes or suggest tags.                                                                                                                                                                     |
| GET / POST   | `/api/decks`                                                                                                                 | List or create decks.                                                                                                                                                                             |
| PUT / DELETE | `/api/decks/{id}`                                                                                                            | Rename using `name` and `expected_version` in the body, or delete with `expected_version` in the query. Deletion returns HTTP 204 on success, 409 for a stale version and 404 for a missing deck. |
| GET          | `/api/cards`, `/api/decks/{id}/cards`                                                                                        | Search all cards or a deck with the same filters, total count and UUID pagination.                                                                                                                |
| POST         | `/api/decks/{id}/cards`, `/api/decks/{id}/positions`, `/api/decks/{id}/decisions`                                            | Add one decision or errors from a completed analysis, add up to 200 selected position UUIDs, or add up to 200 analysed decision references (`job_id` or `imported_analysis_id`, `game_no`, `decision_no`), each with an individual outcome. |
| GET          | `/api/decks/{id}/next`, `/api/decks/{id}/export.apkg`                                                                        | Fetch the next due card, including a checked attempt to resume when available, or download an Anki package.                                                                                       |
| POST         | `/api/decks/{id}/send-to-anki`                                                                                               | Send a native deck to local AnkiConnect. Accepts a JSON body with optional `locale` and returns 204 on confirmed import. |
| POST         | `/api/decks/{id}/sync-anki-reviews`                                                                                          | Exchange missing reviews for matching cards in the active Anki profile. Send an empty JSON object. |
| GET / PUT    | `/api/decks/{id}/anki-auto-sync`                                                                                             | Read or change automatic review sync for a deck. Enabling links the open Anki collection. |
| POST         | `/api/decks/{id}/anki-auto-sync/action`                                                                                      | Run a linked deck's review sync once (`sync_now`), retry its open errors (`retry_open_errors`, with optional `include_result`), or `pause` or `resume` its background sync. |
| GET          | `/api/decks/{id}/anki-auto-sync/history`                                                                                     | List up to 100 recent review sync attempts for a linked deck. |
| GET          | `/api/study/anki-auto-sync`                                                                                                  | List linked decks with sync status and transfer totals. |
| GET          | `/api/study/learning-summary`                                                                                               | Count distinct decked cards and due cards, and get the next due date and recommended deck.                                                                                                      |
| POST         | `/api/cards/{id}/attempt`                                                                                                   | Check a complete checker turn or cube action against the card version. A stable `request_id` lets the client retry the same submission.                                                            |
| POST         | `/api/cards/{id}/reveal`, `/api/cards/{id}/review`                                                                           | Reveal a card's answer or submit a review.                                                                                                                                                        |

### Other endpoint groups

The remaining endpoints are listed in [OpenAPI](../openapi.json) under these tags. The linked references describe their behavior.

| Path prefix | OpenAPI tag | Behavior |
| --- | --- | --- |
| `/api/play/` | Playing | [Play API and persistence](../reference/playing.md#play-api-and-persistence) (`POST /api/play/sessions` accepts per-match `human_name`, `play_engine`, `play_plies`, `play_top_two`, and `play_difficulty`; sessions report `engine.top_two` and `engine.difficulty`) |
| `/api/study/`, `/api/cards/`, further `/api/decks/` paths, `/api/anki-media/` | Study, Learning | [Study](../reference/study.md#study), [Flashcards and Anki](../reference/study.md#flashcards-and-anki) |
| `/api/positions/{id}/similar`, `/api/positions/{id}/bookmark`, `/api/positions/{id}/practice-answer`, `/api/positions/{id}/practice-score` | Positions | [Saved positions and searches](../reference/library.md#saved-positions-and-searches), [Similar positions](../reference/library.md#similar-positions) |
| `/api/game-plans`, `/api/move-explanations`, `/api/cube-context`, `/api/cube-chances` | Positions, Match analysis | [Analysis](../reference/analysis.md#analysis-workspace) |
| `/api/matches/{id}/analysis/estimate`, `/api/jobs/{id}/remaining-time` | Match analysis | [Analysis time estimates](../reference/analysis.md#analysis-time-estimates) |
| `/api/players`, `/api/virtual-players` | Players | [Player database](../reference/library.md#player-database) |
| `/api/configuration`, `/api/profiles`, `/api/engines/bgsage` | Configuration, Profiles, Settings | [Settings, engines and profiles](../reference/settings.md#settings-and-engines) |
| `/api/benchmarks`, `/api/engine-matches`, `/api/version` | Benchmark, Engine matches, System | [Extras](../reference/extras.md#extras) |
