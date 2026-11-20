# Changelog

All notable changes to BG Tutor are recorded in this file. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versions follow [Semantic Versioning](https://semver.org/). Each release is tagged as `v<version>`, and the version matches `backend/Cargo.toml` and `web/package.json`. The backend applies pending database migrations at startup.

## [Unreleased]

### Changed

- **Faster numbered error lists.** A numbered page of the error list in Analysis no longer projects every decision of the analysis to count them and pick its 50. Migration `0082_review_hits` adds a derived list of each analysis's validated decisions, built once from the saved analysis and rebuilt when the analysis, its checkpoints or the source change. Pages count and select there with the current error thresholds and project only their own decisions. On WC2014 a page takes about 11 ms instead of 48 ms. Cursor pages are unchanged. [Analysis workspace](docs/reference/analysis.md#analysis-workspace)

## [0.3.0] - 2026-11-19

Supports the Open Sage fork release 2.0.20261119 and speeds up the decision index, the analysis catalog, GNUbg dice equity and moving between moves in Analysis. Migrations `0080_decision_index_row_fingerprint` and `0081_catalog_native_lookup` run at startup. Open Sage results change with the new fork release; existing analyses keep their values.

### Changed

- **Open Sage fork release 2.0.20261119.** BG Tutor accepts the fork's release package `2.0.20261119` as well as the earlier `2.0.20260907` builds, so saved jobs and duels stay valid. **Extras → Versions** recognizes the release by its model fingerprint. The release changes Open Sage's results: cube decisions use a new money cube life, and cubeful 2-ply checker play searches only the candidates its move filter keeps. New Open Sage 2-ply analyses therefore list only fully searched candidates plus the played move, for example 5 instead of 16 after the opening 31. Play hints still rate every legal move at full depth. [#26](https://github.com/HendrikReh/bgtutor/pull/26) · [Local Open Sage setup](docs/open-sage-local.md#fork-release-2020261119--19-november-2026)
- **Faster freshness check for the decision index.** **My recurring mistakes** and **All analyzed decisions** no longer hash the analysis data of every one of the player's native decisions on each request. The check now counts the saved decisions and sums their row IDs. Migration `0080_decision_index_row_fingerprint` adds a trigger that drops a match from the index when one of its saved decisions changes in place. It also carries up-to-date index entries over to the new fingerprint, so the upgrade does not re-index the library. [#21](https://github.com/HendrikReh/bgtutor/pull/21) · [Recurring mistakes](docs/reference/study.md#my-recurring-mistakes)
- **Faster lookup of the displayed analysis.** Finding the displayed analysis of a match no longer counts the saved decisions of each of its GNUbg and Open Sage analyses. Migration `0081_catalog_native_lookup` checks that a saved decision exists instead and counts the decisions only when the analysis catalog shows them. The decision index uses the same lookup, so its freshness check also gets faster. The choice of the displayed analysis is unchanged. [#22](https://github.com/HendrikReh/bgtutor/pull/22) · [Displayed analysis](docs/reference/analysis.md#imported-analysis-and-opening-books)
- **Faster dice equity with GNUbg.** A GNUbg dice equity job evaluates all 21 rolls in one GNUbg run instead of one run per roll, and it checks the engine version once before and once after that run instead of around each evaluation. GNUbg starts per job drop from 64 to 4, including the check when the job is submitted. The opening position takes about 0.6 s instead of 7.6 s at 1-ply and 1.8 s instead of 6.9 s at 3-ply. Progress still advances roll by roll. Forced passes are recognized by the rules check in advance and evaluated in the same run. A version change during the job still fails it without a result. [#24](https://github.com/HendrikReh/bgtutor/pull/24) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Moving between moves keeps the analysis.** Stepping to another move in Analysis no longer reloads the move colors and the match statistics, and the move history keeps its colors throughout. The displayed analysis now stays known for the match while the catalog entries of the new position load. Stepping ten moves forward and back used to send 42 such requests and now sends none. [#25](https://github.com/HendrikReh/bgtutor/pull/25) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)

### Fixed

- **Decision index keeps the newest analysis.** When two requests rebuilt the index entry of the same match, the slower one could replace the newer entry with decisions it had loaded earlier. Each rebuild now reads the match's fingerprint again before it stores the entry, and loads the decisions again if they changed. [#23](https://github.com/HendrikReh/bgtutor/pull/23) · [Recurring mistakes](docs/reference/study.md#my-recurring-mistakes)

## [0.2.0] - 2026-11-18

Adds two features and fixes dice equity and Open Sage fork recognition. This release has no new database migration; the latest migration is still `0079_xg_ply_convention`.

### Added

- **Delete dice equity.** **Checker → Dice equity** can permanently delete the displayed finished calculation after a confirmation. The tab then shows an older calculation of the same engine or stays empty until you request a new one. `DELETE /api/positions/{id}/jobs/{job_id}` deletes finished dice equity jobs only. [#17](https://github.com/HendrikReh/bgtutor/pull/17) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Open engine duels in Analysis.** Each result row in **Extras → Engine duels** has an **Analysis** action next to the SGF download. It opens the archived library entry, also after a move to another database, and saves a deleted entry again under **Engine Duels**. [#14](https://github.com/HendrikReh/bgtutor/pull/14) · [Engine duels](docs/reference/extras.md#engine-duels)

### Fixed

- **Dice equity follows the board.** The grid starts on the roll shown on the board and outlines it, instead of always starting on 1-1. Selecting a cell shows that roll's best play on the board with its dice, and selecting another position starts again on its roll. [#12](https://github.com/HendrikReh/bgtutor/pull/12) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Dice equity uses the active engine.** A saved result from another engine is no longer shown in place of a calculation with the active analysis engine. **Show calculated result** and the wait for a running position calculation also follow the active engine. [#13](https://github.com/HendrikReh/bgtutor/pull/13) · [Analysis workspace](docs/reference/analysis.md#analysis-workspace)
- **Open Sage fork `340fc53` is recognized.** **Extras → Versions** and `GET /api/engines` name the repository and source commit of fork build `340fc53`, as they already did for earlier documented builds. [#16](https://github.com/HendrikReh/bgtutor/pull/16) · [Version information](docs/reference/extras.md#version-information)

### Documentation

- The local Open Sage setup names fork build `340fc53` as the active installation and documents its fix for post-Crawford cube decisions. Up to fork commit `5b146c7`, Open Sage undervalued "no double" for the trailing player after Crawford. Analyses run with `5b146c7` keep their values and need a new analysis for corrected cube values. The setup also records the results of re-analyzing affected matches with `340fc53`. [#11](https://github.com/HendrikReh/bgtutor/pull/11) · [#15](https://github.com/HendrikReh/bgtutor/pull/15) · [Local Open Sage setup](docs/open-sage-local.md)

## [0.1.0] - 2026-11-18

First tagged release. It covers everything since the first commit on 2026-11-08. The latest migration in this release is `0079_xg_ply_convention`.

### Upgrade notes

- **One ply convention for GNUbg and Open Sage.** BG Tutor now stores, shows and compares every depth in the XG and Open Sage convention, where 1-ply is the raw neural-net evaluation. BG Tutor's 3-ply is GNUbg's native 2-ply, and the backend converts only where it talks to GNUbg. Migration `0079` raises every stored GNUbg depth by one level: settings, profiles, jobs and results, match checkpoints, benchmarks, duel series, play sessions, flashcards and imported GNUbg SGF evaluations. Recorded text keeps its original numbering, including archived duel SGFs, finished practice attempts, reviews, study-report snapshots and flashcards exported to Anki before the migration. [#8](https://github.com/HendrikReh/bgtutor/pull/8) · [Ply convention](docs/reference/settings.md#ply-convention)
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
- **Simultaneous analysis jobs** from 1 to 8 on one computer. Open Sage analyses share one sidecar process and run one at a time. [#3](https://github.com/HendrikReh/bgtutor/pull/3)

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
- Engine installation checks that run a real position evaluation. Thread limits are detected dynamically and configured separately for analysis and playing. [#2](https://github.com/HendrikReh/bgtutor/pull/2)
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

- An illustrated English user guide, reference pages for each workspace, and development guides for the API, architecture and verification. [#4](https://github.com/HendrikReh/bgtutor/pull/4) · [#5](https://github.com/HendrikReh/bgtutor/pull/5) · [#6](https://github.com/HendrikReh/bgtutor/pull/6) · [#7](https://github.com/HendrikReh/bgtutor/pull/7)
- The local restart procedure on port 8787 and the local Open Sage setup.

[Unreleased]: https://github.com/HendrikReh/bgtutor/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/HendrikReh/bgtutor/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/HendrikReh/bgtutor/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/HendrikReh/bgtutor/releases/tag/v0.1.0
