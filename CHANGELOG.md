# Changelog

All notable changes to NeonDojo are recorded in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/). Each release is tagged as `v<version>`, and the version matches `backend/Cargo.toml` and `web/package.json`. The backend applies pending database migrations at startup.

NeonDojo 1.0.0 is the first release under this name; BG Tutor 0.1.0 to 0.3.0 are its predecessors. From 1.0.0 on, a new major version marks every change that requires action from users: an incompatible change to the REST API, data or backup formats that are not migrated automatically, renamed or removed configuration such as environment variables, and dropped support for an engine version. New features that keep these intact raise the minor version, and fixes raise the patch version.

## [Unreleased]

### Added

- **Duplicate match finder.** **Duplicates** in the Match database groups matches with the same played record in the selected database or in all databases, including the same match imported from different formats or with the sides swapped. Groups of byte-identical files are marked. **Select copies** selects all but the oldest match of each group for the existing bulk actions. New endpoint `GET /api/library/duplicates`. [Duplicate match finder](docs/reference/library.md#duplicate-match-finder)

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
- **NeonGammon 3.0.0 replaces Open Sage.** The engine is the [NeonGammon](https://github.com/HendrikReh/neongammon) fork of Open Sage, and NeonDojo now calls it NeonGammon in the interface, all six languages, engine versions and documentation. Its internal ID stays `bgsage`, so settings, jobs and the API keep their values. [Local NeonGammon setup](docs/open-sage-local.md#neongammon-300--20-november-2026)
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

- **Open Sage fork release 2.0.20261119.** BG Tutor accepts the fork's release package `2.0.20261119` as well as the earlier `2.0.20260907` builds, so saved jobs and duels stay valid. **Extras → Versions** recognizes the release by its model fingerprint. The release changes Open Sage's results: cube decisions use a new money cube life, and cubeful 2-ply checker play searches only the candidates its move filter keeps. New Open Sage 2-ply analyses therefore list only fully searched candidates plus the played move, for example 5 instead of 16 after the opening 31. Play hints still rate every legal move at full depth. [#26](https://github.com/HendrikReh/neondojo/pull/26) · [Local Open Sage setup](docs/open-sage-local.md#fork-release-2020261119--19-november-2026)
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
- **Builds on macOS.** The workspace `Cargo.toml` sets `strip = "none"` for the `dev` and `release` profiles to avoid `mis-aligned LINKEDIT string pool` failures with Rust 1.94.1 on macOS 27. Do not re-enable stripping through environment variables, Cargo config or `RUSTFLAGS`. [Local restart](README.md#restart-bg-tutor-locally-on-port-8787)

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

[Unreleased]: https://github.com/HendrikReh/neondojo/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/HendrikReh/neondojo/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/HendrikReh/neondojo/compare/v0.3.0...v1.0.0
[0.3.0]: https://github.com/HendrikReh/neondojo/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/HendrikReh/neondojo/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/HendrikReh/neondojo/releases/tag/v0.1.0
