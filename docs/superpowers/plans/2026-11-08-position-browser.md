# Position Browser Implementation Plan

> Execute independent search/backend, card-transfer and UI domains using dispatching-parallel-agents. Root integrates navigation/header, deck handoff, OpenAPI, browser regressions and verification.

**Goal:** Find saved checker positions with persistent named filters, open exact analyses and transfer selected positions to existing FSRS/Anki decks.

**Architecture:** Indexed PostgreSQL context and correlated note/native-analysis queries produce context-preserving hits. Saved filter definitions and cursors share one validated versioned schema. Bulk learning reuses existing card persistence and solutions.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, React/TypeScript, Vitest, Playwright.

- [x] Record exact private XG rejection names/reasons from the existing manifest; preserve source files and parser checks.
- [x] Add search/context/saved-filter migration0017 and validated REST contracts. Observe failing SQLx tests for note scope, dice/score/cube, pagination and filter CAS, then implement and verify.
- [x] Add bounded POST /api/decks/{id}/positions with per-position outcomes. Refactor existing atomic card-write helper without changing FSRS, identity, export or deck limits. Test mixed/missing/unanalysed/duplicate requests and history preservation.
- [x] Build the Position Browser with filter editor, saved-search controls, list/pagination, board preview and selected-position deck actions. Use generated contracts, six-language messages, abort/identity fences and visible outcomes.
- [x] Remove the visible header import button, retain database import and shared file/drop handling. Add the main Positions tab and exact-position analysis callback; hand the selected deck to the mounted learning view without stale reveal state.
- [x] Regenerate OpenAPI/TypeScript; run meaningful targeted red/green and full frontend/native PostgreSQL checks, formatting/build/lint and isolated browser workflows. Inspect desktop/mobile and private-library search performance.
- [x] Independently review changes, preserve demo fingerprints, remove disposable test databases, restart the regular backend and document validation.


## Verification evidence

- Full Rust suite with PostgreSQL, native GNUbg, opening books and ignored integration tests: **208 passed**, zero failures. The separate opt-in private corpus test requiring every file to import is excluded because the corpus deliberately retains nine rejected XG sources; their original hashes and concrete diagnostics were checked against the private import manifest instead.
- Frontend: **92 tests in 19 files passed**. TypeScript/Vite production build, Prettier, rustfmt and Clippy (`--all-targets -- -D warnings`) passed. Vite retains its advisory warning for the existing main bundle over 500kB.
- **53 browser tests passed**, zero failures. Browser regressions run against a disposable PostgreSQL library and a separate backend, covering saved-filter persistence/CAS, deleted database isolation, stale responses, exact analysis navigation, mixed card-transfer results, actual APKG downloads and concealed study answers. The mobile layout and active-tab hover regression was observed failing before its CSS fix and passing after it.
- Desktop and 390px mobile inspection uses read-only access to the normal demo library. A relatively positioned table scroller contains the hidden accessibility label and wide table; the viewport itself remains fixed-width. The selected main tab keeps its dark background on hover.
- Read-only search samples against 47,936 saved positions: default 25-result page median 64.8ms; database scope 53.7ms; dice pair 64.7ms; score/cube scope 200.8ms; loss sort 461.6ms warm (917.8ms first sample). These are local samples, not a universal latency promise.
- The original 33 matches, 73,057 imported analysis rows, 69 cards, 62 reviews, 63 decks and three named demo databases are fingerprint-checked for preservation. Private sources, books and screenshots stay outside tracked files. Migration 0017 is additive; the regular backend reports healthy GNUbg 1.08.003.

The disposable browser backend was stopped and its dedicated database removed after verification. The final normal-library fingerprint still matches the pre-test snapshot.
