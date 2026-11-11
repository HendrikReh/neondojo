# Continuous money games with per-game reports

Goal: Money games continue from GameOver into the next native game, retaining native cumulative scores. Every finished game has a distinct durable report including its original decisions and historical board previews.

- [x] Add native two-game regression: scoring, next opening, per-game reports, retries/idempotence, old report/history after later play and match deletion, legacy money MatchOver continuation. Retain point-match completion behavior.
- [x] Add immutable study_report_snapshots (id, session_id/game_no unique, nullable library link, captured session/SGF) and backfill existing reports without changing their IDs. Keep legacy report fallback for compatibility.
- [x] Store a report before next_game advances; generate only the completed money game's analysis and original attempts. Clear current report for the next game while preserving all history and cumulative learning/library metadata. Use archived snapshots for report board previews.
- [x] Expose archived reports and history through Study and latest report through match metadata. Add game labels. Keep normal point reports and archive cursors compatible.
- [x] Add next-game UI/automatic report save and report access between games; validate no score reset, stale response races and stored report isolation.
- [x] Verify native, archive, frontend and browser regressions, scoped review, docs and local app refresh.

Validation: 35 native play tests, 21 database tests and 4 archive tests passed (the separate long whole-match simulation was excluded). Both money games included actual checker decisions; old reports and history stayed unchanged after subsequent games and library deletion. Report-generation failure preserved the checkpoint, stale next-game retries returned 409, and an older single-game money report retained its ID when the session continued. 48 browser tests, 387 frontend tests and the production build passed. Scoped independent review found no concrete issues. Migration 0032 and the refreshed local app serve the existing Hendrik/GNUbg report under its original ID with all 111 history snapshots.
