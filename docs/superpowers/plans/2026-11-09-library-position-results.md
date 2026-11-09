# Position search in the match table

**Goal:** The Match database's exact position search filters the existing compact match list instead of rendering separate position-result cards.

**Design:** Reuse library position_id filtering (already implemented with an indexed EXISTS). Each matching match occurs once; current database, other filters, sorting, pagination, selection and resized columns apply normally. Keep the existing exact-search input/current-position helper as a draft, synchronize it when advanced filters change the applied position ID, add an explicit clear-only-position action, validate the exact ID before applying. Opening a filtered row by match button/Enter/double-click resolves its first exact occurrence and uses existing position navigation; an abort/version/scope guard prevents late responses stealing navigation. Normal unfiltered match opening and saved Positions collection remain intact. No backend/schema changes or commit.

- [x] Tests first: real three-match search shows only matching rows, no boxes, same columns/actions; multiple occurrences dedupe; compounded filters and clearing; direct exact-position opening by button/Enter/double-click; late old row lookup cannot replace a newer click/filter/tab; scoped multi-page matches and no results.
- [x] Remove separate position request/results/pagination state; apply search to library filters; add clear/filter indicator/validation; reuse current table and error feedback.
- [x] Add focused match-opening helper with stale-request fences, then connect all row open interactions. Update existing workflow assertion and remove obsolete box CSS.
- [x] Verify frontend tests/build/format and real browser regressions on a disposable database at port 8788; review. Activate only tested static assets without restarting the main backend; clean disposable data. No private/demo data added to Git.


TDD browser failure: the legacy exact search retained three unfiltered match rows instead of the two matching matches. The common position_id filter now deduplicates matches, provides a clear-only action, and validates exact IDs with localized feedback. The existing workflow assertion checks the common table and absence of separate position boxes.

Five focused browser cases cover duplicate occurrences; sorting, other filters and selection; button/Enter/double-click navigation to the exact occurrence; stale-request fences; 51-match pagination; visible lookup failures and retries with the form closed; and opening current rows during a background refresh. That last regression was found by the existing database-management test: a blanket loading guard prevented immediate reopening after returning from Analysis. A result-scope guard now blocks obsolete rows while keeping current results usable during refresh.

195 frontend tests, the production build, formatting check and 87 browser regression tests pass. The two expensive analysis-depth suites were excluded from this frontend-only regression run. Independent review found no actionable issues, checked eight additional hook scenarios, and reviewed the result-scope correction.

Activated the verified static assets on port 8787 without restarting its backend. A read-only browser smoke test against the existing Hendrik/Beginner match verified common-table filtering, absence of position boxes, and navigation to the exact first occurrence; no write requests occurred. The disposable service/database and temporary bundles were removed. No backend, schema or main-database changes were made for this task.
