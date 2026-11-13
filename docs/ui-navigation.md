# Navigation style

BG Tutor uses the same text-navigation language in its workspaces, analysis
subviews, board views, sources, opening books, decision kinds, note scopes,
settings categories and database collections.

Navigation has a transparent background, muted inactive text and no rounded
button frame. The current item uses the theme accent, semibold text and a 2 px
underline. Hover remains transparent; keyboard focus has a visible accent
outline. Disabled choices retain native disabled behavior and reduced opacity.
The settings appearance preview depicts the same navigation.

In the match database sidebar, system databases use a database-with-gear icon
in the theme's accent color. The underline identifies the selected database.

Use `text-navigation` on the container and keep buttons as direct children.
Existing `aria-selected="true"`, `aria-pressed="true"`, `aria-current="page"` or
`active` state drives the underline. Keep existing keyboard handlers and ARIA
relationships. `text-navigation--compact` reduces vertical padding in dense
analysis panels; `text-navigation--stacked` arranges sidebar choices vertically.
The main header retains its 20 px gap; other horizontal groups use 16 px.
The header orders its workspaces as Analysis, Positions, Study, Play,
Match database, Player database, and Extras. The arrow keys follow this order.

The shared rules live in `web/src/text-navigation.css`, imported after the other
styles. Local CSS manages surrounding layout and should not reintroduce filled
navigation states or competing padding/gap rules.

Action buttons, field controls, bookmarks, sortable table headers, selected rows,
error badges, candidate previews and artwork/theme tiles retain their existing
visual roles. This keeps navigation, actions and content selection recognizable.

`web/e2e/text-navigation.spec.ts` verifies active, hover, disabled and keyboard
focus states in all six light/dark theme variants, actual spacing between labels,
and the established desktop viewports. Related workflow tests cover settings,
notes, sources, board views and header navigation.

## Search results

Result lists with multiple pages use the same underlined navigation. The
numbered buttons show up to five nearby pages. **First** and **Last** jump
directly to the ends of the current result set. A new filter or sort starts at
page 1. Match, saved position and flashcard selections can span pages.
In Positions, moving to another page previews its first result as soon as the
page loads. Checkbox selections carry across pages.

The player database uses the Positions layout with a result table on the left
and the selected profile on the right. Click a row or use the arrow keys to
change the profile. The match list begins below the profile header. Its
database filter and page controls sit outside the scrollable table. Edit and
merge actions open dialogs and return focus to their trigger when closed.
The merge search has its own page controls.

## Match analysis

**Overview** places the position analysis beside the board, with the error list
below both. Its analysis views are **Candidates**, **Dice equity**, and **Game
rolls**. Native match and game statistics are available in **Statistics**.

The position pane shows saved results with their source and depth. **Recalculate
position …** opens the search depth and the start action in a dialog. The dice
view has its own **Recalculate dice equity …** dialog. **Analyze game or match …**
reveals the scope, engine, depth and time estimate in the command bar. The
scope and engine select the jobs used by **Errors** and **Statistics**. The
latest job for that selection and other running jobs tracked in the session
appear above the analysis tabs.
Open **Compare game analyses** or **Compare match analyses** to see GNUbg and
Open Sage side by side for the selected game or the whole match. The game comparison also uses the selected game’s saved statistics from completed whole-match analyses. Each engine defaults to a completed 3-ply run when
available, with other recorded depths and job states in its selector. The table
shows each player's saved checker and cube decision counts and equity losses.
GNUbg's native PR is shown in the GNUbg column when available.
Above the board, **Previous error** and **Next error** follow the list filters
and show the current result's index and the total. When all games are selected,
the buttons cycle across game boundaries. Turning off **Inaccuracies only**
switches the buttons to **Previous decision** and **Next decision**.
A checker error keeps the move preview controls available.
A cube error shows the original decision context with its played and best cube
actions. **Played move** and **Best move** switch the checker preview while
preserving the selected board view.
Candidate columns size to their contents so notation and evaluation values
stay close on wide displays.
Engine labels show the name and the recorded version when available. Hover
over the label or focus it with Tab to see the recorded details, such as search
depth, evaluation method, build, model, and thread count. The same labels are
used for local and imported analyses, in Playing, and in Study.
The board and an empty error list explain whether evaluations are missing, the current
filters hide all matches, or the evaluated decisions have no inaccuracies.
The actions open analysis settings, clear the selection filters while keeping
the current error threshold, or show all decisions in the selected scope.
Switching tabs keeps the selected game and move. Selecting a statistics metric
opens **Errors** with the matching filters. Selecting a metric for a different
game opens that game.

In every desktop analysis tab, a focusable separator resizes the board. Left/Right
changes its width, Shift uses larger steps, Home/End select the available limits,
and Enter or double-click restores the default for the current view. Escape
cancels a drag. The profile stores separate widths for **Overview** and for the
four detail tabs as a group. Overview caps the board to leave space for the error
list below. The other tabs can use the full available height. The default
board grows gradually on windows wider than 1800 logical pixels. A saved
manual width takes precedence and is limited by the space in the current window.

## Study areas

Study opens on Overview, followed by Flashcards & Anki, Reports, My recurring
mistakes, and Practice history. Left/Right wrap between all five tabs. Home
selects Overview and End selects Practice history. Opening a deck from another
workspace still selects Flashcards & Anki.

Overview counts each due card once across its decks and can start the oldest due
card directly. When no cards are due, it recommends the selected player's
eligible mistake group with the highest equity loss per 100 decisions, if
available. Recommendations require at least 30 evaluable decisions and errors
in at least two matches. The existing practice dialog opens from the
recommendation or the group table and
returns focus to the button on close. Overview also shows the next scheduled
card when the due list is empty.

In Flashcards & Anki, **Manage decks** opens a list searchable by deck name.
**Import Anki package** accepts several `.apkg` files at once and imports their
source decks and rendered card faces when the backend has Anki's Python package
configured. The import shows progress and lists failed files by name. To add
multiple BG Tutor packages to the selected deck, use **Import BG Tutor Anki deck**.
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
On wide windows, drag the divider between the card list and the preview. The
Left and Right arrow keys move the focused divider. Double-click it or press
Enter to restore the default width. The browser remembers the chosen width.
The results have selection
checkboxes and actions above the table. Cards can be added to a deck or deleted
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

**Saved topics** restores applied card filters and their deck scope. Edit
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
Completed local analyses take precedence per game; comparable imported XG and
BGBlitz analyses cover games without one. Practice displays the original source,
engine and search/rollout depth; unknown depth remains explicitly unknown.

## Profile in der Navigation

Die globale Kopfzeile enthält die Profilauswahl mit aktivem Namen, Änderungsmarkierung und Sicherungsstatus. „Profile verwalten“ öffnet direkt die Kategorie „Profile“ im Einstellungsdialog.

Die Profilkategorie bietet Anlegen aus der aktuellen Konfiguration, Aktivieren, ausdrücklich bestätigtes Überschreiben und Löschen sowie Umbenennen, Zusammenfassung und Unterschiede. Ein nicht angewendeter Einstellungsentwurf wird vor dem Wechsel in diese Kategorie ausdrücklich angewendet oder verworfen; die Aktion kann auch abgebrochen werden.

Normale Einstellungsänderungen werden mit „Anwenden“ übernommen. Die Profilsicherung bleibt eine eigene Aktion. Suchansichten übernehmen beim Profilwechsel die gespeicherten Kriterien, während Notizentwürfe, laufende Aufträge und bereits aktive Lernkarten ihren Arbeitsstand behalten.

## Extras

**Extras** contains **Benchmark**, **Engine duels**, **Database backup**,
and **Versions**. Use Left/Right or Home/End to switch tabs. BG Tutor remembers
the selected tool. The link in Settings opens **Benchmark**.

Benchmark uses one engine selector for **Match analysis** and **Play-turn
latency**. Each engine keeps its own depth selections while the app is open.
The selected measurement type determines the setup, result columns and
comparison. Active runs remain visible across both measurement types.
Reference methodology and per-run technical data can be expanded on demand.

Engine duels lists series beside the selected result on wide desktop windows.
**New series** opens setup with the optional seed under **Advanced**. Series
continue when leaving the tool.

Database backup shows the scope of a restore before file selection. BG Tutor
automatically checks each selected archive and compares its schema with the
destination. The preview shows the creation time and counts for matches, saved
positions, and flashcards. Older archives can lack a creation time. Only a
compatible archive enables **Review database replacement**.
After **Replace database**, the backend checks the archive again and validates
the rows during the restore transaction. Reload BG Tutor after success.

Versions shows the build and both engines. **Technical details** gives access
to the full commit, the engine version output, and the source commit for
recognized Open Sage forks. **Copy version information** copies the report.

## Dice equity

Selecting **Dice equity** in the position evaluation automatically requests the
21 dice-pair evaluations used by its 36-cell grid. There is no separate Dice
Distribution action. The latest saved result for the current position and
engine is reused if its search depth matches, after saved jobs have loaded.
Changing position or depth while this view is visible requests missing results;
hidden evaluation views
do not start work. Running jobs keep their captured depth and configuration.
After a submission or engine failure, selecting **Dice equity** again retries
once; failures never cause an automatic retry loop.
