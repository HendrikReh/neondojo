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

## Match analysis

**Overview** places the position analysis beside the board, with the error list
below both. Its analysis views are **Candidates**, **Dice equity**, and **Game
rolls**. Native match and game statistics are available in **Statistics**.
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

Study contains Flashcards & Anki, Reports, My recurring mistakes, and Practice
history. Left/Right wrap between all four tabs; Home and End choose the first and last. Opening a
deck from another workspace still selects Flashcards & Anki.

My recurring mistakes requires a player selection and offers database, game-plan
and minimum-loss filters. Practice opens a modal from the selected group, with
the original board and a separate Reveal action. Next hides the solution again.
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

Extras contains **Benchmark** for analysis speed and turn latency,
**Engine vs. engine** for playing point matches, and **Version information**
for the running backend and configured engines. The BG Tutor row shows the first
seven characters of the build's Git commit ID, with the full ID on hover. The
Open Sage row identifies recognized fork builds by repository and short commit,
linked to the full commit on GitHub. The benchmark link in
Settings always opens **Benchmark**. Engine-match series keep running
when the user switches tools or leaves Extras.

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
