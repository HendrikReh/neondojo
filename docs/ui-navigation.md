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
The analysis command bar uses the profile engine as the initial selection for
each match. Choose **Analysis engine** to select the provider for game or match
analysis, then choose a supported **Analysis depth**. GNUbg and Open Sage offer 4-ply. The
time estimate uses the selected engine, depth, and scope. The status below the
command bar shows the latest job for the selected engine and scope across all
analysis tabs. Switch engines to view their respective jobs.
Open **Compare game analyses** or **Compare match analyses** to see GNUbg and
Open Sage side by side for the selected game or the whole match. The game comparison also uses the selected game’s saved statistics from completed whole-match analyses. Each engine defaults to a completed 3-ply run when
available, with other recorded depths and job states in its selector. The table
shows each player's saved checker and cube decision counts and equity losses.
GNUbg's native PR is shown in the GNUbg column when available.
Candidate columns size to their contents so notation and evaluation values
stay close on wide displays.
Switching tabs keeps the selected game and move. Selecting a statistics metric
opens **Errors** with the matching filters. Selecting a metric for a different
game opens that game.

In every desktop analysis tab, a focusable separator resizes the board. Left/Right
changes its width, Shift uses larger steps, Home/End select the available limits,
and Enter or double-click restores the default for the current view. Escape
cancels a drag. The profile stores separate widths for **Overview** and for the
four detail tabs as a group. Overview caps the board to leave space for the error
list below. The other tabs use the available window height for a larger board.
The displayed limit can change without overwriting either preferred width.

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
**Import Anki package** opens a file picker and imports the source decks and
rendered card faces when the backend has Anki's Python package configured.
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
the selected tool. The backup tool exports a database archive and restores it
on an installation with the same schema after confirmation.

Engine duels lists series beside the selected result on wide desktop windows.
**New series** opens setup with the optional seed under **Advanced**. Series
continue when leaving the tool.

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
