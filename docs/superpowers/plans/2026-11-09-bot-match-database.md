# Bot matches in Match Database

User-authorized scope: automatically keep bot matches in a dedicated logical database, remove Saved Matches from Playing, organize them through existing Match Database tools.

Design: create `Bot Matches` logical database with stable ID. Link each play session to one ordinary match entry as soon as created. Synchronize named native SGF, score, result, games and positions atomically with session revisions; preserve stable identities, annotations and user organization. Existing sessions backfill idempotently on startup. Deleting an archived entry detaches it permanently; copying produces an independent snapshot. Resume live originals via a separate Match Database action; normal row opening keeps analysis behavior. Playing retains automatic last-session restoration and new-match setup, without a separate saved-match list. Explicit resume requests override the last remembered match, including repeated requests to the same match.

Implementation tasks:
1. Backend migration, atomic archive/sync, startup backfill, library response contract. Isolated integration tests for creation, advancement, stable IDs, copy/move/delete, backfill and stale revision rollback.
2. Frontend remove saved-list UI/API reads; resume requests from Match Database with race protection; translated Resume action, library refresh on saved changes. Browser regression tests including reload and completed/copy records.
3. Review backend and frontend, verify generated OpenAPI, build, unit tests, browser tests, isolated native match persistence.
4. Back up main database, gracefully restart updated server after active jobs complete, verify backfill and deploy tested frontend assets. Preserve all existing play history and ongoing sessions.

## Completion evidence

Implemented and deployed locally on 2026-11-09. Dedicated database UUID ends in003 (002 is reserved for Saved positions). All three existing play sessions were backfilled without changing checkpoints, native IDs, revisions or timestamps. New sessions and actions archive atomically; copies remain independent and deletion stays detached. Current match analysis is fenced by source revision, with scope derived under the owning match lock. Native Black setup exports are normalized only in the archived library artifact, with strict/native decision-context equivalence checks.

Verification: frontend build,312 unit tests,15 core browser tests,41 play regressions,22 database integration tests,8 backend bot integration cases and8 native browser scenarios including two full matches, both directions and0/1/2-ply cube cases. Additional5 browser cases verify resume/load cancellation and action-column bounds. Independent frontend/backend reviews have no remaining blocking findings. Main-service read-only browser verification confirmed three library entries, three Resume actions, successful resume and no Saved Matches list. Database backup, final SGFs and verification report are in ignored backups/bot-match-library-20261109. Owned test services and disposable cloned database were removed; main service remains healthy on8787.
