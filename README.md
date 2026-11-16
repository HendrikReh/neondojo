# BG Tutor

BG Tutor is a local backgammon application for importing matches, finding positions, analyzing decisions with GNU Backgammon (GNUbg) or Open Sage, playing against an engine, and practicing your own mistakes. Its headless Rust backend uses Tokio, Axum, PostgreSQL, and SQLx. The React and TypeScript client connects through REST and server-sent events (SSE).

[Quick start](#quick-start) · [Restart on port 8787](#restart-bg-tutor-locally-on-port-8787) · [Analyze a match](#analyze-your-first-match) · [Import formats](#import-formats) · [Playing](#playing-against-gnubg) · [Study](#study) · [HedgeHog comparison](#hedgehog-comparison) · [API](#api-reference) · [Verification](#verification) · [Repository guidance](AGENTS.md)

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

## Analysis workspace

The header lists **Playing**, **Analysis**, **Study**, **Positions**, **Match database**, **Player database**, and **Extras** from left to right, with **Settings** on the right. With a header tab focused, Left and Right select the previous and next tabs. Home selects **Playing** and End selects **Extras**. The app opens **Analysis** by default. The **Study** workspace is named **Lernen**, **Estudio**, **Étude**, **Nauka** and **Studio** in German, Spanish, French, Polish and Italian.

Arrow keys and Home/End also navigate the detail tabs. The app restores the last match, game, and position locally. Switching tabs preserves filters, note drafts, and SSE subscriptions. A submitted job continues to be tracked if you change the selected game or position while the request is pending.

The desktop **Checker** places move history on the left and position evaluation beside the board. Long lists scroll independently and keep the current move visible. Dedicated tabs include **Statistics**, **Cube**, **Notes**, and **Study cards**. The header shows the players, match ID and length, saved score, date, event, and PR per player. Date and event come from saved metadata and show `n/a` when unavailable. PR is shown with one decimal throughout the app. The header also shows the round and location when saved. Match PR updates when a complete native match analysis finishes.

In **Checker → Dice equity**, BG Tutor checks the selected position for a saved
result or a queued or running job. If none exists, it starts a calculation with
the engine and search depth from the active analysis profile. Selecting another
position does the same while **Dice equity** is visible. After a submission or
engine failure, select **Dice equity** again to retry.

The position pane shows each result with its source and recorded depth. To
choose a depth and calculate again, use **Recalculate position …** or
**Recalculate dice equity …** in the header's **Analysis options**.

**Analyze game** stays visible in the Analysis header and starts the selected
game. Its options include the engine, depth and time estimate. Start a
whole-match analysis from the selection toolbar in the **Match database**.
For one match, the dialog shows the engine, depth and estimate before you
confirm. In Analysis, **Displayed analysis** determines which saved results
the decision reviews and **Statistics** show. Running jobs stay
visible as you switch tabs or positions.

### Moves, errors, and statistics

Move history shows all checker moves in the selected game by default, including those without an evaluation. Choose **All errors** or a specific grade to narrow the list without changing the board position. The filter uses the saved assessments and the configured error grades. Each row shows notation, player, dice, and localized equity loss. A zero loss is shown as `0.000` in English, and a dash indicates that no evaluation is available. Hover over the loss to see the grade, analysis source and depth. With **All moves** selected, recorded doubles, takes and passes appear as lines between the checker moves, with the player who acted. Select one of these lines to open its saved review in **Cube**; a line without a saved evaluation shows `Not evaluated`. While **Cube** shows one of these decisions on the board, its line is marked in place of the checker move.

**Cube** reviews only cube decisions. **Inaccuracies only** starts at the
configured **Questionable** threshold. Turn it off to include all cube
decisions, or use **Reset filters** to restore the default. Filter the review by
player, severity and game. A match review can cover all games or one game.

In **Statistics**, checker and cube metrics open the corresponding tab with the
metric's player, severity and game scope. Metrics that combine both decision
types let you choose **Checker** or **Cube**. A checker selection adds review
controls beside the candidate analysis and can span the full match. Close the
statistics selection to return to the usual checker view. The move history
continues to follow the selected game.

In **Analysis → Statistics**, open **Compare saved analyses** and choose any two catalog entries to see their saved results side by side. Each entry retains its engine, depth, scope and coverage. This comparison leaves **Displayed analysis** unchanged, and GNUbg PR is displayed only where that engine recorded it.

The selected decision shows the played and best actions, the loss, and the analysis source. Cube review includes missed doubles and double, take, and drop decisions. Game buttons in **Statistics** open that game. Native PR uses GNUbg job statistics. New jobs save complete per-game statistics atomically. Older jobs can show totals reconstructed from their decisions, with per-game PR shown as unavailable where it was never saved.

### Candidate comparison and decision review

Candidate tables share a compact layout for local, imported and opening-book results.
They identify the best and played moves separately from the current preview, show
comparable loss to the best result, and show win and loss probabilities for the
total and for each separate outcome: single, gammon, and backgammon. Win is on
top and loss below within each cell. Percentages use one decimal place. Missing
or inconsistent probabilities appear as dashes.
Equity differences stay within the same source and known equity unit; MWC differences
use percentage points. Unknown or incompatible values remain unavailable. Engine,
depth, unit and player perspective stay visible, including differing candidate depths.
The table ranks imported XG checker candidates by descending equity and shows
each candidate's original evaluation depth. Ties keep their file order.

In **Analysis → Checker → Compare moves** (German **Zugvergleich**), plays that
reach the same board share a row across saved eXtreme Gammon, GNU Backgammon and
Open Sage analyses. Each engine column shows its rank, equity and actual
candidate depth. Use the column selector to choose another saved analysis.
Select a row to show each step on the board in **Arrows** view. The preview
uses the recorded XG path when that candidate is available, followed by the
first local source otherwise.
Select an engine's rank, equity or depth cell to use that engine's move. If
the source stores only the endpoints, the preview reconstructs a legal route.
New GNU Backgammon analyses retain each original route available in the native
SGF output alongside the display notation. Open Sage's current checker API
returns candidate end boards, so its move notation is derived from those boards.

In **Cube**, **Previous error** and **Next error** above the board also work
without a local analysis job. Review uses saved local
game checkpoints from the selected job and imported evaluations
for games not covered by that job. Each decision identifies its source and depth.
Imported evaluations do not
supply native GNUbg PR or imply a complete native match analysis. The board shows
the filtered scope and the current result's index. Both directions
cycle through matching results across game boundaries when all games are selected.
When the error threshold is off, the board navigates all matching decisions.
The board and the filter pane explain why there are no matching decisions.
Changing selection or filters cancels pending navigation. Cube errors open on the main board with the played
action, best action, loss and source. The inspector shows the recorded cube
options with their unit and the name of the player whose perspective is used.
Older local results may only provide the saved played and best equities.
For imported decisions, **Cube details** opens the original evaluation and notes.

`GET /api/matches/{id}/review-decisions` supplies this paginated view. Only
evaluations with known units and a validated decision context enter imported
review. GNUbg SGF checker evaluations are supported; SGF cube records without
unambiguous units and perspective remain visible in their original source view
but are excluded from the combined error review. A missed double opens the
validated position before the roll. A take or drop shows the outstanding cube
offer from the original decision context.

### Cube context, take point and move explanations

Cube review, cube practice results, flashcards and Playing feedback describe a cube decision in words: match length or money rules, the points each player still needs (for example `Alice 7-away, Bob 6-away`), Crawford or post-Crawford, and the cube value with its owner or the pending offer. Offers list all three outcomes (**No double**, **Double / take**, **Double / pass**) with their unit and the player whose perspective is used, and mark the outcome reached when both sides act correctly. A position that is too good to double is labeled. A response lists **Take** and **Pass**.

New native GNUbg and Open Sage analyses also save the cubeless win, gammon and backgammon chances of each cube decision, and the table shows them for both players. Analyses saved before this change show a note until the game is recalculated; imported analyses do not record them.

The take point is the game winning chance at which taking breaks even, ignoring gammons and any later use of the cube. In money play it is 25 %. In match play BG Tutor derives it from the Kazaross-XG2 match equity table that GNUbg ships, using the Crawford entry when a result reaches a one-away score. The line names the table and, when chances are saved, how often the taker wins here. No take point is shown in the Crawford game, when the player cannot double, or beyond 25-away. `POST /api/cube-context` takes a GNUbg match ID and the doubler and returns this context. The table and its notice are in [met_kazaross_xg2.json](backend/src/met_kazaross_xg2.json).

When the played or chosen checker move is not the best one, **What the plays do differently** lists up to three counted differences between the two resulting boards: opponent checkers hit, opponent rolls out of 36 that hit a blot, points only one play holds, the longest row of adjacent points, blots left, and gammon chances when both plays have saved probabilities. It appears under the candidate table in Analysis, in Coach and Tutor details, and in checker practice results. `POST /api/move-explanations` counts these facts from the board, mover, dice and both notations. The counts describe the plays; the engine's equity decides which is better. **Terms** below the list explains equity, equity loss, PR and ply.

### Preview a move

**Initial**, **Arrows** (German: **Pfeile**), and **Final** show the starting position, the move path, and the resulting position. The default view uses outlined green arrows for the played move and shows legal intermediate points. Select a candidate row from an import, BackgammonDB, or local analysis to preview it. Use Enter or Space on the move name. Up/Down selects the previous or next candidate and keeps it in view. Left shows **Initial**, Right shows **Final**, and other controls keep their normal keyboard behavior. The selected board view persists when changing candidates. **Reset preview** returns to the recorded move.

A checker that uses multiple dice is usually shown by its start and end points, for example `24/15`. Hits remain explicit, for example `24/21*/15`. **Arrows** shows each die as a separate step and follows the recorded route when the source supplies it.

The checker table marks the played move and the best move. The loss comes
from the same source as the best move.

The board identifies the candidate source and rank. **Final** keeps the board orientation and hides the dice. All themes use the same legality checks for bar entry, hits, multi-step moves, doubles, and bear-off. Unrecognized or illegal notation shows the starting position. Analysis, notes, position IDs, and study cards continue to use the pre-move decision. Changing position or source clears the previous candidate preview.

### Search from the analysis board

In Analysis, **Save position** and the magnifying glass share a toolbar above the board. The magnifying glass to the right of the bookmark shows **Search this position** on hover or keyboard focus. On narrow screens both icons move to the next line together.

Select **Search this position** to open Match database and run an exact search for the displayed position. The position ID appears in the expanded search form and as an active filter. The action selects **All databases**, clears the other filters, and keeps the current sort order. Selecting the action again reruns the search. Switching tabs preserves any subsequent decision to remove the filter.

### Desktop layout

**Checker** places the board beside the checker candidate table. Open **Cube**
to review cube decisions on the board and **Statistics** for native match and game
statistics. Switching tabs keeps the selected game and move.

Drag the divider beside the board to resize it in **Checker**, **Statistics**,
**Cube**, **Notes**, or **Study cards**. The divider also accepts Left/Right
arrows, with Shift for larger steps. Double-click the divider or press End
to set the board to its maximum width for the current window. Enter restores
the default width. The profile stores separate widths
for **Checker**, **Cube**, **Statistics**, **Notes**, and **Study cards**.
On wide screens, the move history has room for more rows. **Statistics** uses
larger table text and side-by-side player summaries when there is room.
The comment field in **Notes** grows with the available height. **Study cards**
uses larger deck controls and a taller list with larger board previews. Every
analysis tab scales the artwork to fit above the navigation controls.
On wide displays, each analysis tab can use a board column up to
1680 logical pixels wide. The checker candidate table uses more of its pane on
wide displays, with larger text and row spacing when there is room.

The desktop layout is designed for side-by-side review using a mouse or keyboard. Layout checks use logical viewports of 1800×1169, 2560×1440, and 3840×2160, including under macOS **More Space**. The shared type scale uses 24 px page titles, 14 px section titles, 13 px controls and body text, and 12 px tables. CSS tokens define the type scale, with monospace for position IDs and separate type for the board and logo. [Typography](docs/typography.md)

Text tabs use the active theme's accent and a 2 px underline. The same pattern covers workspace and analysis tabs, board views, analysis sources, checker and cube selection, note scopes, settings, and database selection. [Navigation style](docs/ui-navigation.md)

## Settings and engines

**Settings** opens a dialog with **General**, **Appearance**, **Analysis**, **Playing**, **Board**, and **Profiles**. The **General → Language** menu lists Deutsch, English, Español, Français, Italiano, and Polski alphabetically. New installations default to English. **Board** offers previews of Forest, Walnut, Slate, and local BGBlitz themes from `themes/*.zip`. **Apply** saves the draft and keeps the current category open. **Close** returns to the workspace. Closing the dialog, pressing Escape, or opening Profiles, Opening books, or Benchmark with unapplied edits offers **Apply**, **Discard draft**, or **Keep editing**. [Configuration profiles](#configuration-profiles)

Set a player name, default analysis depth (0–4 ply, default 2), separate playing strength (0–2 ply), and bear-off direction. New matches use the playing settings in effect at creation. Choose GNUbg or Open Sage independently for analysis and playing. The pinned Open Sage fork supports 1- and 2-ply for play and up to 4-ply for analysis. The original wheel does not pass the 1-ply capability check. New sessions pin the bot, analysis and GNUbg rules installations separately. There is no automatic fallback to another engine.

Open Sage requires a locked native ARM64 Python 3.12 environment. In **Settings → Analysis → Engines on this computer**, enter that environment's `bin/python` path and choose **Verify and save**. The backend checks package and model integrity and runs a real evaluation before enabling selection. Installation paths stay on the host. GNUbg handles MAT conversion and provides the play rules, dice, and SGF handling. Open Sage makes bot decisions when selected. Open Sage 1-ply requires a successful capability check with a bounded thread count. Open Sage 3- and 4-ply are available for retrospective analysis and engine duels. New jobs and duels probe the requested profile before starting. [Local Open Sage setup](docs/open-sage-local.md)

**Settings → Analysis** shows the default analysis engine and depth, the error thresholds, and the configured opening-book source. Set the four inclusive lower bounds in normalized equity loss. They must be strictly increasing, greater than zero, and at most one.

| Grade | Default threshold |
| --- | ---: |
| Questionable | 0.005 |
| Error | 0.020 |
| Blunder | 0.080 |
| Large Blunder | 0.120 |

The thresholds are part of the current configuration and can be saved in a profile. Applying changes updates the classification of existing analysis results from their stored equity losses. New matches capture the scale for feedback.

Expand **Engines on this computer** to see the local installation paths and set the maximum calculation threads per engine. The collapsed section shows availability for both engines. The backend detects the logical CPUs available to its process and probes how many of those GNUbg accepts. The dialog offers whole-number overrides within the detected range, starting at 1 for GNUbg and 2 for Open Sage. GNUbg defaults to 1. Open Sage defaults to 2 for play and 4 for analyses and engine duels. These host-local settings apply to new analysis jobs, play sessions and duel series. Each captures the thread count when created. Reference benchmarks retain their fixed thread counts, and their timings are only used for estimates with a matching thread count.

Automatic result readers stay within the selected analysis provider. Historical results and explicitly selected jobs retain their original provider and profile; copying a match retains completed results from both providers. Sage results carry model/profile identity but do not claim GNUbg PR statistics.

The executable path is specific to the backend computer. Installation checks, analysis, playing, and benchmarks use the same resolution rules. An absolute path is resolved directly, a relative path containing a directory separator is resolved from the backend's working directory, and a bare name such as `gnubg` uses the first matching file on the backend's `PATH`. The backend reports an error if resolving or running that file fails. The resolved path must be valid UTF-8.

**Check installation** and saving a changed executable path check the version and run a real position evaluation within a single 10-second time limit. A failed check prevents an invalid path from replacing the saved installation. Installation settings use PostgreSQL revision checks. A concurrent change surfaces a conflict and retains the draft.

A new analysis job records its engine configuration and effective ply. Active-job deduplication includes the target, scope, ply, configured executable path, and version. An engine version change at the same path fails a job that expects the original version and preserves completed game checkpoints. Legacy jobs without a snapshot use the engine configured at backend startup. MAT imports use the currently saved executable path.

Migration 0019 added 3-ply analysis and changed the original unsaved default to 2-ply. Migration 0020 added 4-ply for positions, games, matches, and dice distributions. Explicit profiles, existing jobs, and completed results retain their recorded depth. The requested depth is the GNUbg search profile, which allows early exit when the result is clear. Higher depths take longer, especially for full matches and dice distributions.

New GNUbg 2- and 4-ply analysis jobs save a checker move filter that sends up to the best 14 candidates from the 0-ply evaluation to 2-ply. At 4-ply it also sends up to the best 14 candidates from the 2-ply evaluation to 4-ply. Game and match analyses retain the actual depth of each exported candidate. Existing jobs keep the filter saved at submission or the GNUbg startup filter for older jobs.

## Saved positions and searches

In **Analysis**, choose **Save position** to add the specific pre-move decision and its match context to **Positions**. The collection starts empty and fills through explicit saves. **Remove from collection** works for individual or selected entries. The row's remove icon appears for the row under the pointer, the row with keyboard focus and the previewed row. After a removal, **Undo** above the results puts those positions back. A saved decision can survive deletion of its source match or database, retaining player, roll, score, original match ID, related notes, and position analyses. It can still be analyzed, annotated, and used for study or Anki. Full game and match analysis and original-file export require the source match.

Use **All analyzed decisions** above the collection to search every checker and cube decision of one player in the displayed analyses, whether or not the position was saved. Choose the player, then combine database, decision type, error type, game plan, an equity-loss range, the points the player and the opponent still need, cube value and cube owner. Results are sorted by loss, paged on the server and show the score as away values, the cube, the played and best action, the loss and the analysis source. Select a row to preview the board with its cube context, open a checker decision in Analysis, or choose **Practice results** to practice up to 20 errors of the current page in the recurring-mistakes dialog. `GET /api/study/decisions` supplies the list from the decision index described under [My recurring mistakes](#my-recurring-mistakes). The filters and the page are evaluated in PostgreSQL; the order by loss follows the `decision_index_player_loss` index.

The position table has a fixed preview area on the right. Click or use the arrow keys to select a row, then press Enter, double-click, or use **Open analysis** next to the preview title to open the exact decision. Expand **Source and position details** below the board for the originating database, checker position ID, and analysis details. The table has its own sort and column widths. Its row tooltip gives context and analysis provenance. The sortable **Decks** column shows deck names alphabetically. Equivalent decisions share deck membership when the checker arrangement, roll, score, and cube context match. Membership updates across loaded pages and in the preview after adding positions to a deck.

The default columns show match, game, move, player on roll, dice, score and equity loss. Use **Columns** to show or hide fields, reveal all selectable columns, or restore the default selection. Column choices are saved per profile and hidden columns retain their widths. **Show played moves** controls the recorded move independently. When the current sort column is hidden, the toolbar shows the sort and its direction.

Filter by database, search text, player on roll, match name, event, round, exact tag, comment, checker position ID, match points, roll, score, Crawford, cube value, local analysis, or native equity loss. One die matches either die in the recorded roll. A pair matches either order. For example, to see how Mochy played a specific position, combine **Player on roll** with the checker position ID and roll. **Show played moves** adds the recorded decision to the table and preview. It starts off so answers stay hidden during study.

Applied filters appear below the search controls, including when **More filters** is collapsed. Removing one applies that change immediately. Choose **Search positions** to apply edited fields. **More filters** groups the fields by **Match**, **Position**, **Analysis**, and **Tags and comments**; value ranges and the two dice sit side by side. Sort with the column headings; a saved search keeps its sort and direction. After **Delete search**, **Undo** saves the search again under the same name. If a search has no matches, **Show all saved positions** clears its filters.

Results retain the full GNUbg position and match IDs. Comment filters consider the match, the relevant game, the position, and the matching checker move. Selected-analysis loss values show their actual engine and depth, with unavailable evaluations shown as unknown. The collection searches checker decisions. Use **Statistics** or **Cube and move notes** to review cube decisions.

Saved imported decisions are validated once into a derived index before filtering and pagination. Older libraries are prepared in bounded batches (16 source catalogs or 2,000 decision records per request); if preparation is still pending, narrow the database or retry the search. The index records the validation-rule and match-source revisions. Original records remain unchanged.

**Saved searches** lets you load, save, update, rename, and delete named searches. PostgreSQL stores the filters, sort, direction, and revision. Concurrent edits return HTTP 409 and preserve the draft. A search referencing a deleted database flags the missing database. The numbered navigation jumps within the current search, including directly to its first and last pages. The board previews the first result when another page loads. Checkbox selections carry across pages. Returning to the workspace keeps the current page, selection, and preview in place while refreshing.

The checkbox in the table header selects the current page. Select up to 200 positions to show the selection count, **Add to deck**, and **Clear selection** in the toolbar above the table. The three-dot button opens **More selection actions** next to the selection controls and contains **Remove from collection**. The add dialog checks how many positions have a usable answer in their selected analysis. Its add button shows the number ready for transfer. If none are ready and a selected position needs analysis, **Open analysis** opens the first such position and keeps the selection for your return. The transfer reports each position as added, updated, already in the deck, requiring analysis, or unavailable. Study answers require a validated result from the selected analysis, including comparable imported checker analysis. Missing numeric depth remains unknown. Existing cards retain their saved solution and provenance; explicitly adding an existing question may refresh its solution while preserving review history. Identical questions share FSRS review history. **Study this deck** opens a fresh study view, and **Download entire deck as Anki** uses the Rust APKG export. A deck can hold 1,000 cards. If a batch would exceed that limit, the entire batch is rejected.

## Imported analysis and opening books

**Displayed analysis** is one persistent choice per match, shared by checker/cube review, move colors, statistics and new learning cards. The catalog lists all retained GNUbg, Open Sage and imported results, including source, scope, depth, time and coverage. **Automatic** prefers a complete current whole-match analysis, then meaningful imported results, then a completed scoped analysis; time and stable identity break ties, never cross-engine ply counts. An explicit choice remains selected across reloads and calculation-profile changes. Missing positions in that analysis remain unavailable; no other result fills the gap. **Load more analyses** exposes older history. If an import contains several records for one decision, the shared projection prefers real values over known placeholders and then the first record in original source order. Original records remain intact and accessible through the source-analysis endpoints and original-file export. **Compare saved analyses** compares any two catalog entries without changing the displayed analysis. New calculations use the configured engine and depth, independently of this result selection. The expandable **Opening book** section is a separate reference. Imported XG, native GNUbg SGF, and available BGBlitz analysis stay in their own records, with the original depth or profile label, method, candidates, cube responses, raw values, equity units, and engine version where known. Missing analysis and unknown engines or units stay unavailable. GNUbg match-winning chances retain their original units. Analysis jobs and independent match copies preserve source results. Older imports can reconstruct analysis once from the stored original file. Source evaluations for recorded double, take, and drop actions also appear in **Cube and move notes**. Imported cube evaluations show whose perspective the values use when the source identifies the player. This includes positions where the player on roll did not double.

Manage books under **Settings → Analysis → Opening books**. Select **Automatic** to compare available books, choose a provider directly, or select **Off**. The book dialog saves the provider and paths with revision checks. Apply or discard any Settings draft before opening it. Book paths are on the backend computer. The analysis inspector shows the selected source, engine, and depth for the current position. New calculations use the configured engine and depth. Book results remain an optional reference alongside the displayed analysis and do not contribute to match PR.

- **BackgammonDB** is a local SQLite book with 4,230,443 decisions across 351 contexts. It provides up to eight ranked moves for a position and roll, plus a separate pre-roll cube lookup. Coverage is pruned and score-dependent. Evaluations use GNUbg 1.08.003 at regular 3-ply. The app credits the CC BY 4.0 data at the result. [Data and format](https://github.com/chalkstreamdev/bgdb-opening-book), [book description](https://backgammondb.com/opening-book)
- **XG Opening Book 3.70** is an official `.ob` file with 53,210 pre-roll position evaluations. It provides a position reference with the original engine, profile or rollout, and author. Values from the previous player's perspective are converted to the player on roll. Historical depth codes that cannot be identified stay unknown. The terms allow free use with attribution to the rollout authors. [Official download and terms](https://www.extremegammon.com/GetOb.aspx)

Book lookups use the full decision context, including Crawford, away scores, cube value and ownership, and money-game rules. BackgammonDB has no beaver contexts. The XG book reads money-game rules such as Jacoby and beavers from the original header and requires a supported context. Its pre-roll values apply to the resulting position independently of any displayed dice.

Keep the downloaded book files outside Git, for example in `~/Library/Application Support/BG Tutor/` on macOS, and enter their backend paths in the book dialog. Extract the BackgammonDB serving SQLite file and the XG `OpeningbookV2.ob` file. The check and install endpoints validate the data formats directly.

## Match databases

**Match database** organizes named collections within a single PostgreSQL instance. Choose a database to filter the list, or choose **All databases** to search across the library. The selector beside **Import match** shows the import destination. It starts with the current collection when it accepts imports and otherwise uses **Unsorted**. Choosing another destination keeps the current collection open and applies to subsequent imports in the current app session. Multi-file imports process each file separately and report duplicates and failures per file.

BG Tutor automatically provides **Unsorted** for imports without an explicit target, **Bot Matches** for matches played against an engine such as GNUbg or Open Sage, **Paused Matches** for resumable sessions, and **Engine Duels** for completed engine-vs.-engine matches. These system databases have fixed names and are protected from deletion. You can edit their descriptions. The sidebar tooltips for **Bot Matches** and **Paused Matches** show a localized default when the description is empty or repeats the database name. **Empty database** is available for **Unsorted**, **Bot Matches**, and **Engine Duels**. It requires confirmation and preserves saved positions and their relevant notes, flashcards and review history. BG Tutor rejects the operation if any of the database's matches has a queued or running analysis. **Paused Matches** is populated by pausing games in Playing. You can create, describe, rename and delete your own databases.

### Filtering and navigating matches

- The match list loads when you open **Match database**. It requests 50 results at a time and fetches another page when you select it.
- Use the search bar to find matches by player or file name. **Filters** opens the advanced match filters, **Notes** searches tags and comments, and **Position ID** finds exact board occurrences. Opening a panel collapses the previous panel and keeps its inputs. The match list and the exact position search share the table filters.
- Applied table filters appear below the search field, even when the filter panel is collapsed. Remove a chip to rerun the search with the remaining filters.
- An exact GNUbg Position ID filter shows every occurrence as its own row in the match table, including repeats in the same game. Use the **Database**, **Game** and **Move** columns to locate or sort the occurrences. Click the move number to open that exact occurrence. The result count refers to occurrences. A checkbox selects the entire match, and the bulk action bar counts each selected match once.
- Select a single match and choose **Open match** above the list, or open it from a linked player name, by pressing Enter or by double-clicking the row. With a position filter active, this opens the exact occurrence. Use the arrow keys to move between rows.
- Sorting applies to all results before pagination. Click a data column header to sort ascending, then click again to sort descending. Names sort case-insensitively. IDs, PR, position counts, scores, game numbers and move numbers sort numerically. Scores sort by White, then Black. Missing values appear last in both directions. Changing the sort returns to the first page and clears the selection. Without a position filter, the occurrence-specific sorts fall back to import date.

### Table and metadata

The table uses the available width alongside the database selector and scrolls horizontally when needed. The default layout gives the two player columns equal room. Names can wrap to two lines in narrow columns, and the row summary shows the full names. The table shows PR, score, match length or money play, played and import dates, event, analysis status, tags and comments. **ID** is a stable match number. It survives a rename or move, copies get new numbers, and gaps are possible. REST uses the match UUID. **Positions** counts stored positions. The source format is available for import and export.

Use **Columns** to choose and reorder data columns, **Show all columns** to reveal them, or **Default columns** to restore the default layout. **Tournament** and **Comment** are hidden by default, and **Tags** is visible. The selection column stays available, and the table requires at least one data column. Hidden values remain searchable and are included in the row summary where applicable. The current sort is shown above the table if its column is hidden. Column visibility and widths are saved separately, and a hidden column retains its width.

Drag the right edge of a header to resize it, or focus the divider and use the arrow keys. Shift increases the step, Home and End jump to the width limits, and Escape cancels an in-progress resize. **Reset column widths** restores defaults for the current window, including the occurrence columns. Widths and the occurrence sort follow the active profile. Widths from the former notes column carry over to **Tags**. Each row has a shared hover summary with the match ID, players, match length, result, available PR, analysis profile and excerpts of tags and comments.

The metadata editor supports display name, played date, event, round, venue and result, with version checks. Player names and an optional display name identify the match in the table. XG and BGF imports retain the source event, played date, round, and location when supplied, and use the score after the last completed game. At startup, BG Tutor fills missing match fields for older XG and BGF imports from the stored originals. Earlier manual saves own the entire metadata form, including empty fields. Migration `0018` and a bounded backfill correct automatically derived results from original files while preserving manual edits, stored positions, analysis, notes and learning progress. Money game results and results without reliable source data remain unknown.

The library shows PR for a complete match analysis with native GNUbg statistics. Individual game analyses appear as partial coverage. Native PR divides normalized equity loss × 500 by the number of unforced checker decisions plus close or actual cube decisions. GNUbg uses a different decision count from XG and BMAB. Older or incomplete results without qualifying statistics leave PR unavailable. [GNUbg match statistics](https://www.gnu.org/software/gnubg/manual/gnubg.html#gnubg-matches_review_stat_overall)

### Bulk actions and exports

Select matches with the checkboxes and choose an action in the toolbar above the list. **Open match**, **Resume match** and **Edit match details** require a single selection. **Copy**, **Move**, **Tags**, **Export** and **Delete** support multiple matches. Most actions support up to 100 matches. Exports support SGF and original format, with a limit of 50 matches and 64 MiB of combined source data.

**Analyze selected matches …** uses one engine and depth for the entire selection and submits one background job per available match. The dialog counts matches that cannot be analyzed and reports rejected submissions by match. **Try again** resubmits the rejected matches. **Bulk analysis** above the list counts completed analyses out of the accepted jobs for the selected database and shows failures separately, including after the dialog closes. **All databases** shows the combined progress, including older jobs tracked without a database assignment. The match list refreshes as tracked analyses complete. Reloading the Match database during a bulk analysis restores the list and resumes progress tracking in the same browser tab.

- Copies get independent match, position, game and move IDs, with copied tags, comments and all completed analyses with their original scopes and the selected analysis. Review history and active jobs belong to their original records.
- Moves preserve IDs. BG Tutor detects duplicates within a database by original-file hash. If the destination already contains that file, it retains both the existing destination record and the source match, including their notes.
- Deletion requires confirmation and skips matches with queued or running analyses. Saved positions and flashcards can still be used after their source is deleted.
- Exports return the original file or canonical SGF, with a ZIP for multiple matches. Use [Database backup](#database-backup) to capture the full library, including BG Tutor notes and review history.

The named collections, sortable tables and bulk actions draw on [ChessBase's database organization](https://en.chessbase.com/post/chessbase-18-tips-for-beginners-part-7-organising-databases). The position preview draws on its [database preview window](https://help.chessbase.com/CBase/16/Eng/preview_window_in_the_database.htm).

## Player database

Open **Player database** to find players by display name or imported alias. The result table shows match and game counts. It starts with the players who have the most matches and shows the first profile until you select another. Click a column heading to sort by name, matches or games, and click it again to reverse the order; match and game counts start with the highest value, and players with equal counts follow by name. `GET /api/players` takes the same order as `sort` (`name`, `matches`, `games`) and `direction` (`asc`, `desc`); an `after` cursor continues the default name order only. When an alias accounts for a hit, its name and source appear under the player name. Select a row to see the profile and its matches alongside the results. The profile header lists the player's other imported names and shows the PR per analysis profile above the matches; the analysis depth is visible, and the tooltip names the full engine profile. The match list shows the opponent, the score with the player's points first, the player's PR in that match, the match format, event and round, database, date and game count. Missing values show as a dash. Click a match row, or press Enter on it, to open the match; click the opponent's name to switch to that player's profile. Below the matches, from two fully analyzed matches on, the profile shows a chart of the PR per match with the average of the last ten matches, weighted by rated decisions. Dashed lines show the same average for checker play and for the cube. The checker value divides the saved checker losses by the unforced checker plays; the cube value divides the remaining loss by the close cube decisions. Filter by database above the table and change pages below it. Both tables scroll independently.

While matches of the profile have no PR, **Analyze unrated matches …** next to the rating collects those without a queued, running or completed analysis and opens the analysis dialog of the match database, where you choose the engine and depth before any job starts.

Choose **Edit** to change the display name or add an alias for future imports; **Save** stores both. Galaxy and OpenGammon usernames have separate platform scopes, and an alias from another source is listed by its name alone. **Merge …** opens with the search field focused and, until you type, offers profiles that contain the first word of the current display name. Select one to see the retained and absorbed profiles side by side with their aliases and match counts. Confirm to assign the absorbed profile’s aliases and matches to the retained profile. The absorbed profile disappears from the player list. Match records retain their stored names.

PR is shown by analysis profile. BG Tutor uses complete, compatible match analyses and divides total normalized equity loss by the total number of rated decisions, using the same PR scale as the match database. Each value shows how many of the player’s total matches contributed to it, with the analysis profile beneath the value.

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

Under **Settings → Appearance**, choose **Graphite**, **Club** or **Paper** and a **Light**, **Dark** or **System** display mode. System follows the operating system. On desktop, the interface preview updates to the right of the controls. Under **Settings → Board**, scroll the gallery on the left to browse artwork while the selected board stays visible on the right. Use **Apply** to save both previews. The browser applies its saved appearance before the first render. Tables, review sessions, dialogs and error messages use the same appearance.

Use **Apply** to save draft language and board settings. To discard a preview, choose **Close** and then **Discard draft**. BG Tutor localizes the interface, status messages, errors and number formats while preserving visible analysis, player names, filenames and GNUbg move notation. Native engine diagnostics accompany localized errors. All six catalogs are type-checked and their keys and placeholders are validated at startup. The translations are pending native-speaker review.

### Action icons and tooltips

The app uses a shared Lucide Outline icon set for actions such as saving, undoing, requesting hints, searching, and importing or exporting. Compact actions have localized accessible names and show tooltips on hover and keyboard focus. Key play decisions, **Undo step**, **Hint**, primary form submissions, and study actions also show text labels.

Tooltips stay above modal dialogs and remain visible when you move the pointer onto their text. Escape dismisses the tooltip and preserves the dialog's Escape behavior. The shared implementation lives in `web/src/action-icons.tsx`, `web/src/ActionButton.tsx`, and `web/src/useActionTooltip.tsx`. Action buttons preserve native submit and disabled behavior and forward refs. Export controls are download links.

## Game plan

**Game plan** classifies White's and Black's structures independently as **Pure race**, **Racing (contact)**, **Attacking / blitz**, **Priming** or **Anchoring**. Both sides are labeled **Opening** while each has at most four checkers off its starting points, which covers about the first two rolls per player. The opening label is BG Tutor's own addition on top of the pinned Open Sage rules, so opening errors form their own mistake group. **Initial** and **Arrows** use the starting position. **Final** uses the displayed position after the move and falls back to the starting position if the move notation cannot be applied. The display is also available for saved positions whose source match has been deleted.

The classifier uses the board alone. It works without an analysis job, and GNUbg supplies move and cube recommendations. `POST /api/game-plans` validates the 26-entry board and returns both classifications with the rule version. The browser caches a bounded number of identical boards. Source and license information is in [game_plan_source.txt](backend/src/game_plan_source.txt) and [backend/licenses](backend/licenses/). [Open Sage reference implementation](https://github.com/markbgsage/bgsage/blob/139427d826dd67798226e7d23c003c90315306ba/cpp/src/encoding.cpp)

## Checker and cube error statistics

Native statistics show a **Total / Checker / Cube** breakdown per player for **Inaccuracies**, **Questionable**, **Error**, **Blunder** and **Large Blunder**. Inaccuracies includes all four severity levels. Click a count to open the decisions with the matching player, decision type and severity. A count in the game overview also filters to that game. A verified zero opens an empty list.

BG Tutor reads the breakdown from stored decisions for fully checkpointed games, including those from earlier analyses. Forced, unavailable and loss-free decisions do not contribute to the error counts. If the stored decisions lack verifiable coverage, the UI shows **—** with an explanation. `GET /api/jobs/{id}/statistics` returns `error_breakdown` for the job and its games. The decision list validates its pagination cursor against the selected filters.

## Tags and comments

Open **Analysis → Notes** and select **Match** or **Position** to edit its tags and comment. **Cube and move notes** also opens game notes and recorded actions such as doubles, takes, drops and accepted resignations, with the board immediately before each action. A pending double displays the offered cube value in the center. Games without checker moves can have notes too. Checker move notes use the same annotation as the corresponding position.

**Save notes** writes to PostgreSQL. Drafts survive navigation and dialog changes during the current session. If another edit has been saved, BG Tutor shows a version conflict and keeps the draft. Tags are normalized and limited to 30 per note and 64 characters each. Comments are displayed as text and can contain up to 20,000 characters.

Use **Search tags & comments** to combine free text (3–200 characters), an exact tag, a note type and a database filter. Results link to the relevant match, game, position or move. The regular match search also indexes annotations across those scopes.

## Flashcards and Anki

### Create and manage decks

In **Analysis → Study cards**, use **Positions as flashcards** to create or select a deck. Add the current analyzed position or import errors from the selected game by player and minimum equity loss. BG Tutor uses a completed game analysis or the selected game's decisions from a completed match analysis. Cards support checker play, double/no double and take/pass questions.

**Suggest positions** previews up to ten decisions across all games of the current match with completed local analysis. It uses the same player and minimum-loss filters. Up to three slots go to distinct mistake groups that recur for that player in other matches in the same database. The other slots go to the largest equity losses. Each suggestion shows the board, decision type, game plan, loss and reason. Choose the cards to add to the selected deck. Cards already in that deck are marked and left unselected. A card from another deck keeps its review progress when added. If the analysis changes before saving, reload the suggestions.

A card's identity includes the board, full match context and question type. A single card can belong to several decks. Adding the decision again updates the analysis source and preserves its review history. BG Tutor reads current source tags and comments when you reveal or export a card. Each deck holds up to 1,000 cards. Card lists return up to 100 per request.

In **Study → Flashcards & Anki**, open **Manage decks** to create a deck or to find, open, rename or delete an existing deck. Renaming keeps the deck ID, cards, preview selection and progress. The Anki download uses the new name. Deleting requires confirmation with the deck name and card count and removes the deck and its memberships. Saved positions, cards, review history and other decks are retained. Adding a decision to another deck reuses its existing card and progress.

**Find duplicates** lists shared cards and separately stored copies of the same board, full match context and question type. Remove an individual deck membership to keep the card and its reviews in the collection. To delete a separately stored copy, select the card to keep and confirm. The kept card takes over the deleted copy’s deck memberships and keeps its own reviews. Deleting the copy also deletes that copy’s review history. These actions apply to the BG Tutor collection.

Renaming or adding memberships advances the deck version. If the deck changes while a dialog is open, **Refresh deck details** loads the current version and keeps the draft. Confirm again to apply the change. The Position Browser refreshes deck choices and membership labels after a change. Deleting a deck closes its active study session.

### Find and preview cards

**All flashcards** searches the shared collection. Choose **Without a deck** to show only cards without memberships, or choose a deck to search within it. Each result appears once and shows its deck memberships. The server searches players, questions, position IDs, deck names, tags and comments before paginating. Combine **Player on roll**, exact tag, comment text, exact checker ID, **Decision type** and **Review status**. **New** means no completed reviews, **Due** can include new cards, and **Reviewed** means at least one completed review. The search reads current match, game, position and move notes, including retained notes after a source match is deleted.

Check individual cards or use the checkbox in the table heading to select the current page. You can select up to 200 cards across pages. Changing the search or deck clears the selection. Use **Add to deck** to give the same card another deck membership. To use **Move cards**, select the source deck and choose a different target deck. Moving removes the source membership and adds the target membership if needed. **Remove from this deck** removes just the current membership. These three actions preserve the card and its review history. **Delete permanently** asks for confirmation and deletes the selected cards, their memberships and their review history.

Each row shows the score and match length (or money game), dice when relevant and cube value. This distinguishes cards that share a checker position but ask about different match situations. The list selects the first card and shows its question and position on the preview board in the chosen theme. Click a row or use the arrow keys, Home or End to preview a different card. The list scrolls independently, and loading more results preserves the selection. Switching decks clears the previous preview. Previewing shows only the question. Choose a deck to show its panel below the search filters, with **Study now** and an **Anki** menu that holds **Download Anki deck**, **Send to Anki**, **Sync Anki reviews** and **Import BG Tutor Anki deck**. These actions use the selected deck, independently of the search filters. **Decks**, **Find duplicates**, **Anki sync** and **Import Anki package** apply to the whole collection and sit beside the page title.

### Review cards and export to Anki

**Practice results**, above the result table with its round size, starts a shuffled round of up to 10 or 20 cards from the applied card filters, including across decks and result pages. Play the move or choose a cube action. After the answer, use **Mark for review** for positions you want to see again. When the round ends, **Retry errors and marked cards** opens a second pass through those cards. Revealed and removed cards are counted separately. Return to the list and use **Resume practice** to reopen the latest round. These extra rounds use the same decision evaluation and leave the FSRS schedule and review history untouched.

During an upgrade, tags from existing Anki imports are carried into the shared card tag field.

**Card tags** can be entered when adding an analyzed position, edited on selected search results, or edited after revealing a flashcard. Use `::` for subtopics, such as `cube::take-point`. The comma-separated **Topic tags** filter requires every term and matches descendants by segment. **Saved topics** stores the applied filters and deck scope, including the game plan and decision type. Reopening the topic searches the current collection.

**Transfer practice** offers a baseline and a follow-up for a player, game plan, and decision type. It draws up to ten unseen decisions from completed native analyses outside matches represented by flashcards. Cube rounds contain equal numbers of both correct actions. The history reports scored first attempts, correct answers, reveals and mean normalized equity loss. To check transfer, repeat the same topic after 7–14 days. Use the optional confidence estimate before checking a card and record a cause or a personal takeaway afterward.

Select one card in the list to practice counting both sides' pips. The board reveals pip counts after submission and the result includes bar contributions. Select a cube card to answer in money play and at other legal scores, then compare the engine evaluations. **Timed preview** shows up to 10 or 20 filtered cards using independently adjustable question and answer times.

Study shows due cards with the chosen board theme, match length, score and player on roll. Money games also show the Jacoby status. Play your checker move on the board and check it, or select and check your cube answer. An offer asks whether to double. A response asks whether to take or pass and shows the offered cube value. Undo lets you adjust a checker move before checking it. The solution stays hidden until you check the answer or choose **Show answer** for self-assessment. While you review a deck, the page heading and the library actions make room for the card: one bar holds **Back to card search**, the deck choice, **Decks** and the remaining due and new cards. On wide windows the board sits beside the question, the solution and the rating buttons, sized so that all of them fit on one screen.

The server verifies your move and scores it against the best decision using comparable saved evaluations when available. Otherwise it evaluates your choice and the best decision together with the configured analysis engine and labels the result with its source and depth. A legal answer with a positive equity loss below 0.040 is playable. If the engine is busy or unavailable, you can keep your answer and retry or reveal the solution. The first checked answer and its result survive a reload.

After checking or revealing the answer, choose **Again**, **Hard**, **Good** or **Easy** to schedule the next review. Rust's `fsrs` library schedules reviews at **90%** target retention. **Again** schedules a review in 60 seconds and the first **Hard** in 10 minutes. Later Hard, Good and Easy intervals use FSRS in whole days. Easy is scheduled at least one day after Good. PostgreSQL stores the card state, rating history and due dates and prevents concurrent or retried requests from applying a review twice.

New cards are due as soon as they are created. A daily allowance, 10 by default, limits how many never-reviewed cards are introduced per local day; due reviews are always offered first and are not limited. **Study → Overview** shows today's due reviews and new cards separately, how many new cards wait for later days, and **New cards per day** (0 to 1000, 0 pauses new cards). A new card counts on the day of its first review. A deck shows how many of its new cards are still available today. `GET` and `PUT /api/study/learning-preferences` read and save the allowance.

Practice works from the keyboard. In cube questions, **N**, **D**, **T** and **P** select No double, Double, Take and Pass. **Enter** checks the answer and then moves to the next question, **A** shows the answer, and **1** to **4** rate a flashcard Again, Hard, Good or Easy. In Analysis, **E** and **Shift+E** go to the next and previous error or decision of the active review. Each button names its key in its tooltip, and the buttons of a study card also show it next to their label. Keys are ignored while typing in a field, and an open dialog only reacts to its own buttons.

With more than twelve decks, **Filter decks** narrows the deck menu, which groups decks under their parent path.

**Download Anki deck** creates a self-contained `.apkg` in Rust, with a stable note model and GUIDs, SVG diagrams, score, cube, dice, answers, tags and comments. Labels use the selected language. German exports also show cube answers and played actions as Doppeln, Nicht doppeln, Annehmen, or Passen. The package has no runtime dependency on BGBlitz artwork or Python. Reimporting updates existing Anki notes. A stored content fingerprint advances Anki's modification time even when comments or the export language change in quick succession. **Import BG Tutor Anki deck** accepts packages carrying the structured BG Tutor decision model and adds them to the selected deck. You can select several packages for the same deck. It reuses matching cards and their review histories. The package transfers card content. Use the review sync below to exchange history.

In **Study → Flashcards & Anki**, choose **Import Anki package** to select one or more third-party `.apkg` files (up to 128 MiB per file). BG Tutor processes each package in order, shows the file being imported and reports failures by filename. It continues with the remaining packages after a failure. The backend renders both card faces in a temporary Anki collection and saves the cards and packaged images in PostgreSQL. Original deck paths appear in the deck picker. Study opens the question and answer in isolated frames and uses BG Tutor's own FSRS schedule. Anki desktop add-ons and links to local applications require Anki. Keep the original package for export.

For browser imports, install Anki's Python package in a Python 3.11+ environment. Set `BGTUTOR_ANKI_PYTHON` to that environment's Python executable when starting the BG Tutor backend. If the setting is omitted, the backend uses `python3`. The Rust import for structured BG Tutor cards uses its own **Import BG Tutor Anki deck** action.

With Anki Desktop open on the same machine and AnkiConnect listening on `127.0.0.1:8765`, select a native BG Tutor deck and choose **Send to Anki**. The backend transfers the same package produced by **Download Anki deck**, and confirms when AnkiConnect reports a successful import. Repeated transfers use the same note GUIDs. If the add-on requires an API key, set `BGTUTOR_ANKI_CONNECT_KEY` on the BG Tutor backend. [AnkiConnect package import](https://github.com/ankicommunity/anki-desktop-addon-connect#importpackage)

Open the intended Anki profile and choose **Sync Anki reviews** for a native BG Tutor deck. Use **Send to Anki** first for cards that are missing from that profile. The sync matches the decision identity in the exported BG Tutor note, even if the note was moved to another Anki deck. It copies missing rated reviews in both directions and reports the active profile, matched cards and added reviews. Each app retains its existing scheduling state and due dates. The card list in BG Tutor counts imported Anki reviews alongside local reviews. Anki's review log includes transferred BG Tutor reviews, and Anki may use that log if you explicitly recompute scheduling later. The sync is additive, so deleted reviews and undo actions are local. Third-party Anki templates are outside this sync. It requires the current AnkiConnect add-on with the versioned `/api/v1` and review import capability. [AnkiConnect](https://git.sr.ht/~foosoft/anki-connect)

Enable **Sync reviews automatically** under the selected deck while its Anki profile is open. At least one card must already be present in that collection. The setting is off by default and saved per deck. BG Tutor checks for Anki about every ten seconds and retries each linked deck about a minute after its previous run. It works while the BG Tutor backend is running, even with the browser closed. A different Anki profile pauses the link. If you move or rename the Anki profile, turn the setting off and enable it again with that profile open. The automatic and manual actions use the same review history and deduplication. After restoring a database backup, open the intended Anki profile and enable automatic sync again for each deck.

Open **Anki sync** in **Flashcards & Anki** to see all linked decks, their Anki profiles, the last successful sync, and the current pause or error reason. **Sync now** runs one exchange with the linked profile, including when automatic sync is paused. **Pause** stops future background runs and lets a run already in progress finish. **Resume** makes the deck eligible for the next background check. Both background runs and **Sync now** in the overview update its counts. The totals include confirmed transfers through the current link since tracking began.

Open **History** on a linked deck to see its latest 100 completed background runs and **Sync now** attempts, newest first. Each entry shows when the attempt ended, its Anki profile, confirmed reviews in each direction and any reported error. An interrupted exchange may show confirmed transfers from before the error. Background connection checks continue to appear in the overview status. History starts with the upgrade and follows the deck across profile links.

Use **Failed runs only** to see all failures or **Open errors only** to see unresolved failures. **Retry sync** on an open error runs a fresh exchange with the deck's currently linked Anki profile, including when background sync is paused. A successful retry marks the selected error as **Resolved** and keeps its details and counts in history. Each retry adds its own entry, transfers only missing reviews, and preserves both apps' due dates.

Select a deck's **Open errors** count in the overview to jump directly to its unresolved failures.
Enable **Decks with open errors only** to hide decks with a zero count. A deck leaves the filtered list when its last error is resolved.
**Retry all open errors** runs one exchange per affected deck, in sequence. A successful exchange resolves all of that deck's open errors. The action continues after a failed exchange. The results show each deck's status, confirmed transfers in both directions during the attempt, and its remaining open errors. A dash indicates a value the server could not confirm.

For batch imports, run the local command from the repository root with the same database settings as the running backend.

```sh
python -m pip install anki
cargo build -p bgtutor --example import_anki_flashcards
python tools/import_anki_flashcards.py /path/to/anki --importer target/debug/examples/import_anki_flashcards
```

The source may be one APKG or a directory, including its subdirectories. Reimporting matches the Anki note GUID and template number so cards keep their BG Tutor review history.

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
| `.ogxm` | Native import of HedgeHog OGXM v2 match files, including archive exports. Keeps the played line, scores, match metadata, and available checker and cube evaluations with their original units and provenance. No external codec or engine is needed to import. |

The XG importer rejects XG1, edited starting positions, automatic doubles, beavers, raccoons and other variants. BGF variants, custom starting positions and restricted cube rules are also rejected. XG candidate and cube evaluations, depth labels and rollout references, and supported BGF analysis are stored as source analysis. The original artifact retains full XG rollout streams and comments. BG Tutor does not interpret XG RTF comments. [Import fixtures and format references](backend/tests/fixtures/README.md)

OGXM import supports standard match and money play. It replays the recorded
moves, validates the final score and checks a supplied CRC32 or SHA-256 digest.
OGXM v1, `.ogxms` streams, standalone analysis files, variants, custom setups,
and special actions such as beavers and settlements are unsupported. The
original file retains annotations, signatures, clock/video data and other
source data; BG Tutor does not display them or verify signatures. Imported OGXM
evaluations are visible as source analysis; they do not count as local GNUbg
analysis or feed recurring-mistake statistics.
When an OGXM export stores only summary values for a checker move,
The selected imported analysis shows the recorded best equity and loss when available.
The view labels the unit as unknown if the source omits it. It displays
known legacy HedgeHog zero placeholders as **Not evaluated**.
[HedgeHog format and codec](https://hedgehog-bg.com/community)

An upload takes one file and has a **16 MiB** limit including multipart framing. SHA-256 detects identical files within the target database. The original, canonical SGF and position records are saved in one transaction. Parsers and archives have separate size and record limits. Filenames and file contents are handled as data.

## Playing against GNUbg

### Start, resume, or end a match

Open **Playing** to start a 1–25-point match or choose **Money game**. **New match** starts with the active profile's last saved settings. During a match, the **More actions** menu (**⋯**) at the right of the match bar holds **Resign game**, **Forfeit match**, **New match**, and **Paused Matches**. The setup form offers your color and the four play modes as radio choices, each mode with its description, and the match length as a list. Your choices of color, match length, play mode and automatic training analysis apply to the new session. The gear beside the engine line opens **Settings → Playing**. **Start match** is the primary action; while a match exists, **Back to match** returns to it, or **Resume match** if it is paused. A pending profile conflict remains available for recovery under **Settings → Profiles** while you start a match. GNUbg handles opening rolls, legal moves, doubling, scoring, the Crawford rule, and match completion. BG Tutor automatically declines bot resignation offers, so play continues until a cube or bear-off result.

Money games use the Jacoby rule and ordinary doubling, with beavers disabled and no target score (`match_length: 0` in the API). Playing automatically generates a learning review after each completed money game. **Next game** carries the running score forward once the review is saved. Older single-game money sessions can also continue. Individual game reports appear under [Study → Reports](#reports).

**Pause match** in the match bar pauses an unfinished point match or money session, including between games. BG Tutor saves the last confirmed checkpoint, including the score, dice, pending decision and engine settings. If your turn contains unsaved checker steps, the confirmation dialog lets you cancel and continue the turn or discard those steps and pause. Choose **Resume match** on the paused board or beside the entry in **Match database → Paused Matches**. **Paused Matches** on the setup page, or in the **More actions** menu during a match, opens that database for the current view. The same entry returns to its previous database. If that database was deleted or now contains a copy, BG Tutor tries **Bot Matches** and then **Unsorted**, preserving any existing copies.

BG Tutor automatically archives bot matches, including previously saved sessions, in **Match database → Bot Matches**. Manage individual matches there using the standard controls for names, tags, comments, moves between databases, copies, deletion, export, and analysis. Choose **Resume match** to return to a running original. Completed matches and independent copies open in Analysis. **Playing** restores the last session after reload and requires an explicit action to resume a paused match. **Download SGF** exports the match at any time.

Each new match records the player name, bear-off direction, play engine, resolved GNUbg executable path, installed version, and playing depth from Settings. Existing matches keep those settings. GNUbg must include its embedded Python module. If the executable becomes unavailable, the version changes, or the engine fails or times out, the saved checkpoint remains available for resumption when the engine is restored.

### Rolls, checker moves, and the cube

Every game begins with an automatic opening roll on the board. Tied rolls animate in order, followed by the deciding pair. If you win the roll, you can start moving checkers with those dice. If the bot wins, its first move plays on the board. Reloading resumes from the saved checkpoint.

After the bot plays or passes because it has no legal move, its dice remain faded on the board and the game waits for your choice. Hover over the dice or the gap between them to lift both dice off the board visually. Click anywhere in that area to start your roll, or click the cube to offer a double when allowed. An available cube lifts and casts a shadow when you hover over it. The action panel beside the board always offers the same choices as buttons: **Roll dice** (Enter) and **Double** (D). When doubling is not allowed, **Double** stays visible but disabled, with the reason below it. If the Crawford rule, your opponent's ownership of the cube, or a cube value that already covers your remaining match points prevents doubling, the action panel explains why. When the bot doubles, the response dialog shows the current and offered cube values.

Your checkers are always at the bottom of the board. Your saved bear-off direction determines the side of your home board. Hover over or focus a checker to preview its destination, then click its point to move it. When only one die permits a complete legal turn from that checker, the click uses that die. If either die is legal there, the first die takes priority. Before moving, click the dice or the gap between them to swap their order. **Undo step** reverses one unconfirmed checker step at a time and shows the number of pending steps. The action panel stays accessible while scrolling and shows the current notation. When the turn is legal and complete and animation has finished, choose **Confirm turn** in the action panel, press Enter, or click the dice pair, including the gap. If you have no legal move, **Pass**, Enter or a click on the same area passes. You can also focus the pair and press Enter or Space. **Z** undoes a step, **H** requests a hint in the modes that offer one, and Escape returns from a reviewed move to the game. The panel names each shortcut in the button's tooltip and shows the instructions for the current step. Hovering anywhere over an interactive pair lifts both dice and casts a shadow beneath them. When pressed, the interactive dice and cube settle briefly toward the board. Used dice fade. A partly used double fades exactly half of one die.

The move history follows the latest saved action and uses the same row layout as Analysis, with notation, player colors, dice, and neutral analysis markers. It sits to the left of the board and moves below it on narrow screens. On large desktop windows, the board grows and the wider history scrolls alongside it with larger text and more visible moves. Selecting a historical move shows preview controls and **Return to game** beside the board. The board stays in place. The game-end dialog shows the winner, points won, and the calculation when the record supports it. Choose **Next game** to continue. At the end of a match, **New match** opens setup and **Back to board** returns to the finished board.

### Decision dialogs and hints

Incoming cube offers, Training and Tutor feedback, and game or match results use a modal dialog when a decision is required. It opens over the center of the board. Drag its title to see the checkers underneath. The position is retained while the dialog is open and stays within the viewport when the window or dialog changes size. Keyboard focus stays inside, and the dialog requires a decision before you can use the board again. Escape and outside clicks leave it open. Errors and pending acknowledgments appear inside the active dialog. Expand **Active mode** to access the mode and [Tutor warning threshold](#playing-feedback-modes).

**Hint** opens a movable dialog with its own loading and error states. Select a checker suggestion with the mouse or Up/Down to see the full saved turn on a separate, read-only board. Use **Initial**, **Arrows**, or **Final** to see the starting position, move arrows, or projected result. Cube hints are text only. **Close** or Escape returns to the game or to the required decision dialog. Decision feedback is shown in dialogs. The live dice order and unconfirmed steps are preserved.

### Resigning

**Resign game** is available in the **More actions** menu before rolling and while moving checkers. Choose a single, gammon, or backgammon resignation in the movable dialog. It uses the current cube to show the points awarded if accepted, the projected score, the target score for point matches, and whether the match would end. GNUbg decides whether to accept and supplies the resulting score, SGF, and move history. Cancellation or refusal preserves the checkpoint and unconfirmed steps. An accepted resignation opens the usual result dialog. Resolve any pending feedback before resigning.

**Forfeit match** is available in the **More actions** menu during an unfinished point match and in the dialog between games. Confirm to record a match loss and award the bot the points it needs to reach the target score. Confirmed moves and completed game results stay in the history. Unconfirmed moves are discarded only upon confirmation. The SGF export includes the conceding color in the BG Tutor property `BTFORFEIT` and preserves the played games.

### Animation and game sounds

The opening roll uses the same dice artwork and animation as regular rolls. When the bot opens, the actual opening dice stay visible while its checker moves play in sequence. Hits animate to the bar and bear-offs to the tray. The decision controls wait for playback, rapid legal source clicks queue in order, and each undo animates its reversal. Reload restores the saved checkpoint without replaying old moves. Leaving Playing stops playback, and reduced-motion mode uses short fades.

Apply **Settings → Playing → Game sounds** to update the current configuration and the browser’s playback preference immediately, including for an ongoing game. Both players use recordings from `sounds/`. `rollingDice.au` plays for rolls, `move.au` for moves and bear-offs, and `hit22.wav` for hits. A new bot double plays `double.au` once when the response dialog appears after checker playback. Loading a checkpoint, hovering, undoing, and confirming steps whose sounds already played are silent. Audio failure does not interrupt play.

The bundled PCM WAV files preserve the recordings' decoded samples. Run `python3 tools/prepare_play_sounds.py` with FFmpeg installed to regenerate them, then build normally. Playback uses the bundled files directly.

### Play API and persistence

| Method | Endpoint                          | Purpose                                                                                                           |
| ------ | --------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| GET    | `/api/play/sessions`              | List the 100 most recently updated sessions.                                                                      |
| POST   | `/api/play/sessions`              | Start a session with a request such as `{ "human_player": 0, "match_length": 5 }`. `0` is white and `1` is black. |
| GET    | `/api/play/sessions/{id}`         | Read a saved session by ID, including sessions outside the list limit.                                            |
| POST   | `/api/play/sessions/{id}/actions` | Apply an action using the current `version`.                                                                      |
| GET    | `/api/play/sessions/{id}/sgf`     | Download the saved SGF, including the escaped player name.                                                        |

Actions include `opening_roll`, `start_opening_move`, `roll`, `move`, `pass`, `double`, `take`, `drop`, `resign`, `forfeit_match`, `next_game`, `pause`, and `resume`. For `move`, send the zero-based `move_index` from the returned legal move list. For `resign`, send `resign_level: 1 | 2 | 3` during a human Roll or Move decision. For `forfeit_match`, send only the action and current version. The completed point session returns `match_forfeited: true`. `pause` and `resume` change the session's `paused` status and database assignment atomically. The legacy API action `end_session` still closes a money session after a completed game. Stale or competing actions and attempts to play a paused match return HTTP 409. An unavailable decision or declined resignation returns HTTP 422 and preserves the checkpoint.

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

New matches default to **Coach**. Choose any mode during setup. **Settings → Playing** stores the default mode and **Open training analysis automatically**. New matches capture the **Error** threshold from **Settings → Analysis** (0.020 by default) for Coach feedback and as the initial Tutor warning threshold. They capture these preferences and the analysis depth independently of the bot's playing strength. Older sessions saved before feedback modes use Competition behavior.

| Mode            | Feedback during play                                                                                                                                                                                                    |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Competition** | Hides analysis, pip counts, error markers, and PR.                                                                                                                                                                      |
| **Training**    | Shows full equities and win, gammon, and backgammon probabilities. With automatic analysis disabled, use **Hint** to request it.                                                                                        |
| **Coach**       | Shows equity differences and compact feedback using the move history’s severity colors.                                                                                                                                 |
| **Tutor**       | Warns before a checker or cube decision at or above the threshold. **Try again**, **Show solution**, and **Play anyway** preserve the first attempt. **Show solution** reveals the evaluation and any checker previews. |

A single match bar above the board groups the score, match format, game number, the engine name, and the current turn status, followed by the play mode, **Pause match** and the **More actions** menu. The engine name's tooltip shows the engine version, the playing and analysis depth, and the bear-off direction. The player rows above and below the board show each player's name, points, and pip count side by side. Its **Play mode** selector switches an ongoing match immediately and saves the choice for reload. It preserves the current position and partial checker steps. In Tutor mode, edit **Tutor warning threshold** beside the selector, then leave the field or press Enter to save and recheck any pending warning. The saving indicator confirms success for three seconds. These controls also appear under **Active mode** in decision dialogs. Live changes apply to the current match. The default warning threshold for new matches comes from Settings → Analysis.

In Coach mode, a checker move at or above the captured **Error** threshold opens feedback before the turn is played. Click your dice again to play the move, or open **Details** to inspect alternatives and try again. Moves below the threshold and forced passes play on the first click and are rated in the move history. The history rates the move that was played. When it replaced an earlier choice, the first attempt and its loss appear under the row, and the row names the engine and depth that analyzed the session. Clicking the opponent’s dice starts your roll unless skipping a double reaches the same threshold. In that case, choose **Continue** to roll or **Details** to inspect the alternatives. Training starts the roll directly and retains automatic analysis for checker plays and explicit cube actions. In Tutor mode, clicking the dice or the cube can trigger a warning when the decision reaches your threshold. Tutor warnings offer **Try again**, **Show solution**, and **Play anyway** directly in the warning. Previewing an alternative leaves the saved choice intact. A Competition confirmation shows only the pending choice. Reload restores pending feedback, and stale requests or engine failures preserve the checkpoint.

The **Play mode** column in Match database shows the mode and use of learning aids. You can hide or resize it independently of existing column profiles. Opening the original of an ongoing Competition match resumes Playing, including at startup. The original match and its positions become available for analysis when the match ends, or after a completed money game. Switching to Competition clears cached analysis for that source. Explicit independent library copies remain available for review.

**Learning review** distinguishes first attempts, played choices, retries, and assistance, including in mixed-mode matches. First-attempt PR uses `500 × original normalized equity loss / native-relevant decisions` at the positions actually reached. It excludes forced checker moves and applies GNUbg's strict close-cube rule to no-double choices. The review identifies its method and profile separately from GNUbg's aggregate rating of the played match. Mode and assistance metadata are retained for originals, copies, and bookmarked occurrences. The match overview omits PR for matches that used learning aids.

To verify mode switching and feedback with the configured GNUbg and an isolated test backend and database, run the native browser suite.

```sh
cd web
BGTUTOR_FEEDBACK_NATIVE_TEST=1 \
  npx playwright test e2e/play-feedback-native.spec.ts
```

## Version information

Open **Extras → Versions** for the running BG Tutor build and the availability and versions of GNU Backgammon and Open Sage. **Technical details** shows the complete build commit and engine version output, including the source commit link for recognized Open Sage forks. **Copy version information** copies the full report for troubleshooting. If one request fails, **Try again** retries that request and keeps the information already loaded.

`GET /api/version` returns the full commit ID in `version`, or an empty string when it is unavailable. `GET /api/engines` includes the repository and full source commit in the optional `fork` field.

## Engine benchmark

In **Extras → Benchmark**, choose an engine for both **Match analysis** and **Play-turn latency**. Each measurement type has its own history and comparison. GNUbg supports 0–4 ply; Open Sage supports 1–4 ply after a capability probe. Depths initially come from Settings. The benchmark remembers depth selections separately for each engine while the app is open and flags an automatic fallback to a supported depth. Selected analysis depths run in increasing order in the background and continue when you leave the view. **Cancel run** also stops the active engine process. Other BG Tutor engine work waits while a benchmark is in progress, and a notice appears across workspaces. Other programs on the machine can still affect the measurement.

The fixed **WC2014 v1** reference is the 11-point match between Akiko Yazawa and Douglas Mayfield, with ten games and 460 checker moves. The app bundles an immutable copy of the unanalysed GNUbg SGF fixture, with a reference ID, version, and SHA-256 checksum. See the [fixture documentation](backend/tests/fixtures/README.md) for provenance and reproduction. Existing evaluations are cleared before analysis.

**Total analysis time** sums the full analysis pipeline for each game, including process startup, loading, checker and cube analysis, and result preparation. Each game starts a fresh GNUbg process. The analysis is cubeful and deterministic, with zero noise and luck analysis disabled. The timer excludes waiting, installation and version checks, and database writes. **Ø analysis time / move** divides the total by all 460 checker moves, including forced moves. It represents the match-wide average. New GNUbg runs use `gnubg-match-v3-2ply-top14` or `gnubg-match-v3-4ply-top14` at those depths and `gnubg-match-v2-unlimited` at other depths. These protocols have no fixed per-game timeout. Older runs retain their recorded filter and timeout.

**Measure play-turn latency** queues a separate run using the benchmark engine and play-depth selectors (GNUbg 0–2 ply; Open Sage 1/2-ply). The `gnubg-checker-turn-v1` protocol plays 11 fixed checker turns with dice already rolled, sampled from opening, contact, race and bear-off positions in the WC2014 reference. Each sample restores its own SGF checkpoint in a fresh process through the Playing adapter, plays one bot turn, and reads the resulting snapshot. The timer includes preparation, native process startup, checkpoint restoration, version guards, and result processing. Queue wait, installation checks, database writes, network time and animation are outside the timed scope. The history reports the arithmetic mean in milliseconds. Play measurements use the same queue and cancellation controls. They are independent of analysis-time estimates and do not create games in the library.

History shows date, engine, depth, timing, and status. Expand **Details** for hardware, the complete engine version and executable path, the reference, and the protocol. History is independent of library matches and profiles. Backend restarts mark interrupted runs as failed. A new run starts its timing from the beginning. During a run, the engine availability indicator reuses the verified installation snapshot to avoid starting a competing probe.

| Method | Endpoint                      | Purpose                                                                             |
| ------ | ----------------------------- | ----------------------------------------------------------------------------------- |
| GET    | `/api/benchmarks/reference`   | Read the versioned reference and measurement protocol.                              |
| GET    | `/api/benchmarks/status`      | Read the active status shown in the global notice.                                  |
| GET    | `/api/benchmarks`             | List the latest 1,000 attempts, newest first.                                       |
| POST   | `/api/benchmarks`             | Queue attempts with a request such as `{ "engine_id": "gnubg", "plies": [0, 2] }`.  |
| POST   | `/api/benchmarks/play`        | Queue a play-turn run with `{ "engine_id": "bgsage", "plies": 2 }`; omitted body preserves the legacy saved-play-default behavior. |
| POST   | `/api/benchmarks/{id}/cancel` | Cancel a queued or running attempt. Finished results are retained.                  |

Open Sage uses depth-specific `bgsage-match-v3-{depth}ply-4threads` and `bgsage-checker-turn-v3-{depth}ply-2threads-gnubg-rules` protocols. A fresh sidecar starts once per run and is then reused. Total elapsed time includes that one initialization plus the measured application pipeline; initialization is also shown separately. Each analysis game is an atomic block with complete installation verification before and after it; no result is returned or persisted before the final check succeeds. Individual requests retain their own boundary checks. Hashes are reused only within one check, never across checks. Playing retains one GNUbg rule process from checkpoint inspection through action execution, then terminates it before committing the result. Playing timing includes preparation, SGF restoration and action execution. Historical v2 results remain available, but are not pooled with v3 timings. Once three complete runs share the same machine, provider, full installation/model/rules snapshot, profile and workload, the view shows the median and minimum–maximum range of the three latest runs. The comparison shows the recorded thread count when known. Expand a group to see the engine version, the measurement protocol, and the GNUbg rules version for Open Sage when recorded. Times measure application execution, not engine playing strength. ETA calibration never borrows GNUbg timings for Sage.

The server creates all attempts for a multi-depth request together before starting them. New requests during a benchmark return HTTP 409. Unavailable engines and invalid depths return HTTP 422. OpenAPI documents the endpoints and types.

## Engine duels

Open **Extras → Engine duels** to see the selected series and its results. Select **New series** and choose a provider and playing strength for each side, a match length from 1 to 25 points, and up to 1,000 matches or pairs. GNUbg executes legal checker and cube actions and applies scoring and the Crawford rule. Each engine makes its own decisions. Both engines offer 3-ply and 4-ply for duels. Open Sage requires a local installation that passes a real probe at the requested depth.

The default **Paired with sides swapped** plays two complete matches per seed. The opening dice and the following rolls stay with their board sides when the engines swap. Each game number uses its own sequence, so a game ending early does not shift the dice for the next game. **Independent random rolls** uses a fresh sequence for each match. A blank seed under **Advanced** generates a fresh series seed. **Repeat series …** copies the configuration into the form so it can be run again. Exact replay also depends on the same engine installations and the dice protocol recorded with the original run.

Series keep running in the background. Each completed match is automatically saved under **Match database → Engine Duels** as the series advances. Its **Event** records both engines and strengths in series order, for example **Duel GNUbg 3-ply vs. Open Sage 3-ply**. The same event applies to both legs of a pair. Replaying a seed creates new match entries. Existing series results stay in the series archive. Cancellation preserves completed results. If a worker loses its lease, the next worker restarts the unfinished match with the same seed. Download individual matches as SGF. In paired mode the win rates count only complete pairs, and a conservative 95% confidence interval on the pair means is shown from 20 complete pairs. A first leg without its return leg remains visible in the results.

New duels retain Open Sage's checker evaluations from play, including up to five candidates, probabilities, depth and engine/model provenance. These appear alongside GNUbg's saved evaluations in Analysis and survive BG Tutor's SGF export/import. The Open Sage values use the custom `BGTSC` SGF property; other SGF applications may ignore it. These are each engine's own playing evaluations, not a full match analysis by a common engine. Previously completed duels without saved Sage evaluations remain unchanged; select the duel in the **Match database** and use **Analyze selected matches …** to obtain a new analysis of both players.

## Study

**Study** opens on **Overview**, followed by **Flashcards & Anki**, **Reports**, **My recurring mistakes**, and **Practice history**. Use Left/Right to switch tabs and Home/End to jump to the first or last. Opening a deck from Positions still goes straight to **Flashcards & Anki**.

Overview counts each due flashcard with a deck once, even when it belongs to several decks. **Study due cards** opens the deck containing the oldest due card and resumes its saved answer when available; the line above the button names that deck. When no cards are due, the overview shows the next scheduled review if one exists.

Overview can preselect the player from the active profile and shows up to three mistake groups for that player. Each group must occur in at least two matches. When no cards are due, **Practice mistake group** opens the eligible group with the highest equity loss per 100 decisions. A recommendation requires at least 30 evaluable decisions in the group. Smaller groups can still be practiced from their rows. The overview table shows the error rate, its trend and the loss per 100 decisions; **Open mistake overview** switches to **My recurring mistakes** with the same player selected, where the full list with counts and totals sits under its own filters. [Review flashcards](#review-cards-and-export-to-anki) · [Practice recurring mistakes](#my-recurring-mistakes)

### Reports

PostgreSQL stores each completed learning review as an immutable snapshot with its SGF. Point-match reports appear individually. Each completed money game has its own report with its game number and cumulative score at the time. The money session header shows the current total from the session, in player order, and its **Ongoing** or **Ended** status. Open individual reports from the games listed beneath the header in game-number order. The archive sorts sessions by their newest report and loads entire sessions a page at a time, including sessions beyond the 100-entry Playing list. It refreshes when you return or select **Refresh reports**.

The overview shows date, match name, score and match length, play mode, learning aids, analysis depth, and first-attempt PR when available. The archive contains finished games with saved reviews and remains accessible after the corresponding library match is deleted.

**Open report** opens the match report in Study. **Decisions and attempts** shows first attempts, corrections, and retries. The money-game archive retains its review dialog. In Playing, **Learning review** is next to **Download SGF** in the move history. Creating a review there opens it automatically. In Analysis, **Learning review** appears beside the match name when a saved report exists. It loads the report on demand and preserves the current board. Opening a saved report reads stored values without running the engine or updating the session.

In Playing, the review dialog uses the same title grip and keyboard behavior as **Hint**. Summary statistics sit above the scrollable decisions and attempts. On short screens the full report scrolls below the title. **Close** or Escape returns focus to the control that opened it.

Select a decision to see the original position and the saved recommendation. The board uses your chosen theme. **Initial**, **Arrows** (German: **Pfeile**), and **Final** show the starting position, move arrows, and projected result. Use the arrow keys or Home/End to move through decisions, and Enter or Space to select one. On narrow screens the preview is brought into view. **Decisions and attempts** returns you to the selected row.

Checker decisions are matched by game, color, dice, and native checker decision number. A pre-roll no-double decision shows the board with no dice. Cube actions show their original cube context and the recommended action. Missing positions and moves that cannot be projected legally are marked. The first selection loads the versioned archived positions, which later selections reuse. Previewing requires neither a library entry nor a new engine job.

| Method | Endpoint                                    | Purpose                                                                                                                                                                                         |
| ------ | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| GET    | `/api/study/reports`                        | List compact summaries with cursor pagination. With `group_by=session`, the limit counts complete money sessions and individual point-match reports. Cursors are specific to the grouping mode. |
| GET    | `/api/study/reports/{id}`                   | Read a saved report.                                                                                                                                                                            |
| GET    | `/api/study/reports/{id}/history?version=N` | Read archived board positions with a version check.                                                                                                                                             |

Summaries include `session_id`, the current `session_scores`, and `money_session_ended`.

#### AI assessment

At the end of a point match, **Create AI assessment** opens the same report as **Study → Reports → Open report → AI assessment**. If the learning review has not yet been saved, BG Tutor prepares it first. Start a new assessment only through an explicit action. The background job and result are stored per report version and language. Reopening a completed result reads it from PostgreSQL. Interrupted or rejected requests show a retry action. A conceded match covers only the decisions reached before concession.

The assessment includes a summary, ten topics, up to three key decisions, and practice tasks. Select a reference to see the archived position and engine recommendation. Coverage for hitting uses observed hit-versus-nonhit comparisons between the played and best saved moves. The backgame topic uses two anchors in the opponent home board while behind in pips to identify candidates for review. It does not classify timing or optimal backgame strategy. Opening covers the first three checker plies.

Set `OPENAI_API_KEY` in the backend environment or local `.env` and restart BG Tutor. The default model is `gpt-6.1-terra` with low reasoning effort. To change the model for newly created assessments, set `BGTUTOR_ASSESSMENT_MODEL`. The backend uses the OpenAI Responses API via the operating system curl. The API request includes game and decision data needed for the report. It omits personal names, free-text notes, and raw SGF. Existing assessments remain readable without an API key. When a request fails, the backend log records the HTTP status, the curl exit code, and recognized provider error codes when available. Diagnostic logs omit the API key, match data, and response text.

| Method | Endpoint | Purpose |
| ------ | -------- | ------- |
| GET | `/api/study/reports/{id}/assessment?language=de` | Read status, availability, and any saved result for the report version and language. |
| POST | `/api/study/reports/{id}/assessment?language=de` | Start or retry creation with the saved `report_version` in the JSON body. Returns the existing job if it is queued, running, or complete. |


### My recurring mistakes

**Study → My recurring mistakes** groups a selected player's errors by game plan and decision type. The groups cover checker play, missed doubles, premature doubles, wrong takes, wrong passes, and cube errors that cannot be assigned to one of those types. Player matching uses the full name, ignores case and surrounding spaces, and includes both colors. Combine database, game plan, and minimum-loss filters. The minimum defaults to 0.08 normalized equity.

The player selector lists names from the selected match database, including players whose matches have no saved analysis. When the current filters yield no evaluable decisions, the view reports that explicitly.

Each group shows its error count, distinct matches with errors, and total equity loss. **Earlier → recent** compares the group's error rate in the earlier and the recent half of the player's matches by date, once each half has at least 10 evaluable decisions. Where a denominator is defined, it also shows the number of evaluable decisions, the error rate, and the sum of qualifying losses per 100 evaluable decisions. The checker denominator includes all evaluable checker plays in that game plan. Cube groups use the best action to define the opportunity. For example, the missed-double rate divides missed doubles that meet the selected minimum loss by all evaluable opportunities where doubling was best. Changing the loss threshold changes the errors and their summed loss. The set of evaluable opportunities stays fixed for the selected player, database, and game plan. A group with fewer than 30 decisions is marked as a small sample. Unclassified cube errors have no defined denominator and display a dash for the rates.

The groups and the decision search read a persisted decision index. For each match of the selected player it stores the decisions of the displayed analysis with their game plan, error type, loss, score and cube. A match is indexed on first use and again when its fingerprint changes: another analysis is displayed, the source, a player name or a saved decision changed, or the classifier moved on. The index is derived data: a database backup leaves it out and a restore clears it.

Each match contributes only decisions from its effective catalog selection. This is the same saved choice used by Analysis and Statistics. Missing games or decisions in that selection stay absent, including when another local or imported result exists. Imported XG, GNUbg SGF and BGBlitz results preserve their source units and depth. Removed source matches, forced or unevaluable decisions, and ongoing Competition games are excluded. Completed money games are eligible.

Imported checker decisions require both the best and played moves to be verifiable legal candidates. Loss is calculated from comparable normalized XG equities or explicit BGBlitz EMG values. Match-winning chances (MWC), unknown units, and BGBlitz placeholders such as −99 are excluded from the calculation. Cube decisions use the perspective of the player making or answering the offer. Engine, source format, method, and original depth remain visible, including XGRoller++ and rollout labels. Missing depth is reported as unknown.

**Practice** opens up to 20 distinct questions and prioritizes the group's highest losses. Missed-double practice adds saved positions where doubling was available and rolling on was correct. Wrong-take practice adds correct takes, and wrong-pass practice adds correct passes. Each of these rounds adds up to 10 such questions from the same player, database, and game-plan scope and shuffles the order. If there are no suitable examples, the round uses the qualifying errors from the selected group.

Play a checker move on the board or choose a cube action, then check your answer. The board shows the original match context and centers the offered cube for a take or pass. A pre-roll cube question hides the subsequent roll. The result shows your choice, the best answer, the equity loss and the source used to evaluate it, alongside the decision from the original match. If the saved analysis cannot score your choice, the configured engine evaluates it and the best answer together. **Reveal answer** also lets you inspect the original decision directly. **Next** advances to a hidden answer. Closing practice, changing filters, or leaving the tab ends the exercise and discards late responses. This exercise creates no flashcards or FSRS reviews. Checked answers are saved by day, player and mistake group, and **Study → Practice history** lists the number of answers, how many matched the best decision and the mean equity loss.

**Positions → More filters → Game plan** searches the collection for any of the six game plans from the perspective of the player on roll. The browser tab retains the applied filter, and a named search saves it for future use. BG Tutor classifies older positions in a versioned, indexed cache without engine work. Large collections are prepared in bounded steps, and the app prompts you to search again if preparation is still in progress. Saved positions remain searchable after the source match is removed.

`GET /api/study/mistakes` lists the groups. `GET /api/study/mistakes/history` lists checked practice answers by day and group. `POST /api/study/mistakes/practice` returns questions with the answers and played notation hidden. `POST /api/study/mistakes/attempt` checks a submitted answer and returns its evaluation and the original decision. `POST /api/study/mistakes/reveal` returns the original decision for self-assessment. Both check that the source remains the selected analysis. Local references carry `job_id`, imported references carry `imported_analysis_id`, and both identify the game and decision. Older libraries fill gaps in the import and move catalogs in bounded steps as needed. Current libraries use saved move contexts. OpenAPI documents all parameters and response types.

## HedgeHog comparison

This comparison records six observations from the HedgeHog review and reflects the BG Tutor implementation as of November 13, 2026. It covers features and workflow. The quality of HedgeHog's written explanations was outside the review.

| Area | BG Tutor | HedgeHog reference |
| --- | --- | --- |
| Daily orientation | **Study → Overview** puts due cards, recurring mistakes, and the next practice action together. The application opens in **Analysis**, where you can reopen or import a match. | The signed-in home page also brings together recent matches, performance, and the largest mistake of the week. [Home](https://hedgehog-bg.com/help/getting-started) |
| Active practice | Flashcards and recurring-mistake exercises accept checker plays and cube answers and show their equity cost. Missed-double, wrong-take, and wrong-pass rounds mix and shuffle positions with both possible best actions. For flashcards, a separate FSRS rating schedules the next review. | HedgeHog offers both board play and shuffled answer choices. For themes that would give away the answer, it mixes in positions with a different correct decision. [Practice](https://hedgehog-bg.com/help/practice) · [Themed practice](https://hedgehog-bg.com/help/positions#presets) |
| Exploring variations | Candidate previews show the result of individual moves. Saved positions and notes let you return to them. | Study keeps branches for alternative plays and rolls, with multiple starting positions, notes, and drawings. [Study](https://hedgehog-bg.com/help/study) |
| Score sensitivity | Analysis evaluates the position in its recorded match context. | The score matrix observed in the review compared the same position at money play, double match point, and different away scores. It showed whether the best move changed and how much that change mattered. [Study workspace](https://hedgehog-bg.com/study) |
| Explaining decisions | Reports capture the original decisions, first attempts, and corrections. Counted board differences explain a checker error, and cube decisions show all outcomes, chances and the take point at the score. | **Why?** explains individual decisions. The written review links key moments to the board and adds context on luck, mistakes, strong decisions, and a practice priority. [Match review](https://hedgehog-bg.com/help/reading-the-analysis#match-review) |
| Understanding recurring weaknesses | Error groups show the error rate and equity loss per 100 evaluable decisions by game plan and decision type, with a small-sample label. | Topics also groups by structure, score, and cube role. Its weak-spot indicator adjusts for how difficult each category is for other players. [Topics](https://hedgehog-bg.com/help/positions#topics) |

## Configuration profiles

The header shows the active profile and marks it **modified** when your current settings differ from its saved values. Select a profile to switch or choose **Manage profiles** to open **Settings → Profiles**.

Choose **Show changes** in the profile menu to see each changed setting with its category, saved value, and current value. The arrow next to a current value restores that field from the saved profile and saves the current configuration. Related values such as a dice pair or a numeric or date range are restored together. The dialog shows any errors. The **modified** marker disappears after the last change is reverted. **Close** or Escape dismisses the dialog.

**Reset all changes** restores the entire saved profile in one operation after confirmation. **Cancel** or Escape dismisses the confirmation. A failed restore preserves the current configuration. After success, the changes dialog stays open with an empty list.

Profiles capture engines, default depths and error thresholds, playing and training defaults, language, sound, interface and board appearance, and opening-book references. **Settings → Profiles** shows the active profile and its change count at the top. Select a profile to compare it with the current configuration. Its full saved configuration can be expanded below the comparison. Profile timestamps use the app language and the browser's local time zone.

Each browser tab keeps its own database selection, search filters, sorting, Study deck, card-creation choices, and analysis filters. Table columns, column widths, and analysis board layouts are stored in the browser. Both sets of preferences survive reloads and profile switches. The browser uses legacy configuration values to seed these preferences once. Older profile files retain those values for compatibility. A removed database or deck is handled in the workspace that uses the selection.

**Apply** saves the current settings. To update a named profile, open **Settings → Profiles** and choose **Save in profile**. **Save as new profile** asks for a name, then creates and activates a profile. The **Profile actions** menu contains **Rename** and **Delete**. Activate a different profile before deleting the active one. **Restore saved profile** reapplies the active profile's saved values after confirmation. To save changes while switching profiles, use **Save and switch** in **Settings → Profiles**.

On the first launch after the profiles update, existing server and browser preferences are imported once into **Standard**. Profiles and current settings apply across the installation, including other browsers. On a version conflict, you can load the server version or save your local changes as a new profile. A recovery journal protects pending local changes.

Running analyses and matches retain their captured settings. If the profile cannot load, workspaces use the last confirmed settings or the built-in defaults. The profile controls report the failure and offer a retry. Engine installation paths are device-specific, and book files are stored as references. Activation checks the referenced resources and reports missing items. See the [original profile specification](docs/superpowers/specs/2026-11-10-profiles-design.md) and [implementation plan](docs/superpowers/plans/2026-11-10-profiles.md).

## Analysis time estimates

### Before analysis

Before you start a game or match analysis, **Estimated analysis time: approx. …** appears below the scope and depth selectors. It multiplies the number of checker moves in scope by the average time per move from the most recent matching, fully completed benchmark. The UI shows the benchmark date, engine version, ply depth, and checker move count. Changing the match, game, scope, depth, applied profile, or engine installation invalidates an outstanding estimate. The estimate is hidden during submission and execution.

A matching benchmark uses the same local machine and hardware, engine installation and version, ply depth, and move filter, with compatible reference and analysis parameters. With a matching filter, completed GNUbg runs using the former 600-second per-game limit remain eligible. Calibration requires a successful, complete local run.

The estimate includes process startup and cube analysis. Time spent waiting for other jobs is outside its scope. Different types of positions can take substantially different amounts of time. If the benchmark, checker moves, or engine installation is unavailable, or the request fails, the UI explains the issue and still lets you start analysis. Estimates use existing measurements.

`GET /api/matches/{id}/analysis/estimate?plies=2&game_no=1&engine_id=gnubg` returns the estimate and its measurement basis for the requested provider. Omit `engine_id` to use the saved analysis engine, `game_no` for the whole match, or `plies` to use the saved default depth. The endpoint is read-only and observes the source and Competition restrictions.

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

The match, player, saved position, annotation, flashcard, duplicate and decision
search endpoints accept a one-based `page` and return the filtered `total`.
Existing cursor clients can continue to use `after` on its own.

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
| GET          | `/api/matches/{id}/analyses` | Paginated catalog, effective selection and explicit selection identity. |
| DELETE       | `/api/matches/{id}/analyses/{analysis_id}?source_revision=` | Remove one saved analysis. Returns 409 for a changed match source or active calculation. |
| PUT          | `/api/matches/{id}/analysis-selection` | Persist `analysis_id`; null restores the shared automatic policy. |
| GET          | `/api/positions/{id}/analysis-result` | Exact selected result for this occurrence; null for missing scope. |
| GET          | `/api/matches/{id}/analysis-statistics` | Selected native or imported statistics; missing depth/coverage remain unknown. |
| GET          | `/api/positions/{id}/jobs` | Discover persisted position calculation jobs without browser storage. |
| GET          | `/api/matches/{id}/games`                                        | Game catalog with starting scores, Crawford status and move counts.                                                                                                           |
| GET          | `/api/matches/{id}/dice?game_no=`                                | Observed roll frequencies, optionally for one game.                                                                                                                           |
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
| GET        | `/api/health`, `/api/engines`                                                | PostgreSQL and GNUbg health, or available engines, versions and depths.                                                                                                                                                    |
| GET        | `/api/database-backup` | Export a compressed BG Tutor backup as a file. |
| POST       | `/api/database-backup/check` | Check a raw backup archive and report schema compatibility and archive contents. |
| GET        | `/api/database-backup/target-counts` | Read the current match, saved position, and flashcard counts in the destination. |
| POST       | `/api/database-backup?replace=true` | Restore a BG Tutor backup with media type `application/vnd.bgtutor.backup`. |
| GET / PUT  | `/api/settings`                                                              | Persistent engine and player settings with revision checks.                                                                                                                                                                |
| POST       | `/api/engines/gnubg/check`                                                   | Check the installation using `{ "path": "..." }`.                                                                                                                                                                          |
| GET / PUT  | `/api/opening-book`                                                          | Provider, local BGDB and XG paths, availability and version checks.                                                                                                                                                        |
| POST       | `/api/opening-book/check`, `/api/opening-book/install`                       | Validate or atomically install a local BGDB file.                                                                                                                                                                          |
| POST       | `/api/opening-book/xg/check`, `/api/opening-book/xg/install`                 | Validate or install an official XG book file.                                                                                                                                                                              |
| GET        | `/api/positions/{id}/opening-book`                                           | Context-specific book result or explicit miss, with a separate XG position reference.                                                                                                                                      |
| GET        | `/api/matches/{id}/imported-analysis`                                        | Source analysis provenance and coverage.                                                                                                                                                                                   |
| GET        | `/api/positions/{id}/imported-analysis`, `/api/moves/{id}/imported-analysis` | Original checker or cube analysis.                                                                                                                                                                                         |
| POST       | `/api/jobs`                                                                  | Submit a position analysis with `{ "position_id": "<position UUID>", "kind": "analysis", "plies": 0 }`.                                                                                                                    |
| GET / POST | `/api/matches/{id}/analysis`                                                 | Get the latest job or submit a new one. Pass `engine_id` in the query or request body to override the saved engine. Use `game_no` to select an individual game. Omit it for the whole match. For example, submit `{ "plies": 0, "game_no": 2 }` for game 2. |
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

## Analysis jobs

GNUbg supports 0–4 ply for position, game and match analysis. The default analysis depth is 2-ply. A job captures its engine configuration and depth. Identical active jobs are deduplicated by scope, depth and engine configuration. Each backend runs one worker, and PostgreSQL `FOR UPDATE SKIP LOCKED` keeps workers from claiming the same job.

Jobs move through `queued → running → completed|failed`. GNUbg has a 90-second process limit for position analysis and MAT conversion and a 600-second limit for a game analysis. The worker's 120-second lease renews every 20 seconds. After expiration, the job can be claimed again, up to three attempts in total. A regular engine error marks the job `failed` and the UI can submit a new job.

SSE event types are `queued`, `running`, `progress`, `completed` and `failed`. The `id:` field is a durable event number and `data:` contains `JobProgress` JSON. `Last-Event-ID` resumes after that event. With no header, the server replays the event history and then sends keepalives until the job ends. SIGINT and SIGTERM close streams and terminate running child processes. On restart, the worker can reclaim expired jobs. The browser discovers saved position jobs from PostgreSQL and reconciles SSE with polling. It also loads match and game jobs after a browser reload; job monitoring does not choose the displayed analysis.

Match analysis checkpoints decisions, partial statistics and SSE progress for each completed game in one transaction. `Job` and SSE report `completed_games` and `total_games`, so the percentage advances when a game finishes. After an expired lease, a replacement worker resumes at the next incomplete game. An expired worker cannot write checkpoints or results. Completed games and their reviewed decisions remain accessible after a later game fails. Submitting a new job analyzes the requested scope from the beginning.

New GNUbg and Open Sage game and match analyses save the ranked checker candidates returned by the engine, including the played move, equity, outcome probabilities, and actual evaluation depth. The **Candidates** table displays these details for the selected saved result. Recalculate an older game or match analysis to obtain candidates if it only saved the played and best moves. To remove a saved analysis, select it under **Displayed analysis**, choose **Delete analysis**, and confirm the identified result. This keeps the match and its source file available.

GNUbg evaluation uses `analyse game` output at six decimal places. BG Tutor clears existing analysis annotations before the calculation, normalizes inherited match data and setups, and verifies board and match context for each checker and cube action. Mixed SGF collections can include empty or cube-only games. Missing or incomplete native exports fail the job. Each native export is capped at 64 MiB.

A forced pass has zero loss and no alternative equity (`evaluation_status=forced`). Analysis views apply the configured normalized equity thresholds to evaluated decisions. The API identifiers are `doubtful`, `bad`, `blunder` and `very_bad`. The `performance_rating` field includes the counts of relevant decisions and weighted native GNUbg PR. Cube equities use the deciding player's perspective and the cube value before the offer as their unit. Cube-only decisions have stable source action IDs and are accessible from **Cube and move notes** with their pre-action board.

## Database backup

On the source installation, open **Extras → Database backup → Export backup**.
The downloaded `.bgtutor-backup` file contains the PostgreSQL application data,
including all match databases, original match files, notes, completed analyses,
saved positions, player profiles, preferences and learning history.

On the destination, open **Extras → Database backup** and choose a file. BG Tutor
checks the archive and compares its migration fingerprint with the destination
database. The preview shows when the archive was created and how many library
matches, saved positions, and flashcards it contains. If an older archive lacks
a recorded creation time, BG Tutor uses a valid original filename
(`bgtutor-YYYY-MM-DD-HHMMSS.bgtutor-backup`, UTC) and labels the time
**from filename**. A successful check enables **Review database replacement**.
The confirmation dialog shows the creation time and compares the backup counts
with the destination counts fetched when the dialog opens.
**Difference** shows how many entries each category gains or loses after
replacement. Increases have a plus sign and appear in green. Decreases have a
minus sign and appear in red. Unchanged counts show a neutral zero.
The check uses a temporary upload and leaves the destination data intact.
Damaged archives and incompatible schemas prevent confirmation.

**Replace database** overwrites the application data on the destination. Export
its current database first if you need to keep that data. After confirmation,
the backend checks the archive and schema again. PostgreSQL validates the data
as the replacement is applied in one transaction. If restore fails, the
previous data stays in place. Wait for the success message and reload BG Tutor.
Finish any work running on the destination before restoring.

The destination keeps its local engine installations, thread overrides, and
opening-book paths. In-flight or queued jobs from the backup are marked as
interrupted. Saved playing checkpoints still require compatible local engine
versions or models. Install external engines and opening books on the destination
through their normal setup. The archive holds unencrypted data. It supports up to
16 GiB compressed and 64 GiB uncompressed. Large restores need temporary disk
space for both forms of the archive.

## Verification

Run the checks relevant to the change from the repository root. Install the frontend dependencies with `npm ci --prefix web`. For the full Rust suite, configure `DATABASE_URL` as described below. Use `cargo test --workspace --lib` to run the Rust library tests without the SQLx integration targets.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test --prefix web
npm run test:themes --prefix web
npm run build --prefix web
```

After changing an API contract, run `npm run api:types --prefix web` before the frontend checks. It regenerates [OpenAPI](docs/openapi.json) and [the TypeScript types](web/src/generated-api.d.ts) from the Rust API. The `test:themes` script discovers the Python tests under `tools/`, including tests for tools beyond theme preparation.

### PostgreSQL and GNUbg integration tests

Start PostgreSQL with `docker compose up -d --wait db` or use a dedicated test server. Export `DATABASE_URL` for a maintenance database on that server using a role with `CREATEDB`. For the default local Compose instance, use the connection settings from [.env.example](.env.example) with `/postgres` as the database name. SQLx creates and drops separate test databases. Set `GNUBG_BIN` if GNUbg is outside `PATH`. Native analysis needs the neural-network weights, and native play tests also need GNUbg's embedded Python support.

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

Playwright requires Google Chrome, `psql`, PostgreSQL, and the frontend dependencies. GNUbg workflows require a working local engine. The harness creates a fresh database, verifies that the API is using it, starts a dedicated backend and Vite on local ports, and removes the test database after the run. Vite serves HTTP and its development WebSocket on the same loopback port. Screenshots and other test artifacts are written to `web/test-results/`.

```sh
npm run test:e2e --prefix web
```

Pass a spec path to run only the affected workflow. These specs cover native
match analysis, automatic dice equity, and the Study overview with its two
direct practice actions.

```sh
npm run test:e2e --prefix web -- e2e/workflow.spec.ts
npm run test:e2e --prefix web -- e2e/dice-equity.spec.ts
npm run test:e2e --prefix web -- e2e/study-overview.spec.ts
```

The bulk analysis regression uses GNUbg to analyze a short match and the
ten-game WC2014 fixture. It reloads while an analysis is running and checks the
restored progress and persisted job IDs in the disposable database.

```sh
npm run test:e2e --prefix web -- e2e/analysis-calculation-controls.spec.ts --grep "reload resumes native bulk"
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
| [`backend/src/engines/`](backend/src/engines/) | Engine providers, capability checks, and Open Sage integration |
| [`backend/src/learning/`](backend/src/learning/), [`backend/src/recurring_mistakes.rs`](backend/src/recurring_mistakes.rs), [`backend/src/practice.rs`](backend/src/practice.rs) | Flashcards and FSRS, recurring mistakes, and shared answer evaluation |
| [`backend/src/store.rs`](backend/src/store.rs), [`backend/migrations/`](backend/migrations/)                       | SQLx persistence, schema, and search indexes                        |
| [`web/src/`](web/src/)                                                                                             | React client, SVG board, and generated API types                    |
| [`web/e2e/`](web/e2e/), [`web/playwright.setup.ts`](web/playwright.setup.ts) | Browser workflows and isolated PostgreSQL test setup |

The Rust backend runs independently of the browser. A future Tauri shell could start it as a sidecar and reuse the HTTP API.
