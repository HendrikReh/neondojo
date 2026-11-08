# Individual game navigation and analysis

User scope: select one game inside an imported match; analyze that game by default, retain optional whole-match analysis; show one notes area; explain the existing normalized database/position encoding. The user explicitly selected scoped game analysis plus whole-match option.

1. Persist a small match_games catalog (game number, initial scores, Crawford, checker count), including empty/cube-only games. Populate it atomically during imports and lazily backfill existing immutable matches. Add an index on positions(match_uuid,game_no,ordinal). Expose GET /matches/{id}/games, GET /positions/{id}, and game_no filters on positions/dice/move errors with scoped cursor checks.
2. Add optional game_no on match analysis jobs. Existing no-game requests remain whole-match jobs. Deduplicate active jobs per match/game/plies, persist actual source game numbers in checkpoints, keep progress counts at1game for scoped jobs, retain SSE and lease fencing, and support latest-job lookup by scope. Keep learning sources compatible with scoped job results.
3. React game selector with previous/next game, per-game move history/counts, and transparent bounded paging only inside a very long game. Selecting a game never loads previous games. Direct search/note/error links select the target's match and game before loading its position; delayed responses must not replace a newer selection.
4. Analysis panel scope defaults to selected game. Scope changes abort old watchers/requests; full-match results stay available when explicitly selected. Observed dice and learning-card error extraction follow the selected scope.
5. One notes editor with Match/Position tabs; drafts and saved annotation targets retained. Localize new controls in all six languages.
6. Portable PostgreSQL/GNUbg tests for game catalogs/scoped jobs/filter isolation and recovery; browser regressions for selecting late games, end-of-game boundaries, long-game paging, stale navigation, search jumps, scoped/native analysis and single notes UI. Regenerate OpenAPI/types, build and run relevant checks. The earlier corpus import changes and local game files are preserved.

Data model answer: original file + canonical SGF in matches; individual positions with26signed integer counts, dice, player,game/ply,80-bit GNUbg position encoding rendered as14Base64characters and a separate72-bit MatchID. Exact position lookup uses an indexed ID and does not parse original files. Position storage is not an occupancy-only bitboard.

## Completed verification

- Full Rust suite with PostgreSQL and `/opt/local/bin/gnubg`: 88 passed, no ignored tests. Includes all 37 local XG/MAT/BGF files again (17 new imports, 20 duplicates), scoped native analysis, persisted SSE, lease recovery and query plans.
- React: 13 Vitest tests, six theme-tool tests, TypeScript/production build, generated OpenAPI/types and Prettier passed.
- Chrome: all 19 Playwright tests passed. Includes 126 legal turns in one game with automatic two-page loading, late-game direct lookup, latest-navigation wins, selected-game/full-match native analysis, scope isolation during delayed catalogs, filter pagination reset, empty cube-only games, localized notes tabs and separate drafts, six languages and desktop/mobile layout.
- Independent read-only review found and verified fixes for a reused filter cursor and whole-match results briefly appearing while the game catalog loads. No remaining findings. The mobile move-list grid now wraps long notation without widening the document.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `git diff --check` passed. Backend is running with migration 0007 applied; Vite and the production build contain the new UI. No commit made for this request.
