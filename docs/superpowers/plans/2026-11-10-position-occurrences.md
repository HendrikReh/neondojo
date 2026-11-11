# Individual position-search occurrences

User requested 2026-11-10: every occurrence is its own result row with database, game and move, including repeated positions in one game. Preserve the global unfiltered analysis action and normal unfiltered match library.

- [x] Write failing PostgreSQL tests for repeated positions within one game/match, global/scoped lookup, stable cursor pagination within a match across normal/null/compact sorts, and exact position identity.
- [x] Extend position-filtered library browsing with occurrence metadata and occurrence-aware joins/keyset ties while retaining ordinary match results and scope validation. Regenerate OpenAPI/types.
- [x] Add mandatory database/game/move columns in filtered results, unique occurrence row keys and pagination, exact occurrence opening with existing async fences; keep match-level bulk actions deduplicated.
- [x] Update browser tests for repeated positions in the same game, per-occurrence pagination/opening, rapid selection/errors, global search and manual filters. Run backend/frontend checks, inspect desktop/mobile and review scoped changes.

Verified: 31 PostgreSQL library/database tests, all 358 frontend tests, 13 full-app browser cases, production build, scoped independent review and local server smoke check. Real SGF fixture covers game 1 moves 2/4 and game 2 move 2; frontend pagination keeps 52 unique occurrence rows across 51 matches. The migration regression test now compares every original source field while permitting columns added by later migrations.
