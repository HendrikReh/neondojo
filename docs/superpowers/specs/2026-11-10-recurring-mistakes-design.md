# My recurring mistakes

The user approved implementation of the assessed feature: searchable game plans, directional cube errors, and one-click practice per recurring group. Build on the current working tree; it contains the existing Study, learning, play and profile foundations that are not all committed. Preserve that work and do not commit unrelated changes.

## User experience

Add a third Study area, **My recurring mistakes**. Select an exact player name from available analyzed matches, matched case-insensitively after trimming whitespace (never as a substring), optionally a database, game plan and minimum equity loss (default 0.08). An unselected player prompts selection and does not combine everyone into a personal score. Group results by the decision-maker's existing Open Sage game-plan label and mistake kind: checker play, missed double, premature double, wrong take, wrong pass, or unclassified cube. Show occurrences, contributing matches and accumulated normalized equity loss, ordered by loss descending with stable ties. Counts describe decisions, not unique board diagrams; no PR or promised recoverable winnings is invented.

Each group has **Practice**, opening at most 20 distinct decision questions in a keyboard-accessible dialog. Show the original board, players, dice, score, cube, question type and source/depth. Hide played move, best move, loss and arrows until an explicit reveal. Reveal loads the saved answer from the backend; Next hides the answer again. State the session progress and completion. Closing or switching Study areas cancels pending requests and restores focus. This practice is read-only and leaves decks, cards and FSRS reviews unchanged.

Add game-plan filtering to the existing saved-position browser, retaining the current five versioned labels and all existing filters, saved-search revisions, sorting and cursor validation. The plan is that of the player on roll, never arbitrarily White. Existing saved filters retain their serialization when the optional filter is absent.

## Data and correctness

Only completed native match/game analyses with current source revisions, source matches still available, and durable game checkpoints are eligible. Exclude unfinished competition sessions. Select exactly one job per match and game, highest depth then newest timestamp and UUID; apply thresholds only after selection. Count every eligible decision occurrence once, even when several jobs or game/whole-match scopes overlap. Forced, unavailable, malformed and non-finite evaluations do not become mistakes. Filter on the actual decision player (including cube responses), not the owner of the preceding checker position. Imported analyses remain separate and are explicitly outside this first dashboard's coverage.

Reconstruct original decision contexts using the existing SGF decision sequence rules: no-double precedes an action without advancing it, while actual checker/cube actions advance. Support cube-only games and both colors. Reuse the local game-plan classifier and decode helpers where practical. Classifier labels used for SQL search may be cached by position and classifier version; lazily backfill existing positions in bounded batches before plan-filtered search, and never invoke an engine. Add an index for lookup. No silent list truncation: reject an excessively broad request with a useful error.

## API contract

- `GET /api/study/mistakes`: query `player?`, `database_id?`, `game_plan?`, `min_loss?` (default .08). Returns `{players: string[], groups: MistakeGroup[], analyzed_decisions: number, analyzed_matches: number}`. With no player, return available players and no groups. `MistakeGroup = {game_plan, mistake_kind, count, match_count, total_loss}`. Game plans are `pure_race|racing|attacking|priming|anchoring`; kinds are `checker|missed_double|premature_double|wrong_take|wrong_pass|unclassified_cube`.
- `POST /api/study/mistakes/practice`: JSON `{player, database_id?, game_plan, min_loss?, mistake_kind}`. Returns `{items: PracticeQuestion[]}` bounded to 20 distinct full-context questions, worst loss first. `PracticeQuestion = {reference:{job_id,game_no,decision_no}, kind, position:Position, players:[string,string], engine, plies}`. No answer fields, and position notation is cleared.
- `POST /api/study/mistakes/reveal`: JSON `{job_id,game_no,decision_no}`. Revalidate eligibility/current source and return `{best,played,equity_loss,engine,plies}`. The route reads saved analysis only; no scheduling or domain mutations.
- Position filter optional `game_plan` uses the same label enum and is applied before sorting/pagination.

## Validation

Backend tests use isolated SQLx PostgreSQL databases and an unavailable engine. Cover overlapping jobs/scopes, depth-first selection, current revisions/checkpoints, active competition, both colors, cube-only decisions, original cube contexts, all directions, threshold and player filtering, repeated occurrences vs distinct practice questions, no answer leakage, read-only practice and invalid references. Verify search classification for both colors, old saved-filter serialization, cursor scope and retained positions. Frontend unit and Playwright tests cover accessible navigation, selecting/filtering, empty/error/retry states, hidden/revealed/reset answers, source details, closing and stale asynchronous responses. Regenerate OpenAPI/types and run Rust and web checks appropriate to these changes.
