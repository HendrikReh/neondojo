# Navigation style

NeonDojo uses the same text-navigation language in its workspaces, analysis
subviews, board views, sources, opening books, decision kinds, note scopes,
settings categories and database collections.

Navigation has a transparent background, muted inactive text and no rounded
button frame. The current item uses the theme accent, semibold text and a 2 px
underline. On hover the background stays transparent; an inactive item turns
the accent color over a subtle underline. Keyboard focus has a visible accent
outline. Disabled choices keep native disabled behavior and reduced opacity.
The settings appearance preview shows the same navigation.

In the match database sidebar, system databases use a database-with-gear icon
in the theme's accent color. The underline identifies the selected database.

Use `text-navigation` on the container and keep buttons as direct children.
An `aria-selected="true"`, `aria-pressed="true"` or `aria-current="page"`
attribute, or the `active` class, drives the underline. Keep existing keyboard
handlers and ARIA relationships. `text-navigation--compact` reduces vertical padding in dense
analysis panels; `text-navigation--stacked` arranges sidebar choices vertically.
The main header retains its 20 px gap; other horizontal groups use 16 px.
The header orders its workspaces as Playing, Analysis, Study, Positions,
Match database, Player database, and Extras. Rendering and keyboard navigation
share the `views` list in `web/src/app/App.tsx`: Left/Right move through it and
wrap, and Home/End select the first and last workspace.

The shared rules live in `web/src/styles/text-navigation.css`, imported after the other
styles. Local CSS manages surrounding layout and should not reintroduce filled
navigation states or competing padding/gap rules.

Action buttons, field controls, bookmarks, sortable table headers, selected rows,
error badges, candidate previews and artwork/theme tiles retain their existing
visual roles. This keeps navigation, actions and content selection recognizable.

`web/e2e/text-navigation.spec.ts` verifies the active and hover states and the
spacing between labels in one light (`graphite` light) and one dark (`club`
dark) palette, while `web/e2e/ui-themes.spec.ts` verifies all six light/dark
theme palettes. It also checks the header order with mouse and keyboard,
keyboard focus in the analysis and notes tabs, and the absence of horizontal
overflow at the desktop viewports 1800×1169, 2560×1440 and 3840×2160. Related
workflow tests cover settings, notes, sources and board views.

## Search results

Result lists with multiple pages use the same underlined navigation. The
numbered buttons show up to five nearby pages. **First** and **Last** jump
directly to the ends of the current result set. A new filter or sort starts at
page 1. Match, saved position and flashcard selections can span pages.
In Positions, moving to another page previews its first result as soon as the
page loads. Checkbox selections carry across pages.

The match database keeps its action icons visible above the results. They
start disabled and become available for the current selection. Opening,
editing, and resuming apply to one match. Resume requires a paused or unfinished
play session. Batch actions are available when the selected matches support them.
**Filters**, **Notes**, **Position ID** and **Duplicates** toggle the expandable
search panels above the table. **Duplicates** groups matches with the same
played record in the selected database (or across **All databases**), separates
**Identical files** from **Same moves**, and offers **Select copies** to select
every copy except the oldest match in each listed group (up to 100 matches) for
the toolbar batch actions. A warning icon next to PR marks matches that carry
saved import notes.

The player database uses the Positions layout with a result table on the left
and the selected profile on the right. Click a row or use the arrow keys to
change the profile. The column headings sort the result table by name, matches
or games. The table starts sorted by most matches and shows the first profile.
The profile header shows the PR, and the match list begins below it. Click a match row or press Enter on it to open the match; the
opponent's name switches to that player's profile. The database filter and page
controls sit outside the scrollable table. Edit and merge actions open dialogs
and return focus to their trigger when closed. The merge dialog focuses its
search field. **Analyze unrated matches …** beside the PR opens the
match analysis dialog for the profile's matches without a rating.
The merge search has its own page controls.

**Virtual profiles** sits between the page title and the player search. Its
buttons select a virtual profile in place of a real one, and **My player
profile** opens first when one is chosen. A virtual profile's member names
switch to that member's profile. **Add member …**, **Rename …**, **Delete …**
and **New virtual profile …** open dialogs that return focus to their trigger;
the member and name dialogs focus their field.

## Positions

**Saved positions** and **All analyzed decisions** switch the Positions
workspace between the bookmarked collection and a search over every analyzed
decision of one player. They stay visible while **Similar positions** is open
and close it. The decision search keeps its filters above a result
table with a board preview on the right. **Practice results** opens the
recurring-mistakes practice dialog with the errors of the current page.

In **Saved positions**, the header checkbox selects the current page and the
selection actions appear in the toolbar above the table, so the rows keep
their place. The remove icon of a row shows for the hovered, focused or
previewed row and keeps its 44 px target. Removing positions or deleting a
saved search offers **Undo**. **More filters** groups its fields by match,
position, analysis, and tags and comments. Missing values show as a dash.

The page title carries the result count. **Saved searches** is one field in
the search row: choosing a search loads it, and the three-dot button beside it
holds save, delete and reload. The preview has no heading of its own, grows
with the window and shows equity loss, decks, tags and notes below the board;
only the source details stay collapsed. Deck choice and Anki export belong to
Study; after adding positions to a deck, **Study this deck** appears beside
the transfer outcome.

**All analyzed decisions** uses the same filter card and keeps its filters,
results and chosen row while the saved positions are open. Before the first
search it names **Search decisions** as the next step. The preview of a checker
decision offers **Add to deck**. A row is selected
with a click or the arrow keys; Enter or a double-click opens the decision in
Analysis (checker decisions in **Checker**, cube decisions in **Cube**). The
**Loss** heading reverses the order.

## Keyboard shortcuts in practice and review

Buttons that have a shortcut show it as a keycap badge (↵ for Enter, Esc,
⇧E for Shift+E), name it in their tooltip, for example `Undo step (Z)`,
and expose it through `aria-keyshortcuts`. In the narrow Playing panel the
badge sits on the button's top corner so labels keep one line; icon-only
buttons keep the key in the tooltip only. **N**, **D**,
**T** and **P** choose a cube action, **Enter** checks an answer and then
continues, **A** shows the answer, **1**–**4** rate a flashcard, and **E** /
**Shift+E** move to the next or previous error (or decision, without an error
filter) in Analysis. In **Playing**, **Enter** rolls, confirms a complete turn
or passes, also after a Coach or Tutor warning, and continues Training and
Tutor feedback, **D** doubles, **H** requests a hint, **Z** undoes a checker step,
and **Escape** returns from a reviewed move to the game.

The shortcuts are declared with the `shortcut` property of `ActionButton` and
handled once in `web/src/app/useButtonShortcuts.ts`. Only visible, enabled buttons
respond. Shortcuts are ignored in text fields, and **Enter** is left to a
focused button, link, tab or row. An open dialog confines them to its own
buttons.

Left and Right have separate handlers. In Analysis, outside controls and tab
lists, they step to the previous or next position of the game. While a
candidate or dice-grid preview is selected in Analysis, or a move from the
history is reviewed in Playing, they show the **Initial** and **Final** board
instead (`web/src/shared/review/useBoardReviewShortcuts.ts`).

## Match analysis

**Checker** places the position analysis beside the board. Its analysis views
are **Candidate moves**, **Compare moves**, **Dice equity**, and **Game rolls**.
**Game rolls** loads the selected game's recorded roll frequencies on demand
when opened, so switching games paints the board without waiting for observed
rolls; failed requests offer **Try again**. Move history defaults to
**All moves**. Filter by **All errors** or by a specific grade without changing
the board selection. Unrated moves remain visible under **All moves**.
**Previous error** and **Next error** sit above the board whenever the
displayed analysis has matching checker errors.
**Cube** reviews cube decisions on the board and opens on the next matching
decision from the board position when none is selected. Native match and
game statistics are available in **Statistics**. Matches imported with
irregularities show their **Import notes** count in the analysis header and
list the read-only **Import notes** above the match or game comment in **Notes**.
**Displayed analysis** in the header chooses one saved result for checker and
cube review, move marks, statistics and new learning cards. The list starts
with engine and evaluation depth, followed by scope and date. The selected
entry's details include origin and coverage.
The choice persists per match and is independent of calculation preferences.
Missing results remain explicitly unavailable. Older results are paginated;
**Load more analyses** includes them in the same selector. The **Candidate moves**
table shows rank, equity, outcome probabilities, and evaluation depth for
imported and local analyses. Imported XG checker moves are ranked by descending
equity and show the original depth on every row. Older game and match results may
only show the played and best moves. Recalculate the game or match to save its
candidates.
Open **Compare moves** beside **Candidate moves** to see eXtreme Gammon, GNU Backgammon,
and NeonGammon for the current position. The table aligns moves by their ending
boards and shows rank, equity, and actual depth per candidate. The XG column
shows the rank by equity when the saved values are comparable. Choose a
different saved analysis in a column when available. The comparison starts with the deepest loaded result per engine and prefers the
latest result at that depth. The local columns show their equity minus XG
equity for candidates with comparable saved values. Rank arrows compare each
local rank with the XG rank by equity. A dash indicates a missing value. Select
**|Δ|** in a local engine’s Equity column to sort by the largest absolute
differences to XG, with missing differences at the bottom. Select the active
sort button again to restore the original order.
Select a move row or activate its move button to show the route on the board
in **Arrows** view. Up and Down move the preview to the adjacent row.
Select a cell in an engine column to preview that engine's move. Each engine's
rank button also works with Enter or Space.
Use **Delete analysis** beside the selector to permanently remove the chosen
result after confirming its details. The selector returns to **Automatic** if
the explicitly selected result is deleted. Deleting imported analysis values
preserves the original match file.
**Opening book** is an expandable reference beneath the selected result.
Opening **Dice equity** displays the saved calculation or starts a missing one.
**Analysis options** in the header includes **Recalculate position …** and
**Recalculate dice equity …** for explicit calculations at a selected depth,
and **Analyze entire match …**, which opens the whole-match analysis dialog for
the open match; a completed run opens its **Statistics**.

**Analyze game** in the Analysis header always starts the selected game. Its
options show the engine, depth and estimated time. In the **Match database**,
select matches with the checkboxes and use **Analyze selected matches …** in
the toolbar above the list. **Open match**, **Resume match** and **Edit match
details** are disabled for multiple selected matches. For a single match, the
analysis dialog shows the engine, depth and estimated time. When the analysis
finishes with the dialog open, NeonDojo opens **Statistics** for the whole match and
selects the completed analysis. If you close the dialog during the
run, the analysis continues in the background. Open the dialog again and
choose **Show results** to view a saved analysis.
For multiple matches, the dialog uses the same engine and depth for each
available match and adds their jobs to the background queue. It reports
rejected submissions and allows you to retry them from the dialog.
**Bulk analysis** above the match list shows how many accepted jobs for the
selected database have completed and counts failed jobs separately, including
after the dialog closes. **All databases** shows the combined progress.
The match list refreshes as tracked analyses complete.
The **Analysis** column shows queued and running whole-match jobs before the
first game result is saved. Any previous match PR remains visible during a rerun.
Reloading the Match database during a bulk analysis reopens the list and
refreshes the progress from the saved jobs.
Analysis jobs use the selected engine and depth. A profile conflict
does not block these jobs; pending changes
remain available for recovery in **Profiles**. Progress and job controls monitor
calculations without choosing a displayed source. **Compare saved analyses**
lets the user select any two catalog entries side by side without changing the
match's displayed analysis.

The checker candidate table grows with its pane, with room to expand beyond
1280 logical pixels on wide displays. Its text and row spacing increase as
more space becomes available.
Engine labels show the name and the recorded version when available. Hover
over the label or focus it with Tab to see the recorded details, such as search
depth, evaluation method, build, model, and thread count. The same labels are
used for local and imported analyses, in Playing, and in Study.
Selecting a checker or cube metric opens the corresponding tab with the
matching filters and scope. Combined metrics offer both tabs as destinations.
**Checker** shows review controls alongside the candidate analysis until you
close the statistics selection. Selecting a metric for a different game opens
that game.

The board and the filter pane explain whether evaluations are missing, the
current filters hide all matches, or the evaluated decisions have no inaccuracies.
The actions lead to the analysis start button, clear the selection filters
while keeping the current error threshold, or show all decisions in the
selected scope.

In every desktop analysis tab, a focusable separator resizes the board. Left/Right
changes its width, Shift uses larger steps, and Home/End select the available
limits. A double-click sets the maximum width for the current window and
saves it in the profile. Enter restores the default. Escape cancels a drag.
The profile stores a separate board width for each tab. On wide screens the
board and move history grow. The move history filter stays visible while the
rows scroll. **Statistics** places the player summaries side by side when
their pane is wide enough. The comment field in **Notes** grows with the
available height. On wide screens, **Study cards** gives deck controls more
room and enlarges the board previews in the suggestions list. The board scales
to the available height above the move navigation and position ID in every
analysis tab. The default width grows
with the window and leaves room for the analysis pane. A saved manual width
controls the board column within the current window's limits.

## Playing

On large desktop windows, the board grows and the move history uses the height
of the playing area. The history scrolls independently and follows the latest
move. The layout leaves room for the action panel beside the board. The panel
names the current step, always shows its primary action (**Roll dice**,
**Confirm turn** or **Pass**) and carries the instructions for that step. The
dice pair and the cube on the board remain a second way to trigger the same
actions.

During a match, one bar above the board replaces the page title. It shows the
score, format, engine and turn status, then the play mode, **Resign game**,
**Pause match** and the **More actions** menu with **Forfeit match**, **New
match** and **Paused Matches**. After a finished money game, the result dialog
offers **Next game**, **Pause match** and **End session**. Escape closes the
**More actions** menu and returns focus to its button.

The **New match** form is a dialog over the playing view, anchored to the
board like the other play dialogs. It makes the playing view behind it inert
but leaves the app header usable. Opened from **New match** it takes the
focus; opened automatically when Playing has no match, the focus stays in the
header so keyboard navigation across the workspaces continues. Name, bot
engine and strength come first, followed by color, match length and play mode.
When **NeonGammon** is selected as the bot engine, **Playing strength** groups
**Difficulty presets** (**Beginner**, **Casual**, **Intermediate**, and
**Full Strength · 3-ply checkers / 2-ply cube**, annotated with approximate
5-point PR labels when **Match length** is 5 points) above the fixed depths
(`1-ply`, `2-ply`) and **2-ply + Top-2 (10 mEq)**. Switching to NeonGammon for
a match starts at `2-ply`. Color and play mode are
radio choices, **Start match** is the primary button, and the gear beside the
analysis and rules engine line opens **Settings → Playing**. When a match is
already open, **Back to match** (or **Resume match** if paused) appears beside
**Start match** and **Paused Matches**. **Close** and
Escape dismiss the dialog; without a match, **New match** above the empty
board reopens it.

While a move from the history is reviewed, an accent frame around the board
and the primary **Return to game** button mark that the board does not show
the live position.

## Study areas

**Reports → Open report** opens a saved point match in a dedicated view.
**AI assessment** and **Decisions and attempts** use the same underlined
navigation. Left/Right switches tabs, Home/End selects the first/last tab,
and Escape returns to the archive. Focus returns to the report opener.
The match-end action in **Playing** opens **AI assessment** directly.
The assessment can continue while you switch views. Select a decision
reference to open its original position next to the report and continue
with the same board controls used for learning review.

The Study workspace carries a translated name outside English: Lernen,
Estudio, Étude, Nauka and Studio.

Study opens on Overview, followed by Flashcards & Anki, Reports, My recurring
mistakes, and Practice history. Left/Right wrap between all five tabs. Home
selects Overview and End selects Practice history. Opening a deck from another
workspace still selects Flashcards & Anki.

Overview counts each due card once across its decks and can start the oldest due
card directly. When no cards are due, it recommends the selected player's
eligible mistake group with the highest equity loss per 100 decisions, if
available. Recommendations require at least 30 evaluable decisions and errors
in at least two matches. The existing practice dialog opens from the
recommendation or the group table and returns focus to the button on close. Overview also shows the next scheduled
card when the due list is empty.

In Flashcards & Anki, **Manage decks** opens a list searchable by deck name.
**Import Anki package** accepts several `.apkg` files at once and imports their
source decks and rendered card faces when the backend has Anki's Python package
configured. The import shows progress and lists failed files by name. To add
multiple NeonDojo packages to the selected deck, use **Import NeonDojo Anki deck**.
In **Manage decks**, create, open, rename or delete a deck. When a specific deck
is selected, its study and export actions appear below the search.
**Send to Anki** transfers the selected native deck to local Anki Desktop
through AnkiConnect.
**Sync Anki reviews** exchanges missing reviews for the selected native deck
with the active Anki profile. The result shows how many cards matched and how
many reviews each application received.
Enable **Sync reviews automatically** to run the same exchange in the
background when the linked Anki profile is open. The deck shows the linked
profile and the last successful sync. Enable it again after restoring a backup.
Open **Anki sync** above the card filters to see each linked deck's last
successful sync, transfers in both directions, and pause or error reasons.
**Sync now** runs once with the linked profile. **Pause** stops future
background runs, and **Resume** returns the deck to the next background check.
Open **History** in a deck row to see the last 100 attempts with their time,
sent and received reviews, and any error details.
Use **Failed runs only** for all failures and **Open errors only** for unresolved
ones. **Retry sync** starts a fresh exchange and adds a history entry. A successful
retry marks the selected error as **Resolved** and keeps its details available.
Select the **Open errors** count in a deck row to open its history with
**Open errors only** already selected.
**Decks with open errors only** filters the overview. Decks disappear from that
view when their last error is resolved.
**Retry all open errors** runs one exchange per affected deck and resolves its
open errors on success. Its results show each deck's status, sent and received
reviews for the attempt, and remaining open errors.
On wide windows, drag the divider between the card list and the preview. The
Left and Right arrow keys move the focused divider, Shift uses larger steps,
and Home/End select the limits. Double-click it or press Enter to restore the
default width; Escape cancels a drag. The browser remembers the chosen width.
The results have selection checkboxes and actions above the table. Cards can be added to a deck or deleted
from the collection after confirmation. Select a deck to move cards or remove
its memberships, or choose **Without a deck** to find unassigned cards. **Find duplicates**
shows shared deck memberships and duplicate records, with confirmation before
removing a membership or deleting a copy.

In Flashcards & Anki, Study now opens the next due card. Checker cards accept a
complete turn on the board, with undo before submission. Cube cards use separate
offer and response choices and show the offered cube on response cards. Check
the answer to see your choice, the best answer and the analysis source, or use
Show answer for self-assessment. A checked answer reopens at the review step
after a reload. The four FSRS buttons schedule the next review.

**Practice results** starts a shuffled round from the applied card filters.
After an answer, **Mark for review** adds that card to the optional second
pass alongside incorrect answers. **Resume practice** reopens the latest
round. Practice rounds leave FSRS reviews and due dates untouched.

**Transfer practice and saved topics**, folded between the search and the
results, holds both tools. **Saved topics** restores applied card filters
and their deck scope. Edit
**Card tags** from selected rows or after revealing a solution. Use the
transfer panel for baseline and follow-up sessions. Select one card for pip
count practice, or a cube card for score variants. The timed preview uses
the current search and has separate question and answer durations.

My recurring mistakes requires a player selection and offers database, game-plan
and minimum-loss filters. Players in the selected database remain selectable
before their matches are analyzed. The view reports when the current filters
yield no evaluable decisions. The table shows the number of evaluable decisions,
error rate, and equity loss per 100 decisions where the error type has a defined
denominator. Smaller samples are labeled and remain available for practice.
Practice opens a modal from the selected group. Missed-double rounds can include
correct rolls. Wrong-take rounds can include correct takes, and wrong-pass rounds
can include correct passes. These rounds use saved examples from the selected
player, database, and game plan and present them in shuffled order.
Play a checker move on the original board or choose a cube action and check it.
The evaluation and the original decision are shown together. Reveal is also
available. Next hides the solution again.
Escape closes practice and returns focus to its trigger. Leaving the area or
changing its filters dismisses practice and cancels outstanding requests.
Each match uses its selected catalog analysis. Comparable imported XG, GNUbg SGF
and BGBlitz decisions are available through the same selection. Missing scopes
remain absent; decisions from different analyses are never combined implicitly.

## Settings

With focus in the settings sidebar, **Up Arrow** and **Down Arrow** switch to
the previous or next category. **Home** and **End** open the first or last
category. Switching to **Profiles** with a draft opens the same confirmation as
clicking the category.

On desktop, **Appearance** and **Board** put the selection on the left and
the preview on the right. Opening **Board** brings the selected tile into
view. You can then scroll the gallery independently of the preview. The
chosen board has a strong outline and a **✓ Selected** label on its thumbnail.
Both previews update as you select options and apply to the rest of the app
when you choose **Apply**.

Analysis shows the engine and default depth side by side, followed by four
editable error thresholds and the configured opening-book source. The thresholds
classify normalized equity loss in error review, move history and statistics.
In new matches, the Error threshold sets when Coach pauses for checker errors
and missed doubles, and sets the initial Tutor warning threshold. Expand
**Engines on this computer** for paths, installation checks, and thread limits. The collapsed section shows the
availability of GNUbg and NeonGammon.

Closing **Opening books** returns to **Analysis** in Settings and restores focus
to **Manage opening books**.

**Playing** puts the engine and strength for new matches side by side. With
NeonGammon selected, **Strength for future games** includes
**2-ply + Top-2 (10 mEq)** in addition to fixed depths, saved in the active
configuration profile. The next group contains the default play mode and
automatic training analysis. Game sounds and bear-off direction are grouped
below them.

Closing Settings or opening Profiles, Opening books, or Benchmark with an
unapplied draft asks whether to apply it, discard it, or keep editing. Escape
in the confirmation returns to the draft. **Apply** appears for a changed draft
and names the affected save destinations. If a newer configuration was saved
in another window, **Apply** retries the changed settings against that version.
**Apply** saves the draft and keeps the current category open. Board theme
changes appear immediately on the open board. Choose **Close** to return to
the workspace. If an earlier automatic save is in conflict, unrelated
edits remain available for recovery in **Profiles**. **Verify and save** stores
the NeonGammon Python path on the NeonDojo computer.

## Profiles in navigation

The global header contains the profile switcher with the active name, change marker, and save status. Warnings and a failed switch collapse into one warning badge beside it, which opens a popover with the details, so the header keeps a single row. Switching from a modified profile offers **Save and switch** as the primary choice. **Manage profiles** opens the **Profiles** category directly in the Settings dialog.

Database selection, search filters, result sorting, the active Study deck, card creation, and analysis filters belong to the browser tab's session. The browser stores columns, column widths, and the analysis board layout locally. Profile switches and reloads preserve these views.

The profile category shows the active profile and the number of changes from the saved profile at the top. The selected profile determines the comparison with the current configuration. The full saved configuration can be expanded below. Timestamps in the profile list follow the app language.

**Save in profile** shows the selected target profile. **Save as new profile** sits next to it. **Profile actions** contains **Rename** and **Delete**. **Restore saved profile** applies the active profile's saved values after confirmation. In the comparison, columns keep their width and long values wrap within their cell.

On a profile conflict, the notice appears in the app language directly below **Active profile**, before the profile list and the save actions. There you can choose **Load server settings** or **Save local changes as a new profile**.

**Apply** adopts the current settings. Named profiles are saved in the **Profiles** category. If loading fails, the profile controls and profile management offer **Retry**. Playing and analysis use the last confirmed settings or the built-in defaults. Running jobs keep the settings captured when they started.

## Extras

**Extras** contains **Benchmark**, **Engine duels**, **Database backup**, and
**Versions**. Use Left/Right or Home/End to switch tabs. NeonDojo remembers the
last chosen tab on this browser. The link in Settings opens **Benchmark**.
The selected tab names the tool, so the tools show no separate page title; each
keeps its heading for assistive technology.

Benchmark uses one engine selector for **Match analysis** and **Play-turn
latency**, which share a segmented switch below it. Each engine keeps its own
depth selections while the app is open.
The selected measurement type determines the setup, result columns and
comparison. Active runs remain visible across both measurement types.
The comparison table lists the median and range of the three most recent
matching runs for each engine and depth. History shows total times of one
second or more in seconds, minutes, or hours, with the exact milliseconds in the
tooltip, and shows progress counts only for runs that did not complete.
History headings (**Date**, **Engine**, **Ply**, **Total analysis time**,
**Ø analysis time / move** or **Ø play-turn latency**, and **Status**) sort the
table and reverse on a second click. Finished runs (completed, failed, or
cancelled) can be deleted permanently after an inline confirmation.
Reference methodology and per-run technical data can be expanded on demand.

Engine duels lists series beside the selected result on wide desktop windows.
**New series** sits above the series list and opens setup, with the optional
seed under **Advanced**. Series continue when leaving the tool. Results show
progress and both match win rates in one card with a share bar. When the sample
supports a confidence interval, a note says whether it includes 50%. Paired
matches are grouped by pair. Each result row offers **Analysis**, which opens
the match in the Analysis workspace, and an SGF download. **Repeat series …**
reopens setup with the settings and seed of the selected series. A series that
stopped without a counted match shows its status and diagnostic without empty
result tables.

Database backup shows the scope of a restore before file selection. NeonDojo
automatically checks each selected archive and compares its schema with the
destination. The preview shows the creation time and counts for matches, saved
positions, and flashcards. For older archives without a stored creation time,
a valid NeonDojo filename supplies the time, marked **from filename**. Only
a compatible archive enables **Review database replacement**.
The confirmation dialog shows the creation time and compares the backup counts
with the destination counts fetched when the dialog opens.
The **Difference** column shows the expected change in each count after
replacement. Increases are green with a plus sign, decreases are red with a
minus sign, and zero uses the regular text color.
After **Replace database**, the backend checks the archive again and validates
the rows during the restore transaction. Reload NeonDojo after success.

Versions shows the build and both engines. **Technical details** gives access
to the full commit, the engine version output, and the source commit for
recognized NeonGammon forks. **Copy version information** copies the report.

## Dice equity

Selecting **Dice equity** in the position evaluation loads the latest saved
calculation for the current position from the active analysis engine. If one
is still running, the tab waits for it. A missing calculation starts with the active engine and profile depth
and evaluates the 21 dice pairs used by the 36-cell grid.
Changing position requests missing results while this tab is visible.
Running jobs keep their captured depth and configuration.
The grid starts on the roll shown on the board and outlines it. Selecting a
cell previews that roll's best play on the board with its dice; **Reset
preview** and the left and right arrow keys work as for candidate previews.
After a submission or engine failure, selecting **Dice equity** again retries
once. Use **Analysis options → Recalculate dice equity …** to choose a different
depth explicitly. **Delete dice equity** removes the displayed finished
calculation after a confirmation and does not start a new one until
**Dice equity** is selected again.
