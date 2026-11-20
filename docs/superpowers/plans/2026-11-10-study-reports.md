# Study Reports Implementation Plan

> Execute inline in the existing feature branch; preserve all unrelated uncommitted work. User approved the design and implementation on 2026-11-10.

**Goal:** Organize automatically saved learning reports and existing flashcards in Study.

**Architecture:** Reuse durable session reviews; add a lightweight paginated archive and read-only detail endpoint. Wrap the existing flashcard workspace with Study navigation and render archived reports using PlayLearningReview.

**Tech Stack:** Rust/Axum/SQLx/PostgreSQL; React/TypeScript; Playwright/Vitest.

- [x] Add backend/tests/study_reports.rs covering existing reports, pagination past 100, missing/unfinished reports, invalid cursors and persisted detail without an engine. Run DATABASE_URL=postgres://neondojo:neondojo_local@127.0.0.1:54329/neondojo cargo test -p neondojo --test study_reports -- --ignored; expect 404 before implementation.
- [x] Add backend/src/study.rs with GET /api/study/reports and /api/study/reports/{id}, DTOs and cursor validation; register routes and OpenAPI in backend/src/lib.rs and api.rs. Add migration 0030_study_reports.sql for a partial recent-report index. Re-run backend tests.
- [x] Add web/e2e/study-reports.spec.ts for archive navigation/detail, refresh/reload, pagination/error/retry and deck routing. Run against current app; expect missing Study tab.
- [x] Add web/src/study-api.ts, study-messages.ts, StudyWorkspace.tsx, StudyReports.tsx and study.css. Wrap LearningPanel in App, keep flashcards as initial area for compatibility and open reports via second tab. Preserve hidden flashcard state; archive loads only while visible. Reuse PlayLearningReview for full persisted details.
- [x] Rename Study & Anki labels to Study and update existing E2E labels. Regenerate schema with npm run api:types; run npm test and npm run build.
- [x] Run Rust format/clippy and archive/native persistence tests; run Study and flashcard Playwright checks. Inspect screenshots and changes; record results here and in README.


Verification on 2026-11-10:

- Backend archive tests: red with missing routes before implementation, then 3 PostgreSQL tests passed (105 reports with cursor pagination; legacy/empty/invalid/missing cases; preservation after library deletion).
- Native GNUbg completed-match review test passed: generated report automatically appears once in the archive, persisted detail matches the original review/decisions, repeated access stays idempotent.
- Frontend: 56 files / 332 Vitest tests passed; TypeScript and Vite production build passed. Existing Vite bundle-size advisory remains.
- Browser: 5 new archive tests passed, including refresh, initial/pagination/detail retry, stale detail responses, reload, read-only requests, German labels, unknown PR and keyboard navigation. A 50-row focus regression failed before the fix and passed after it. The existing real Positions → Study deck flow passed, including switching out of Reports; its fixture uses an isolated 61-point context to avoid sharing FSRS progress with seven-point tests. 33 existing flashcard/deck/Playing browser cases passed.
- Review identified the long-list visibility issue; fixed by focusing the opened report heading and adding a return-to-list action. Follow-up review found no remaining actionable issues.
- An additional existing Positions row-height test fails at 55px versus its <=40px expectation. Reproduced with the Study wrapper removed, confirming it is independent of this feature; no unrelated layout change made.
- Rust fmt, clippy with warnings denied, and git diff --check passed.
- Local backend restarted with migration 0030 and updated production frontend. Actual archive returned the existing report and its detail via HTTP 200. A real German browser check opened it without JavaScript errors or API writes. Screenshots inspected at /tmp/neondojo-study-archive.png and /tmp/neondojo-study-detail.png.
- Existing feature branch and all pre-existing uncommitted work preserved; no merge, push, or unrelated staging.


Follow-up on 2026-11-10: resolved the Positions layout-test failure requested by the user. Runtime measurements confirmed 55px rows composed of the intended 44px icon click target, 10px vertical cell padding and a 1px border. The prior 40px assertions predated the authorized icon design. Both Positions specs now use a shared layout assertion that enforces at least 44×44px click targets, compact rows up to 56px including rounding tolerance, and complete button containment. Desktop coverage checks first/last rows at 1440, 1800, 2560 and 3840px. All 10 tests in position-browser.spec.ts and desktop-positions.spec.ts passed against the local production app; formatting and git diff --check passed. Production layout was preserved.
