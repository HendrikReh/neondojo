# Changelog

All notable changes to NeonDojo are recorded in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/). Each release is tagged as `v<version>`, and the version matches `backend/Cargo.toml` and `web/package.json`. The backend applies pending database migrations at startup.

NeonDojo 1.0.0 is the first release under this name; BG Tutor 0.1.0 to 0.3.0 are its predecessors. From 1.0.0 on, a new major version marks every change that requires action from users: an incompatible change to the REST API, data or backup formats that are not migrated automatically, renamed or removed configuration such as environment variables, and dropped support for an engine version. New features that keep these intact raise the minor version, and fixes raise the patch version.

## [Unreleased]

### Added

- **Decision search fills itself and builds decks.** When **My player profile** pre-selects the player, **All analyzed decisions** lists that player's decisions on the first visit without a click. **Add to deck** in the preview now covers cube decisions and imported analyses as well as checker plays, and **Add page to deck** adds every decision of the current page in one step, reporting how many were added, updated, already in the deck or not available. `POST /api/decks/{id}/decisions` accepts up to 200 decision references with an outcome for each. [Saved positions and searches](docs/reference/library.md#saved-positions-and-searches)

### Changed

- **One way to define translations.** All 41 message catalogues of the interface use the same helper and the same formatting, and a test checks every catalogue in all six languages for missing keys, lost placeholders and missing plural forms. Texts that passed numbers without localizing them now write them the way the language does. An unknown text key from the server shows the key instead of breaking the view. The visible texts are otherwise unchanged in all six languages. [Settings](docs/reference/settings.md) · [Architecture](docs/development/architecture.md#web-client-layout)
- **NeonGammon 3.1.1.** Supports and locks the official [NeonGammon 3.1.1](https://github.com/HendrikReh/neongammon/releases/tag/v3.1.1) ARM64 wheel (`abaedf9`, runtime fingerprint `eaa6f2a8…`), bringing faster move generation, contact encoding, N-ply search caches/work-stealing, and cube/cubeless batch reuse while keeping `3.1.0` and `3.0.0` supported for existing installations and snapshots. [Local NeonGammon setup](docs/open-sage-local.md)
- **Faster start.** Playing, Study, Positions, the Player database, Extras and the Settings dialog now load on their first visit and then stay open in the background, so returning to them keeps their state. The code loaded at start shrinks from 1,510 KB to 1,095 KB (475 to 352 KB compressed). On a Mac mini M4 Pro, the header appears after about 129 ms instead of 153 ms, with 51 instead of 72 ms of script time. If NeonDojo was updated while a page was open and a part can no longer be loaded, that part asks you to reload the page. All stylesheets now load in one fixed order, so the layout no longer depends on which workspace you open first. [Architecture](docs/development/architecture.md#web-client-layout)
- The five-point NeonGammon preset labels now show approximately 20.3, 15.8, 9.3, and 1.0 PR. All four presets were scored on the same 6,912 decisions against a shared rollout reference. [Five-point preset PR benchmark](docs/open-sage-local.md#five-point-preset-pr-benchmark)
- **Theme preparation no longer edits the tracked theme list.** With `NEONDOJO_ALLOW_MISSING_THEMES=1`, `npm run themes:prepare` only writes the untracked catalog and artwork and leaves `shared/configuration-options.json` unchanged, so the backend keeps accepting saved BGBlitz themes. Adding or removing a theme now requires `npm run themes:prepare --prefix web -- --update-options`; a new archive in `themes/` without that flag stops preparation and names it. [Board artwork](docs/reference/settings.md#board-artwork)

- **Match database columns and toolbar.** PR, play mode and date columns are wide enough for their values in all six languages; hovering a row no longer covers the rows below it, and only clipped cells show their full text. The toolbar groups single-match, batch and destructive actions, delete buttons are tinted, disabled icons stay readable on light palettes, and the import destination is labelled **Into:**. [Library](docs/reference/library.md)
- **Fewer background requests.** The Match database and the analysis catalog follow running jobs over the job event stream instead of reloading every few seconds, and the benchmark status is only polled while a run is active or after the window regains focus.
- **Analysis review controls.** **Previous/Next error** (`⇧E`/`E`) are available directly in **Checker**, opening **Cube** shows the next matching cube decision, and the Analysis workspace fits the window without a page scrollbar. [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Positions and decision search.** The position scopes stay visible beside a similar-position search, an idle decision search names **Search decisions**, and a checker hit can be added to a deck from its preview. [Library](docs/reference/library.md)
- **Profile notices in the header.** Profile warnings collapse into one badge beside the profile selector, and switching away from a modified profile offers **Save and switch** first. [Settings](docs/reference/settings.md)
- **Faster NeonGammon and GNUbg checks.** Between full integrity checks, a NeonGammon request compares file metadata instead of rehashing the installed package, bot and duel cube decisions skip the unused probability evaluation, and a pinned GNUbg version is checked by hashing the executable instead of starting `gnubg --version` twice. [Local NeonGammon setup](docs/open-sage-local.md)
- **New NeonGammon benchmark protocols.** Because these checks make the measured pipeline faster, new NeonGammon benchmark runs record `bgsage-match-v4-{depth}ply-4threads` and `bgsage-checker-turn-v4-{depth}ply-2threads-gnubg-rules`. v3 results stay in the history but are not pooled with v4 timings. NeonGammon analysis time estimates use only v4 runs, so they are unavailable until a new analysis benchmark completes on this machine. A NeonGammon benchmark still queued under v3 is refused when it starts. [Engine benchmark](docs/reference/extras.md#engine-benchmark)
- **New GNUbg benchmark protocols.** Every timed GNUbg play turn and every timed analysis game checks that GNUbg is the pinned executable, which now hashes it instead of starting it twice. New GNUbg runs therefore record `gnubg-checker-turn-v2` for play turns and `gnubg-match-v4-unlimited`, `gnubg-match-v4-2ply-top14` (3-ply) and `gnubg-match-v4-4ply-top14` (5-ply) for analysis. Earlier results, including `gnubg-match-v1`, stay in the history but are not pooled with the new ones and no longer calibrate GNUbg analysis time estimates, so those estimates are unavailable until a new GNUbg analysis benchmark completes on this machine. A GNUbg benchmark still queued under an earlier protocol is refused when it starts. [Engine benchmark](docs/reference/extras.md#engine-benchmark)
- **Database pool.** The backend opens up to 32 PostgreSQL connections by default (`NEONDOJO_DB_MAX_CONNECTIONS`), and a transient database error while sampling progress or renewing a lease no longer fails a running analysis. [README](README.md)
- **Responsive backend during analysis and imports.** Reading the played match for a GNUbg or NeonGammon game analysis, checking GNUbg's results, archiving a bot match, scoping an analysis job for a match without a game list, locating the GNUbg executable and reading a rendered Anki package no longer occupy the threads that serve requests, so job progress, event streams and other requests keep responding meanwhile. A GNUbg game analysis also parses the played match once instead of three times.
- **Faster saved-position browsing on large libraries.** **Positions** sorts and counts saved positions before it reads their analysis, which it then reads only for the rows of the page, and the database filter uses an index. On a synthetic library of 10,000 matches with 22,500 saved positions, a page in match order takes 44 ms instead of 685 ms in PostgreSQL, a database-scoped page 6 ms instead of 119 ms, and a page sorted by loss 860 ms instead of 1,511 ms. Checking saved positions for flashcards reads the analysis of their own matches only. Migration `0092_matches_effective_database` adds the index; it changes no backed-up table, so backups made before the update stay restorable. [Verification](docs/development/testing.md#postgresql-and-gnubg-integration-tests)
- **Similar-position practice uses the practice engine.** Scoring a similar-position answer uses the configured analysis engine and the practice depth limit, like Study, and never falls back to GNUbg. It answers HTTP 429 while another practice check runs and 503 when the engine is unavailable.
- **Faster decision search and recurring mistakes on large libraries.** The check that keeps the decision index current no longer reads the position analyses of matches that have a match or imported analysis, and finds the player's matches and older uncataloged imports through indexes. *All analyzed decisions* chooses each page from index entries, joins matches only for a database filter, and reads the page and its total in one snapshot, so they always agree. Equal results are ordered by their position in the match, and a page must start within the first 1,000,000 decisions (HTTP 422 otherwise). With 10,000 matches and 399,000 indexed decisions, a warm search for a player with 5,000 matches took about 95 ms instead of 315 ms of PostgreSQL planning and execution. Migration `0093_decision_search_indexes` adds the indexes and redefines `effective_analysis_selections` with the same results; it does not change the backup schema fingerprint, so earlier backups stay restorable. [Library](docs/reference/library.md) · [Verification](docs/development/testing.md#verification)
- **Smaller job history and faster duplicate search.** When an analysis job finishes, its progress events are deleted in the same transaction; its queued, running and final events stay, so a reconnecting event stream still ends with the result. Migration `0094_duplicate_finder_and_job_history` deletes the progress events of jobs that already finished, adds an index the duplicate match finder reads instead of the whole `matches` table, and replaces the job claim index with one that holds only queued and running jobs. Archiving a bot match writes its new positions in batches of 500 instead of one statement per position. On 10,000 synthetic matches the first page of duplicate groups took 8 ms instead of 13 ms in PostgreSQL. The migration does not change the backup schema fingerprint, so earlier backups stay restorable; they keep their old progress events. [Library](docs/reference/library.md#duplicate-match-finder) · [Verification](docs/development/testing.md#postgresql-and-gnubg-integration-tests)

### Security

- **Local API guard.** The API rejects requests whose `Host` is not a loopback name (DNS rebinding) and state-changing requests from a foreign `Origin`, `Origin: null`, or `Sec-Fetch-Site: cross-site` with HTTP 403. `POST /api/benchmarks/play` requires a JSON content type. [Local access](docs/development/api.md#local-access)
- **Untrusted files.** Anki package size limits count decompressed bytes instead of trusting ZIP headers, SQLite opening books are opened with `TRUSTED_SCHEMA` off and `DEFENSIVE` on and must use regular tables, and MAT error recovery stops after 60 seconds.

### Fixed

- **Cards from imported analyses.** Adding the decisions of a match whose displayed analysis is imported (XG, BGBlitz or SGF) to a deck failed with an internal server error, because the query filtered the player on a column the imported-analysis view does not have. It now filters on the player of each recorded move.
- **Concurrent profile, topic and transfer edits.** Choosing **My player profile** while the same virtual player profile was renamed, deleted or had members changed could fail with an internal error (a database deadlock). Saving a card topic, starting a transfer round and choosing My player profile no longer lock their whole table, so deleting topics and creating or editing other virtual player profiles no longer wait for them, and autovacuum is not held off. Concurrent saves still respect the 200-topic limit, and concurrent transfer starts still never pick the same unseen positions.
- **Faster database restore without a job event storm.** A restore no longer recomputes the played-record fingerprints and analysis revisions row by row while it copies the backup, and it no longer announces every historical job event, which woke open job event streams and overflowed the event channel when the restore committed. It keeps the fingerprints from the backup, writes the analysis revisions once after the copy, and only announces the jobs it interrupts. Foreign keys and other constraints stay enforced. [Extras](docs/reference/extras.md)
- **Database restore while jobs start.** A restore now stops analysis jobs that are claimed while it drains, and refuses at once with HTTP 409 naming the running benchmark, engine duel, AI assessment or Anki sync instead of blocking every API request for 30 seconds. [Extras](docs/reference/extras.md)
- **Analysis time estimates in every language.** The estimated analysis time and the remaining time of a running match analysis appeared in English in Spanish, French, Italian and Polish, and durations always used English unit letters. They are now translated, with localized units such as `1 min 20 s` and Polish plural forms; the Polish count of opening-book score contexts uses its plural forms as well.
- **NeonGammon 4-ply analysis estimates.** The estimate started a NeonGammon worker to confirm the engine version and gave up after 2 seconds; a 4-ply worker needs about 2 seconds to start, so 4-ply estimates always reported the engine as unavailable. Estimates at any depth also shut down the cached worker when its profile differed. The estimate now takes the version from the verified installation, checks only that its interpreter exists, and starts no engine process; the analysis verifies the installation when it starts. [Analysis time estimates](docs/reference/analysis.md#analysis-time-estimates)
- **Readable Play, Benchmark and history text in all six palettes.** Play match details on light palettes, blunder feedback, the saved Tutor threshold and failed benchmarks on dark palettes fell below the WCAG AA contrast of 4.5:1; they now use the palette's muted, danger and success text colours. Several Play, Match database and game result styles referred to colour variables that did not exist, so their colour or border was dropped: game result boxes in the history now have their border, and feedback tables their row lines. Action tooltips follow the selected palette, and board point numbers keep their light colour on the dark frame.

## [1.2.0] - 2026-11-21

NeonDojo 1.2.0 adds calibrated NeonGammon playing strengths, finds duplicate matches, exports matches as XG files, imports whole folders and plain-text MAT transcripts, and keeps faulty MAT transcripts with import notes instead of rejecting them. No change requires action from users: NeonGammon 3.0.0 installations keep working, and the difficulty presets need NeonGammon 3.1.0. Migrations `0086_played_record_fingerprint`, `0087_match_import_warnings`, `0088_play_top_two` and `0089_decision_source_revisions` run at startup; as with every schema migration, a backup made with 1.1.0 can only be restored with 1.1.0.

### Added

- **Duplicate match finder.** **Duplicates** in the Match database groups matches with the same played record in the selected database or in all databases, including the same match imported from different formats or with the sides swapped. Groups of byte-identical files are marked. **Select copies** selects all but the oldest match of each group for the existing bulk actions. New endpoint `GET /api/library/duplicates`. Migration `0086_played_record_fingerprint` stores the played record of every library match, keeps it current on every write to its positions and fills it for existing matches at startup. Like every schema migration, it changes the backup schema: a backup made before the update can only be restored with the earlier version. [Duplicate match finder](docs/reference/library.md#duplicate-match-finder)
- **Import notes for faulty MAT transcripts.** A MAT or `.txt` transcript with an illegal or unreadable move is no longer rejected as a whole. The game ends before that move, later games are imported from their own headers, and the import result lists each case with game, move number, line and move text. Header scores that contradict the previous result are kept as recorded and listed as well. Migration `0087_match_import_warnings` saves the notes with the match and its copies. **Notes** shows them as **Import notes** above the match or game comment, the analysis header shows their number, and the match database marks the PR of these matches. `GET /api/matches/{id}/import-warnings` and `import_warning_count` on library rows expose them. Matches imported earlier have no saved notes. [Import formats](docs/reference/library.md#import-formats)
- **NeonGammon difficulty presets.** With NeonGammon as the bot engine, **Playing strength** in **New match** offers **Beginner**, **Casual**, **Intermediate** and **Full Strength**. The first three sample checker alternatives and close cube decisions. Their five-point labels show about 22, 16.2 and 8.1 PR, measured against Full Strength at 3-ply for checker moves and 2-ply for cube decisions. Saved matches use their pinned depth. `POST /api/play/sessions` accepts `play_difficulty`, and sessions report it as `engine.difficulty`. The presets use the policy exported by the verified NeonGammon 3.1.0 wheel. New Full Strength matches keep 3-ply checker search and use the wheel’s 2-ply cube decisions. [NeonGammon difficulty presets](docs/reference/playing.md#neongammon-difficulty-presets)
- **NeonGammon 2-ply + Top-2.** **2-ply + Top-2 (10 mEq)** is a NeonGammon playing strength in **New match** for one match and in **Settings → Playing** as the default; the active configuration profile saves it. In contact positions where the two leading checker plays are at most 0.010 equity apart at 2-ply, NeonGammon re-evaluates both at 3-ply and plays the better one. Other checker decisions and all cube decisions stay at 2-ply, and paused matches keep their strength. Choosing NeonGammon for a single match now starts at 2-ply. `POST /api/play/sessions` and `PUT /api/settings` accept `play_top_two`, which requires NeonGammon at 2-ply and cannot be combined with a difficulty preset; sessions report it as `engine.top_two`. Migration `0088_play_top_two` adds the setting with `false` for existing installations; like every schema migration, it changes the backup schema. [Playing against an engine](docs/reference/playing.md#playing-against-an-engine)
- **XG export.** **Export** in the Match database offers **XG** beside the original file and SGF and writes an eXtreme Gammon `.xg` file (XG2) for programs that read that format; several matches download as a ZIP. It contains the played line with names, match length or money session, Crawford and Jacoby rules, scores, checker plays, cube actions, resignations and results, and the event, round, venue and date from **Edit match details**. Analyses and notes are not included, and every decision is marked as not analyzed. `POST /api/library/export` accepts `"format": "xg"` and returns 422 with the file name for a match that does not replay. [Bulk actions and exports](docs/reference/library.md#bulk-actions-and-exports)
- **Folder import.** **Import folder** beside **Import match** imports every `.mat`, `.txt`, `.sgf`, `.xg`, `.bgf` and `.ogxm` file of a chosen folder and its subfolders into the selected destination, in natural path order (`Round 2` before `Round 10`). Hidden files such as macOS `._*` files and unsupported files are skipped and counted. A results panel shows progress, imported, duplicate and failed counts and one row per file; **Stop import** finishes the current file and skips the rest. Dropping a folder on the Analysis welcome panel does the same. [Match databases](docs/reference/library.md#match-databases)
- **Plain-text MAT transcripts.** `.txt` files are imported as MAT transcripts, the format tournament archives publish, including those with a boxed title instead of `;` header lines. They are stored as MAT under their original filename. Other text files, such as GNUbg board exports, return 422 with "TXT import failed; expected a MAT match transcript". [Import formats](docs/reference/library.md#import-formats)

### Changed

- **NeonGammon 3.1.0.** NeonDojo accepts the NeonGammon 3.1.0 wheel beside 3.0.0; the difficulty presets use its `DifficultyPolicy`. Register a 3.1.0 environment to offer them. [Local NeonGammon setup](docs/open-sage-local.md)
- **Game rolls load on demand.** **Game rolls** and **Match rolls** load the recorded roll frequencies when the tab opens instead of with every move, so stepping through a game makes fewer requests. [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Faster dice equity and analysis checkpoints.** A GNUbg dice equity job evaluates all 21 rolls in one engine run instead of starting GNUbg per roll: in five fresh jobs per depth with one GNUbg thread, the time to completion fell from 5.13 to 0.58 s at 1-ply and from 6.67 to 1.83 s at 3-ply. Match analyses write their checkpoints in batches, and the review list keeps fast plans when switching between numbered and cursor pages.
- **NeonGammon threads from one budget.** The analysis and playing workers reserve their configured threads for each evaluation from one shared budget. When an analysis and a move together exceed it, a waiting move runs between analysis evaluations instead of competing for the CPU. A saved profile from a larger machine keeps its thread count and runs on its own. [Local NeonGammon setup](docs/open-sage-local.md)

### Fixed

- **Builds without local theme archives stop.** `npm run build`, `npm run dev` and `npm test` in `web/` no longer drop BGBlitz themes from `shared/configuration-options.json` when their ZIPs are missing from `themes/`, for example in a fresh worktree. Such a build served no theme images and showed "The theme image could not be loaded" or "Invalid configuration: appearance". Theme preparation now names the missing archives and stops; `NEONDOJO_ALLOW_MISSING_THEMES=1` removes the themes on purpose. [Board themes](docs/reference/settings.md#board-artwork)
- **White plays the light checkers on BGBlitz boards.** BGBlitz names its checker and dice sets after the players "red" and "green", and in most themes "red" is the dark set. Theme preparation assigned "red" to White, so with BG Moon and eleven other archives choosing **White** in **New match** gave you the dark checkers, and the board preview in **Settings → Board** labelled them the other way round. `npm run themes:prepare` now assigns the set whose opaque checker pixels are lighter to White, with its dice. Run it again (or restart `npm run dev`) to rebuild the local catalog. [Board theme formats](themes/README.md)
- **Decision search follows analysis changes.** The decision index behind *My recurring mistakes* and *All analyzed decisions* could miss a change to a displayed analysis whose decision count and row sums stayed the same. Every native job and imported analysis now has a revision that changes in the same transaction as its decisions or checkpoints; migration `0089_decision_source_revisions` adds it. [Study](docs/reference/study.md#study)
- **Analysis results refresh with their source.** Changing the displayed analysis, replacing its source or completing another checkpoint now refreshes move colors and match statistics in Analysis. [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Rolls on which both players dance.** When both sides could only enter or move with one number and both danced, the board returned to the same GNUbg position and Playing reported HTTP 503 with "GNUbg rejected action without changing state". Play sessions and engine duels now also count the recorded actions as progress. A failed engine duel series is now written to the backend log with its ID and error. [Playing against an engine](docs/reference/playing.md#playing-against-an-engine)
- **NeonGammon turns waiting for a benchmark.** The three-minute deadline of a NeonGammon play action now includes the wait behind an exclusive benchmark, so a turn no longer waits without limit. [Playing against an engine](docs/reference/playing.md#playing-against-an-engine)
- **MAT imports without a date stay undated.** A MAT transcript without `EventDate` received the import date as its played date. Only a valid `EventDate` from the file is kept now. [Import formats](docs/reference/library.md#import-formats)
- **GNUbg diagnostics appear once.** GNUbg repeats some messages once per parse attempt, so import errors listed "not a valid .mat file" up to six times. Each distinct diagnostic line is reported once, in order.
- **Configuration values in the OpenAPI contract.** `docs/openapi.json` and the generated TypeScript types listed the Rust variant names of the ten configuration enums, for example `"Dark"` and `"OpenSage"`, instead of the values the API sends and accepts, `"dark"` and `"bgsage"`. The contract now publishes the wire values; requests and responses are unchanged. [API reference](docs/development/api.md#api-reference)

## [1.1.0] - 2026-11-20

NeonDojo 1.1.0 adds virtual player profiles, catalogues bot matches as real NeonDojo players and opens a new match in a setup dialog with per-match bot choices. One schema baseline replaces the migrations 0001–0084. Databases from 1.0.0 and later development builds are updated automatically at startup without changing their data, and backups stay restorable. Migration `0085_bot_match_players` then runs.

### Added

- **Virtual player profiles.** A virtual profile groups real player profiles without changing them, for example the same person's Galaxy and OpenGammon accounts. Unlike **Merge …** it is reversible: members keep their aliases and their own rows in the player list, which name their group, and can be removed again. Each real profile belongs to at most one virtual profile. The virtual profile shows the consolidated PR, PR chart, matches and opponents of its members. [Virtual player profiles](docs/reference/library.md#virtual-player-profiles)
  - **Settings → General → My player profile** chooses the virtual profile that stands for you. Study, *My recurring mistakes*, *All analyzed decisions* and the player database start with it, and your NeonDojo bot matches count towards it; their PR contributes only from unassisted Competition play.
  - In *My recurring mistakes* and *All analyzed decisions* a virtual profile is an additional player choice; members stay selectable on their own.
  - Merging a member into a profile without a group passes the membership on.
  - New endpoints under `/api/virtual-players`, `virtual_player_id` on `GET /api/library/matches`, and `virtual_profile` on player profiles. `GET /api/study/mistakes` now returns `resolved_player` for **My player profile** even without `resolve`.
  - The schema baseline contains their tables and the view `virtual_player_sides`; a 1.0.0 database receives them at startup. Backups include the virtual profiles and their members.
- **Bot matches belong to real players.** The human side of every NeonDojo bot match is catalogued as a player under the platform `neondojo`; the bot side stays uncatalogued. Existing NeonDojo players join **My player profile**, so earlier results stay the same. The view `player_match_sides` is the one definition of which match sides can enter a PR: assisted and running bot matches stay out. Migration `0085_bot_match_players` stores the human side on each bot match. [Player database](docs/reference/library.md#player-database)
- **Bot choices per match.** The **New match** setup offers the player name, bot engine and playing strength for that match only, and the saved settings stay unchanged. `POST /api/play/sessions` accepts optional `human_name`, `play_engine` and `play_plies`. [Playing against an engine](docs/reference/playing.md#playing-against-an-engine)
- **Engine roles in Playing.** The setup and match bar name the **Bot engine**, **Analysis engine** and **Rules engine**, and the bot tooltip shows its version, depth and bear-off direction. [Playing against an engine](docs/reference/playing.md#playing-against-an-engine)
- **Legal checker paths from the backend.** A play session in the move phase includes `legal_turns`, the complete checker paths of the roll with their dice in execution order. `POST /api/play/checker-turns` returns the paths for any training position, using the same step rules as practice answer validation. [API reference](docs/development/api.md#checker-turns)
- **NeonDojo version with commit.** **Extras → Versions** shows the running version with its build commit, for example `1.1.0 (b9bff70)`, and copies both. `GET /api/version` returns `release` and `commit`; `version` is deprecated and repeats the commit. [Version information](docs/reference/extras.md#version-information)
- **NeonDojo logo.** A torii whose pillars are backgammon points, in front of a checker as the rising sun, replaces the "bg" mark in the header, as the favicon and in the README.
- **Sort benchmark history.** In **Extras → Benchmark → History**, the date, engine, ply, total analysis time, average time and status headings sort the list; a second click reverses it. Runs without a measured time stay last, and status sorts by lifecycle. [Engine benchmark](docs/reference/extras.md#engine-benchmark)
- **Delete benchmark runs.** In **Extras → Benchmark → History**, finished runs (completed, failed or cancelled) can be deleted after an inline confirmation. Queued and running runs must be cancelled first. A deleted completed run no longer calibrates analysis time estimates. New endpoint `DELETE /api/benchmarks/{id}` (204; 404 if missing; 409 while queued or running). [Engine benchmark](docs/reference/extras.md#engine-benchmark)

### Changed

- **One database schema baseline.** `backend/migrations/0001_schema.sql` replaces the migrations 0001–0084 and creates the same schema and seed rows. At startup NeonDojo adopts a database migrated with the former files without changing its data. A database from 1.0.0 (migrations 0001–0083) first receives the former `0084_virtual_player_profiles`. Other histories are rejected with a message instead of being migrated. Existing backups stay restorable. [Database schema](docs/development/architecture.md#database-schema)
- **New match setup as a dialog.** The setup opens over the playing view, so the current match or an empty starting board stays visible. It covers only the playing view; the app header stays usable, and Close or Escape dismisses it. [Playing against an engine](docs/reference/playing.md#playing-against-an-engine)
- **XG transcripts with recorded irregularities import.** Plays that XG itself marks as illegal become an SGF setup instead of a checker decision, a game may start from its first recorded position, and an unfinished game followed by the next game is closed without a result. The import lists each resolution as a warning. The nine previously rejected local XG files now import. [Import formats](docs/reference/library.md#import-formats)
- **Players with the same name can be told apart.** When several profiles in **Player database** share a display name, for example the same username on Galaxy and OpenGammon, each row shows its platforms under the name. `GET /api/players` returns them in `name_platforms`. [Player database](docs/reference/library.md#player-database)

### Fixed

- **NeonGammon 4-ply analysis benchmarks run.** They failed before starting with "Unsupported benchmark protocol or reference version". The runner and the time estimate now share one protocol rule.
- **A deleted last match no longer breaks the start.** When the stored Analysis selection no longer exists, the app forgets it and opens the empty workspace instead of reporting HTTP 404.
- **A missing stored play session opens the setup.** If the last session no longer exists, Playing opens **New match** instead of an error. An explicitly selected missing session still reports it.
- **PR coverage of "My player profile".** Bot matches played with learning aids no longer count as unrated: the coverage beside the PR counts only matches that can enter it, and **Analyze unrated matches …** no longer stays visible for them. Virtual profiles report these matches as `rateable_match_count`. [Virtual player profiles](docs/reference/library.md#virtual-player-profiles)

## [1.0.0] - 2026-11-20

The first release as NeonDojo (formerly BG Tutor 0.3.0), with NeonGammon 3.0.0 replacing Open Sage. This release breaks compatibility with BG Tutor: the name, environment variables, database, backups, browser preferences and Anki note type change, and only the `neongammon` 3.0.0 package is accepted. Start with a new database, register the NeonGammon 3.0.0 environment again and reimport your matches. Migrations `0082_review_hits` and `0083_neongammon_label` run at startup. The release also speeds up numbered error lists, NeonGammon moves during analyses and job progress.

### Changed

- **The project is now NeonDojo** (it was BG Tutor). The repository moves to [HendrikReh/neondojo](https://github.com/HendrikReh/neondojo). This is a clean break with no compatibility with BG Tutor installations. Reimport your matches into the new database.
  - The Cargo package and binary are `neondojo`, and the web package is `neondojo-web`.
  - `NEONDOJO_*` environment variables replace `BGTUTOR_*`, and the old names are no longer read.
  - The Compose project, PostgreSQL role and database are `neondojo`, with the container `neondojo-db-1` and the volume `neondojo_postgres_data`. A new installation starts with an empty database, and the old `bgtutor_postgres_data` volume is left untouched.
  - The default opening book path is `~/Library/Application Support/NeonDojo/` on macOS and `$XDG_DATA_HOME/neondojo/` elsewhere.
  - Browser preferences use `neondojo.*` storage keys, so the theme, language and column layouts start from their defaults.
  - Backups are saved as `.neondojo-backup` files with the media type `application/vnd.neondojo.backup`.
  - Anki exports use the note type `NeonDojo Decision v1`. Review synchronization does not find notes exported by BG Tutor.
- **NeonGammon 3.0.0 replaces Open Sage.** The engine is the [NeonGammon](https://github.com/HendrikReh/neongammon) fork of Open Sage, and NeonDojo now calls it NeonGammon in the interface, all six languages, engine versions and documentation. Its internal ID stays `bgsage`, so settings, jobs and the API keep their values. [Local NeonGammon setup](docs/open-sage-local.md#neongammon-300--november-20-2026)
  - NeonDojo accepts only the package `neongammon` 3.0.0. Installations of the earlier `bgsage` packages 2.0.20260907 and 2.0.20261119 are no longer accepted, and the worker and the probe import only `neongammon`.
  - **Extras → Versions** recognizes release v3.0.0 by its model fingerprint and links its tag commit in `HendrikReh/neongammon`, labeled **NeonGammon source**.
  - NeonGammon 3.0.0 evaluates cubeful 2-ply and deeper checker play in a native routine and passes the thread budget to it. The installation check now observes that budget as well as the Python candidate pools, so 2-ply profiles are verified again; 1-ply stays serial.
  - Migration `0083_neongammon_label` shows NeonGammon as the engine of native analyses without a recorded engine name.
  - `tools/open-sage/requirements-macos-arm64.lock` installs the hash-pinned wheel from the GitHub release, because NeonGammon is not published on PyPI.
- **Faster numbered error lists.** A numbered page of the error list in Analysis no longer projects every decision of the analysis to count them and pick its 50. Migration `0082_review_hits` adds a derived list of each analysis's validated decisions, built once from the saved analysis and rebuilt when the analysis, its checkpoints or the source change. Pages count and select there with the current error thresholds and project only their own decisions. On WC2014 a page takes about 11 ms instead of 48 ms. Cursor pages are unchanged. [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **NeonGammon moves no longer wait for analyses.** Playing and analysis use separate NeonGammon worker processes with their own locks. During a 3-ply game analysis a NeonGammon bot move took 3.9 s at the median and now takes about 33 ms, as without a running analysis. While both run, the playing and analysis thread counts share the CPU (4 + 2 by default). A move's deadline now includes waiting for its worker. Analyses stay serial, and benchmarks still pause all engine work. [Settings](docs/reference/settings.md#settings-and-engines)
- **Fewer requests while jobs run.** Every view that follows a job shares one watch of it. A visible job reports progress over one event stream without reading the full job for each event; the full job is read when the watch starts, when the job ends, after a lost connection and every 15 seconds as a safeguard. Jobs that only the global progress display follows are still read every 3 seconds, so bulk analyses do not exhaust the browser's connections. A 3-ply dice equity job with 21 progress events took 25 full reads and now takes 3. [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Faster game checks.** Requests scoped to one game, such as a game's positions or its match analysis, check the game in the game catalog instead of parsing the match's SGF. On WC2014 these requests take about 1 ms instead of 5 ms.
- **Saved positions load their notes in one query.** A page of saved positions reads the match, game, position and move notes of all its positions together instead of once per position.

## [0.3.0] - 2026-11-19

Supports the Open Sage fork release 2.0.20261119 and speeds up the decision index, the analysis catalog, GNUbg dice equity and moving between moves in Analysis. Migrations `0080_decision_index_row_fingerprint` and `0081_catalog_native_lookup` run at startup. Open Sage results change with the new fork release; existing analyses keep their values.

### Changed

- **Open Sage fork release 2.0.20261119.** BG Tutor accepts the fork's release package `2.0.20261119` as well as the earlier `2.0.20260907` builds, so saved jobs and duels stay valid. **Extras → Versions** recognizes the release by its model fingerprint. The release changes Open Sage's results: cube decisions use a new money cube life, and cubeful 2-ply checker play searches only the candidates its move filter keeps. New Open Sage 2-ply analyses therefore list only fully searched candidates plus the played move, for example 5 instead of 16 after the opening 31. Play hints still rate every legal move at full depth. [#26](https://github.com/HendrikReh/neondojo/pull/26) · [Local Open Sage setup](docs/open-sage-local.md#fork-release-2020261119--november-19-2026)
- **Faster freshness check for the decision index.** **My recurring mistakes** and **All analyzed decisions** no longer hash the analysis data of every one of the player's native decisions on each request. The check now counts the saved decisions and sums their row IDs. Migration `0080_decision_index_row_fingerprint` adds a trigger that drops a match from the index when one of its saved decisions changes in place. It also carries up-to-date index entries over to the new fingerprint, so the upgrade does not re-index the library. [#21](https://github.com/HendrikReh/neondojo/pull/21) · [Recurring mistakes](docs/reference/study.md#my-recurring-mistakes)
- **Faster lookup of the displayed analysis.** Finding the displayed analysis of a match no longer counts the saved decisions of each of its GNUbg and Open Sage analyses. Migration `0081_catalog_native_lookup` checks that a saved decision exists instead and counts the decisions only when the analysis catalog shows them. The decision index uses the same lookup, so its freshness check also gets faster. The choice of the displayed analysis is unchanged. [#22](https://github.com/HendrikReh/neondojo/pull/22) · [Displayed analysis](docs/reference/analysis.md#imported-analysis-and-opening-books)
- **Faster dice equity with GNUbg.** A GNUbg dice equity job evaluates all 21 rolls in one GNUbg run instead of one run per roll, and it checks the engine version once before and once after that run instead of around each evaluation. GNUbg starts per job drop from 64 to 4, including the check when the job is submitted. The opening position takes about 0.6 s instead of 7.6 s at 1-ply and 1.8 s instead of 6.9 s at 3-ply. Progress still advances roll by roll. Forced passes are recognized by the rules check in advance and evaluated in the same run. A version change during the job still fails it without a result. [#24](https://github.com/HendrikReh/neondojo/pull/24) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Moving between moves keeps the analysis.** Stepping to another move in Analysis no longer reloads the move colors and the match statistics, and the move history keeps its colors throughout. The displayed analysis now stays known for the match while the catalog entries of the new position load. Stepping ten moves forward and back used to send 42 such requests and now sends none. [#25](https://github.com/HendrikReh/neondojo/pull/25) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)

### Fixed

- **Decision index keeps the newest analysis.** When two requests rebuilt the index entry of the same match, the slower one could replace the newer entry with decisions it had loaded earlier. Each rebuild now reads the match's fingerprint again before it stores the entry, and loads the decisions again if they changed. [#23](https://github.com/HendrikReh/neondojo/pull/23) · [Recurring mistakes](docs/reference/study.md#my-recurring-mistakes)

## [0.2.0] - 2026-11-18

Adds two features and fixes dice equity and Open Sage fork recognition. This release has no new database migration; the latest migration is still `0079_xg_ply_convention`.

### Added


- **Delete dice equity.** **Checker → Dice equity** can permanently delete the displayed finished calculation after a confirmation. The tab then shows an older calculation of the same engine or stays empty until you request a new one. `DELETE /api/positions/{id}/jobs/{job_id}` deletes finished dice equity jobs only. [#17](https://github.com/HendrikReh/neondojo/pull/17) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Open engine duels in Analysis.** Each result row in **Extras → Engine duels** has an **Analysis** action next to the SGF download. It opens the archived library entry, also after a move to another database, and saves a deleted entry again under **Engine Duels**. [#14](https://github.com/HendrikReh/neondojo/pull/14) · [Engine duels](docs/reference/extras.md#engine-duels)

### Fixed

- **Dice equity follows the board.** The grid starts on the roll shown on the board and outlines it, instead of always starting on 1-1. Selecting a cell shows that roll's best play on the board with its dice, and selecting another position starts again on its roll. [#12](https://github.com/HendrikReh/neondojo/pull/12) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Dice equity uses the active engine.** A saved result from another engine is no longer shown in place of a calculation with the active analysis engine. **Show calculated result** and the wait for a running position calculation also follow the active engine. [#13](https://github.com/HendrikReh/neondojo/pull/13) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Open Sage fork `340fc53` is recognized.** **Extras → Versions** and `GET /api/engines` name the repository and source commit of fork build `340fc53`, as they already did for earlier documented builds. [#16](https://github.com/HendrikReh/neondojo/pull/16) · [Version information](docs/reference/extras.md#version-information)

### Documentation

- The local Open Sage setup names fork build `340fc53` as the active installation and documents its fix for post-Crawford cube decisions. Up to fork commit `5b146c7`, Open Sage undervalued "no double" for the trailing player after Crawford. Analyses run with `5b146c7` keep their values and need a new analysis for corrected cube values. The setup also records the results of re-analyzing affected matches with `340fc53`. [#11](https://github.com/HendrikReh/neondojo/pull/11) · [#15](https://github.com/HendrikReh/neondojo/pull/15) · [Local Open Sage setup](docs/open-sage-local.md)

## [0.1.0] - 2026-11-18

First tagged release. It covers everything since the first commit on 2026-11-08. The latest migration in this release is `0079_xg_ply_convention`.

### Upgrade notes

- **One ply convention for GNUbg and Open Sage.** BG Tutor now stores, shows and compares every depth in the XG and Open Sage convention, where 1-ply is the raw neural-net evaluation. BG Tutor's 3-ply is GNUbg's native 2-ply, and the backend converts only where it talks to GNUbg. Migration `0079` raises every stored GNUbg depth by one level: settings, profiles, jobs and results, match checkpoints, benchmarks, duel series, play sessions, flashcards and imported GNUbg SGF evaluations. Recorded text keeps its original numbering, including archived duel SGFs, finished practice attempts, reviews, study-report snapshots and flashcards exported to Anki before the migration. [#8](https://github.com/HendrikReh/neondojo/pull/8) · [Ply convention](docs/reference/settings.md#ply-convention)
- **Builds on macOS.** The workspace `Cargo.toml` sets `strip = "none"` for the `dev` and `release` profiles to avoid `mis-aligned LINKEDIT string pool` failures with Rust 1.94.1 on macOS 27. Do not re-enable stripping through environment variables, Cargo config or `RUSTFLAGS`. [Local restart](README.md#restart-neondojo-locally-on-port-8787)

### Added


#### Match library and import

- Named match databases in one PostgreSQL instance, with the protected system databases **Unsorted**, **Bot Matches**, **Paused Matches** and **Engine Duels**. Imports without an explicit target go to **Unsorted**.
- Import of Jellyfish `.mat` (converted by GNUbg), `.sgf`, XG2 `.xg` (save versions 24–40), BGBlitz `.bgf` 1.0 and HedgeHog `.ogxm` v2. Each import stores the original file, the canonical SGF and the positions in one transaction, and SHA-256 detects identical files within the target database. Multi-file imports report duplicates and failures per file.
- XG and BGF imports keep the source event, played date, round and location. A startup backfill fills these fields for older imports from the stored originals.
- Imported XG, GNUbg SGF, BGBlitz and OGXM evaluations are kept as source analysis with their original units, depth labels and provenance.
- Server-side filtering, sorting and pagination with numbered pages, advanced match filters, a notes search, configurable and resizable columns, and an exact Position ID search that lists every occurrence as its own row.
- Bulk actions for copying, moving, tagging, exporting (SGF or original format) and deleting matches, and **Analyze selected matches …** with progress tracked per database across reloads.
- Player database with profiles, aliases, merging of duplicates, PR, sortable columns and **Analyze unrated matches …**.
- Saved positions with saved searches, game-plan filters, undo for removals and a deck check before adding positions to decks.
- **Find similar positions** for a saved position, with checker-move practice and a saved practice history.
- Database backup export and restore with archive validation, a content preview, a comparison with the current database and a migration fingerprint check.

#### Analysis

- Persistent background jobs for position, game and match analysis with SSE progress, per-game checkpoints, deduplication, lease recovery and protection from stale workers.
- Open Sage, through the pinned local fork, as a second engine for analysis (up to 4-ply) and playing, enabled only after a real capability probe. [Local Open Sage setup](docs/open-sage-local.md)
- Analysis workspace with game selection, move history, ranked candidates, checker and cube decision review on the board, move previews, cube context with take points, and explanations of why the best play is better.
- A shared catalog of saved GNUbg, Open Sage and imported analyses. **Displayed analysis** chooses one result per match, **Compare saved analyses** compares two entries, and **Delete analysis** removes one result while keeping the match.
- **Dice equity** for all 21 unordered dice pairs, weighted over 36 ordered rolls and including forced passes. Missing values are calculated automatically through the position-job flow.
- Match statistics with native GNUbg PR and four configurable error thresholds.
- Analysis time estimates before a job starts and remaining-time estimates while it runs.
- Game-plan classification for both sides, including a separate **Opening** plan, together with tags, comments and opening-book references from the XG Opening Book and BackgammonDB.
- Searches from the analysis board across all match databases, and a decision search over all analyzed decisions sorted by equity loss.
- **Simultaneous analysis jobs** from 1 to 8 on one computer. Open Sage analyses share one sidecar process and run one at a time. [#3](https://github.com/HendrikReh/neondojo/pull/3)

#### Playing

- Point matches from 1 to 25 points and money games against GNUbg or Open Sage. GNUbg supplies the dice, legal moves, scoring and the Crawford rule.
- Resigning, forfeiting, pausing and resuming matches. Bot matches are archived automatically in **Bot Matches**, and paused sessions in **Paused Matches**.
- The play modes **Competition**, **Training**, **Coach** and **Tutor**, with a learning review that distinguishes first attempts, retries and assistance, and reports first-attempt PR.
- Checker animations, recorded game sounds, keyboard shortcuts and a single match bar above the board.
- Recurring-mistake statistics count only matches played without learning aids.

#### Study

- **Study** with an overview, saved reports, an optional AI assessment of a completed match, recurring mistakes by game plan and decision type, and a practice history.
- Flashcards with an FSRS schedule shared across decks, card tags with saved topics, filtered practice rounds, transfer practice, pip-count, score and timed exercises, and active checker and cube answers.
- Anki integration: export decks, send decks to Anki Desktop, import one or more Anki packages, and sync the review history on demand or automatically, with sync status, controls and history.

#### Extras and settings

- **Extras → Versions** with the build commit and engine versions, and a copyable diagnostic report.
- **Extras → Benchmark** for match analysis against the fixed WC2014 reference and for play-turn latency, with separate histories.
- **Extras → Engine duels** between engines and depths, with paired seeds and swapped sides, a confidence interval based on the variance of the pair means, CPU time per move, cubeless money-game blocks and archived Open Sage evaluations.
- A settings dialog with protection for unapplied drafts, and configuration profiles.
- Engine installation checks that run a real position evaluation. Thread limits are detected dynamically and configured separately for analysis and playing. [#2](https://github.com/HendrikReh/neondojo/pull/2)
- Interface themes **Graphite**, **Club** and **Paper** with light, dark and system modes, the board artwork Forest, Walnut and Slate, and locally supplied BGBlitz themes.
- Localization in English, German, Spanish, French, Italian and Polish, including plural forms for counted nouns.
- Shared action icons with accessible names and tooltips, and keyboard navigation across workspaces.

### Changed

- The header orders its workspaces as Playing, Analysis, Study, Positions, Match database, Player database and Extras. Header rendering and the arrow keys follow the same order from `web/src/App.tsx`. [UI navigation](docs/ui-navigation.md)
- Search workspaces use numbered pages. The match library loads its pages on demand.
- The Playing header, the Analysis header, the Extras tools, the player database, saved positions and flashcard search were condensed for desktop density.
- Settings were reorganized into **General**, **Appearance**, **Analysis**, **Playing**, **Board** and **Profiles**.
- Engine labels are consistent across workspaces, and event labels are consistent across match views.
- Analysis setup is separate from running jobs, and position recalculation has its own dialog.

### Performance

- The Open Sage playing sidecar stays warm between turns, and the adapter uses verified batches and cube strategy optimizations.
- Library pages with imported analyses load without summarizing them, and the position list no longer ranks every imported analysis.
- A card's game notes are found by game key instead of scanning all games.

### Fixed

- XG imports handle inactive cube placeholders, open money sessions and compact checker notation.
- Native GNUbg take and pass equities are normalized.
- Restores keep the generated position context.
- A terminal job status is accepted even when it carries an older timestamp.
- GNUbg health checks stay available during a slow engine startup.
- Engine duels keep the Open Sage evaluations from play.
- The choice to double and to pause is preserved after a checker error.
- Leaving a move preview reveals the played move again.
- Completed position analysis stays visible while it is recalculated.

### Documentation

- An illustrated English user guide, reference pages for each workspace, and development guides for the API, architecture and verification. [#4](https://github.com/HendrikReh/neondojo/pull/4) · [#5](https://github.com/HendrikReh/neondojo/pull/5) · [#6](https://github.com/HendrikReh/neondojo/pull/6) · [#7](https://github.com/HendrikReh/neondojo/pull/7)
- The local restart procedure on port 8787 and the local Open Sage setup.

[Unreleased]: https://github.com/HendrikReh/neondojo/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/HendrikReh/neondojo/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/HendrikReh/neondojo/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/HendrikReh/neondojo/compare/v0.3.0...v1.0.0
[0.3.0]: https://github.com/HendrikReh/neondojo/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/HendrikReh/neondojo/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/HendrikReh/neondojo/releases/tag/v0.1.0
