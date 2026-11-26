# Analysis

How the **Analysis** workspace, saved and imported analyses, game plans, error statistics, notes, time estimates and analysis jobs behave. For a guided introduction, see the [user guide](../user-guide/README.md#3-analysis).

## Analysis workspace

The header lists **Playing**, **Analysis**, **Study**, **Positions**, **Match database**, **Player database**, and **Extras** from left to right. **Help** sits immediately after **Extras** and opens the NeonDojo and NeonGammon manual links. **Settings** is on the right. With a header tab focused, Left and Right select the previous and next tabs. Home selects **Playing** and End selects **Extras**. The first visit opens **Home**. Opening the NeonDojo logo returns there and keeps the analysis selection. Reloading Home keeps the start page visible. Opening the app with a saved analysis selection restores it in **Analysis**. The **Study** workspace is named **Lernen** in German.

Arrow keys and Home/End also navigate the detail tabs. The app restores the last match, game, and position locally. If that match or position no longer exists, the app forgets it and starts from the empty workspace without an error. Switching tabs preserves filters, note drafts, and SSE subscriptions. A submitted job continues to be tracked if you change the selected game or position while the request is pending.

The detail tabs are **Checker**, **Statistics**, **Cube**, **Notes**, and **Study cards**. For a saved position whose source match was deleted, **Statistics** and **Cube** are hidden. The desktop **Checker** places move history on the left and position evaluation beside the board. Long lists scroll independently and keep the current move visible. The evaluation pane offers **Candidate moves**, **Compare moves**, **Dice equity**, and **Game rolls** (**Match rolls** when no game is selected). **Game rolls** loads the frequencies of recorded rolls on demand (`GET /api/matches/{id}/dice?game_no=…`) when you open the tab — these are observed roll counts, not an equity analysis — refreshes if the match source revision changes, and offers a retry button if loading fails. The board and move history are ready as soon as the game's positions load.

The header shows the players, match ID and length, result, date, event, and PR per player, plus the round, location, and saved **Import notes** count when present. Date and event come from saved metadata and show `n/a` when unavailable. PR comes from **Displayed analysis** when that is a complete native whole-match analysis and shows `n/a` otherwise. PR is shown with one decimal throughout the app.

In **Checker → Dice equity**, NeonDojo checks the selected position for a saved
result or a queued or running job from the active analysis engine. Results from
another engine are not reused. If none exists, it starts a calculation with
the engine and search depth from the active analysis profile. Selecting another
position does the same while **Dice equity** is visible. After a submission or
engine failure, select **Dice equity** again to retry. GNUbg evaluates all 21
distinct rolls in one run and reports progress roll by roll, confirming the
final roll only after validating the full batch so unexpected output markers
cannot mark the run finished early; NeonGammon evaluates rolls one at a time. A
roll without a legal play is evaluated as a forced pass.

The grid shows the best play's equity for each of the 36 rolls. Cells at or
above the weighted mean **Ø** use the success color, cells below it the
warning color. The grid starts on the roll shown on the board, which stays
outlined, or on 1-1 when the position has no roll. Below the grid, the selected
roll shows its best play, equity and probability. Selecting a cell also shows
that play on the board with its dice, like a candidate preview; **Reset preview**
returns to the recorded move. Selecting another position starts again on its
roll.

**Delete dice equity** beside **Ø** permanently deletes the displayed
calculation (`DELETE /api/positions/{id}/jobs/{job_id}`) after a confirmation.
A queued or running calculation cannot be deleted. If an older calculation of
the same engine remains, the grid shows it. Otherwise the tab stays empty
instead of recalculating; select **Dice equity** again or use
**Recalculate dice equity …** to calculate it. Checker analyses are deleted in
**Displayed analysis** instead.

The position pane shows each result with its source and recorded depth. To
choose a depth and calculate again, use **Recalculate position …** or
**Recalculate dice equity …** in the header's **Analysis options**.

**Analyze game** stays visible in the Analysis header and starts the selected
game. Its options include the engine, depth and time estimate. Start a
whole-match analysis of the open match with **Analysis options → Analyze entire
match …**, or with **Analyze selected matches …** in the selection
toolbar of the **Match database**. For one match, the dialog shows the engine, depth and estimate before you
confirm. In Analysis, **Displayed analysis** determines which saved results
the decision reviews and **Statistics** show. Running jobs stay
visible as you switch tabs or positions.

### Moves, errors, and statistics

Move history shows all checker moves in the selected game by default, including those without an evaluation. Choose **All errors** or a specific grade to narrow the list without changing the board position. The filter uses the saved assessments and the configured error grades. Each row shows notation, player, dice, and localized equity loss. A zero loss is shown as `0.000` in English, and a dash indicates that no evaluation is available. Hover over the loss to see the grade, analysis source and depth. With **All moves** selected, recorded doubles, takes and passes appear as lines between the checker moves, with the player who acted. Select one of these lines to open its saved review in **Cube**; a line without a saved evaluation shows `Not evaluated`. While **Cube** shows one of these decisions on the board, its line is marked in place of the checker move.

**Cube** reviews only cube decisions. **Inaccuracies only** starts at the
configured **Questionable** threshold. Turn it off to include all cube
decisions, or use **Reset filters** to restore the default. Filter the review by
player, severity and game. A match review can cover all games or one game.
Opening **Cube** without a selected cube decision shows the next matching
decision from the board position on, as **Next error** would. A decision that
is already selected stays selected, and **Back to the checker move** keeps the
board on the checker position until you open **Cube** again.

When the displayed analysis has matching checker errors, **Checker** shows
**Previous error** and **Next error** (**Shift+E** / **E**) above the board, so
you can step through them without opening **Statistics** first.

In **Statistics**, checker and cube metrics open the corresponding tab with the
metric's player, severity and game scope. Metrics that combine both decision
types let you choose **Checker** or **Cube**. A checker selection adds review
controls beside the candidate analysis and can span the full match. Close the
statistics selection to return to the usual checker view. The move history
continues to follow the selected game.

In **Analysis → Statistics**, open **Compare saved analyses** and choose any two catalog entries to see their saved results side by side. Each entry retains its engine, depth, scope and coverage. This comparison leaves **Displayed analysis** unchanged, and GNUbg PR is displayed only where that engine recorded it.

The selected decision shows the played and best actions, the loss, and the analysis source. Cube review includes missed doubles and double, take, and drop decisions. Game buttons in **Statistics** open that game. Native PR uses GNUbg job statistics. New jobs save complete per-game statistics atomically. Older jobs can show totals reconstructed from their decisions, with per-game PR shown as unavailable where it was never saved.

Games imported with the SGF rule `RU[NoCube]` use cubeless checker analysis and have zero cube decisions. GNUbg uses the source game rules for checker evaluation and PR. NeonGammon supports cubeless checker evaluation for money games without Jacoby and reports other cube-disabled configurations as unsupported. Statistics marks older native checkpoints for these games so you can choose to analyze them again. Historical decisions and study schedules stay available. A cube-disabled checker card has its own schedule even when its board and Match ID also occur in a cube-enabled game. New cube practice questions require a game that permits doubling. For saved occurrences from older libraries whose source has already been deleted, the cube rule can be unknown. Evaluations that need it report it as unavailable.

NeonGammon replays the stored match once when a job starts or resumes. It indexes checker occurrences by game and ply, then verifies and checkpoints each remaining game independently.

### Candidate comparison and decision review

Candidate tables share a compact layout for local, imported and opening-book results.
They identify the best and played moves separately from the current preview, show
comparable loss to the best result, and show win and loss probabilities for the
total, gammon, and backgammon on one line: three **Wins** columns followed by
three **Losses** columns. The total includes gammons and backgammons; gammon
excludes backgammons. Percentages use one decimal place. Missing
or inconsistent probabilities appear as dashes.
Equity differences stay within the same source and known equity unit; MWC differences
use percentage points. Unknown or incompatible values remain unavailable. Engine,
depth, unit and player perspective stay visible, including differing candidate depths.
The table ranks imported XG checker candidates by descending equity and shows
each candidate's original evaluation depth. Ties keep their file order.

In **Analysis → Checker → Compare moves** (German **Zugvergleich**), plays that
reach the same board share a row across saved eXtreme Gammon, GNU Backgammon and
NeonGammon analyses. Each engine column shows its rank, equity and actual
candidate depth. NeonGammon 2-ply analyses contain only the candidates its
move filter searched at full depth, plus the played move. Use the column selector to choose another saved analysis.
Select a row to show each step on the board in **Arrows** view. The preview
uses the recorded XG path when that candidate is available, followed by the
first local source otherwise.
Select an engine's rank, equity or depth cell to use that engine's move. If
the source stores only the endpoints, the preview reconstructs a legal route.
New GNU Backgammon analyses retain each original route available in the native
SGF output alongside the display notation. NeonGammon's current checker API
returns candidate end boards, so its move notation is derived from those boards.

Each engine column starts with the latest, deepest saved analysis of that
engine for the selected game or match scope. A dash marks a move that the
analysis does not contain. In the GNU Backgammon and NeonGammon columns,
**|Δ|** sorts the rows by the absolute equity difference to XG, largest first.
Rows without a comparable value come last, and selecting **|Δ|** again restores
the original order. Values are comparable only in the same known unit. Each
local equity cell also shows its difference to XG, and an arrow beside the
rank shows whether the engine ranks the move higher (↑) or lower (↓) than XG's
equity ranking. The tooltip names the XG rank.

In **Cube**, **Previous error** and **Next error** above the board also work
without a local analysis job. Review uses saved local game checkpoints from the
selected job and imported evaluations for games not covered by that job. Each
decision identifies its source and depth. Imported evaluations do not supply
native GNUbg PR or imply a complete native match analysis. The board shows the
filtered scope and the current result's index. When all games are selected, both
directions cycle through matching results across game boundaries. When the error
threshold is off, the board navigates all matching decisions. The board and the
filter pane explain why there are no matching decisions. Changing selection or
filters cancels pending navigation.

Cube errors open on the main board with the played action, best action, loss
and source. The inspector shows the recorded cube options with their unit and
the name of the player whose perspective is used. Older local results may only
provide the saved played and best equities. For imported decisions,
**Cube details** opens the original evaluation and notes. When a checker
position is selected, **Back to the checker move** closes the cube review and
returns the board to that position.

`GET /api/matches/{id}/review-decisions` supplies this paginated view. Numbered
pages count and select decisions in a derived list of the analysis's validated
decisions and return only the requested page. The list is rebuilt when the
analysis, its checkpoints or the source change; backups leave it out and a
restore clears it. Only evaluations with known units and a validated decision
context enter imported review. GNUbg SGF checker evaluations are supported; SGF
cube records without unambiguous units and perspective remain visible in their
original source view but are excluded from the combined error review. A missed double opens the
validated position before the roll. A take or drop shows the outstanding cube
offer from the original decision context.

### Cube context, take point and move explanations

Cube review, cube practice results, flashcards and Playing feedback describe a cube decision in words: match length or money rules, the points each player still needs (for example `Alice 7-away, Bob 6-away`), Crawford or post-Crawford, and the cube value with its owner or the pending offer. Offers list all three outcomes (**No double**, **Double / take**, **Double / pass**) with their unit and the player whose perspective is used, and mark the outcome reached when both sides act correctly. A position that is too good to double is labeled. A response lists **Take** and **Pass**.

New native GNUbg and NeonGammon analyses also save the cubeless win, gammon and backgammon chances of each cube decision, and the table shows them for both players. For a native analysis saved before this change, **Calculate winning chances** evaluates that one cube decision now with the practice engine and depth (the analysis depth, capped at 3-ply for GNUbg and 2-ply for NeonGammon). The result names its engine and depth, is shown as not part of the saved analysis and is not stored; recalculating the game saves the chances with the analysis. `POST /api/cube-chances` takes the decision's position and whether it is a take or pass. Imported analyses do not record chances.

The take point is the game winning chance at which taking breaks even, ignoring gammons and any later use of the cube. In money play it is 25 %. In match play NeonDojo derives it from the Kazaross-XG2 match equity table that GNUbg ships, using the Crawford entry when a result reaches a one-away score. The line names the table and, when chances are saved, how often the taker wins here. No take point is shown in the Crawford game, when the player cannot double, or beyond 25-away. `POST /api/cube-context` takes a GNUbg match ID and the doubler and returns this context. The table and its notice are in [met_kazaross_xg2.json](../../backend/src/met_kazaross_xg2.json).

When the played or chosen checker move is not the best one, **What the plays do differently** compares the two resulting boards. **Speaks for the best play** comes first with up to three differences, followed by up to two under **Speaks for the chosen play**, so a concession of the better play never stands alone. The differences are, in this order: the chance to win when both plays have saved probabilities, opponent checkers hit, opponent rolls out of 36 that hit a blot together with the pips those hits cost on average, points only one play holds, the longest row of adjacent points, checkers out of play on the 1 to 3 points while there is still contact, checkers stacked beyond the third on a point, blots left (only when the shots are equal), and the chances to win and to lose a gammon. A hit costs the pips from the blot back to the bar, so a blot in the opponent's home board weighs little and one in the own home board much; the pips decide which play the shots speak for, not the number of rolls. When no counted difference favors the best play, the section says that its advantage shows only in the engine's evaluation. It appears under the candidate table in Analysis, in Coach and Tutor details, and in checker practice results. `POST /api/move-explanations` counts these facts from the board, mover, dice and both notations. The counts describe the plays; the engine's equity decides which is better. **Terms** below the list explains equity, equity loss, PR and ply.

### Preview a move

**Initial**, **Arrows** (German: **Pfeile**), and **Final** show the starting position, the move path, and the resulting position. The default view uses outlined green arrows for the played move and shows legal intermediate points. Select a candidate row from an import, BackgammonDB, or local analysis to preview it. Use Enter or Space on the move name. Up/Down selects the previous or next candidate and keeps it in view. Left shows **Initial**, Right shows **Final**, and other controls keep their normal keyboard behavior. The selected board view persists when changing candidates. **Reset preview** returns to the recorded move.

A checker that uses multiple dice is usually shown by its start and end points, for example `24/15`. Hits remain explicit, for example `24/21*/15`. **Arrows** shows each die as a separate step and follows the recorded route when the source supplies it.

The board identifies the candidate source and rank. **Final** keeps the board orientation and hides the dice. All themes use the same legality checks for bar entry, hits, multi-step moves, doubles, and bear-off. Unrecognized or illegal notation shows the starting position. Analysis, notes, Position IDs, and study cards continue to use the pre-move decision. Changing position or source clears the previous candidate preview.

### Search from the analysis board

In Analysis, **Save position** and the magnifying glass share a toolbar above the board. The magnifying glass to the right of the bookmark shows **Search this position** on hover or keyboard focus. On narrow screens both icons move to the next line together.

Select **Search this position** to open **Match database** and run an exact search for the displayed position. The Position ID appears in the expanded search form and as an active filter. The action selects **All databases**, clears the other filters, and keeps the current sort order. Selecting the action again reruns the search. Switching tabs preserves any subsequent decision to remove the filter.

### Desktop layout

**Checker** places the board beside the checker candidate table. Open **Cube**
to review cube decisions on the board and **Statistics** for native match and game
statistics. Switching tabs keeps the selected game and move.

Drag the divider beside the board to resize it in **Checker**, **Statistics**,
**Cube**, **Notes**, or **Study cards**. The divider also accepts Left/Right
arrows, with Shift for larger steps. Double-click the divider or press End
to set the board to its maximum width for the current window. Home sets the
minimum width and Enter restores the default width. The browser stores separate widths
for **Checker**, **Cube**, **Statistics**, **Notes**, and **Study cards**.
On wide screens, the move history has room for more rows. **Statistics** uses
larger table text and side-by-side player summaries when there is room.
The comment field in **Notes** grows with the available height. **Study cards**
uses larger deck controls and a taller list with larger board previews. Every
analysis tab scales the artwork to fit above the navigation controls.
On wide displays, each analysis tab can use a board column up to
1680 logical pixels wide. The checker candidate table uses more of its pane on
wide displays, with larger text and row spacing when there is room.

The desktop layout is designed for side-by-side review using a mouse or keyboard. Layout checks use logical viewports of 1800×1169, 2560×1440, and 3840×2160, including under macOS **More Space**. The shared type scale uses 24 px page titles, 14 px section titles, 13 px controls and body text, and 12 px tables. CSS tokens define the type scale, with monospace for Position IDs and separate type for the board and logo. [Typography](../typography.md)

Text tabs use the active theme's accent and a 2 px underline. The same pattern covers workspace and analysis tabs, board views, analysis sources, checker and cube selection, note scopes, settings, and database selection. [Navigation style](../ui-navigation.md)

## Imported analysis and opening books

**Displayed analysis** is one persistent choice per match, shared by checker/cube review, move colors, statistics and new learning cards. The catalog lists all retained GNUbg, NeonGammon and imported results, including source, scope, depth, time and coverage. **Automatic** prefers a complete current whole-match analysis, then meaningful imported results, then a completed scoped analysis; time and stable identity break ties, never cross-engine ply counts. An explicit choice remains selected across reloads and calculation-profile changes. Missing positions in that analysis remain unavailable; no other result fills the gap. **Load more analyses** exposes older history. If an import contains several records for one decision, the shared projection prefers real values over known placeholders and then the first record in original source order. Original records remain intact and accessible through the source-analysis endpoints and original-file export. New calculations use the configured engine and depth, independently of this result selection. The expandable **Opening book** section is a separate reference. Imported XG, native GNUbg SGF, and available BGBlitz analysis stay in their own records, with the original depth or profile label, method, candidates, cube responses, raw values, equity units, and engine version where known. Missing analysis and unknown engines or units stay unavailable. GNUbg match-winning chances retain their original units. Analysis jobs and independent match copies preserve source results. Older imports can reconstruct analysis once from the stored original file. Source evaluations for recorded double, take, and drop actions also appear in **Cube and move notes**. Imported cube evaluations show whose perspective the values use when the source identifies the player. This includes positions where the player on roll did not double.

When you step through a game, the move colors and match statistics carry over while the board and candidates follow the current move. Changing games loads the new game's move colors. Changing the displayed analysis, replacing its source (a new source revision), or completing another analysis checkpoint refreshes the affected results; **Load more analyses** ignores stale pages if the source revision changes mid-request.

Manage books under **Settings → Analysis → Opening books** with **Manage opening books**. Select **Automatic** to compare available books, choose a provider directly, or select **Off**. The book dialog saves the provider and paths with revision checks. Apply or discard any Settings draft before opening it. Book paths are on the backend computer. The analysis inspector shows the selected source, engine, and depth for the current position. Book results remain an optional reference alongside the displayed analysis and do not contribute to match PR.

- **BackgammonDB** is a local SQLite book with 4,230,443 decisions across 351 contexts. It provides up to eight ranked moves for a position and roll, plus a separate pre-roll cube lookup. Coverage is pruned and score-dependent. Evaluations use GNUbg 1.08.003 at its regular 3-ply, shown as 4-ply in NeonDojo's [ply convention](settings.md#ply-convention). The app credits the CC BY 4.0 data at the result. [Data and format](https://github.com/chalkstreamdev/bgdb-opening-book), [book description](https://backgammondb.com/opening-book)
- **XG Opening Book 3.70** is an official `.ob` file with 53,210 pre-roll position evaluations. It provides a position reference with the original engine, profile or rollout, and author. Values from the previous player's perspective are converted to the player on roll. Historical depth codes that cannot be identified stay unknown. The terms allow free use with attribution to the rollout authors. [Official download and terms](https://www.extremegammon.com/GetOb.aspx)

Book lookups use the full decision context, including Crawford, away scores, cube value and ownership, and money-game rules. BackgammonDB has no beaver contexts. The XG book reads money-game rules such as Jacoby and beavers from the original header and requires a supported context. Its pre-roll values apply to the resulting position independently of any displayed dice.

Keep the downloaded book files outside Git, for example in `~/Library/Application Support/NeonDojo/` on macOS, and enter their backend paths in the book dialog. Extract the BackgammonDB serving SQLite file and the XG `OpeningbookV2.ob` file. The check and install endpoints validate the data formats directly.

## Game plan

**Game plan** classifies White's and Black's structures independently as **Pure race**, **Racing (contact)**, **Attacking / blitz**, **Priming** or **Anchoring**. Both sides are labeled **Opening** while each has at most four checkers off its starting points, which covers about the first two rolls per player. The opening label is NeonDojo's own addition on top of the pinned NeonGammon rules, so opening errors form their own mistake group. **Initial** and **Arrows** use the starting position. **Final** uses the displayed position after the move and falls back to the starting position if the move notation cannot be applied. The display is also available for saved positions whose source match has been deleted.

The classifier uses the board alone and needs no analysis job. Move and cube recommendations still come from the analysis engine. `POST /api/game-plans` validates the 26-entry board and returns both classifications with the rule version. The browser caches a bounded number of identical boards. Source and license information is in [game_plan_source.txt](../../backend/src/game_plan_source.txt) and [backend/licenses](../../backend/licenses/). [NeonGammon reference implementation](https://github.com/markbgsage/bgsage/blob/139427d826dd67798226e7d23c003c90315306ba/cpp/src/encoding.cpp)

## Checker and cube error statistics

Native statistics show a **Total / Checker / Cube** breakdown per player for **Inaccuracies**, **Questionable**, **Error**, **Blunder** and **Large Blunder**. Inaccuracies includes all four severity levels. Click a count to open the decisions with the matching player, decision type and severity. A count in the game overview also filters to that game. A verified zero opens an empty list.

NeonDojo reads the breakdown from stored decisions for fully checkpointed games, including those from earlier analyses. Forced, unavailable and loss-free decisions do not contribute to the error counts. If the stored decisions lack verifiable coverage, the UI shows **—** with an explanation. `GET /api/jobs/{id}/statistics` returns `error_breakdown` for the job and its games. The decision list validates its pagination cursor against the selected filters.

## Tags and comments

Open **Analysis → Notes** and select **Match**, **Game** or **Position** to edit its tags and comment. **Cube and move notes** also opens game notes and recorded actions such as doubles, takes, drops and accepted resignations, with the board immediately before each action. A pending double displays the offered cube value in the center. Games without checker moves can have notes too. Checker move notes use the same annotation as the corresponding position. When a match has saved import warnings (such as a MAT game ended before an illegal move or a header score mismatch), they appear read-only as **Import notes** above the match or game comment; see [Import formats](library.md#import-formats).

Press Enter or **Add tag** to add each tag. **Save notes** writes to PostgreSQL. Drafts survive navigation and dialog changes during the current session. If another edit has been saved, NeonDojo shows a version conflict and keeps the draft. Tags are normalized and limited to 30 per note and 64 characters each. Comments are displayed as text and can contain up to 20,000 characters.

Use **Search tags & comments** to combine free text (3–200 characters), an exact tag, a note type and a database filter. Results link to the relevant match, game, position or move. The regular match search also indexes annotations across those scopes.

## Analysis time estimates

### Before analysis

Before you start a game or match analysis, **Estimated analysis time: approx. …** appears below the scope and depth selectors. It multiplies the number of checker moves in scope by the average time per move from the most recent matching, fully completed benchmark. The UI shows the benchmark date, engine version, ply depth, and checker move count. Changing the match, game, scope, depth, applied profile, or engine installation invalidates an outstanding estimate. The estimate is hidden during submission and execution.

A matching benchmark uses the same local machine and hardware, engine installation and version, ply depth, and move filter, with compatible reference and analysis parameters. With a matching filter, completed GNUbg runs using the former 600-second per-game limit remain eligible. Calibration requires a successful, complete local run.

The estimate includes process startup and cube analysis. Time spent waiting for other jobs is outside its scope. Different types of positions can take substantially different amounts of time. If the benchmark, checker moves, or engine installation is unavailable, or the request fails, the UI explains the issue and still lets you start analysis. Estimates use existing measurements. A NeonGammon estimate takes the engine version from the verified installation and only checks that its Python interpreter still exists; it starts no engine process, so it answers at every depth without displacing a worker. The analysis itself verifies the installation completely when it starts.

`GET /api/matches/{id}/analysis/estimate?plies=2&game_no=1&engine_id=gnubg` returns the estimate and its measurement basis for the requested provider. Omit `engine_id` to use the saved analysis engine, `game_no` for the whole match, or `plies` to use the saved default depth. The endpoint is read-only and observes the source and Competition restrictions.

### During analysis

While a game or match analysis runs, the progress area shows **Estimated time remaining: approx. …**. The initial estimate uses the saved matching benchmark. Once a game completes, the estimate uses confirmed analysis time divided by the number of timed checker moves, multiplied by the number of remaining checker moves. This weights longer games according to their move counts. The display counts down in seconds between completed games, when the engine reports confirmed progress.

Timing uses a monotonic clock and excludes time in the queue or waiting for engine access. Confirmed work is saved with the job and survives reload and resumption. An interrupted game is timed again. Moves already completed by older jobs count toward progress. Only timed moves contribute to the measured average. The job keeps the measurement basis it started with when the user profile changes.

If a game outlasts its estimate, the UI shows **Recalculating time remaining …**. Without a matching benchmark, an estimate can appear after the first usable game measurement. The active estimate disappears when the job completes or fails.

`GET /api/jobs/{id}/remaining-time` returns status, the measurement basis, confirmed and timed move counts, and the current estimate. The read-only endpoint follows the same source and Competition restrictions as the job and starts no engine. Active timing is saved about once per second. The browser counts down monotonically between updates.

## Analysis jobs

GNUbg supports 1–5 ply and NeonGammon supports 1–4 ply for position, game and match analysis. The default analysis depth is 3-ply. A job captures its engine configuration and depth. Identical active jobs are deduplicated by scope, depth and engine configuration. Each backend runs one worker, and PostgreSQL `FOR UPDATE SKIP LOCKED` keeps workers from claiming the same job.

Jobs move through `queued → running → completed|failed`. The worker starts queued jobs in creation order up to the configured number of simultaneous analysis jobs (`1–8`, default `1`) and runs each in its own task. NeonGammon analyses share one general sidecar process and run one at a time, while GNUbg jobs can use any remaining slots. Hints, Coach and Tutor evaluations and Study answers use a process of their own and do not wait for it. GNUbg and all NeonGammon workers share the host CPU budget described in [Settings](settings.md#settings-and-engines), reserving threads per request so a waiting play turn, hint or Study answer can run between game-analysis batches. Batches hold up to 64 rows at 1–2 ply and 4 rows at 3–4 ply. NeonGammon 3.2.1 shares the thread budget across legal moves in 3- and 4-ply checker batches. On NeonGammon 3.2.0, deep checker rows share one worker request and each row can use all configured engine threads. A single remaining cube row also uses the full thread budget. A queued benchmark takes the next free slot, and no new job starts until it finishes. GNUbg has a 90-second process limit for position analysis and MAT conversion, 90 seconds per roll for a dice equity run, and 600 seconds for a game analysis. NeonGammon allows 180 seconds per requested row in a batch, 180 seconds for an individual evaluation, 600 seconds per game analysis, and 120 seconds for batch initialization or final integrity verification. The worker's 120-second lease renews every 20 seconds by default. A failed time sample for the remaining-time estimate is skipped, and a renewal that fails because no database connection is free or the connection broke is retried while the lease lasts; neither stops the analysis. After expiration, the job can be claimed again, up to three attempts in total. A regular engine error marks the job `failed` and the UI can submit a new job. **Cancel analysis**, in the match analysis dialog, the analysis progress and beside each background analysis in the Analysis header, stops a queued or running job at once and terminates its engine process. A cancelled job also ends as `failed`, with the error `Cancelled by user`, like a job a database restore interrupts; the UI shows it as cancelled. A worker that picks up a job just before cancellation checks its database lease before starting the engine. Games a match analysis completed before the cancellation stay saved.

SSE event types are `queued`, `running`, `progress`, `completed` and `failed`. The `id:` field is a durable event number and `data:` contains `JobProgress` JSON. `Last-Event-ID` resumes after that event. With no header, the server replays the event history. When a job finishes, the same transaction removes its `progress` events (migration `0094`); its `queued`, `running`, `completed` and `failed` events stay, and the final event carries the final progress. A stream resuming after a removed event continues with the next remaining one. A shared PostgreSQL listener wakes streams after a commit. The server also checks for missed events every ten seconds during inactivity and sends keepalives while the job is active. The stream closes if the job disappears or a finished job has no more events to replay. On disconnect the browser reads the job status and stops watching once the job has finished or is missing. SIGINT and SIGTERM close streams and terminate running child processes. On restart, the worker can reclaim expired jobs. The browser discovers saved position jobs from PostgreSQL and reconciles SSE with polling. It also loads match and game jobs after a browser reload. Jobs followed only in the background are read in groups of up to 100 every three seconds. Job monitoring does not choose the displayed analysis.

Match analysis validates and encodes a completed game's decisions before locking the job lease row, then checkpoints decisions in batches of 500 together with partial statistics and SSE progress in one transaction so lease heartbeats are not blocked by checkpoint preparation. `Job` and SSE report `completed_games` and `total_games`, so the percentage advances when a game finishes. After an expired lease, a replacement worker resumes at the next incomplete game. An expired worker cannot write checkpoints or results. Completed games and their reviewed decisions remain accessible after a later game fails. Submitting a new job analyzes the requested scope from the beginning.

New GNUbg and NeonGammon game and match analyses save the ranked checker candidates returned by the engine, including the played move, equity, outcome probabilities, and actual evaluation depth. The **Candidate moves** table displays these details for the selected saved result. Recalculate an older game or match analysis to obtain candidates if it only saved the played and best moves. To remove a saved analysis, select it under **Displayed analysis**, choose **Delete analysis**, and confirm the identified result. This keeps the match and its source file available.

GNUbg evaluation uses `analyse game` output at six decimal places. NeonDojo clears existing analysis annotations before the calculation, normalizes inherited match data and setups, and verifies board and match context for each checker and cube action. Mixed SGF collections can include empty or cube-only games. Missing or incomplete native exports fail the job. Each native export is capped at 64 MiB.

A forced pass has zero loss and no alternative equity (`evaluation_status=forced`). Analysis views apply the configured normalized equity thresholds to evaluated decisions. The API identifiers are `doubtful`, `bad`, `blunder` and `very_bad`. The `performance_rating` field includes the counts of relevant decisions and weighted native GNUbg PR. Cube equities use the deciding player's perspective and the cube value before the offer as their unit. Cube-only decisions have stable source action IDs and are accessible from **Cube and move notes** with their pre-action board.
