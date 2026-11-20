# Demo Import and Opening Books Implementation Plan

> Use executing-plans with independent backend domains delegated under dispatching-parallel-agents; root integrates frontend, reset and research.

**Goal:** Rebuild three private demo collections, preserve analysis provenance, add round filtering and real local BackgammonDB and XG opening books.

**Architecture:** Preserve imported results in a separate normalized table. Read the local SQLite BackgammonDB book and the official XG binary book asynchronously. Keep source views separate from fresh engine jobs. Build demo data in staging collections before replacing the library under a transaction and retain an external backup.

**Tech Stack:** Rust, Axum, Tokio, SQLx/PostgreSQL, SQLite, GNUbg, React/TypeScript, Python stdlib tooling, Playwright.

- [x] Add red import-preservation tests, implement migration0012 and source extraction (XG, SGF, BGF), transactional insertion and independent-copy helper.
- [x] Add round filter, indexed search and cursor tests; expose six-language filter control.
- [x] Implement canonical book keys, compressed payload decoder, actual book status/config/lookup endpoints and full verified local download; migrations0013/0015. Keep XG position references and mandatory author attributions separate from rolled candidates.
- [x] Register APIs, retain generated OpenAPI/TypeScript contracts, expose source switches and an opening-book dialog from analysis settings.
- [x] Back up current PostgreSQL outside Git; add a reusable private demo rebuild script and stage BGF/MAT/XG imports. Verify preserved results before swapping three collections.
- [x] Read blog/manual and visually inspect requested match URL; produce prioritized, source-linked product/UX report distinguishing verified observations from suggestions.
- [x] Run targeted red/green checks followed by relevant full Rust, native engine, frontend, browser, format/build/lint and desktop/mobile checks. Document rejected corrupt local files without weakening parser validation.


## Verified outcome

- Activated exactly Demo BGF (5 matches), Demo MAT (5), Demo XG (23). All 42 private source files were processed; nine invalid/inconsistent XG inputs remain documented as HTTP422 in the external manifest. No parser checks were relaxed.
- Retained 72,959 checker/cube analysis records plus 98 raw source-statistics records. Existing 69 learning cards, 62 reviews, 63 decks and 70 deck memberships are unchanged by activation.
- Both official books installed outside Git: BackgammonDB 4,230,443 entries / 351 contexts; XG3.70 53,210 attributed evaluations. Point-match Beaver flags are normalized only where inert; distinct money-game rules, owned cubes and mirrored players have oracle coverage.
- Verification: 177 Rust tests (PostgreSQL, native GNUbg and installed-book oracles included), 36 Vitest tests, 42 Playwright workflows, 18 Python tests including isolated PostgreSQL reset guards; fmt, Clippy with warnings denied, Prettier, TypeScript and production build pass. Vite emits its existing bundle-size advisory.
- Browser workflows ran before final activation. Afterwards, read-only API and desktop/mobile checks confirmed the three final collections, source analysis, both books, reachable dialog actions and no runtime errors or 390px horizontal overflow.
- Full verified pre-activation PostgreSQL dump, source manifest, SHA256 checks and screenshots are outside the repository in Application Support/Neondojo. Private games are ignored by Git. The implementation remains in the working tree.
