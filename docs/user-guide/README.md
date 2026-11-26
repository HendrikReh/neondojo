# NeonDojo User Guide

NeonDojo is a local backgammon workbench. You can play against an engine, import and analyze matches, search your library for positions, and turn your own mistakes into practice material. Everything runs on your own computer. Your matches, notes and learning progress stay in your local database.

This guide walks through every workspace in the header and each of its tabs. Each chapter starts with what a newcomer needs and ends with details for stronger players. Exact rules, limits and edge cases are in the [reference](../index.md#feature-reference); a *Details* link at the end of a section leads to the matching part.

| Level | You are … | Start with |
| --- | --- | --- |
| **Novice** | learning the rules and basic strategy, or new to engine analysis | [Playing](#2-playing), the [Coach and Tutor modes](#play-modes), and the [glossary](#13-glossary) |
| **Advanced** | playing tournaments or online and want to find your recurring errors | [Analysis](#3-analysis), [My recurring mistakes](#my-recurring-mistakes), and [flashcards](#flashcards--anki) |
| **Professional** | preparing seriously, comparing engines, or managing a large library | [Compare moves](#compare-moves), [Displayed analysis](#displayed-analysis-and-new-calculations), [All analyzed decisions](#all-analyzed-decisions), [Engine duels](#engine-duels) |

Tips for a particular level are marked with these labels:

> **Novice:** basic tips for getting started.
>
> **Advanced:** working more efficiently and drawing better conclusions.
>
> **Pro:** precise details about sources, depth and units.

## Contents

1. [Finding your way around](#1-finding-your-way-around)
2. [Playing](#2-playing)
3. [Analysis](#3-analysis)
4. [Study](#4-study)
5. [Positions](#5-positions)
6. [Match database](#6-match-database)
7. [Player database](#7-player-database)
8. [Extras](#8-extras)
9. [Profiles and Settings](#9-profiles-and-settings)
10. [Learning paths by level](#10-learning-paths-by-level)
11. [Keyboard reference](#11-keyboard-reference)
12. [Troubleshooting](#12-troubleshooting)
13. [Glossary](#13-glossary)

---

## 1. Finding your way around

Open NeonDojo in your browser at <http://127.0.0.1:8787>. The header is the same in every workspace.

![The NeonDojo header with its seven workspaces, the profile selector, Settings and both manual links](images/header.webp)

| # | Header item | What you do there |
| --- | --- | --- |
| 1 | **Playing** | Play a match or money session against GNUbg or NeonGammon, with optional feedback. |
| 2 | **Analysis** | Review a match move by move, compare candidate moves, and study cube decisions and statistics. NeonDojo opens here. |
| 3 | **Study** | Review flashcards, read learning reports, and practice your recurring mistakes. |
| 4 | **Positions** | Browse saved positions and search every analyzed decision of a player. |
| 5 | **Match database** | Import matches, organize them into databases, search, filter, export, and start bulk analyses. |
| 6 | **Player database** | Look up players, their aliases, PR and match history. |
| 7 | **Extras** | Run engine benchmarks and engine duels, back up the database, and check versions. |
| 8 | **Profile** | Switch configuration profiles and see whether the current settings differ from the saved profile. |
| 9 | **Settings** (gear) | Set the language, appearance, engines, error thresholds, playing defaults and board artwork. |
| 10 | **NeonDojo manual** | Open this user guide in a new browser tab. |
| 11 | **NeonGammon manual** | Open the [NeonGammon engine manual](https://hendrikreh.github.io/neongammon/) in a new browser tab. |

Each workspace contains further tabs, as the map below shows.

```mermaid
flowchart LR
  H["Header"] --> P["Playing"]
  H --> A["Analysis"]
  H --> S["Study"]
  H --> PO["Positions"]
  H --> M["Match database"]
  H --> PL["Player database"]
  H --> E["Extras"]
  H --> SET["Settings"]

  A --> A1["Checker"]
  A --> A2["Statistics"]
  A --> A3["Cube"]
  A --> A4["Notes"]
  A --> A5["Study cards"]
  A1 --> A11["Candidate moves"]
  A1 --> A12["Compare moves"]
  A1 --> A13["Dice equity"]
  A1 --> A14["Game rolls"]

  S --> S1["Overview"]
  S --> S2["Flashcards & Anki"]
  S --> S3["Reports"]
  S --> S4["My recurring mistakes"]
  S --> S5["Practice history"]

  PO --> PO1["Saved positions"]
  PO --> PO2["All analyzed decisions"]

  E --> E1["Benchmark"]
  E --> E2["Engine duels"]
  E --> E3["Database backup"]
  E --> E4["Versions"]

  SET --> SET1["General · Appearance · Analysis · Playing · Board · Engines · Profiles"]
```

**Keyboard:** Focus a header tab and use **Left**/**Right** to move between workspaces. **Home** selects Playing and **End** selects Extras. The same keys work in the tab rows inside each workspace.

**Navigation style:** The current tab is shown with the accent color and an underline. Buttons with an icon show a tooltip when you hover over them or focus them with **Tab**. If a button has a keyboard shortcut, the tooltip names it.

**Home:** Select the NeonDojo logo to see the last analysis board and continue at the displayed game and move. Quick start opens computer match setup and position search. You can also reopen your unfinished computer match or study available cards. On your first visit, select **Import match**, drop a file or folder, or open an existing match from the database.

**What NeonDojo remembers:** Each browser tab keeps its own database selection, search filters, sort order, study deck and analysis filters. Column choices, column widths and board sizes are stored in the browser. The selected match, game and move are restored after a reload. Details: [Configuration profiles](../reference/settings.md#configuration-profiles)

### The typical workflow

Most users follow the same loop. You can enter it at any point.

```mermaid
flowchart LR
  I["Import a match<br/>(Match database)"] --> AN["Analyze it<br/>(Analyze game / Analyze selected matches)"]
  PL["Play the engine<br/>(Playing)"] --> AN
  AN --> R["Review errors<br/>(Analysis: Checker, Cube, Statistics)"]
  R --> SV["Save positions<br/>and create flashcards"]
  R --> RM["Find recurring mistakes<br/>(Study)"]
  SV --> ST["Study due cards<br/>(Study: Flashcards & Anki)"]
  RM --> PR["Practice a mistake group"]
  ST --> PL
  PR --> PL
```

---

## 2. Playing

**Playing** pits you against GNU Backgammon (GNUbg) or NeonGammon. Depending on the play mode, NeonDojo stays silent, explains your errors afterwards, or stops you before you play a mistake.

### Starting a match

![The New match form with color, match length and play mode](images/playing-setup.webp)

The **New match** dialog opens over the board. Your current match stays visible behind it.

1. Check **Your name** and choose the **Bot engine** and its **Playing strength**. These start from your settings and apply only to this match.
   - With **NeonGammon** (`3.1.0`), **Playing strength** offers four **Difficulty presets** (**Beginner**, **Casual**, **Intermediate**, and **Full Strength**) alongside fixed depths (`1-ply`, `2-ply`) and **2-ply + Top-2 (10 mEq)**. For five-point matches, the presets show about 20.3, 15.8, 9.3, and 1.0 PR, respectively. Full Strength uses 3-ply for checker moves and 2-ply for cube decisions. In **2-ply + Top-2**, NeonGammon re-evaluates the two leading checker plays at 3-ply in contact positions when they are within 0.010 equity at 2-ply, while other checker moves and all cube decisions stay at 2-ply. Switching **Bot engine** to NeonGammon for a match starts at `2-ply`.
2. Choose **Your checkers**: **White** or **Black**. Your checkers are always shown at the bottom of the board.
3. Choose the **Match length**: a **Money game** or a match of 1, 3, 5, 7, 9, 11, 15, 21 or 25 points.
4. Choose a **Play mode**. The modes are described in the next section.
5. Select **Start match**.

The line above the button shows which engine analyzes your decisions and which rules engine runs the game. Select the gear button beside it to change the saved engine defaults in **Settings → Playing**. A match keeps the engine settings it was started with. Close the dialog with **Close** or Escape, or select **Back to match** (or **Resume match** if paused) when a match is open; without a match, **New match** above the board opens it again.

**Paused Matches**, in the dialog or above the empty board, opens the paused sessions in the Match database. From there you can resume one.

Details: [NeonGammon difficulty presets](../reference/playing.md#neongammon-difficulty-presets)

### Play modes

| Mode | During the game | Best for |
| --- | --- | --- |
| **Competition** | No analysis, pip counts, error marks or PR until the match ends. | Testing yourself under realistic conditions. |
| **Training** | Full equities and win, gammon and backgammon chances after your decisions. | Learning why one play is better than another. |
| **Coach** *(default)* | Compact feedback with equity differences and the history's severity colors. | Everyday practice. |
| **Tutor** | Warns you **before** you play a checker or cube decision that reaches the error threshold. | Novices, and anyone drilling a weakness. |

Each new match takes the **Error** threshold from **Settings → Analysis**, 0.020 by default. Coach uses it for checker moves and for skipping a double; your own doubles, takes and passes always get Coach feedback. Tutor uses it as the starting **Tutor warning threshold**, which you can change for the current match beside the **Play mode** menu in the match bar. That menu also switches the mode during a match. The current position and any checkers you have already moved stay as they are.

```mermaid
flowchart TD
  D["You choose a move<br/>or a cube action"] --> Q{"Equity loss at or above<br/>the threshold?"}
  Q -- "No" --> OK["Played immediately<br/>and rated in the move history"]
  Q -- "Yes, Coach" --> C["Feedback appears before the turn is played.<br/>Click your dice again to play it,<br/>or open Details and try another move."]
  Q -- "Yes, Tutor" --> T["Warning dialog:<br/>Try again · Show solution · Play anyway"]
  T --> K["Your first attempt is kept<br/>for the learning review"]
  C --> K
```

> **Novice:** Start with **Tutor** in short matches of 3 or 5 points. Every warning is a free lesson, and the learning review shows your first attempt next to the better play.
>
> **Advanced:** Use **Coach** for normal practice and play a **Competition** match from time to time. Tick **Only matches played without learning aids** in Study to measure yourself on those matches alone.

Details: [Playing feedback modes](../reference/playing.md#playing-feedback-modes)

### The match screen

![A match in progress in Coach mode with the match bar, move history, board and action panel](images/playing-match.webp)

- **Match bar** (top): score, match format, game number, the bot, analysis and rules engines, and whose turn it is. Hover over or focus the bot engine to see its version, playing strength (depth or preset) and bear-off direction. On the right are **Play mode**, **Resign game**, **Pause match** and the **⋯** menu with **Forfeit match**, **New match** and **Paused Matches**.
- **Move history** (left): every move with notation, dice and severity colors. Select a past move to review it. An accent frame around the board and the **Return to game** button show that the board is not showing the live position. **Download SGF** exports the match at any time.
- **Board** (center): your checkers are at the bottom. Player rows show names, points and pip counts. Pip counts are hidden in Competition mode.
- **Action panel** (right): names the current step and shows its main button, which is **Roll dice**, **Confirm turn** or **Pass**, together with **Double**, **Hint** and **Undo step** where they apply. A disabled button explains why it is unavailable.

**Moving checkers**

1. Click the dice, or the gap between them, to roll. You can also press **Enter**.
2. Hover over a checker to see where it would go. Click it to move it. If both dice could be used, the first die takes priority. Click the dice before you move to swap their order.
3. **Undo step** (**Z**) takes back one unconfirmed step at a time.
4. **Confirm turn** (**Enter**) completes the move.

**The doubling cube**

- Click the cube, or select **Double** (**D**), to offer a double before you roll. When the cube lifts as you hover over it, you are allowed to double.
- When the bot doubles, a dialog asks you to **Take double** or **Pass double**.

**Hints:** **Hint** (**H**) opens a movable dialog with the engine's suggestions. It is not available in Competition mode. Use **Up**/**Down** to step through them, and **Initial**, **Arrows** and **Final** to preview each one on a separate board. Using a hint is recorded as a learning aid.

**Resigning, pausing and forfeiting**

- **Resign game** lets you offer a single, gammon or backgammon resignation. The dialog shows the points at stake and the resulting score. GNUbg decides whether to accept.
- **Pause match** saves the last confirmed position, including the score, dice and any pending decision. Resume it later from the paused board or from **Match database → Paused Matches**.
- **Forfeit match** in the **⋯** menu concedes a point match after confirmation.

**After a game:** A dialog shows the winner, the points won and how they were calculated, for example *Gammon (2) × Cube 2 = 4 points*. After a money game it offers **Next game**, **Pause match** and **End session**. A learning review of each completed money game is saved automatically. You can find it under [Study → Reports](#reports).

**Where played matches go:** Every bot match is archived in **Match database → Bot Matches**, where you can analyze, tag, export or copy it like an imported match.

> **Pro:** The bot's strength in **Settings → Playing** is independent of the analysis depth used for feedback. Learning reviews report a *first-attempt PR*, computed from your original choices at the positions you actually reached. This PR is separate from GNUbg's rating of the moves you finally played, and it requires GNUbg as the analysis engine; a review analyzed with NeonGammon shows no first-attempt PR.

Details: [Playing against an engine](../reference/playing.md#playing-against-an-engine)

---

## 3. Analysis

**Analysis** is the main review screen for a match. It opens with a welcome panel until you open a match.

![The Analysis welcome panel with Open match and Import match](images/analysis-welcome.webp)

Select **Open match** to go to the Match database, select **Import match**, or drop match files or a whole folder on the panel. See [Importing](#importing) for the supported formats.

### Screen layout

![The Analysis workspace with numbered areas](images/analysis-checker.webp)

| # | Area | Purpose |
| --- | --- | --- |
| 1 | **Match header** | Players, match ID, length, result, date, event (plus round and venue when present), PR per player if a complete analysis exists, and the **Import notes** count if the import recorded irregularities. |
| 2 | **Displayed analysis** | Chooses which saved analysis is shown everywhere in this match. The trash button deletes the selected analysis after confirmation. |
| 3 | **Game selector** | Choose a game, or use the arrows for the previous and next game. Each entry shows the starting score, the number of moves and the Crawford status. |
| 4 | **Analysis tabs** | **Checker**, **Statistics**, **Cube**, **Notes** and **Study cards**. |
| 5 | **Analyze game / Analysis options** | Start an analysis of the selected game, or choose the engine and depth and recalculate a single position or its dice equity. The progress bar shows running jobs. |
| 6 | **Move history** | Every move of the game with notation, player, dice and equity loss. Doubles, takes and passes appear between the checker moves. |
| 7 | **Board views** | **Initial**, **Arrows** and **Final** show the position before, during and after the selected move. |
| 8 | **Save position / Search this position** | Bookmark the decision into [Positions](#5-positions), or search the library for the same board. |
| 9 | **Evaluation pane** | **Candidate moves**, **Compare moves**, **Dice equity** and **Game rolls**. |
| 10 | **Position ID** | GNUbg's 14-character identifier of the board. Use it to search for the exact position. |

The arrows below the board step to the previous and next move. With no control focused, **Left** and **Right** do the same. **Game plan** below the board classifies each side's structure, as described in the [glossary](#13-glossary).

**Move history colors:** Each row shows the equity loss of the move. `0.000` means the best move was played and a dash means the move has not been evaluated. Colored badges mark the grades from *Questionable* to *Large Blunder*, as defined in the [error grades](#error-grades) table. Hover over a value to see its grade, source and depth. Use the menu at the top of the list to show **All moves**, **All errors** or a single grade. The filter does not change the board selection.

**Resizing the board:** Drag the divider beside the board, or focus it and use **Left**/**Right**. Hold **Shift** for larger steps. **Home** and **End** jump to the smallest and largest width. Double-click the divider to make the board as large as the window allows, and press **Enter** to restore the default size. Each tab remembers its own board width.

Details: [Analysis workspace](../reference/analysis.md#analysis-workspace)

### Checker

The Checker tab shows the move history, the board and the evaluation of the selected move side by side.

#### Candidate moves

![Candidate moves table with equity, loss and win, gammon and backgammon chances](images/analysis-candidates.webp)

- The header line names the **engine**, the **depth** (for example *2-ply cubeful*), the **unit** and **whose perspective** the values are shown from.
- **Best** marks the engine's choice and **Played** marks the recorded move.
- **Equity** is the expected result of each move. **Loss** is the difference to the best move.
- **Wins** and **Losses** show total, gammon and backgammon chances. Total includes gammons and backgammons; the gammon column excludes backgammons.
- Select a move, or press **Enter** on it, to preview it on the board. **Up**/**Down** move to the next or previous candidate. **Left** shows *Initial* and **Right** shows *Final*.
- **What the plays do differently** compares the best play with the chosen play on the board. It counts facts such as hits, the opponent's shots and their pip cost, points held, the longest row of adjacent points, checkers out of play or stacked, blots, and gammon chances. The counts describe the plays; the engine's equity decides which one is better. **Terms** explains equity, equity loss, PR and ply.
- **Opening book** expands a reference from BackgammonDB or the XG Opening Book, if one is configured.

> **Novice:** You don't need to read every column. Compare the **Loss** of your move with the grade colors. Then switch to **Arrows** and look at what the best move does differently on the board.

Details: [Candidate comparison and decision review](../reference/analysis.md#candidate-comparison-and-decision-review)

#### Compare moves

![Compare moves aligns eXtreme Gammon, GNU Backgammon and NeonGammon candidates by their resulting position](images/analysis-compare.webp)

**Compare moves** lines up saved eXtreme Gammon, GNU Backgammon and NeonGammon analyses of the same position. Moves that reach the same board share a row. Each engine column shows the rank, equity and actual search depth. A dash means that engine has no value for that move.

- Use the menu in a column header to choose another saved analysis of that engine.
- Select **|Δ|** in a local engine's equity column to sort by the largest difference from XG. Arrows show whether the local rank is higher or lower than XG's rank.
- Select a cell to preview that engine's version of the move on the board.

> **Pro:** Compare moves starts with the deepest loaded result for each engine. The local columns show their equity minus XG's equity only where the values are comparable. Equities are never compared across different units.

#### Dice equity

![Dice equity grid with 36 cells, color-coded by the expected equity of each roll](images/analysis-dice.webp)

**Dice equity** shows how good each roll would be in the current position. The 6 × 6 grid covers all 36 ordered rolls. They come from 21 distinct pairs: each double counts once (1/36) and each other pair counts twice (2/36). **Ø** is the weighted average over all rolls. Green cells are at or above the average and yellow cells are below it. Select a cell to see the best play for that roll and its probability, and to show that play on the board. When a roll has no legal move, the forced pass is evaluated.

If no saved result exists, NeonDojo starts the calculation automatically with the engine and depth of your active profile. After a failure, select **Dice equity** again to retry. Use **Analysis options → Recalculate dice equity …** to calculate again at a different depth. **Delete dice equity** beside **Ø** removes the displayed result after confirmation.

> **Advanced:** Dice equity shows which rolls you are playing for and which rolls hurt you. It helps when you are deciding between a safe play and a bold one.

#### Game rolls

**Game rolls** shows how often each roll came up in the selected game (observed frequencies, not an equity analysis). It loads on demand when you open the tab so switching games paints the board without waiting for roll counts; if loading fails, choose **Try again**. Use it to check whether a game was decided by the dice.

![Game rolls shows the distribution of rolls in the selected game](images/analysis-rolls.webp)

### Analysis options and Analyze game

![Analysis options with engine, depth, Recalculate position and Recalculate dice equity](images/analysis-options.webp)

- **Analyze game** analyzes the selected game with the engine and depth shown in **Analysis options**. The time estimate is based on your [benchmarks](#benchmark). To analyze whole matches, use [Analyze selected matches …](#bulk-actions) in the Match database.
- **Recalculate position …** and **Recalculate dice equity …** calculate only the current position at the depth you choose.
- Analyses run as background jobs. They keep running when you switch tabs or close the dialog. Progress and remaining time appear in the header.

NeonDojo numbers depths the same way for GNUbg and NeonGammon, like XG: 1-ply is the raw evaluation. GNUbg's own manual calls that level 0-ply, so NeonDojo's 3-ply is GNUbg's 2-ply.

> **Pro:** Every job keeps the engine version, depth and move filter it started with, even if you change Settings while it is running.

Details: [Analysis time estimates](../reference/analysis.md#analysis-time-estimates) · [Analysis jobs](../reference/analysis.md#analysis-jobs) · [Settings and engines](../reference/settings.md#settings-and-engines) · [Ply convention](../reference/settings.md#ply-convention)

### Displayed analysis and new calculations

A match can have several saved analyses: imported XG, GNUbg, BGBlitz or HedgeHog values from the original file, and local GNUbg or NeonGammon analyses at different depths. **Displayed analysis** picks **one** of them for the checker and cube review, the move colors, Statistics and new flashcards.

```mermaid
flowchart TD
  C["Saved analyses of this match<br/>(imported XG / GNUbg SGF / BGBlitz,<br/>local GNUbg, local NeonGammon)"] --> SEL{"Displayed analysis"}
  SEL -- "Automatic" --> AUTO["1. a complete current whole-match analysis<br/>2. otherwise meaningful imported results<br/>3. otherwise a completed partial analysis"]
  SEL -- "Explicit choice" --> EXP["exactly that analysis,<br/>kept across reloads"]
  AUTO --> USE["Checker and Cube review, move colors,<br/>Statistics, new flashcards"]
  EXP --> USE
  NEW["Analyze game / Recalculate"] -. "uses the engine and depth<br/>from Analysis options" .-> C
```

- Positions that the chosen analysis does not cover stay **unavailable**. NeonDojo never fills the gaps with values from another analysis.
- **Load more analyses** shows older results in the same list.
- **Delete analysis** permanently removes the selected result after confirmation. Deleting imported values never changes the original match file.

Details: [Imported analysis and opening books](../reference/analysis.md#imported-analysis-and-opening-books)

### Statistics

![Statistics tab with decisions, normalized equity loss, native PR and an error breakdown per player](images/analysis-statistics.webp)

The Statistics tab shows the selected analysis for each player:

- the number of **decisions**, split into checker and cube decisions
- the **normalized equity loss**
- the **native PR** (performance rating)
- an **error breakdown** by grade, as Total, Checker and Cube

Click any count to open the matching decisions in the **Checker** or **Cube** tab. A count in the game overview filters to that game.

**Compare saved analyses** puts any two saved analyses side by side, for example XG and GNUbg 3-ply. The displayed analysis does not change.

> **Pro:** GNUbg counts decisions differently from XG, so PR values from the two programs are not directly comparable. PR requires a complete native analysis. Imported analyses don't supply native PR.

Details: [Checker and cube error statistics](../reference/analysis.md#checker-and-cube-error-statistics) · [How PR is calculated](../reference/library.md#table-and-metadata)

### Cube

![Cube tab reviewing a doubling decision with the cube evaluation on the right](images/analysis-cube.webp)

The Cube tab reviews **doubling decisions only**. Use **Next error** (**E**) and **Previous error** (**Shift+E**) above the board to step through them. Filter by player, severity and game. **Inaccuracies only** hides decisions below the *Questionable* threshold. **Reset filters** (the eraser button) restores the default filters.

For each decision you see:

- the action that was **played** and the **best** action, together with the loss and the analysis source
- the **cube context**, described in words: match length, how many points each player still needs, Crawford status, and the cube value and owner
- the **cube outcomes** *No double*, *Double / take* and *Double / pass*, with their unit and perspective, and a note such as *Too good to double*
- the **take point** for this score, which is 25 % in money play and is derived from the Kazaross-XG2 match equity table in match play
- **Calculate winning chances**, for older analyses that did not save cubeless chances
- **Cube details**, for imported decisions, which opens the original evaluation and notes
- **Back to the checker move**, which returns to the related checker play

> **Advanced:** Compare the game winning chances with the take point. If your chances are well above the take point, it is a take. Gammon chances and later cube use shift the real decision. The engine's equities include both, so check them before you rely on the simple rule.

Details: [Cube context, take point and move explanations](../reference/analysis.md#cube-context-take-point-and-move-explanations)

### Notes

![Notes tab with Match, Game and Position scopes, tags and a comment field](images/analysis-notes.webp)

Choose **Match**, **Game** or **Position** and add **tags** and a **comment**. Press **Enter** or **Add tag** after each tag. A note can have up to 30 tags of 64 characters each and a comment of up to 20,000 characters. Select **Save notes** to store them. Unsaved drafts survive switching tabs. If someone else saved a newer version, NeonDojo reports a conflict and keeps your draft.

**Cube and move notes** lists game notes and recorded actions such as doubles, takes, passes and accepted resignations. Each entry shows the board immediately before the action. You can search tags and comments from the [Match database](#search-and-filters) with **Notes**.

Details: [Tags and comments](../reference/analysis.md#tags-and-comments)

### Study cards

![Study cards tab with deck choice, minimum loss, player and Suggest positions](images/analysis-study-cards.webp)

Use this tab to turn analyzed positions into flashcards.

1. **Choose deck**, or create one with **Create deck**.
2. Select **Add current position**, or set a **Minimum loss** and a **Player** and select **Add errors from game &lt;N&gt;** (or **Add match errors** when no game is selected) to import that player's errors from the selected game.
3. **Suggest positions** proposes up to ten decisions from the whole match. Up to three of them come from mistake groups that recur for this player in other matches, and the rest are the largest losses. Choose the cards you want to keep.
4. Optional **Topic tags** help you filter cards later. Use `::` for subtopics, for example `cube::take-point`.

A card is identified by its board, its full match context and its question type. Adding the same decision to another deck reuses the card and its review progress.

Details: [Create and manage decks](../reference/study.md#create-and-manage-decks)

---

## 4. Study

**Study** brings together everything that helps you remember what you learned. It has five tabs.

### Overview

![Study overview with the next step, due flashcards and recurring mistakes](images/study-overview.webp)

- **Next step** suggests what to do now. **Study due cards** opens the deck that holds the card that has been due longest. If no cards are due, **Practice mistake group** opens your costliest mistake group.
- **Due flashcards** shows today's due reviews and new cards. **New cards per day** (0 to 1,000, 10 by default) limits how many cards you see for the first time each day. Set it to 0 to pause new cards. Due reviews are always offered.
- **My recurring mistakes** shows up to three mistake groups of your player that occur in at least two matches. **Open mistake overview** opens the full list.

Details: [Study](../reference/study.md#study)

### Flashcards & Anki

![Flashcards & Anki with the card search, deck, actions and card preview](images/study-flashcards.webp)

- **Choose deck** selects the deck. **Manage decks** creates, renames and deletes decks.
- Search cards by text, player on roll, deck or **More filters**, which include topic tags, game plan, card type and review status. Select up to 200 cards for the bulk actions **Add to deck**, **Move cards**, **Remove from this deck** and **Delete permanently**. Only the last one deletes review history, and it asks for confirmation.
- **Study now** opens the next due card of the deck.
- **Practice results** starts a shuffled round of up to 10 or 20 cards from the current filters. **Retry errors and marked cards** repeats the ones you missed or marked. These rounds never change your review schedule.
- **Timed preview** shows the filtered cards with separate question and answer times.
- Expand **Transfer practice and saved topics** below the search for these two tools:
- **Transfer practice** checks whether a lesson transfers to new positions. It offers a baseline and a follow-up round of unseen decisions. Repeat the same topic after 7 to 14 days.
- **Saved topics** stores a set of card filters for later.
- **Find duplicates** finds cards that are stored more than once.
- **Anki**: **Download Anki deck** exports an `.apkg` file, and **Import NeonDojo Anki deck** reads such a file back into the selected deck. **Import Anki package** imports third-party decks. **Send to Anki** and **Sync Anki reviews** exchange cards and reviews with Anki Desktop through AnkiConnect. **Sync reviews automatically** keeps a deck in sync in the background, and **Anki sync** shows the status of each linked deck.

#### Studying a card

![A flashcard asking "What move would you play?" on the board](images/study-card.webp)

1. Read the context: match length, score, whose turn it is, and the cube value and owner.
2. Answer:
   - **Checker card:** play the whole move on the board. **Undo step** corrects it before you check it.
   - **Cube card:** choose **No double** or **Double** (keys **N**/**D**), or **Take** or **Pass** (keys **T**/**P**).
3. **Check move**, or **Check answer** on a cube card (**Enter**), evaluates your answer and shows the best answer and its source. **Show answer** (**A**) reveals the solution without grading, so you can assess yourself.
4. Rate how well you remembered it: **Again**, **Hard**, **Good** or **Easy** (keys **1**–**4**). NeonDojo schedules the next review with the FSRS algorithm at a target retention of 90 %.

```mermaid
stateDiagram-v2
  [*] --> New: card created
  New --> Answering: due or new allowance
  Answering --> Checked: Check move (graded)
  Answering --> Revealed: Show answer (self-assessment)
  Checked --> Scheduled: Again · Hard · Good · Easy
  Revealed --> Scheduled: Again · Hard · Good · Easy
  Scheduled --> Answering: next due date
  note right of Scheduled
    Again = about 1 minute
    first Hard = 10 minutes
    later intervals in whole days (FSRS)
  end note
```

A legal answer that loses less than 0.040 equity counts as **playable**.

> **Novice:** Be honest when you rate a card. *Again* only means you will see the card again soon. It is not a penalty.
>
> **Advanced:** Rate **after** you have read the explanation. The rating is for your memory, while the check grades your play. NeonDojo keeps the two separate.

Details: [Flashcards and Anki](../reference/study.md#flashcards-and-anki)

### Reports

![Reports archive with money sessions and match reports](images/study-reports.webp)

A **learning review** records your decisions in a game or match from Playing. Money games are reviewed automatically after each game. For a point match, create the review with **Learning review** in the move history, or with **Create AI assessment** when the match ends. Reports lists all saved reviews grouped by money session or match. Each entry shows the date, score, play mode, whether learning aids were used, the analysis depth and the first-attempt PR. Select **Open report** to see it.

- **Decisions and attempts** lists your first attempts, retries and corrections. Select a decision to see the original position and the engine's recommendation, with **Initial**, **Arrows** and **Final** views.
- **AI assessment** (optional, point matches only) writes a summary covering ten topics, up to three key decisions and practice tasks. It is created only when you start it. An OpenAI API key must be configured on the backend. Personal names, notes and raw SGF are left out of the request.

Use **Left**/**Right** to switch between the report tabs and **Escape** to return to the archive.

Details: [Reports](../reference/study.md#reports) · [AI assessment](../reference/study.md#ai-assessment)

### My recurring mistakes

![My recurring mistakes grouped by game plan and mistake type with a Practice button per group](images/study-mistakes.webp)

Choose a **Player**, and optionally a **Database**, a **Game plan** and a **Minimum equity loss** (0.08 by default). NeonDojo groups the player's errors by **game plan** and **mistake type**.

| Column | Meaning |
| --- | --- |
| Game plan | The structure of the position: *Opening*, *Pure race*, *Racing (contact)*, *Attacking / blitz*, *Priming* or *Anchoring*. |
| Mistake type | *Checker play*, *Missed double*, *Premature double*, *Wrong take*, *Wrong pass*, or unclassified cube errors. |
| Evaluable decisions / Error rate | How often the situation came up, and how often it went wrong. Small samples are labeled. |
| Earlier → recent | Error rate in the older half of the player's matches compared with the newer half. |
| Equity loss per 100 decisions | The cost of the mistake group, a good guide for what to practice first. |

**Practice** opens a shuffled round of up to 20 saved examples from this group, starting with the largest losses. Play the checker move, or choose the cube action, and compare your answer with the original decision. Rounds about missed doubles, wrong takes or wrong passes also include correct examples, so the answer cannot be guessed from the topic.

**Only matches played without learning aids** limits the statistics to imported matches and Competition games played without hints.

> **Advanced:** Focus on the groups with the highest *equity loss per 100 decisions* that appear in at least two matches. A rare blunder costs less in the long run than a small error that you repeat in every match.
>
> **Pro:** Each match contributes only the decisions of its **displayed analysis**. Decisions from different analyses are never combined.

Details: [My recurring mistakes](../reference/study.md#my-recurring-mistakes)

### Practice history

![Practice history with checked answers per day, player, game plan and error type](images/study-history.webp)

Practice history records every answer you check in mistake practice and in **Practice results** from [All analyzed decisions](#all-analyzed-decisions): the day, player, game plan, error type, number of answers, how often you found the best decision, and your mean equity loss. Rounds from **Similar positions** practice appear in their own section.

---

## 5. Positions

**Positions** has two views, which you select at the top of the page.

### Saved positions

![Saved positions with filters, the results table and a board preview](images/positions-saved.webp)

This is your personal collection of decisions. You add positions with **Save position** in Analysis. A saved position keeps its full match context, even if you later delete the original match.

- **Search row:** text, player on roll, database and **Saved searches**. The **⋯** menu next to Saved searches saves, deletes and reloads searches.
- **More filters** are grouped into **Match**, **Position**, **Analysis**, and **Tags and comments**. They include the position ID, roll, score, Crawford, cube, equity loss and more. One die matches either die of the roll, and a pair matches in either order.
- Applied filters appear as chips. Remove a chip to update the results immediately.
- **Show played moves** reveals the recorded decision. It starts switched off, so the answers stay hidden while you study.
- Select a row to see a large preview with the equity loss, decks, tags and notes. **Open analysis**, or **Enter** or a double-click, opens the exact decision in Analysis. The icon beside it finds **similar positions** ([details](../reference/library.md#similar-positions)).
- Select up to 200 rows with the checkboxes and use **Add to deck**, or **Remove from collection** in the **⋯** menu beside it. Removing offers **Undo**.

### All analyzed decisions

![All analyzed decisions for one player, sorted by equity loss, with a preview of a wrong take](images/positions-decisions.webp)

This view searches **every** checker and cube decision of one player in the displayed analyses, whether or not you saved the position.

1. Choose a **Player**. With **My player profile** configured, that player is pre-selected and the decisions are listed as soon as you open the view. You can also set the database, decision type (checker or cube), error type and an equity-loss range.
2. Select **Search decisions** to apply changed filters. The results start with the largest loss. Select the **Loss** heading to reverse the order.
3. Select a row, or move with the arrow keys, to preview the board and its cube context. **Open in Analysis** — or **Enter** or a double-click — jumps to the decision (checker decisions in **Checker**, cube decisions in **Cube**).
4. **Practice results** turns up to 20 errors on the current page into a practice round.
5. **Add to deck** in the preview turns the selected decision into a flashcard; **Add page to deck** does the same for every decision on the page, cube decisions included.

> **Pro:** Use **More filters** to narrow the search to a score, for example *2-away / 4-away*, or to a cube value and owner. This lets you build a drill set for a specific match situation, such as double-match-point or post-Crawford positions.

Details: [Saved positions and searches](../reference/library.md#saved-positions-and-searches)

---

## 6. Match database

The Match database stores all your matches in named collections.

![Match database with numbered areas](images/database-list.webp)

| # | Area | Purpose |
| --- | --- | --- |
| 1 | **Import match**, **Import folder** and destination | Import one or more files, or every match file of a folder, into the database selected beside the buttons. |
| 2 | **Databases** | Choose a collection, or **All databases**. Use **+** to create a new one. |
| 3 | **Search** | Find matches by player or file name. |
| 4 | **Filters / Notes / Position ID / Duplicates** | Advanced filters, a search of tags and comments, an exact board search, and the duplicate match finder. |
| 5 | **Actions** | Actions for the selected matches. They become available once you select a match. |
| 6 | **Match table** | Sortable columns with PR, score, dates, event, analysis status and tags. |

### Databases

NeonDojo creates four **system databases** automatically. They cannot be deleted.

| Database | Contents |
| --- | --- |
| **Unsorted** | Imports without a chosen destination |
| **Bot Matches** | Every match you played in **Playing** |
| **Paused Matches** | Paused play sessions, ready to resume |
| **Engine Duels** | Completed matches from [Engine duels](#engine-duels) |

You can create, rename, describe and delete your own databases. **Empty database** is available for *Unsorted*, *Bot Matches* and *Engine Duels*. It requires confirmation, and saved positions, flashcards and their review history are kept.

Details: [Match databases](../reference/library.md#match-databases)

### Importing

Select **Import match**, or drop files or a folder on the Analysis welcome panel. Supported formats are **MAT** (Jellyfish/GNUbg, also as a `.txt` match transcript), **XG** (eXtreme Gammon), **SGF** (GNUbg), **BGF** (BGBlitz) and **OGXM** (HedgeHog). Each file may be up to 16 MiB. Each file is imported separately, and duplicates and errors are reported per file. **Import folder** in the Match database, or dropping a folder on the welcome panel, imports all supported files of the folder and its subfolders; other and hidden files are skipped, and **Stop import** ends the run after the current file. Analyses stored in the file are kept as *imported analysis* with their original depth and units.

Unsupported content is rejected, for example XG1 files, edits later in an XG game, custom starting positions in BGF and OGXM files, beavers and other variants. An XG game that starts from a recorded mid-game position is imported from that position, and the import result lists it. In a MAT or `.txt` transcript, an illegal or unreadable move ends its game before that move, and a header score that contradicts the previous result is kept as recorded. The match is still imported; the import result lists each such case, the Analysis header shows the **Import notes** count, and **Notes** shows them as **Import notes** above the match or game comment. A warning sign next to the PR in the match database marks these matches.

Details: [Import formats](../reference/library.md#import-formats)

### Search and filters

![Match database with the advanced filter panel expanded](images/database-filters.webp)

- **Filters** searches by player, match name, event, round, date range, tag, comment, match type, result and match points.
- **Notes** searches tags and comments. Free text must be 3 to 200 characters. You can add an exact tag, a note type and a database.
- **Position ID** finds every occurrence of an exact board in the library. Each occurrence gets its own row with its game and move. Click the move number to open exactly that occurrence.
- **Duplicates** groups matches with the same played record in the selected database (or across **All databases**), even in different formats, with different file names, or with White and Black swapped. Byte-identical file groups are labeled **Identical files** (others **Same moves**). **Select copies** selects all but the oldest match (lowest ID) of each listed group, up to 100 matches; delete or move them with the actions above the list. Details: [Duplicate match finder](../reference/library.md#duplicate-match-finder)
- Applied filters appear as chips below the search field.

Click a column heading to sort ascending, and click it again to sort descending. **Columns** shows, hides and reorders columns. Drag a header edge to resize a column, and use **Reset column widths** to restore the defaults. Hover over a row to see a summary with PR, analysis profile and excerpts from the notes.

### Bulk actions

| Action | Selection | Notes |
| --- | --- | --- |
| **Open match** | 1 match | Opens it in Analysis. **Enter** or a double-click does the same. |
| **Resume match** | 1 paused or unfinished bot match | Continues it in Playing. |
| **Analyze selected matches …** | 1 or more | Uses one engine and depth for all of them. For a single match the dialog also shows a time estimate. **Bulk analysis** above the list shows the progress. |
| **Edit match details** | 1 match | Display name, played date, event, round, venue and result. |
| **Copy** / **Move** | up to 100 | Copies get new IDs and keep their notes and analyses. Moves keep their IDs. |
| **Tags** | up to 100 | Edit the tags of the selected matches. |
| **Export** | up to 50 | Original file, SGF or XG. XG files contain the played moves without analysis, for example for BG Sage. Several matches are exported as a ZIP. |
| **Delete** | up to 100 | Requires confirmation and skips matches that are being analyzed. Saved positions and flashcards survive. |

> **Novice:** After importing your first match, select it and choose **Analyze selected matches …** at the default 3-ply. If you keep the dialog open until the analysis finishes, NeonDojo opens **Statistics** for the match. If you close it, the analysis continues in the background.
>
> **Pro:** The **ID** column is a stable match number. It survives renames and moves, and copies receive new numbers. Use **Position ID** together with the **Database**, **Game** and **Move** columns to find out how often, and in which matches, a reference position occurred.

Details: [Filtering and navigating matches](../reference/library.md#filtering-and-navigating-matches) · [Bulk actions and exports](../reference/library.md#bulk-actions-and-exports)

---

## 7. Player database

![Player database with the result table and a selected player profile](images/players.webp)

The Player database lists everyone who appears in your matches.

- **Search** by display name or imported alias. Sort by name, matches or games. When two profiles have the same name, for example the same username on Galaxy and OpenGammon, the list shows the platform under each name.
- Select a player to see the **profile**: other names (aliases), the **PR** for each analysis profile, and all matches with the opponent, score, PR, format, event, database and date. From two fully analyzed matches on, a chart shows the PR per match with a ten-match average, split into checker and cube play.
- Click a match row to open the match. Click an opponent's name to switch to their profile.
- **Edit** changes the display name or adds an alias for future imports.
- **Merge …** combines two profiles, for example *Akiko_Yazawa* and *Akiko Yazawa*. The matches and aliases of the absorbed profile move to the profile you keep.
- **Analyze unrated matches …** opens the analysis dialog for all of the player's matches that don't have a PR yet.
- **Virtual profiles** group real profiles without changing them, for example your *leihande* accounts on Galaxy and OpenGammon. Use **New virtual profile …**, then **Add member …** for each account. The virtual profile shows the PR, the PR chart and the matches of all members. **Remove** takes a member out again; the real profiles stay in the list.

> **Advanced:** Group your own online accounts in a virtual profile and choose it under **Settings → General → My player profile**. Your NeonDojo player (the name you play under in Playing) belongs in it too; if you start playing under a new name, add that player the same way. Your PR trend, Study, *My recurring mistakes* and *All analyzed decisions* then start with all your matches. Bot matches enter the PR only when they are finished and were played in Competition mode without learning aids.

Details: [Player database](../reference/library.md#player-database) · [Virtual player profiles](../reference/library.md#virtual-player-profiles)

---

## 8. Extras

### Benchmark

![Engine benchmark with engine, measurement type, depths, comparison and history](images/extras-benchmark.webp)

The benchmark measures how fast your computer runs the engines on a fixed reference match: WC2014, Yazawa against Mayfield, with 11 points, 10 games and 460 checker moves.

1. Choose the **Benchmark engine**: GNUbg or NeonGammon.
2. Choose **Match analysis** or **Play-turn latency**.
3. Tick one or more depths and select **Start benchmark** (or **Measure play-turn latency**). The depths run from lowest to highest in the background. **Cancel run** stops the run.

**Comparison** shows the median and range of the last three runs for each engine and depth. NeonDojo uses these results for the time estimates of analyses. In **History**, click any column heading to sort (and click again to reverse), or use **Delete run** on finished runs (completed, failed, or cancelled) to remove them permanently after an inline confirmation; deleting a completed run removes it from time-estimate calibration. While a benchmark runs, other engine work waits.

Details: [Engine benchmark](../reference/extras.md#engine-benchmark)

### Engine duels

![Engine duels with the series list and the results of a running series](images/extras-duels.webp)

Engine duels let two engines play each other many times so you can compare their strength.

- **New series**: choose the engine and depth for each side, the format, the match length (1 to 25 points) and the number of matches or pairs, up to 1,000. **Money games without cube** plays blocks of cubeless money games instead.
- **Paired with sides swapped** (default) plays every set of dice twice, with the engines on opposite sides. This reduces the luck factor considerably.
- Results show each side's **match win rate**, the matches won, the **CPU time per move**, and a **95 % confidence interval** once the sample is large enough. The interval tells you whether the result could still be 50 %.
- Each completed match is saved in **Match database → Engine Duels** and can be downloaded as SGF. **Analysis** in a result row opens the match directly in the Analysis workspace. **Repeat series …** runs the same settings and seed again.

Details: [Engine duels](../reference/extras.md#engine-duels)

### Database backup

![Database backup with Export backup and Restore backup](images/extras-backup.webp)

- **Export backup** downloads a single `.neondojo-backup` file. It contains all match databases, original files, notes, analyses, saved positions, player profiles, settings and learning history.
- **Restore backup** **replaces all data** of the installation with the backup.

```mermaid
sequenceDiagram
  participant U as You
  participant B as NeonDojo
  U->>B: Choose backup file
  B->>B: Check the archive and compare its schema
  B-->>U: Preview: creation time, matches, positions, flashcards
  U->>B: Review database replacement
  B-->>U: Compare backup counts with current counts (Difference column)
  U->>B: Replace database
  B->>B: Check again and restore in one transaction
  B-->>U: Success, then reload NeonDojo
```

> **Novice:** Export a backup regularly, and always before you restore another backup or move to a new computer.

Details: [Database backup](../reference/extras.md#database-backup)

### Versions

![Versions shows the NeonDojo build and the availability of GNUbg and NeonGammon](images/extras-versions.webp)

**Versions** shows the NeonDojo release version and build commit, the versions of GNU Backgammon and NeonGammon, and whether they are available. **Technical details** shows the full commit, the engine version output, and the **NeonGammon source** commit for recognized releases. **Copy version information** copies a report you can attach to a bug report.

Details: [Version information](../reference/extras.md#version-information)

---

## 9. Profiles and Settings

### Profiles

The **Profile** button in the header shows the active configuration profile. It is marked *modified* when your current settings differ from the saved profile. Open it to:

- switch to another profile; with unsaved changes, **Save and switch** keeps them in the current profile first,
- choose **Show changes** to list each changed setting with its saved and current value, and restore single values with the arrow,
- choose **Manage profiles** to open **Settings → Profiles**.

A profile stores engines, default depths, error thresholds, playing defaults, language, sound, appearance, board artwork and opening-book references. Engine paths belong to each computer and are not part of a profile. **My player profile** is saved with the player database, so it is the same in every profile.

Details: [Configuration profiles](../reference/settings.md#configuration-profiles)

### Settings

Select the gear in the header. Choose a category on the left. **Apply** saves your changes and keeps the dialog open. **Close** returns to the workspace. If you have unapplied changes, NeonDojo asks whether to **Apply**, **Discard draft** or **Keep editing**. **Up**/**Down** switch between categories, and **Home**/**End** jump to the first or last.

<table>
<tr>
<td width="50%"><img src="images/settings-general.webp" alt="Settings: General"><br><b>General</b>: the app language (Deutsch or English), your player name for new games, and <b>My player profile</b>, the virtual profile that stands for you.</td>
<td width="50%"><img src="images/settings-appearance.webp" alt="Settings: Appearance"><br><b>Appearance</b>: the <i>Graphite</i>, <i>Club</i> or <i>Paper</i> theme with <i>Light</i>, <i>Dark</i> or <i>System</i> mode, with a live preview.</td>
</tr>
<tr>
<td><img src="images/settings-analysis.webp" alt="Settings: Analysis"><br><b>Analysis</b>: analysis engine and default depth, the four error thresholds, opening books and a link to the benchmark.</td>
<td><img src="images/settings-playing.webp" alt="Settings: Playing"><br><b>Playing</b>: bot engine and strength (including <b>2-ply + Top-2 (10 mEq)</b> for NeonGammon), default play mode, <i>Open training analysis automatically</i>, game sounds and bear-off direction.</td>
</tr>
<tr>
<td><img src="images/settings-board.webp" alt="Settings: Board"><br><b>Board</b>: choose a playing surface from the gallery. The <b>Checker set</b> and <b>Doubling cube</b> menus start at <b>Match board</b>. Choose another theme in either menu to preview a mix. The dice come with the checker set. Use <b>Apply</b> to save your combination.</td>
<td><img src="images/settings-profiles.webp" alt="Settings: Profiles"><br><b>Profiles</b>: the active profile and its changes, <b>Save in profile</b>, <b>Save as new profile</b>, and <b>Profile actions</b> (Rename, Delete).</td>
</tr>
</table>

Details: [Settings and engines](../reference/settings.md#settings-and-engines) · [Themes and language](../reference/settings.md#themes-and-language)

#### Error grades

The error thresholds in **Settings → Analysis** decide how a move is graded in the move history, the statistics, the Cube review and the feedback during play. They are measured in **normalized equity loss**. The defaults are:

| Grade | Default lower bound |
| --- | ---: |
| Questionable | 0.005 |
| Error | 0.020 |
| Blunder | 0.080 |
| Large Blunder | 0.120 |

Each value is the lower bound of its grade. The four values must be greater than 0, at most 1, and strictly increasing. When you change them, existing analyses are graded again from their stored losses. *Inaccuracies* in Statistics counts all four grades together. The **Error** threshold also sets when Coach intervenes and the starting Tutor warning threshold in new matches.

#### Engines (Engines on this computer)

Open **Settings → Engines** to set the GNUbg executable and the NeonGammon Python environment, and the maximum number of threads per engine, separately for analysis (including engine duels) and for playing. Each thread selector displays its default count directly: **Default (1)** for GNUbg (both analysis and playing), **Default (4)** for NeonGammon analysis/duels, and **Default (2)** for NeonGammon playing; NeonGammon workers share one CPU thread budget so playing moves can run between analysis evaluations without oversubscribing the machine. **Simultaneous analysis jobs** (1–8, **Default (1)**) lets several analyses run at the same time; NeonGammon still runs one analysis at a time. **Check installation** and **Verify and save** run a real position evaluation before a setting is accepted. NeonDojo never falls back to another engine on its own. A missing engine is reported as unavailable. See also [Local NeonGammon setup](../open-sage-local.md).

---

## 10. Learning paths by level

```mermaid
flowchart LR
  subgraph N["Novice"]
    direction TB
    N1["Play 3- or 5-point matches in Tutor mode"] --> N2["Read the learning review in Study → Reports"]
    N2 --> N3["Save positions you got wrong"] --> N4["Study due cards daily<br/>(10 new cards per day)"]
  end
  subgraph A["Advanced"]
    direction TB
    A1["Import your online and tournament matches"] --> A2["Analyze them at 3-ply (bulk)"]
    A2 --> A3["My recurring mistakes:<br/>pick the costliest group"] --> A4["Practice the group,<br/>then play Coach or Competition"]
    A4 --> A5["Check the trend<br/>(Earlier → recent, PR chart)"]
  end
  subgraph P["Professional"]
    direction TB
    P1["Analyze key matches at 4-ply<br/>(5-ply with GNUbg)"] --> P2["Compare moves: XG vs GNUbg vs NeonGammon"]
    P2 --> P3["All analyzed decisions:<br/>filter by score and cube"] --> P4["Build topic decks with tags;<br/>sync with Anki"]
    P4 --> P5["Transfer practice after 7–14 days"]
  end
  N ~~~ A ~~~ P
```

**Novice: learn by playing**

1. Play short matches in **Tutor** mode. When a warning appears, choose **Try again** before you look at **Show solution**.
2. After each match, open the report in **Study → Reports** and walk through *Decisions and attempts*.
3. Save positions you want to remember, and add them to a deck from **Positions** or **Analysis → Study cards**.
4. Spend five minutes a day in **Study → Overview → Study due cards**.
5. Look up unfamiliar terms in the [glossary](#13-glossary).

**Advanced: find and fix your leaks**

1. Import your matches into a database of their own, for example *My Online Games*.
2. In **Player database**, group your accounts in a virtual profile, choose it as **My player profile** in Settings, then choose **Analyze unrated matches …**.
3. In **Study → My recurring mistakes**, sort by equity loss per 100 decisions and practice the top group.
4. Watch your PR chart in the Player database and the *Earlier → recent* trend.
5. Use **Cube** with **Inaccuracies only** to go through every cube error in a match. Compare the take point with the winning chances.

**Professional: precise preparation**

1. Choose the **Displayed analysis** deliberately. Use **Compare saved analyses** to see how much a deeper analysis changes the PR and the error counts.
2. Use **Compare moves** and its **|Δ|** sorting to find positions where the engines disagree. These positions usually deserve a rollout in XG.
3. Use **All analyzed decisions** with score and cube filters to build drill sets for specific match situations.
4. Organize flashcards with hierarchical **topic tags**, sync them with Anki, and measure progress with **Transfer practice**.
5. Use **Benchmark** and **Engine duels** to choose the depth that fits your hardware and time budget.

---

## 11. Keyboard reference

| Where | Key | Action |
| --- | --- | --- |
| Header and tab rows | **Left** / **Right**, **Home** / **End** | Previous or next tab, first or last tab |
| Analysis | **Left** / **Right** | Previous or next move, when no control has focus |
| | **E** / **Shift+E** | Next or previous error in the active review |
| Candidate table and board preview | **Up** / **Down** | Previous or next candidate |
| | **Left** / **Right** | Show *Initial* or *Final* |
| | **Enter** / **Space** | Preview the selected move |
| Board divider | **Left** / **Right** (with **Shift**), **Home** / **End**, **Enter** | Resize the board, jump to the limits, restore the default |
| Playing | **Enter** | Roll, confirm the turn, or pass |
| | **D** | Double |
| | **H** | Hint (not in Competition) |
| | **Z** | Undo one checker step |
| | **Left** / **Right** | Show *Initial* or *Final* while reviewing a move from history |
| | **Escape** | Return from a reviewed move to the game |
| Practice and flashcards | **N** / **D** / **T** / **P** | No double, Double, Take, Pass |
| | **Enter** | Check the answer, then continue |
| | **A** | Show the answer |
| | **1** – **4** | Rate: Again, Hard, Good, Easy |
| Study reports | **Left** / **Right**, **Escape** | Switch report tabs, return to the archive |
| Settings | **Up** / **Down**, **Home** / **End** | Switch categories |
| Dialogs and menus | **Escape** | Close and return focus to the button that opened them |

Shortcuts are ignored while you type in a text field. When a dialog is open, only its own buttons react to shortcuts.

---

## 12. Troubleshooting

| Symptom | What to check |
| --- | --- |
| The footer shows *Backend / GNUbg unavailable* | Open <http://127.0.0.1:8787/api/health>. It reports whether the database responds and which GNUbg version was found. If the page does not load, the NeonDojo backend is not running; see the [project README](../../README.md). |
| Analyses fail | **Settings → Engines → Check installation**. GNUbg needs its neural-network weights. Playing also needs GNUbg's embedded Python. |
| NeonGammon is unavailable (or difficulty presets are disabled) | Enter the Python path of its environment and choose **Verify and save**. Named **Difficulty presets** require `neongammon==3.1.0` (`3.0.0` remains supported for analysis and fixed-depth play). See [NeonGammon setup](../open-sage-local.md). |
| A move shows a dash instead of a loss | The displayed analysis does not cover this position. Run **Analyze game**, or choose another **Displayed analysis**. |
| No PR is shown | PR needs a **complete** native GNUbg analysis of the match. Imported, NeonGammon and partial analyses don't supply PR. Bot matches played with learning aids show no PR. |
| *My recurring mistakes* shows nothing | Choose a player, and check the database, minimum-loss and *Only matches played without learning aids* filters. The player's matches need an analysis; **Analyze in Player database** leads to **Analyze unrated matches …**. |
| An import is rejected | Check the format and its limitations in [Importing](#importing). The error message names the file and the reason. |
| *Apply* reports a conflict | Another browser window saved newer settings. **Apply** retries your changed settings against the newer version. If the conflict remains, choose **Load latest and discard draft**, or save your changes as a new profile in **Settings → Profiles**. |
| A benchmark is running and other work waits | This is intended. Engine work resumes when the benchmark finishes or is cancelled. |

For installation and server details, see the [project README](../../README.md). Detailed behavior is described in the [reference](../index.md#feature-reference).

---

## 13. Glossary

| Term | Meaning |
| --- | --- |
| **Equity** | The expected result of a position for the player on roll, in points per game. +1.0 means a certain single win. |
| **Equity loss / normalized equity loss** | How much worse a decision is than the best one. *Normalized* makes match-play values comparable with money play. |
| **Error grades** | *Questionable*, *Error*, *Blunder* and *Large Blunder*, defined by the [thresholds](#error-grades). |
| **PR (performance rating)** | 500 × normalized equity loss per decision. Lower is better. Strong experts average in the low single digits. GNUbg and XG count decisions differently, so compare PR values from the same source. |
| **Ply** | Search depth. 1-ply evaluates each resulting position directly, and each additional ply looks ahead one more roll. NeonDojo uses this XG convention for every engine; GNUbg calls the same levels one lower. |
| **Cubeful / cubeless** | Whether the evaluation takes the doubling cube into account. |
| **MWC** | Match winning chances, the probability of winning the match. Used instead of equity in some analyses. |
| **Take point** | The winning chance at which taking a double breaks even. It is 25 % in money play without gammons, and depends on the score in match play. |
| **Crawford game** | The game right after a player first comes within one point of winning the match. No doubling is allowed in that game. |
| **Jacoby rule** | In money play, gammons and backgammons count only if the cube has been turned. |
| **Position ID** | GNUbg's 14-character code for the checker arrangement. It describes the board only. |
| **Match ID** | GNUbg's code for the match context: length, score, Crawford, cube, dice and turn. |
| **Displayed analysis** | The saved analysis that NeonDojo shows for a match. See [Displayed analysis](#displayed-analysis-and-new-calculations). |
| **Game plan** | NeonDojo's classification of each side's structure: Opening, Pure race, Racing (contact), Attacking / blitz, Priming or Anchoring. |
| **FSRS** | The scheduling algorithm that decides when a flashcard is due again. |
| **Pip count** | The total number of points a player's checkers must travel to bear off. Checkers on the bar count fully. |
