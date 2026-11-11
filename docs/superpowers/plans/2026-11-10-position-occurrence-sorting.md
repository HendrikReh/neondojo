# Position occurrence sorting implementation plan

Goal: The existing database/game/move result columns use the same ascending/descending sorting controls as the match columns. Database sorts alphabetically ignoring case, game and move numerically. All sorting is server-side with existing occurrence-aware cursor ties.

- [x] Add failing PostgreSQL tests covering all three keys, both directions, numeric values above nine, case-insensitive database names, duplicate values and pagination. Reject occurrence-only keys without a position filter.
- [x] Add failing profile-validation and browser tests for persisted sort choices, visible direction, exact opening after sorting, and removing/reapplying the position filter.
- [x] Extend `backend/src/databases.rs` sort expressions with `lower(occurrence_db.name)`, `hit.game_no`, `hit.ply`; validate position-filter scope. Extend backend/frontend profile sort allowlists. Regenerate OpenAPI.
- [x] Extend `web/src/library-api.ts` sort keys and render `LibrarySortHeading` in the three existing context headers in `MatchDatabase.tsx`. Use created-at sorting for normal match browsing while a remembered occurrence-only sort is selected, avoiding invalid requests after clearing a position filter.
- [x] Run backend library/configuration tests, frontend tests/build and browser search regressions, review changes, update README, and refresh local server.

No commit or integration action requested. Preserve all other workspace changes.

Validation: 3 occurrence API tests (all keys/both directions/pagination), 39 configuration/database/library sort regression tests, all 375 frontend tests, production build, 14 full-app browser cases and scoped independent review passed. Local server refreshed and each new sort key checked over HTTP.
