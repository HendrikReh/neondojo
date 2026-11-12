# BG Tutor

BG Tutor is a local backgammon application for importing matches, finding positions, analyzing decisions with GNU Backgammon (GNUbg) or Open Sage, playing, and studying. Its headless Rust backend uses Tokio, Axum, PostgreSQL, and SQLx. The React and TypeScript client connects through REST and server-sent events (SSE).

[Quick start](#quick-start) · [Analyze a match](#analyze-your-first-match) · [Match databases](#match-databases) · [Playing](#playing-against-gnubg) · [Study](#study) · [API](#api-reference) · [Verification](#verification)

## Quick start

- Rust, tested with 1.94.1
- Node.js 22.12+ and npm
- Docker Compose
- Python 3.10 or later for the frontend build and development scripts
- A local GNUbg installation with its neural network weights. Playing also requires GNUbg's embedded Python module. Analysis and playing were tested with GNUbg 1.08.003 on macOS. Earlier analysis checks used 1.05.002.

From the repository root, create `.env` if needed, then start PostgreSQL and the backend.

```sh
[ -f .env ] || cp .env.example .env
docker compose up -d --wait db
cargo run -p bgtutor
```

The backend applies migrations at startup. Cargo can compile with PostgreSQL stopped. The backend reads `.env` from its working directory. See [.env.example](.env.example) for available settings.

Check that `gnubg --version` works. If the executable is outside the backend's `PATH`, set `GNUBG_BIN` in `.env` before the first launch or configure the executable under **Settings → Analysis**. Use **Check installation** to verify that GNUbg can evaluate a position with its installed weights.

In a second terminal at the repository root, start the development client.

```sh
npm ci --prefix web
npm run dev --prefix web
```

| Service     | Address                                                                |
| ----------- | ---------------------------------------------------------------------- |
| Web client  | <http://127.0.0.1:5173>                                                |
| API health  | <http://127.0.0.1:8787/api/health>                                     |
| OpenAPI 3.1 | <http://127.0.0.1:8787/api/openapi.json>                               |
| PostgreSQL  | `127.0.0.1:54329`, backed by the `bgtutor_postgres_data` Docker volume |

To serve the built client from the backend, keep PostgreSQL running and start the backend with `BGTUTOR_WEB_DIR` set.

```sh
npm ci --prefix web
npm run build --prefix web
BGTUTOR_WEB_DIR=web/dist cargo run -p bgtutor --release
```

Open <http://127.0.0.1:8787>. By default the services listen on loopback. BG Tutor has no built-in user accounts, so network access requires separate access control.

### Restart BG Tutor locally on port 8787

1. From the repository root, keep the existing PostgreSQL database and `.env` configuration. Reuse any `DATABASE_URL` and `GNUBG_BIN` overrides from the previous launch. If you use the Compose database and it is stopped, start it with `docker compose up -d --wait db`. If the frontend dependencies are missing or the lockfile changed, run `npm ci --prefix web`.
2. Build the updated application before stopping the running server.

   ```sh
   npm run build --prefix web
   cargo build -p bgtutor --bin bgtutor --release
   ```

   On macOS 27, a `mis-aligned LINKEDIT string pool` error while loading a Rust procedural macro can require `CARGO_PROFILE_RELEASE_STRIP=none` before the Cargo command. This retains release optimization while disabling the affected strip step; see the [Rust issue](https://github.com/rust-lang/rust/issues/157750). Use the same environment setting for later release builds or `cargo run --release`.

3. Stop BG Tutor with `Ctrl+C` in its terminal. If it runs in the background, use `lsof -nP -iTCP:8787 -sTCP:LISTEN` to find the listener. Confirm that it is the `bgtutor` process, then replace `PID` with its process ID in `kill -TERM PID`. Wait until the listener has stopped.
4. Start the backend from the repository root. It applies pending migrations at startup and serves the built frontend on the same port.

   ```sh
   BGTUTOR_BIND=127.0.0.1:8787 BGTUTOR_WEB_DIR="$PWD/web/dist" cargo run -p bgtutor --bin bgtutor --release
   ```

5. In another terminal, check the health endpoint. Once it reports `"status":"ok"`, open or reload [BG Tutor](http://127.0.0.1:8787/). Check [the database list](http://127.0.0.1:8787/api/databases) for **Paused Matches** after applying migration `0040`.

   ```sh
   curl --fail --silent --show-error http://127.0.0.1:8787/api/health
   ```

## Analyze your first match

1. In **Match database**, choose **Import match** and open [tutorial.mat](backend/tests/fixtures/tutorial.mat) or the reference match [WC2014.xg](backend/tests/fixtures/WC2014.xg). You can also drop a file on the welcome screen. Search by player or file name to find an imported match. Results load in pages using stable cursors.
2. In **Analysis**, use **Select game** or its adjacent arrows to choose a game. The selector shows the game number, starting score, move count, and Crawford status when applicable. The move history stays within the selected game and loads more moves as needed.
3. Select a move. The board shows the decision position before the recorded play, together with the score and doubling cube. Point numbering and notation follow the player on roll. Pip counts include checkers on the bar.
4. In **Overview** or **Evaluation**, choose **Analyze position** to queue a persistent GNUbg job. Choose 0–4 ply, with 2-ply as the default. The result includes up to five candidate moves, cubeful equity, and win probabilities.
5. Use **Analyze game** to evaluate the selected game, or set **Analysis scope → Entire match** and choose **Analyze entire match**. Both accept 0–4 ply. The chosen scope determines the jobs, progress, statistics, and errors shown. Games with no checker moves, including cube-only games, can also be selected and analyzed.
6. Open **Evaluation → Dice distribution** to evaluate all 21 unordered dice pairs. The matrix represents 36 equally likely ordered rolls, with weight 1 for doubles and 2 for other pairs. The weighted mean gives the expected equity. When a player has no legal play for a roll, GNUbg evaluates the forced pass. **Game rolls** shows the rolls recorded in the selected game.

**Match database → Find a position in the library** searches for an exact GNUbg position ID, which represents the checker arrangement from the perspective of the player on roll. Analysis also uses the GNUbg match ID for context such as the score, cube, and dice. Each occurrence in the library has its own row with the database, game, and move. Opening a row selects that exact occurrence, even when the same position appears more than once in a match. Use **Find similar positions** beside the analysis board to search for related positions.

## Analysis workspace

The top navigation opens **Playing**, **Analysis**, **Match database**, **Positions**, **Study**, and **Extras**, with **Settings** on the right. Arrow keys and Home/End navigate the main and detail tabs. The app restores the last match, game, and position locally. Switching tabs preserves filters, note drafts, and SSE subscriptions. A submitted job continues to be tracked if you change the selected game or position while the request is pending.

The desktop **Overview** places move history on the left, the board and candidates above the errors in the center, and player and game statistics on the right. Long lists scroll independently and keep the current move visible. Dedicated tabs include **Evaluation**, **Statistics**, **Errors**, **Notes**, and **Study cards**. The header shows the players, match ID and length, saved score, date, event, and PR per player. Date and event come from saved metadata and show `n/a` when unavailable. Match PR updates when a complete native match analysis finishes.

### Moves, errors, and statistics

Move history shows notation, player, dice, and localized equity loss to three decimal places. Green indicates a good move. Yellow, orange, and red mark losses from 0.04, 0.08, and 0.16 respectively. A zero loss is shown as `0.000` in English, and a dash indicates that no evaluation is available. Move assessments load for the selected game, identify the analysis source and depth, and update when a job finishes.

**Errors by move** defaults to **Inaccuracies only (≥ 0.04)**. **Reset filters** restores that threshold. Turning off the threshold or selecting an overall decision count in **Statistics** opens **Decisions by move**. Statistics can filter the visible list by player, checker or cube decision, severity, and game. Single-game analysis keeps the game filter scoped to that game, including after a reset. For a full match, choose all games or an individual game. Filtering within the current game preserves the board selection.

Checker rows show the played and best moves, their equities, and the loss. Cube rows include missed doubles and double, take, and drop decisions. Selecting a move number opens the corresponding board position. Game buttons in **Statistics** open that game, and changing a decision filter restarts pagination. Native PR uses GNUbg job statistics. New jobs save complete per-game statistics atomically. Older jobs can show totals reconstructed from their decisions, with per-game PR shown as unavailable where it was never saved.

### Preview a move

**Initial**, **Arrows**, and **Final** show the starting position, the move path, and the resulting position. The default view uses outlined green arrows for the played move and shows legal intermediate points. Select a candidate row from an import, BackgammonDB, or local analysis to preview it. Use Enter or Space on the move name. Up/Down selects the previous or next candidate and keeps it in view. Left shows **Initial**, Right shows **Final**, and other controls keep their normal keyboard behavior. The selected board view persists when changing candidates. **Reset preview** returns to the recorded move.

The board identifies the candidate source and rank. **Final** keeps the board orientation and hides the dice. All themes use the same legality checks for bar entry, hits, multi-step moves, doubles, and bear-off. Unrecognized or illegal notation shows the starting position. Analysis, notes, position IDs, and study cards continue to use the pre-move decision. Changing position or source clears the previous candidate preview.

### Search from the analysis board

In Analysis, **Save position** and the magnifying glass share a toolbar above the board. The magnifying glass to the right of the bookmark shows **Search this position** on hover or keyboard focus. On narrow screens both icons move to the next line together.

Select **Search this position** to open Match database and run an exact search for the displayed position. The position ID appears in the expanded search form and as an active filter. The action selects **All databases**, clears the other filters, and keeps the current sort order. Selecting the action again reruns the search. Switching tabs preserves any subsequent decision to remove the filter.

### Desktop layout

The desktop layout is designed for side-by-side review using a mouse or keyboard. Layout checks use logical viewports of 1800×1169, 2560×1440, and 3840×2160, including under macOS **More Space**. The shared type scale uses 24 px page titles, 14 px section titles, 13 px controls and body text, and 12 px tables. CSS tokens define the type scale, with monospace for position IDs and separate type for the board and logo. [Typography](docs/typography.md)

Text tabs use the active theme's accent and a 2 px underline. The same pattern covers workspace and analysis tabs, board views, analysis sources, checker and cube selection, note scopes, settings, and database selection. [Navigation style](docs/ui-navigation.md)

## Settings and engines

**Settings** opens a dialog with **General**, **Appearance**, **Analysis**, **Playing**, **Board**, and **Profiles**. The **General → Language** menu lists Deutsch, English, Español, Français, Italiano, and Polski alphabetically. New installations default to English. **Board** offers previews of Forest, Walnut, Slate, and local BGBlitz themes from `themes/*.zip`. Use **Apply** to use the draft settings. [Configuration profiles](#configuration-profiles)

Set a player name, default analysis depth (0–4 ply, default 2), separate playing strength (0–2 ply), and bear-off direction. New matches use the playing settings in effect at creation. Choose GNUbg or Open Sage independently for analysis and playing. Open Sage supports verified 1-ply and 2-ply profiles with the pinned local fork; the original wheel remains limited to 2-ply. New sessions pin the bot, analysis and GNUbg rules installations separately. There is no automatic fallback to another engine.

Open Sage requires the locked native ARM64 Python 3.12 environment described in [Local Open Sage](docs/open-sage-local.md). In **Settings → Analysis → Local Open Sage installation**, enter that environment's `bin/python` path and choose **Verify and save**. The backend checks package/model integrity and real evaluation capabilities before enabling selection. Installation paths are host-local and excluded from portable profiles. GNUbg remains required for imports and for play rules, dice and SGF; Open Sage makes bot decisions when selected. 1-ply is enabled only after the installation passes a real bounded-thread capability check. 3/4-ply and rollouts are not enabled.

Automatic result readers stay within the selected analysis provider. Historical results and explicitly selected jobs retain their original provider and profile; copying a match retains completed results from both providers. Sage results carry model/profile identity but do not claim GNUbg PR statistics.

The executable path is specific to the backend computer. Installation checks, analysis, playing, and benchmarks use the same resolution rules. An absolute path is resolved directly, a relative path containing a directory separator is resolved from the backend's working directory, and a bare name such as `gnubg` uses the first matching file on the backend's `PATH`. The backend reports an error if resolving or running that file fails. The resolved path must be valid UTF-8.

**Check installation** and saving a changed executable path check the version and run a real position evaluation within a single 10-second time limit. A failed check prevents an invalid path from replacing the saved installation. Installation settings use PostgreSQL revision checks. A concurrent change surfaces a conflict and retains the draft.

A new analysis job records its engine configuration and effective ply. Active-job deduplication includes the target, scope, ply, configured executable path, and version. An engine version change at the same path fails a job that expects the original version and preserves completed game checkpoints. Legacy jobs without a snapshot use the engine configured at backend startup. MAT imports use the currently saved executable path.

Migration 0019 added 3-ply analysis and changed the original unsaved default to 2-ply. Migration 0020 added 4-ply for positions, games, matches, and dice distributions. Explicit profiles, existing jobs, and completed results retain their recorded depth. The requested depth is the GNUbg search profile, which allows early exit when the result is clear. Higher depths take longer, especially for full matches and dice distributions.

## Saved positions and searches

In **Analysis**, choose **Save position** to add the specific pre-move decision and its match context to **Positions**. The collection starts empty and fills through explicit saves. **Remove from collection** works for individual or selected entries. A saved decision can survive deletion of its source match or database, retaining player, roll, score, original match ID, related notes, and position analyses. It can still be analyzed, annotated, and used for study or Anki. Full game and match analysis and original-file export require the source match.

The position table has a fixed preview area on the right. Click or use the arrow keys to select a row, then press Enter, double-click, or use **Open analysis** in the preview to open the exact decision. The table has its own sort and column widths. Its row tooltip gives context and analysis provenance. The sortable **Decks** column shows deck names alphabetically. Equivalent decisions share deck membership when the checker arrangement, roll, score, and cube context match. Membership updates across loaded pages and in the preview after adding positions to a deck.

Filter by database, search text, player on roll, match name, tournament, round, exact tag, comment, checker position ID, match points, roll, score, Crawford, cube value, local analysis, or native equity loss. One die matches either die in the recorded roll. A pair matches either order. For example, to see how Mochy played a specific position, combine **Player on roll** with the checker position ID and roll. **Show played moves** adds the recorded decision to the table and preview. It starts off so answers stay hidden during study.

Results retain the full GNUbg position and match IDs. Comment filters consider the match, the relevant game, the position, and the matching checker move. Native loss values show their actual engine and depth, with unavailable evaluations shown as unknown. The collection searches checker decisions. Use **Statistics** or **Cube and move notes** to review cube decisions.

**Saved searches** lets you load, save, update, rename, and delete named searches. PostgreSQL stores the filters, sort, direction, and revision. Concurrent edits return HTTP 409 and preserve the draft. A search referencing a deleted database flags the missing database. Pagination ties each cursor to the complete filter definition. Returning to the workspace keeps loaded results, selection, and preview in place while refreshing.

Add up to 200 selected positions to a new or existing study deck. Each position reports whether it was added, updated, already in the deck, requires analysis, or is no longer available. Study answers require a usable completed local analysis. Identical questions share FSRS review history. **Study this deck** opens a fresh study view, and **Download entire deck as Anki** uses the Rust APKG export. A deck can hold 1,000 cards. If a batch would exceed that limit, the entire batch is rejected.

## Imported analysis and opening books

The analysis inspector offers **Imported analysis**, **Opening book**, and **Local engine**. Imported XG, native GNUbg SGF, and available BGBlitz analysis stay in their own records, with the original depth or profile label, method, candidates, cube responses, raw values, equity units, and engine version where known. Missing analysis and unknown engines or units stay unavailable. GNUbg match-winning chances retain their original units. Analysis jobs and independent match copies preserve source results. Older imports can reconstruct analysis once from the stored original file. Source evaluations for recorded double, take, and drop actions also appear in **Cube and move notes**.

Manage books under **Settings → Analysis → Opening books**. Select **Automatic** to compare available books, choose a provider directly, or select **Off**. The book dialog saves the provider and paths with revision checks while keeping the parent settings draft. Book paths are on the backend computer. The analysis inspector shows the selected source, engine, and depth for the current position. Manually requested analysis uses the selected native GNUbg profile. Book results stay separate from native GNUbg results and match PR.

- **BackgammonDB** is a local SQLite book with 4,230,443 decisions across 351 contexts. It provides up to eight ranked moves for a position and roll, plus a separate pre-roll cube lookup. Coverage is pruned and score-dependent. Evaluations use GNUbg 1.08.003 at regular 3-ply. The app credits the CC BY 4.0 data at the result. [Data and format](https://github.com/chalkstreamdev/bgdb-opening-book), [book description](https://backgammondb.com/opening-book)
- **XG Opening Book 3.70** is an official `.ob` file with 53,210 pre-roll position evaluations. It provides a position reference with the original engine, profile or rollout, and author. Values from the previous player's perspective are converted to the player on roll. Historical depth codes that cannot be identified stay unknown. The terms allow free use with attribution to the rollout authors. [Official download and terms](https://www.extremegammon.com/GetOb.aspx)

Book lookups use the full decision context, including Crawford, away scores, cube value and ownership, and money-game rules. BackgammonDB has no beaver contexts. The XG book reads money-game rules such as Jacoby and beavers from the original header and requires a supported context. Its pre-roll values apply to the resulting position independently of any displayed dice.

Keep the downloaded book files outside Git, for example in `~/Library/Application Support/BG Tutor/` on macOS, and enter their backend paths in the book dialog. Extract the BackgammonDB serving SQLite file and the XG `OpeningbookV2.ob` file. The check and install endpoints validate the data formats directly.

## Match databases

**Match database** organizes named collections within a single PostgreSQL instance. Choose a database to filter the list and target imports, or choose **All databases** to search across the library. Multi-file imports process each file separately and report duplicates and failures per file.

BG Tutor automatically provides **Unsorted** for imports without an explicit target, **Bot Matches** for games played against GNUbg, and **Paused Matches** for resumable sessions. These system databases have fixed names and are protected from deletion. You can edit their descriptions. **Empty database** is available for **Unsorted** and **Bot Matches**. It requires confirmation and preserves saved positions and their relevant notes, flashcards and review history. BG Tutor rejects the operation if any of the database's matches has a queued or running analysis. **Paused Matches** is populated by pausing games in Playing. You can create, describe, rename and delete your own databases.

### Filtering and navigating matches

- Combine filters for players, played or import dates, exact tags, comments, position ID, tournament, match name, match or money play, result, winning color or draw, round and match points. Position and note searches respect the selected database.
- An exact GNUbg Position ID filter shows every occurrence as its own row in the match table, including repeats in the same game. Use the **Database**, **Game** and **Move** columns to locate or sort the occurrences. The count and pagination refer to occurrences. Bulk actions apply once per selected match.
- Click the match button, press Enter or double-click a row to open Analysis. With a position filter active, this opens the exact occurrence. Use the arrow keys to move between rows or click the pencil icon to edit match metadata.
- Sorting applies to all results before pagination. Click a data column header to sort ascending, then click again to sort descending. Names sort case-insensitively. IDs, PR, position counts, scores, game numbers and move numbers sort numerically. Scores sort by White, then Black. Missing values appear last in both directions. Changing the sort returns to the first page and clears the selection. Without a position filter, the occurrence-specific sorts fall back to import date.

### Table and metadata

The table uses the available width alongside the database selector and scrolls horizontally when needed. It shows players, PR, score, match length or money play, played and import dates, tournament, analysis status, tags and comments. **ID** is a stable match number. It survives a rename or move, copies get new numbers, and gaps are possible. REST uses the match UUID. **Positions** counts stored positions. The source format is available for import and export.

Use **Columns** to choose and reorder data columns, **Show all columns** to reveal them, or **Default columns** to restore the default layout. **Tournament** and **Comment** are hidden by default, and **Tags** is visible. Selection and actions stay available, and the table requires at least one data column. Hidden values remain searchable and are included in the row summary where applicable. The current sort is shown above the table if its column is hidden. Column visibility and widths are saved separately, and a hidden column retains its width.

Drag the right edge of a header to resize it, or focus the divider and use the arrow keys. Shift increases the step, Home and End jump to the width limits, and Escape cancels an in-progress resize. **Reset column widths** restores defaults for the current window, including the occurrence columns. Widths and the occurrence sort follow the active profile. Widths from the former notes column carry over to **Tags**. Each row has a shared hover summary with the match ID, players, match length, result, available PR, analysis profile and excerpts of tags and comments.

The metadata editor supports display name, played date, tournament, round, venue and result, with version checks. Player names and an optional display name identify the match in the table. BG Tutor imports a played date only when the SGF source is unambiguous. XG and BGF imports use the score after the last completed game. Migration `0018` and a bounded backfill correct automatically derived results from original files while preserving manual edits, stored positions, analysis, notes and learning progress. Money game results and results without reliable source data remain unknown.

The library shows PR for a complete match analysis with native GNUbg statistics. Individual game analyses appear as partial coverage. Native PR divides normalized equity loss × 500 by the number of unforced checker decisions plus close or actual cube decisions. GNUbg uses a different decision count from XG and BMAB. Older or incomplete results without qualifying statistics leave PR unavailable. [GNUbg match statistics](https://www.gnu.org/software/gnubg/manual/gnubg.html#gnubg-matches_review_stat_overall)

### Bulk actions and exports

Use the selection controls to **Copy**, **Move**, change **Tags**, **Export** or **Delete**. Most actions support up to 100 matches. Exports support up to 50 matches and 64 MiB of combined source data. Each action reports the outcome per match.

- Copies get independent match, position, game and move IDs, with copied tags, comments and the latest completed analysis for each scope. Review history and active jobs belong to their original records.
- Moves preserve IDs. BG Tutor detects duplicates within a database by original-file hash. If the destination already contains that file, it retains both the existing destination record and the source match, including their notes.
- Deletion requires confirmation and skips matches with queued or running analyses. Saved positions and flashcards can still be used after their source is deleted.
- Exports return the original file or canonical SGF, with a ZIP for multiple matches. Use a PostgreSQL backup to capture the full library, including BG Tutor notes and review history.

The named collections, sortable tables and bulk actions draw on [ChessBase's database organization](https://en.chessbase.com/post/chessbase-18-tips-for-beginners-part-7-organising-databases). The position preview draws on its [database preview window](https://help.chessbase.com/CBase/16/Eng/preview_window_in_the_database.htm).

## How matches and positions are stored

Imports store the original file, canonical SGF and normalized positions together in PostgreSQL. A position records 26 signed checker counts including the bar, the player on roll, the dice, the game and move numbers, and the played move. The game catalog also records games without checker moves, with their opening score and Crawford status. On first access, older imports build the catalog from stored SGF.

The **80-bit GNUbg Position ID** encodes checker counts as 14 Base64 characters. Exact board searches use its B-tree index without reparsing the original file. The separate **72-bit GNUbg Match ID** records match length, score, cube, dice and rules. A match, game and move-order index gives direct access to a late game without loading earlier games.

## Private demo data

The Git-ignored folders `games/bgf`, `games/mat` and `games/xg` are for private matches. To build a demo library with the current schema, create **Demo BGF**, **Demo MAT** and **Demo XG** in **Match database** and import each folder's files into its corresponding database. The importer reports duplicates and rejected files individually.

The legacy helper `tools/rebuild_demo.py` is incompatible with the current schema. Staging compares every PostgreSQL database ID with the public REST list, which omits the internal saved-position collection. The activation SQL can select that collection and its matches for deletion.

The helper was designed to stage three databases through REST, verify a PostgreSQL dump and record each file's outcome in a manifest outside Git. Its `--allow-rejected` option allows staged matches from valid files to proceed after you review the individual rejections. Network and server errors still abort staging. With the backend stopped, activation checks the PostgreSQL cluster and database identity, all three staging databases, match IDs, their database assignments and SHA-256 hashes under a transaction lock. A failed check leaves the active library intact. Each manifest can be activated once.

The local rebuild on November 8, 2026 imported 5 BGF, 5 MAT and 23 XG matches, with 72,959 checker and cube analysis records and 98 additional source-statistics records. The private report lists nine XG files rejected for invalid or inconsistent moves. The previous library was backed up and the rebuild preserved the flashcards and review history. Public sources and product research are listed in the [BackgammonDB comparison](docs/research/2026-11-08-backgammondb-product-ux.md).

## Themes and language

### Board artwork

`npm run dev` and `npm run build` prepare artwork from locally supplied BGBlitz archives. Preparation requires **Python 3.10+** and uses the original backgrounds, checker variants, dice, cubes and point-number strips. Place new archives in `themes/` and restart the dev server or run `npm run themes:prepare --prefix web`. The three built-in themes are always available. [Board theme formats, sources and rights](themes/README.md)

### Interface appearance and language

Under **Settings → Appearance**, choose **Graphite**, **Club** or **Paper** and a **Light**, **Dark** or **System** display mode. System follows the operating system. The preview stays in the settings panel until you choose **Apply**. The browser applies its saved appearance before the first render. Tables, review sessions, dialogs and error messages use the same appearance. Choose board artwork separately under **Settings → Board**.

Use **Apply** to save draft language and board settings or **Cancel** to discard the preview. BG Tutor localizes the interface, status messages, errors and number formats while preserving visible analysis, player names, filenames and GNUbg move notation. Native engine diagnostics accompany localized errors. All six catalogs are type-checked and their keys and placeholders are validated at startup. The translations are pending native-speaker review.

### Action icons and tooltips

The app uses a shared Lucide Outline icon set for actions such as saving, undoing, requesting hints, searching, and importing or exporting. Compact actions have localized accessible names and show tooltips on hover and keyboard focus. Key play decisions, **Undo step**, **Hint**, primary form submissions, and study actions also show text labels.

Tooltips stay above modal dialogs and remain visible when you move the pointer onto their text. Escape dismisses the tooltip and preserves the dialog's Escape behavior. The shared implementation lives in `web/src/action-icons.tsx`, `web/src/ActionButton.tsx`, and `web/src/useActionTooltip.tsx`. Action buttons preserve native submit and disabled behavior and forward refs. Export controls are download links.

## Game plan

**Game plan** classifies White's and Black's structures independently as **Pure race**, **Racing (contact)**, **Attacking / blitz**, **Priming** or **Anchoring**. **Initial** and **Arrows** use the starting position. **Final** uses the displayed position after the move and falls back to the starting position if the move notation cannot be applied. The display is also available for saved positions whose source match has been deleted.

The classifier uses the board alone. It works without an analysis job, and GNUbg supplies move and cube recommendations. `POST /api/game-plans` validates the 26-entry board and returns both classifications with the rule version. The browser caches a bounded number of identical boards. Source and license information is in [game_plan_source.txt](backend/src/game_plan_source.txt) and [backend/licenses](backend/licenses/). [Open Sage reference implementation](https://github.com/markbgsage/bgsage/blob/139427d826dd67798226e7d23c003c90315306ba/cpp/src/encoding.cpp)

## Checker and cube error statistics

Native statistics show a **Total / Checker / Cube** breakdown per player for **Inaccuracies**, **Doubtful**, **Bad** and **Very bad**. Inaccuracies includes all three severity levels. Click a count to open the decisions with the matching player, decision type and severity. A count in the game overview also filters to that game. A verified zero opens an empty list.

BG Tutor reads the breakdown from stored decisions for fully checkpointed games, including those from earlier analyses. Forced, unavailable and loss-free decisions do not contribute to the error counts. If the stored decisions lack verifiable coverage, the UI shows **—** with an explanation. `GET /api/jobs/{id}/statistics` returns `error_breakdown` for the job and its games. The decision list validates its pagination cursor against the selected filters.

## Tags and comments

Open **Analysis → Notes** and select **Match** or **Position** to edit its tags and comment. **Cube and move notes** also opens game notes and recorded actions such as doubles, takes, drops and accepted resignations, with the board immediately before each action. A pending double displays the offered cube value in the center. Games without checker moves can have notes too. Checker move notes use the same annotation as the corresponding position.

**Save notes** writes to PostgreSQL. Drafts survive navigation and dialog changes during the current session. If another edit has been saved, BG Tutor shows a version conflict and keeps the draft. Tags are normalized and limited to 30 per note and 64 characters each. Comments are displayed as text and can contain up to 20,000 characters.

Use **Search tags & comments** to combine free text (3–200 characters), an exact tag, a note type and a database filter. Results link to the relevant match, game, position or move. The regular match search also indexes annotations across those scopes.

## Flashcards and Anki

### Create and manage decks

In **Analysis → Study cards**, use **Positions as flashcards** to create or select a deck. Add the current analyzed position or import errors from the selected game by player and minimum equity loss. BG Tutor uses a completed game analysis or the selected game's decisions from a completed match analysis. Cards support checker play, double/no double and take/pass questions.

A card's identity includes the board, full match context and question type. A single card can belong to several decks. Adding the decision again updates the analysis source and preserves its review history. BG Tutor reads current source tags and comments when you reveal or export a card. Each deck holds up to 1,000 cards. Card lists return up to 100 per request.

In **Study → Flashcards & Anki**, use **Rename deck** or **Delete deck** on the selected deck. Renaming keeps the deck ID, cards, preview selection and progress. The Anki download uses the new name. Deleting requires confirmation with the deck name and card count and removes the deck and its memberships. Saved positions, cards, review history and other decks are retained. Adding a decision to another deck reuses its existing card and progress.

Renaming or adding memberships advances the deck version. If the deck changes while a dialog is open, **Refresh deck details** loads the current version and keeps the draft. Confirm again to apply the change. The Position Browser refreshes deck choices and membership labels after a change. Deleting a deck closes its active study session.

### Find and preview cards

**All flashcards** searches the shared collection, including cards without a deck. Select a deck to search within it. Each result appears once and shows its deck memberships. The server searches players, questions, position IDs, deck names, tags and comments before paginating. Combine **Player on roll**, exact tag, comment text, exact checker ID, **Decision type** and **Review status**. **New** means no completed reviews, **Due** can include new cards, and **Reviewed** means at least one completed review. The search reads current match, game, position and move notes, including retained notes after a source match is deleted.

The list selects the first card and shows the question, players, dice, score and cube on the preview board in the chosen theme. Click a row or use the arrow keys, Home or End to preview a different card. The list scrolls independently, and loading more results preserves the selection. Switching decks clears the previous preview. Previewing shows only the question. Choose a deck to enable **Study now** and **Download Anki deck**. These actions use the selected deck, independently of the search filters.

### Review cards and export to Anki

Study shows due cards with the chosen board theme, match length, score and player on roll. Money games also show the Jacoby status. Work out your answer, select **Show answer** and rate it **Again**, **Hard**, **Good** or **Easy**. Rust's `fsrs` library schedules reviews at **90%** target retention. **Again** schedules a review in 60 seconds and the first **Hard** in 10 minutes. Later Hard, Good and Easy intervals use FSRS in whole days. Easy is scheduled at least one day after Good. PostgreSQL stores the card state, rating history and due dates and prevents concurrent or retried requests from applying a review twice.

**Download Anki deck** creates a self-contained `.apkg` in Rust, with a stable note model and GUIDs, SVG diagrams, score, cube, dice, answers, tags and comments. Labels use the selected language. The package has no runtime dependency on BGBlitz artwork or Python. Reimporting updates existing Anki notes. A stored content fingerprint advances Anki's modification time even when comments or the export language change in quick succession. BG Tutor and Anki maintain independent review histories. AnkiConnect, importing external decks and bidirectional sync are future options.

The workflow draws on personal and thematic error decks described by [Dirk Schiemann](https://www.dirk-schiemann.de/the-anki-method.html), [XG2Anki](https://xg2anki.de/) and [AnkiGammon](https://ankigammon.com/). See the [Anki manual](https://docs.ankiweb.net/manual/importing/packaged-decks) and [FSRS-rs](https://docs.rs/fsrs/latest/fsrs/) for package import and scheduling details.

To verify a package in a temporary empty profile, install the official Python `anki` package and run one of these checks. The verifier has been tested with Anki 26.9.4 for import, duplicate detection, HTML escaping and comment and language updates.

```sh
python tools/verify_anki_package.py deck.apkg --expected-cards 10
# Verify an updated package containing the same cards.
python tools/verify_anki_package.py before.apkg --updated after.apkg
```

## Import formats

| Format | Support                                                                                                                                                                                                                                                                                                    |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `.mat` | GNUbg converts Jellyfish MAT to SGF. The importer normalizes recognized resignation lines without shifting columns and expands compound bear-off moves into legal steps. It checks GNUbg diagnostics and replays the full move history before saving.                                                      |
| `.sgf` | Native UTF-8 parser for standard backgammon, including escapes, multiple games, setup positions, cube actions, scores, Crawford, Jacoby and move reconstruction. Ambiguous variation trees are rejected.                                                                                                   |
| `.xg`  | Native decoder for XG2 save versions 24–40. It checks the RichGame container and CRC, bounds Zlib decompression and converts to SGF. Supports inactive cube placeholders, open money sessions, compact checker notation and valid in-progress point matches, including an explicitly unfinished last game. |
| `.bgf` | Native Rust decoder for BGBlitz BGF 1.0 with a JSON header and JSON or Smile payload, optionally Gzip-compressed. It preserves checker moves, forced passes, double/take/drop actions, colors, scores and Crawford or Jacoby rules, then replays the resulting SGF.                                        |

The XG importer rejects XG1, edited starting positions, automatic doubles, beavers, raccoons and other variants. BGF variants, custom starting positions and restricted cube rules are also rejected. XG candidate and cube evaluations, depth labels and rollout references, and supported BGF analysis are stored as source analysis. The original artifact retains full XG rollout streams and comments. BG Tutor does not interpret XG RTF comments. [Import fixtures and format references](backend/tests/fixtures/README.md)

An upload takes one file and has a **16 MiB** limit including multipart framing. SHA-256 detects identical files within the target database. The original, canonical SGF and position records are saved in one transaction. Parsers and archives have separate size and record limits. Filenames and file contents are handled as data.

## Playing against GNUbg

### Start, resume, or end a match

Open **Playing** to start a 1–25-point match or choose **Money game**. You can play either color. The active profile remembers your selection. GNUbg handles opening rolls, legal moves, doubling, scoring, the Crawford rule, and match completion. BG Tutor automatically declines bot resignation offers, so play continues until a cube or bear-off result.

Money games use the Jacoby rule and ordinary doubling, with beavers disabled and no target score (`match_length: 0` in the API). Playing automatically generates a learning review after each completed money game. **Next game** carries the running score forward once the review is saved. Older single-game money sessions can also continue. Individual game reports appear under [Study → Reports](#reports).

**End session** pauses an unfinished point match or money session, including between games. BG Tutor saves the last confirmed checkpoint, including the score, dice, pending decision and engine settings. Enter any unconfirmed checker steps again after resuming. Choose **Resume match** on the paused board or beside the entry in **Match database → Paused Matches**. The **Paused Matches** button in Playing opens that database. The same entry returns to its previous database. If that database was deleted or now contains a copy, BG Tutor tries **Bot Matches** and then **Unsorted**, preserving any existing copies.

BG Tutor automatically archives bot matches, including previously saved sessions, in **Match database → Bot Matches**. Manage individual matches there using the standard controls for names, tags, comments, moves between databases, copies, deletion, export, and analysis. Choose **Resume match** to return to a running original. Completed matches and independent copies open in Analysis. **Playing** restores the last session after reload and requires an explicit action to resume a paused match. **Download SGF** exports the match at any time.

Each new match records the player name, bear-off direction, play engine, resolved GNUbg executable path, installed version, and playing depth from Settings. Existing matches keep those settings. GNUbg must include its embedded Python module. If the executable becomes unavailable, the version changes, or the engine fails or times out, the saved checkpoint remains available for resumption when the engine is restored.

### Rolls, checker moves, and the cube

Each player rolls one die at the start of every game. On a tie, both players roll again. The higher die determines who plays first, and the winner uses that pair for the first checker move. The result and any ties remain visible until you choose **Begin the first move**. Reloading restores that opening checkpoint.

Before each of your regular turns, **Next roll** offers **Roll dice** and **Double**. If the Crawford rule, your opponent's ownership of the cube, or a cube value that already covers your remaining match points prevents doubling, the disabled button explains why. When the bot doubles, the response dialog shows the current and offered cube values.

Your checkers are always at the bottom of the board. Your saved bear-off direction determines the side of your home board. Hover over or focus one of your checkers to see the destination for the first available die. Click the source point to move one checker there. Click either die before moving to swap their order. After your first step, **Undo step** reverses one unconfirmed move at a time. It shows the number of pending steps below **Hint** in the action panel to the right of the board and stays accessible while scrolling. When the turn is legal and complete and animation has finished, click either die to confirm it. Used dice fade. A partly used double fades exactly half of one die.

The move history follows the latest saved action and uses the same row layout as Analysis, with notation, player colors, dice, and neutral analysis markers. It sits to the left of the board and moves below it on narrow screens. The game-end dialog shows the winner, points won, and the calculation when the record supports it. Choose **Next game** to continue. At the end of a match, **New match** opens setup and **Back to board** returns to the finished board.

### Decision dialogs and hints

Opening rolls and their results, pre-roll choices, cube responses, blocking feedback, forced passes, and game or match results use a modal dialog. It opens over the center of the board. Drag its title to see the checkers underneath. The position is retained while the dialog is open and stays within the viewport when the window or dialog changes size. Keyboard focus stays inside, and the dialog requires a decision before you can use the board again. Escape and outside clicks leave it open. Errors and pending acknowledgments appear inside the dialog. Expand **Active mode** to access the mode and [Tutor warning threshold](#playing-feedback-modes).

**Hint** opens a movable dialog with its own loading and error states. Select a checker suggestion with the mouse or Up/Down to see the full saved turn on a separate, read-only board. Use **Initial**, **Arrows**, or **Final** to see the starting position, move arrows, or projected result. Cube hints are text only. **Close** or Escape returns to the game or to the required decision dialog. Decision feedback is shown in dialogs. The live dice order and unconfirmed steps are preserved.

### Resigning

**Resign** appears below **Hint** and **Undo step** and in **Next roll**. Choose a single, gammon, or backgammon resignation in the movable dialog. It uses the current cube to show the points awarded if accepted, the projected score, the target score for point matches, and whether the match would end. GNUbg decides whether to accept and supplies the resulting score, SGF, and move history. Cancellation or refusal preserves the checkpoint and unconfirmed steps. An accepted resignation opens the usual result dialog. Resolve any pending feedback before resigning.

### Animation and game sounds

The opening roll uses the same dice artwork and animation as regular rolls. When the bot opens, the actual opening dice stay visible while its checker moves play in sequence. Hits animate to the bar and bear-offs to the tray. The decision controls wait for playback, rapid legal source clicks queue in order, and each undo animates its reversal. Reload restores the saved checkpoint without replaying old moves. Leaving Playing stops playback, and reduced-motion mode uses short fades.

Apply **Settings → Playing → Game sounds** to update the current configuration and the browser’s playback preference immediately, including for an ongoing game. Both players use recordings from `sounds/`. `rollingDice.au` plays for rolls, `move.au` for moves and bear-offs, and `hit22.wav` for hits. A new bot double plays `double.au` once when the response dialog appears after checker playback. Loading, resuming, hovering, undoing, and confirming steps whose sounds already played are silent. Audio failure does not interrupt play.

The bundled PCM WAV files preserve the recordings' decoded samples. Run `python3 tools/prepare_play_sounds.py` with FFmpeg installed to regenerate them, then build normally. Playback uses the bundled files directly.

### Play API and persistence

| Method | Endpoint                          | Purpose                                                                                                           |
| ------ | --------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| GET    | `/api/play/sessions`              | List the 100 most recently updated sessions.                                                                      |
| POST   | `/api/play/sessions`              | Start a session with a request such as `{ "human_player": 0, "match_length": 5 }`. `0` is white and `1` is black. |
| GET    | `/api/play/sessions/{id}`         | Read a saved session by ID, including sessions outside the list limit.                                            |
| POST   | `/api/play/sessions/{id}/actions` | Apply an action using the current `version`.                                                                      |
| GET    | `/api/play/sessions/{id}/sgf`     | Download the saved SGF, including the escaped player name.                                                        |

Actions include `opening_roll`, `start_opening_move`, `roll`, `move`, `pass`, `double`, `take`, `drop`, `resign`, `next_game`, `pause`, and `resume`. For `move`, send the zero-based `move_index` from the returned legal move list. For `resign`, send `resign_level: 1 | 2 | 3` during a human Roll or Move decision. `pause` and `resume` change the session's `paused` status and database assignment atomically. The legacy API action `end_session` still closes a money session after a completed game. Stale or competing actions and attempts to play a paused match return HTTP 409. An unavailable decision or declined resignation returns HTTP 422 and preserves the checkpoint.

For native play, BG Tutor restores the saved SGF in an isolated GNUbg subprocess with a three-minute deadline, checks the exact saved GNUbg ID, and runs the bot until the next human decision. It holds no database transaction during engine work. An optimistic revision check saves the SGF and response snapshot together. GNUbg's complete 0-ply legal move list defines the allowed final boards at every bot search depth. The client expands combined notation into individual die steps and checks the result against that list, including maximum dice use, the higher-die rule, and immediate game-winning bear-off.

### Native play tests

With PostgreSQL running and `DATABASE_URL` set as described under [Verification](#verification), run the native integration suite. SQLx creates isolated test databases. The tests cover complete matches, both colors, saved engine settings, forced passes, cube offers and responses, gammons and backgammons, Crawford and post-Crawford games, SGF replay, concurrent actions, and failure recovery.

```sh
GNUBG_BIN=/absolute/path/to/gnubg \
cargo test -p bgtutor --test play -- --ignored
```

The opt-in browser suite uses Playwright's test backend, native GNUbg with seeded randomness, and browser clicks for play actions. Its two full three-point matches use white/bear off right and black/bear off left. They check the hover destination and move for each die, swap dice, use the roll and cube controls, and reload the saved match. Three additional scenarios verify opening rolls and ties across reloads. Three cube scenarios verify real bot offers, taking or passing, and starting the next game. Native SGF evaluations verify 0-, 1-, and 2-ply play after resuming and changing games.

```sh
cd web
BGTUTOR_FULL_MATCH_TEST=1 \
GNUBG_BIN=/absolute/path/to/gnubg \
npx playwright test e2e/play-full-match.spec.ts --output=/tmp/bgtutor-full-match-results --reporter=line
```

## Game results in the move history

At the end of each completed game, the move history shows the winner, points won, the calculation from the result and cube, and the new match score. For example, **Gammon (2) × Cube 2 = 4 points** is a four-point win. This applies to imported matches in Analysis and to completed games in ongoing bot matches. Existing matches show results without reimporting or changing stored positions or evaluations.

A declined double uses the cube value before the offer. Resignations and results known only from the game record are labeled accordingly. If the next game's recorded starting score is the only evidence of points won, the calculation shows the difference, such as **6 − 2 = 4 points**. A win type appears only when the game record supports it.

## Playing feedback modes

New matches default to **Coach**. Choose any mode during setup. **Settings → Playing** stores the default mode, **Open training analysis automatically**, and a separate **Tutor warning threshold**. The threshold defaults to 0.04 and accepts values greater than zero and at most one. New matches capture these preferences and the analysis depth independently of the bot's playing strength. Older sessions saved before feedback modes use Competition behavior.

| Mode            | Feedback during play                                                                                                                                                                                                    |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Competition** | Hides analysis, pip counts, error markers, and PR.                                                                                                                                                                      |
| **Training**    | Shows full equities and win, gammon, and backgammon probabilities. With automatic analysis disabled, use **Hint** to request it.                                                                                        |
| **Coach**       | Shows equity differences and compact error feedback.                                                                                                                                                                    |
| **Tutor**       | Warns before a checker or cube decision at or above the threshold. **Try again**, **Show solution**, and **Play anyway** preserve the first attempt. **Show solution** reveals the evaluation and any checker previews. |

**Change play mode** above the board switches an ongoing match immediately and saves the choice for reload. It preserves the current position and partial checker steps. Edit **Tutor warning threshold** beside it, then leave the field or press Enter to save and recheck any pending warning. The saving indicator confirms success for three seconds. These controls also appear under **Active mode** in decision dialogs. Live changes apply to the current match. The default for new matches and the captured analysis depth are managed separately.

Automatic Coach and Training feedback pause before GNUbg commits the choice and plays the bot. Revealed checker alternatives use the same **Initial**, **Arrows**, and **Final** preview as **Hint**. **Continue** or **Play anyway** confirms the saved choice, even after previewing alternatives or switching modes. A Competition confirmation shows only the pending choice. Reload restores pending feedback, and stale requests or engine failures preserve the checkpoint.

The **Play mode** column in Match database shows the mode and use of learning aids. You can hide or resize it independently of existing column profiles. Opening the original of an ongoing Competition match resumes Playing, including at startup. The original match and its positions become available for analysis when the match ends, or after a completed money game. Switching to Competition clears cached analysis for that source. Explicit independent library copies remain available for review.

**Learning review** distinguishes first attempts, played choices, retries, and assistance, including in mixed-mode matches. First-attempt PR uses `500 × original normalized equity loss / native-relevant decisions` at the positions actually reached. It excludes forced checker moves and applies GNUbg's strict close-cube rule to no-double choices. The review identifies its method and profile separately from GNUbg's aggregate rating of the played match. Mode and assistance metadata are retained for originals, copies, and bookmarked occurrences. The match overview omits PR for matches that used learning aids.

To verify mode switching and feedback with the configured GNUbg and an isolated test backend and database, run the native browser suite.

```sh
cd web
BGTUTOR_FEEDBACK_NATIVE_TEST=1 \
  npx playwright test e2e/play-feedback-native.spec.ts
```

## Engine benchmark

Use **Extras → Benchmark** to measure analysis speed for one or more depths. GNUbg supports 0–4 ply; Open Sage supports verified 1/2-ply. Benchmark engine and depth choices are independent of playing and analysis defaults. The selected depths run in selection order in the background and continue when you leave the view. **Cancel run** also stops the active engine process. Other BG Tutor engine work waits while a benchmark is in progress, and a notice appears across workspaces. Other programs on the machine can still affect the measurement.

The fixed **WC2014 v1** reference is the 11-point match between Akiko Yazawa and Douglas Mayfield, with ten games and 460 checker moves. The app bundles an immutable copy of the unanalysed GNUbg SGF fixture, with a reference ID, version, and SHA-256 checksum. See the [fixture documentation](backend/tests/fixtures/README.md) for provenance and reproduction. Existing evaluations are cleared before analysis.

**Total analysis time** sums the full analysis pipeline for each game, including process startup, loading, checker and cube analysis, and result preparation. Each game starts a fresh GNUbg process. The analysis is cubeful and deterministic, with zero noise and luck analysis disabled. The timer excludes waiting, installation and version checks, and database writes. **Ø analysis time / move** divides the total by all 460 checker moves, including forced moves. It represents the match-wide average. New runs use `gnubg-match-v2-unlimited` with no fixed per-game timeout. Older runs retain the timeout recorded in their protocol snapshot.

**Measure play-turn latency** queues a separate run using the benchmark engine and play-depth selectors (GNUbg 0–2 ply; Open Sage 1/2-ply). The `gnubg-checker-turn-v1` protocol plays 11 fixed checker turns with dice already rolled, sampled from opening, contact, race and bear-off positions in the WC2014 reference. Each sample restores its own SGF checkpoint in a fresh process through the Playing adapter, plays one bot turn, and reads the resulting snapshot. The timer includes preparation, native process startup, checkpoint restoration, version guards, and result processing. Queue wait, installation checks, database writes, network time and animation are outside the timed scope. The history reports the arithmetic mean in milliseconds. Play measurements use the same queue and cancellation controls. They are independent of analysis-time estimates and do not create games in the library.

Every attempt records its date, backend machine and hardware, engine version and executable path, ply depth, reference, protocol, progress, and status. Successful runs add total time and average time per move in milliseconds. History is independent of library matches and profiles. Backend restarts mark interrupted runs as failed. A new run starts its timing from the beginning. During a run, the engine availability indicator reuses the verified installation snapshot to avoid starting a competing probe.

| Method | Endpoint                      | Purpose                                                                             |
| ------ | ----------------------------- | ----------------------------------------------------------------------------------- |
| GET    | `/api/benchmarks/reference`   | Read the versioned reference and measurement protocol.                              |
| GET    | `/api/benchmarks/status`      | Read the active status shown in the global notice.                                  |
| GET    | `/api/benchmarks`             | List the latest 1,000 attempts, newest first.                                       |
| POST   | `/api/benchmarks`             | Queue attempts with a request such as `{ "engine_id": "gnubg", "plies": [0, 2] }`.  |
| POST   | `/api/benchmarks/play`        | Queue a play-turn run with `{ "engine_id": "bgsage", "plies": 2 }`; omitted body preserves the legacy saved-play-default behavior. |
| POST   | `/api/benchmarks/{id}/cancel` | Cancel a queued or running attempt. Finished results are retained.                  |

Open Sage uses depth-specific `bgsage-match-v3-{depth}ply-4threads` and `bgsage-checker-turn-v3-{depth}ply-2threads-gnubg-rules` protocols. A fresh sidecar starts once per run and is then reused. Total elapsed time includes that one initialization plus the measured application pipeline; initialization is also shown separately. Each analysis game is an atomic block with complete installation verification before and after it; no result is returned or persisted before the final check succeeds. Individual requests retain their own boundary checks. Hashes are reused only within one check, never across checks. Playing retains one GNUbg rule process from checkpoint inspection through action execution, then terminates it before committing the result. Playing timing includes preparation, SGF restoration and action execution. Historical v2 results remain available, but are not pooled with v3 timings. Once three complete runs share the same machine, provider, full installation/model/rules snapshot, profile and workload, the view shows the median and minimum–maximum range of the three latest runs. Times measure application execution, not engine playing strength. ETA calibration never borrows GNUbg timings for Sage.

The server creates all attempts for a multi-depth request together before starting them. New requests during a benchmark return HTTP 409. Unavailable engines and invalid depths return HTTP 422. OpenAPI documents the endpoints and types.

## Study

**Study** contains **Flashcards & Anki**, **Reports**, **My recurring mistakes**, and **Practice history**. Use Left/Right to switch tabs and Home/End to jump to the first or last. Opening a deck from Positions selects **Flashcards & Anki** and preserves your deck selection, cards, and progress.

### Reports

PostgreSQL stores each completed learning review as an immutable snapshot with its SGF. Point-match reports appear individually. Each completed money game has its own report with its game number and cumulative score at the time. The money session header shows the current total from the session, in player order, and its **Ongoing** or **Ended** status. Open individual reports from the games listed beneath the header in game-number order. The archive sorts sessions by their newest report and loads entire sessions a page at a time, including sessions beyond the 100-entry Playing list. It refreshes when you return or select **Refresh reports**.

The overview shows date, match name, score and match length, play mode, learning aids, analysis depth, and first-attempt PR when available. The archive contains finished games with saved reviews and remains accessible after the corresponding library match is deleted.

**Open report** shows first attempts, corrections, and retries in the same movable dialog used at game or match end. In Playing, **Learning review** is next to **Download SGF** in the move history. Creating a review there opens it automatically. In Analysis, **Learning review** appears beside the match name when a saved report exists. It loads the report on demand and preserves the current board. Opening a saved report reads stored values without running the engine or updating the session.

The dialog uses the same title grip and keyboard behavior as **Hint**. Summary statistics sit above the scrollable decisions and attempts. On short screens the full report scrolls below the title. **Close** or Escape returns focus to the control that opened it.

Select a decision to see the original position and the saved recommendation. The board uses your chosen theme. **Initial**, **Arrows**, and **Final** show the starting position, move arrows, and projected result. Use the arrow keys or Home/End to move through decisions, and Enter or Space to select one. On narrow screens the preview is brought into view. **Decisions and attempts** returns you to the selected row.

Checker decisions are matched by game, color, dice, and native checker decision number. A pre-roll no-double decision shows the board with no dice. Cube actions show their original cube context and the recommended action. Missing positions and moves that cannot be projected legally are marked. The first selection loads the versioned archived positions, which later selections reuse. Previewing requires neither a library entry nor a new engine job.

| Method | Endpoint                                    | Purpose                                                                                                                                                                                         |
| ------ | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET    | `/api/study/reports`                        | List compact summaries with cursor pagination. With `group_by=session`, the limit counts complete money sessions and individual point-match reports. Cursors are specific to the grouping mode. |
| GET    | `/api/study/reports/{id}`                   | Read a saved report.                                                                                                                                                                            |
| GET    | `/api/study/reports/{id}/history?version=N` | Read archived board positions with a version check.                                                                                                                                             |

Summaries include `session_id`, the current `session_scores`, and `money_session_ended`.

### My recurring mistakes

**Study → My recurring mistakes** groups a selected player's errors by game plan and decision type. The groups cover checker play, missed doubles, premature doubles, wrong takes, wrong passes, and cube errors that cannot be assigned to one of those types. Player matching uses the full name, ignores case and surrounding spaces, and includes both colors. Combine database, game plan, and minimum-loss filters. The minimum defaults to 0.08 normalized equity. The overview reports occurrence count, distinct match count, and total equity loss separately.

For each game, the deepest completed local analysis with a current source and a saved game checkpoint takes precedence. The newest wins at equal depth. A qualifying local result takes precedence even if it records no errors. This prevents overlapping analyses from counting decisions more than once. Games without a qualifying local result use imported **XG** or **BGBlitz** analysis when available. Source depths keep their own units. Removed source matches, forced or unevaluable decisions, and ongoing Competition games are excluded. Completed money games are eligible.

Imported checker decisions require both the best and played moves to be verifiable legal candidates. Loss is calculated from comparable normalized XG equities or explicit BGBlitz EMG values. Match-winning chances (MWC), unknown units, and BGBlitz placeholders such as −99 are excluded from the calculation. Cube decisions use the perspective of the player making or answering the offer. Engine, source format, method, and original depth remain visible, including XGRoller++ and rollout labels. Missing depth is reported as unknown.

**Practice** opens up to 20 distinct questions from a group, highest loss first. The board shows the original score, dice, and cube for checker decisions. For a take or pass it centers the offered cube. A pre-roll cube question hides the subsequent roll. **Reveal answer** loads the saved best and played decisions, the loss, and the analysis source. **Next** advances to a hidden answer. Closing practice, changing filters, or leaving the tab ends the exercise and discards late responses. This exercise creates no flashcards or FSRS reviews.

**Positions → More filters → Game plan** searches the collection for any of the five game plans from the perspective of the player on roll. Saved searches and profiles retain the filter. BG Tutor classifies older positions in a versioned, indexed cache without engine work. Large collections are prepared in bounded steps, and the app prompts you to search again if preparation is still in progress. Saved positions remain searchable after the source match is removed.

`GET /api/study/mistakes` lists the groups. `POST /api/study/mistakes/practice` returns questions with the answers and played notation hidden. `POST /api/study/mistakes/reveal` checks the source and precedence again before revealing the answer. Local references carry `job_id`, imported references carry `imported_analysis_id`, and both identify the game and decision. Older libraries fill gaps in the import and move catalogs in bounded steps as needed. Current libraries use saved move contexts. OpenAPI documents all parameters and response types.

## Configuration profiles

The header shows the active profile and marks it **modified** when your current settings differ from its saved values. Select a profile to switch or choose **Manage profiles** to open **Settings → Profiles**.

Choose **Show changes** in the profile menu to see each changed setting with its category, saved value, and current value. The arrow next to a current value restores that field from the saved profile and saves the current configuration. Related values such as a dice pair or a numeric or date range are restored together. The dialog shows any errors. The **modified** marker disappears after the last change is reverted. **Close** or Escape dismisses the dialog.

**Reset all changes** restores the entire saved profile in one operation after confirmation. **Cancel** or Escape dismisses the confirmation. A failed restore preserves the current configuration. After success, the changes dialog stays open with an empty list.

Profiles capture engines and default depths, playing and training settings, language, sound, interface and board appearance, opening-book references, and applied search filters, sorts, and table layouts. The profile summary shows saved values and differences from the current settings.

**Apply** saves the current configuration. To replace a named profile's saved values, choose **Overwrite with current configuration** and confirm. When switching with changes, choose **Save and switch**, **Discard and switch**, or **Cancel**. **Save as new profile** creates and activates a profile from the current configuration. Activate a different profile before deleting the active one.

On the first launch after the profiles update, existing server and browser preferences are imported once into **Standard**. Profiles and current settings apply across the installation, including other browsers. On a version conflict, you can load the server version or save your local changes as a new profile. A recovery journal protects pending local changes.

Running analyses and matches retain their captured settings. New jobs and matches use the current configuration. Switching profiles only applies settings. Engine installation paths are device-specific, and book files are stored as references. Activation checks the referenced resources and reports missing items. See the [profile specification](docs/superpowers/specs/2026-11-10-profiles-design.md) and [implementation plan](docs/superpowers/plans/2026-11-10-profiles.md).

## Analysis time estimates

### Before analysis

Before you start a game or match analysis, **Estimated analysis time: approx. …** appears below the scope and depth selectors. It multiplies the number of checker moves in scope by the average time per move from the most recent matching, fully completed benchmark. The UI shows the benchmark date, engine version, ply depth, and checker move count. Changing the match, game, scope, depth, applied profile, or engine installation invalidates an outstanding estimate. The estimate is hidden during submission and execution.

A matching benchmark uses the same local machine and hardware, engine installation and version, and ply depth, with compatible reference and analysis parameters. Completed runs using the former 600-second per-game limit and the current unlimited protocol are both eligible. Calibration requires a successful, complete local run.

The estimate includes process startup and cube analysis. Time spent waiting for other jobs is outside its scope. Different types of positions can take substantially different amounts of time. If the benchmark, checker moves, or engine installation is unavailable, or the request fails, the UI explains the issue and still lets you start analysis. Estimates use existing measurements.

`GET /api/matches/{id}/analysis/estimate?plies=2&game_no=1&engine_id=gnubg` returns the estimate and its measurement basis. Omit `game_no` for the whole match and omit `plies` to use the saved default depth. The endpoint is read-only and observes the source and Competition restrictions.

### During analysis

While a game or match analysis runs, the progress area shows **Estimated time remaining: approx. …**. The initial estimate uses the saved matching benchmark. Once a game completes, the estimate uses confirmed analysis time divided by the number of timed checker moves, multiplied by the number of remaining checker moves. This weights longer games according to their move counts. The display counts down in seconds between completed games, when the engine reports confirmed progress.

Timing uses a monotonic clock and excludes time in the queue or waiting for engine access. Confirmed work is saved with the job and survives reload and resumption. An interrupted game is timed again. Moves already completed by older jobs count toward progress. Only timed moves contribute to the measured average. The job keeps the measurement basis it started with when the user profile changes.

If a game outlasts its estimate, the UI shows **Recalculating time remaining …**. Without a matching benchmark, an estimate can appear after the first usable game measurement. The active estimate disappears when the job completes or fails.

`GET /api/jobs/{id}/remaining-time` returns status, the measurement basis, confirmed and timed move counts, and the current estimate. The read-only endpoint follows the same source and Competition restrictions as the job and starts no engine. Active timing is saved about once per second. The browser counts down monotonically between updates.

## API reference

These endpoints cover common library and analysis operations. The complete contract, including parameter names and result schemas, is in [OpenAPI](docs/openapi.json). Generate it and the TypeScript types used by the client directly from the Rust handlers. The generator runs without a server.

```sh
npm run api:types --prefix web
```

### Library and position search

| Method       | Path                                                             | Purpose                                                                                                                                                                       |
| ------------ | ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| POST         | `/api/imports?database_id=`                                      | Multipart field `file`. HTTP 201 for a new match, 200 for a duplicate in the target database. Omitting `database_id` uses **Unsorted**.                                       |
| GET / POST   | `/api/databases`                                                 | List databases with match counts or create a database.                                                                                                                        |
| PUT / DELETE | `/api/databases/{id}`                                            | Edit with `version` or delete a user-created database. Deletion requires `version` and `delete_matches=true` when it contains matches.                                        |
| DELETE       | `/api/databases/{id}/matches?version=`                           | Empty a system database while retaining it. Returns HTTP 204 on success and 409 for a stale version, an ineligible database or active analysis.                               |
| GET          | `/api/library/matches`                                           | Filter and sort matches or occurrences before pagination. Limit 1–200. The cursor is bound to the filters and sort.                                                           |
| GET / PUT    | `/api/matches/{id}/metadata`                                     | Read or update match details with version checks.                                                                                                                             |
| POST         | `/api/library/actions`                                           | Bulk action with `action`, `match_ids` and optional source or destination database and tags. Returns individual outcomes.                                                     |
| POST         | `/api/library/export`                                            | Export `match_ids` with `format` set to `original` or `sgf`. Returns a file or ZIP.                                                                                           |
| GET          | `/api/matches?q=&after=&limit=`                                  | Match search. Limit 1–100.                                                                                                                                                    |
| GET          | `/api/matches/{id}`, `/api/matches/{id}/source`                  | Match summary or canonical SGF download.                                                                                                                                      |
| GET          | `/api/matches/{id}/games`                                        | Game catalog with starting scores, Crawford status and move counts.                                                                                                           |
| GET          | `/api/matches/{id}/dice?game_no=`                                | Observed roll frequencies, optionally for one game.                                                                                                                           |
| GET          | `/api/matches/{id}/games/{game_no}/decisions`, `/api/moves/{id}` | Stable game and move IDs and recorded actions with their pre-action positions.                                                                                                |
| GET          | `/api/positions`, `/api/positions/{id}`                          | List match positions or find exact board matches. Filters include `database_id`, `match_id`, `game_no` and `position_id`. A direct lookup includes the match and game number. |
| GET          | `/api/position-browser?filters=&after=&limit=`                   | Search checker decisions in context. `filters` is URL-encoded JSON with `schema_version` set to 1.                                                                            |
| GET / POST   | `/api/position-filters`                                          | List or create saved searches.                                                                                                                                                |
| PUT / DELETE | `/api/position-filters/{id}`                                     | Update or delete a saved search with `expected_version`.                                                                                                                      |

```sh
curl -F file=@backend/tests/fixtures/tutorial.mat http://127.0.0.1:8787/api/imports
curl 'http://127.0.0.1:8787/api/matches?q=Tutor'
```

### Engines and analysis

| Method     | Path                                                                         | Purpose                                                                                                                                                                                                                    |
| ---------- | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET        | `/api/health`, `/api/engines`                                                | PostgreSQL and GNUbg health, or available engines, versions and depths.                                                                                                                                                    |
| GET / PUT  | `/api/settings`                                                              | Persistent engine and player settings with revision checks.                                                                                                                                                                |
| POST       | `/api/engines/gnubg/check`                                                   | Check the installation using `{ "path": "..." }`.                                                                                                                                                                          |
| GET / PUT  | `/api/opening-book`                                                          | Provider, local BGDB and XG paths, availability and version checks.                                                                                                                                                        |
| POST       | `/api/opening-book/check`, `/api/opening-book/install`                       | Validate or atomically install a local BGDB file.                                                                                                                                                                          |
| POST       | `/api/opening-book/xg/check`, `/api/opening-book/xg/install`                 | Validate or install an official XG book file.                                                                                                                                                                              |
| GET        | `/api/positions/{id}/opening-book`                                           | Context-specific book result or explicit miss, with a separate XG position reference.                                                                                                                                      |
| GET        | `/api/matches/{id}/imported-analysis`                                        | Source analysis provenance and coverage.                                                                                                                                                                                   |
| GET        | `/api/positions/{id}/imported-analysis`, `/api/moves/{id}/imported-analysis` | Original checker or cube analysis.                                                                                                                                                                                         |
| POST       | `/api/jobs`                                                                  | Submit a position analysis with `{ "position_id": "<position UUID>", "kind": "analysis", "plies": 0 }`.                                                                                                                    |
| GET / POST | `/api/matches/{id}/analysis`                                                 | Get the latest job or submit a new one. Use `game_no` in the query for a game lookup or in the JSON body to submit a game job. Omit it for the whole match. For example, submit `{ "plies": 0, "game_no": 2 }` for game 2. |
| GET        | `/api/jobs/{id}`, `/api/jobs/{id}/events`                                    | Job state, progress, result and error, or persistent SSE updates.                                                                                                                                                          |
| GET        | `/api/jobs/{id}/moves`                                                       | Cursor-paginated evaluations filtered by game, player, checker or cube, and severity.                                                                                                                                      |
| GET        | `/api/jobs/{id}/statistics`                                                  | Native match and game statistics with coverage, PR where available, and `error_breakdown`.                                                                                                                                 |
| GET        | `/api/matches/{id}/move-assessments?game_no=&job_id=`                        | Stored checker evaluations for a game with source and depth. Optionally restrict to one native job.                                                                                                                        |

### Notes and flashcards

| Method       | Path                                                                                                                         | Purpose                                                                                                                                                                                           |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET / PUT    | `/api/matches/{id}/annotation`, `/api/positions/{id}/annotation`, `/api/games/{id}/annotation`, `/api/moves/{id}/annotation` | Read or save versioned annotations.                                                                                                                                                               |
| GET          | `/api/annotations/search`, `/api/tags`                                                                                       | Search notes or suggest tags.                                                                                                                                                                     |
| GET / POST   | `/api/decks`                                                                                                                 | List or create decks.                                                                                                                                                                             |
| PUT / DELETE | `/api/decks/{id}`                                                                                                            | Rename using `name` and `expected_version` in the body, or delete with `expected_version` in the query. Deletion returns HTTP 204 on success, 409 for a stale version and 404 for a missing deck. |
| GET          | `/api/cards`, `/api/decks/{id}/cards`                                                                                        | Search all cards or a deck with the same filters, total count and UUID pagination.                                                                                                                |
| POST         | `/api/decks/{id}/cards`, `/api/decks/{id}/positions`                                                                         | Add one decision or errors from a completed analysis, or add up to 200 selected position UUIDs with individual outcomes.                                                                          |
| GET          | `/api/decks/{id}/next`, `/api/decks/{id}/export.apkg`                                                                        | Fetch the next due card or download an Anki package.                                                                                                                                              |
| POST         | `/api/cards/{id}/reveal`, `/api/cards/{id}/review`                                                                           | Reveal a card's answer or submit a review.                                                                                                                                                        |

## Analysis jobs

GNUbg supports 0–4 ply for position, game and match analysis. The default analysis depth is 2-ply. A job captures its engine configuration and depth. Identical active jobs are deduplicated by scope, depth and engine configuration. Each backend runs one worker, and PostgreSQL `FOR UPDATE SKIP LOCKED` keeps workers from claiming the same job.

Jobs move through `queued → running → completed|failed`. GNUbg has a 90-second process limit for position analysis and MAT conversion and a 600-second limit for a game analysis. The worker's 120-second lease renews every 20 seconds. After expiration, the job can be claimed again, up to three attempts in total. A regular engine error marks the job `failed` and the UI can submit a new job.

SSE event types are `queued`, `running`, `progress`, `completed` and `failed`. The `id:` field is a durable event number and `data:` contains `JobProgress` JSON. `Last-Event-ID` resumes after that event. With no header, the server replays the event history and then sends keepalives until the job ends. SIGINT and SIGTERM close streams and terminate running child processes. On restart, the worker can reclaim expired jobs. The browser remembers recent position jobs and reconciles SSE with polling. It loads the latest match or game job from PostgreSQL, including after a browser reload.

Match analysis checkpoints decisions, partial statistics and SSE progress for each completed game in one transaction. `Job` and SSE report `completed_games` and `total_games`, so the percentage advances when a game finishes. After an expired lease, a replacement worker resumes at the next incomplete game. An expired worker cannot write checkpoints or results. Completed games and their error lists remain accessible after a later game fails. Submitting a new job analyzes the requested scope from the beginning.

Native evaluation uses GNUbg's `analyse game` output at six decimal places and includes the played move even when it falls outside the top five candidates. BG Tutor clears existing analysis annotations before the calculation, normalizes inherited match data and setups, and verifies board and match context for each checker and cube action. Mixed SGF collections can include empty or cube-only games. Missing or incomplete native exports fail the job. Each native export is capped at 64 MiB.

A forced pass has zero loss and no alternative equity (`evaluation_status=forced`). Normalized loss thresholds are 0.04 for `doubtful`, 0.08 for `bad` and 0.16 for `very_bad`. The `performance_rating` field includes the counts of relevant decisions and weighted native GNUbg PR. Cube equities use the deciding player's perspective and the cube value before the offer as their unit. Cube-only decisions have stable source action IDs and are accessible from **Cube and move notes** with their pre-action board.

## Verification

Run the checks relevant to the change from the repository root. Install the frontend dependencies with `npm ci --prefix web` before running frontend checks. The workspace test suite includes non-ignored SQLx tests, so configure `DATABASE_URL` as described below before running it. Use `cargo test --workspace --lib` to run the Rust library tests without those integration targets.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix web
npm run test:themes --prefix web
npm run build --prefix web
```

### PostgreSQL and GNUbg integration tests

Start PostgreSQL with `docker compose up -d --wait db` or use a dedicated test server. Set `DATABASE_URL` to a connection on that server with permission to create databases. SQLx creates and drops separate test databases. Set `GNUBG_BIN` to a working GNUbg installation with its neural-network weights and embedded Python support for native play tests.

```sh
cargo test --workspace -- --ignored --nocapture --skip all_local_game_files_import_atomically_and_deduplicate
```

Use `cargo test -p bgtutor --test <target> -- --ignored --nocapture` for an affected integration test target. Integration coverage includes import and deduplication, game catalogs and older matches, scope-bound cursors, SSE replay, worker claims and lease recovery, Dice Distribution, and native checker and cube analysis. The reference match exercises 10 games and 460 moves. Native depth tests check actual 3- and 4-ply engine output and game and match exports. Parser tests compare XG positions with the same match imported from MAT through GNUbg.

The search performance test builds 50,000 synthetic matches and 250,000 positions in an isolated database. It uses `EXPLAIN (ANALYZE, BUFFERS)` to verify a GIN trigram index for a selective player-name search and a B-tree index for an exact Position ID search. Each query returns one row. On November 8, 2026, the warm test queries took 0.279 ms and 0.036 ms respectively in PostgreSQL. These measurements cover database execution only.

### Optional local import corpus

The corpus test reads `games/xg`, `games/mat`, and `games/bgf` by default. Set `BGTUTOR_GAMES_DIR` to use a different parent directory. It sends the files through the multipart API in its own SQLx test database and skips when the local files are absent.

```sh
BGTUTOR_CORPUS_REPORT=/tmp/bgtutor-import-results.json \
  cargo test -p bgtutor --test import_corpus -- --ignored --nocapture
```

The November 8, 2026 rerun imported 33 of 42 files and rejected nine invalid XG files. The earlier 37-of-37 result used a different corpus with 17 distinct contents and 20 recognized duplicates. The corpus test requires every supplied file to pass, so the documented rejected files cause it to fail when present. See the [import corpus report](docs/import-corpus-results.md).

### Browser tests

Playwright requires Google Chrome, `psql`, PostgreSQL, and the frontend dependencies. GNUbg workflows require a working local engine. The harness creates a fresh database, verifies that the API is using it, starts a dedicated backend and Vite on local ports, and removes the test database after the run. Screenshots and other test artifacts are written to `web/test-results/`.

```sh
npm run test:e2e --prefix web
```

The default PostgreSQL connection targets the Compose instance. For another server or port, set `BGTUTOR_E2E_POSTGRES_URL` to a connection with `CREATEDB`. For tests that need prepared data, set `BGTUTOR_E2E_POSTGRES_TEMPLATE` to a dedicated fixture database named `bgtutor_e2e_template_*`. The harness copies the template for each run. Unset the obsolete `BGTUTOR_TEST_URL` before running Playwright.

Coverage includes import and analysis, game selection and move pagination, search and filters, saved notes and revision conflicts, deck creation, review and Anki exports, themes and all six locales, and selection changes while responses are in flight.

The opt-in analysis estimate test covers all five ply depths for both game and match scope. It can reuse compatible benchmark runs from the test database template. When measurements are missing, it runs native benchmarks, which can take substantially longer than the standard browser checks.

```sh
BGTUTOR_NATIVE_ESTIMATE=1 npm run test:e2e --prefix web -- e2e/analysis-time-estimate-native.spec.ts
```

## Repository layout

| Path                                                                                                               | Responsibility                                                      |
| ------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| [`backend/src/api.rs`](backend/src/api.rs)                                                                         | REST, SSE, and OpenAPI                                              |
| [`backend/src/import/`](backend/src/import/)                                                                       | Import, rule validation, and GNUbg identifiers                      |
| [`backend/src/engine.rs`](backend/src/engine.rs), [`backend/src/match_analysis.rs`](backend/src/match_analysis.rs) | Isolated GNUbg processes and native checker and cube analysis       |
| [`backend/src/jobs.rs`](backend/src/jobs.rs), [`backend/src/match_jobs.rs`](backend/src/match_jobs.rs)             | Job state, progress events, game checkpoints, and player statistics |
| [`backend/src/store.rs`](backend/src/store.rs), [`backend/migrations/`](backend/migrations/)                       | SQLx persistence, schema, and search indexes                        |
| [`web/src/`](web/src/)                                                                                             | React client, SVG board, and generated API types                    |

The Rust backend runs independently of the browser. A future Tauri shell could start it as a sidecar and reuse the HTTP API.
