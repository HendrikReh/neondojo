# Flashcard search and workspace layout

User request: add search similar to Positions and place Flashcards outside the bordered content. Preserve previews, deck management, study scheduling, Anki export, themes, languages and prior uncommitted work. No commit requested.

Design: reuse the approved Positions pattern. The workspace has an unboxed page heading, compact deck actions, a search form, a results table and right-hand board preview. Search spans the shared card collection, with an optional deck filter; existing selected-deck preferences are retained. The user confirmed all decks with an optional deck filter. Searches are server-side across the full result set, before UUID cursor pagination. Filters: general text (players, IDs, tags/comments), player on roll, tag, comment, exact GNUbg checker ID, decision kind, and new/due/reviewed state. Empty searches list all cards in scope. Search never reveals a solution or creates a review session. Study/export remain scoped to the selected deck and retain existing behavior; explain that list filters do not change them.

Implementation boundaries:

- New `backend/src/learning/search.rs`: validated query, bound SQL, fresh scoped note metadata, distinct deck memberships, batched metadata enrichment and filtered UUID pagination. Extend the existing deck-card list compatibly and expose a global GET `/api/cards`. Return browse context separately from answer data; normal next-card payloads remain answer-free.
- Existing `learning/mod.rs`: route/OpenAPI registration and browse response types; unchanged grading/FSRS/export behavior. No schema migration unless profiling proves a concrete need.
- New frontend search API/filter controls and six-language messages; keep request identity/abort guards, pagination, current preview and deck management intact. Match Positions heading placement and shared typography.
- Meaningful backend/API and browser regressions cover cross-deck deduplication, scoped filters, literal wildcard escaping, notes after source deletion, invalid input, pagination beyond the first page, stale responses, preview, no learning writes and desktop layout.

- [x] Write and run failing API and browser regressions on disposable data.
- [x] Implement backend query and metadata; regenerate OpenAPI/TypeScript and run relevant Rust checks.
- [x] Implement search controls/results integration and heading layout; verify race guards, six languages and all desktop sizes.
- [x] Run unit/build/format and relevant workflow regressions; independent review and visual QA.
- [x] Document, back up main data and activate after checks; verify original data/settings preserved and remove only task-owned test resources.

Verified: all 283 frontend unit tests; 29 relevant Rust integration tests including four search regressions; 23 targeted browser cases plus 21 cross-view/saved-position cases passed. Independent review findings about virtual cube-decision ordinals and retained game/move notes were fixed with failing-then-passing regressions. Disabling JIT only in this read-only search transaction reduced measured local warm text queries from about 0.8 s to 8–10 ms; this is a local sample, not a general benchmark. No database migration required.

Final desktop correction removes the inherited 1800 px cap from Study & Anki. All 11 focused UI checks passed afterward, including full-width geometry. Main activation passed read-only search/preview/theme/locale smoke checks. Before/after counts and digests of notes, cards, reviews, decks and memberships matched; settings were unchanged. No schema migration or commit. The task-owned disposable database/server/bundles were removed; the private database backup remains outside Git.
