# My recurring mistakes implementation plan

> **For agentic workers:** Use focused implementation and review tasks. Preserve the current working tree and keep backend and frontend ownership separate. Steps use checkbox syntax for tracking.

**Goal:** Find a player's recurring mistakes by game plan and cube direction and launch answer-hidden practice in one click.

**Architecture:** A read-only native-decision aggregation module shares the existing game-plan classifier and original SGF contexts. A Study panel consumes that API; position search gains an optional versioned game-plan filter.

**Tech Stack:** Rust/Axum/SQLx/PostgreSQL, React/TypeScript, Vitest and Playwright.

## Task 1: Backend decisions and practice

Files: create `backend/src/recurring_mistakes.rs` (and focused submodules), `backend/tests/recurring_mistakes.rs`; modify `backend/src/lib.rs`, `backend/src/api.rs` for route/OpenAPI registration; reuse/expose `backend/src/learning/content.rs` decoding if useful.

- [x] Write isolated API tests before production changes. Seed imported legal SGF, jobs and checkpointed decision JSON using existing test helpers. Assert `GET /api/study/mistakes?player=Alice` does not double count overlapping game/whole-match jobs, categorizes cube directions by deciding player, and omits stale/unfinished data.
- [x] Run `DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/neondojo cargo test -p neondojo --test recurring_mistakes -- --ignored`; observe missing-route assertion failures.
- [x] Implement eligibility and bounded loading, exact player filtering, source reconstruction, grouping and deterministic ordering. Implement the three endpoints with the exact contract in the approved spec. Invariants include `selected_job.source_revision == match.source_revision`, `status == "completed"`, and checkpointed game membership.
- [x] Add practice privacy/read-only tests: `question["position"]["notation"] == ""`, no `best`/`played`/`equity_loss` fields; reveal only on the separate endpoint; card and review counts unchanged.
- [x] Run the focused tests and fix failures without invoking GNUbg.

## Task 2: Searchable plans

Files: create a versioned position-plan cache module and migration; modify `backend/src/position_browser.rs`; extend backend position-search tests.

- [x] Add failing tests for `{"schema_version":1,"game_plan":"priming"}`, invalid labels, both player colors, old filter serialization and cursor mismatch.
- [x] Cache classification by immutable position UUID and classifier version, using bounded lazy backfill and indexed lookups. Apply the optional plan filter in SQL before cursor/sort/limit.
- [x] Run relevant backend position-browser and profile tests; absent optional fields must not alter saved filter fingerprints.

## Task 3: Study UI and practice

Files: create `web/src/RecurringMistakes.tsx`, focused practice/API/messages/CSS files, unit tests and `web/e2e/recurring-mistakes.spec.ts`; modify `web/src/StudyWorkspace.tsx`, `web/src/study-messages.ts`, `web/src/position-browser-api.ts`, `web/src/PositionBrowserControls.tsx` and position messages.

- [x] Add failing tests for Study navigation, player requirement, grouping and practice reveal. Mock only API boundaries; assert best/played strings are absent before reveal and after Next.
- [x] Implement the agreed contract using handwritten local types initially; parent regenerates schema output after backend integration. Reuse `Board`, `PlayBoardDialog`, `ActionButton`, existing translations and source/depth conventions.
- [x] Make plan filters round-trip through draft/definition and expose a localized selector in position browser.
- [x] Add abort/generation protection for every list/practice/reveal request and close practice when filters or visibility change. Expose retry and honest empty states.
- [x] Run focused Vitest and Playwright tests. Cover keyboard navigation, focus return, re-entry and delayed responses.

## Task 4: Integration, review and verification

Files: `docs/openapi.json`, `web/src/generated-api.d.ts`, README and navigation docs.

- [x] Run `npm run api:types --prefix web`; verify additive schema changes and no missing types.
- [x] Review the implementation against every requirement above, then review source for correctness and maintainability. Resolve findings and rerun affected tests.
- [x] Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, relevant PostgreSQL tests, `npm test --prefix web`, `npm run build --prefix web`, and targeted Playwright workflows.
- [x] Document feature location, native-only coverage, selection rules and read-only practice. Report verified outcomes and any limits. Do not commit unrelated pre-existing edits.

## Verified outcome

Implemented and independently reviewed in the existing working tree, with unrelated work preserved and no commits. The local backend was gracefully restarted after confirming no queued/running analyses.

- Rust workspace suite: 154 passed (278 intentionally ignored integration/engine cases).
- Targeted PostgreSQL/native integration: 63 passed, including real GNUbg, profiles/configuration, position search and retention.
- Web unit suite: 453 passed across 69 files.
- Recurring-mistake and existing Study browser workflows: 16 passed, including hidden/reset answers, stale requests, keyboard focus and match/Money/Jacoby context.
- Rust formatting, strict Clippy, TypeScript/Vite build, OpenAPI generation and diff whitespace checks passed. Existing Vite chunk-size/mixed-import warnings remain.
- Live-library smoke verified player choices, grouping, practice/reveal, and every available game-plan search class against the shared classifier.
