# End money sessions

Goal: After a completed money game, offer End session beside Next game. Persist a final session state with the cumulative score and all archived game reports. Explicitly ended sessions cannot continue; legacy single-game sessions remain continuable.

Design: Add the versioned `end_session` action, accepted only between money games. Ensure the final game report exists before atomically setting phase MatchOver and a default-false `money_session_ended` flag. Keep native SGF, game count and scores unchanged. Existing archived reports retain their IDs and history versions. Use a secondary stop action beside Next game, then the existing finished-match view with a localized Session ended heading. Persisted state restores after reload; point matches have no End session action.

- [x] Add failing backend regression: two games, final score/SGF/report preservation, archive identity, terminal action rejection and reload. Extend the engine-failure regression for end_session. Run the tests red.
- [x] Add failing desktop/mobile browser regression for adjacent actions, finished state and reload. Run red.
- [x] Implement backend action/flag using the existing transaction and report snapshot machinery; regenerate OpenAPI types.
- [x] Add localized UI action and terminal-state handling without changing point matches or legacy Money compatibility.
- [x] Run native Money, point completion, archive and relevant browser regressions; frontend tests and production build. Review the patch, update docs and refresh local app.

Validation: Backend red test failed because end_session was not implemented; browser red tests failed on the missing action. Final verification passed: 36 native play tests and 4 archive tests (separate long full-match simulation excluded), 51 browser regressions, 403 frontend tests, OpenAPI generation and production builds. Native cases cover completed-session reload, unchanged SGF/native ID/scores/history, existing final report reuse without a working engine, every archived report, legacy UUID retention, invalid point-match actions, stale versions and terminal rejection. Browser cases cover desktop and mobile English/German adjacency with matching button heights, failed-save retry, reload, direct report opening, Escape/focus return and final view. Review identified the finished-dialog report visibility condition; its failing regression was reproduced and corrected. Reviewer confirmed the fix and found no further concrete concerns. Local backend and frontend refreshed.
