# Source-click play

User requested immediate movement by clicking the point containing checkers, without selecting a destination or showing frames. Keep hover/focus destination dots and the first unspent die rule. A source has one legal destination for that die, constrained by native complete turns; source click appends exactly that one step. Empty/opponent/blocked points do nothing. Repeated clicks may move multiple checkers from the same point. Bar entry, hits and bearing off follow the same path, including existing sounds. Undo/reset/confirm remain available.

- [x] Add browser regressions proving a point click immediately moves one checker, target clicks are unnecessary, frames are absent, both colors/themes/directions and dice swaps work, repeated clicks consume doubles.
- [x] Observe regression failure against existing implementation.
- [x] Remove selection state/frames and enable only legal sources; update instructions and existing browser drivers to click sources only.
- [x] Run frontend units/build and browser regressions including sounds/opening; complete native matches in both directions using source-only clicks.
- [x] Publish tested frontend, verify live smoke and unchanged stored data, clean up only owned test resources. Keep ongoing source changes uncommitted.


Regression was observed failing because the initial source click produced no played step. Implementation now removes the source-selection state, activates only legal source points, and computes the unique destination from the current native-constrained per-die prefix. Removed checker selection rings, point frames/fills and automatic SVG focus frames; keyboard focus still shows the legal destination dot. Both pointer and Enter/Space input are covered. Existing sound and match browser drivers now click sources only and still assert each chosen prefix. Empty/blocked destinations are disabled; explicit invalid-click tests use physical forced clicks to verify no action. Current unit/build checks pass; 28 browser cases passed, including 7 new source-click cases (4 color/direction/theme combinations, repeated double-clicks consuming a double, and bar entry for each color). Real-engine full matches are running on an isolated test database.


Completed: 303 unit tests, 28 interaction browser tests and production build/format/diff checks passed. Both full native 3-point matches passed using source clicks exclusively (White/right/2-ply; Black/left/0-ply), together with three seeded opening and three cube/profile/reload/next-game cases. The extra native smoke test initially assumed every bot response immediately includes dice/checkers; real GNUbg instead offered a cube in one case. The test now accepts the legitimate cube before asserting checker audio; both native audio smoke cases then passed. No application rule change was needed for that test assumption.

Activated tested frontend assets/index on live8787. All seven source-click regressions passed again against the live bundle. Exact play/settings rows and position/review counts compare unchanged; backend remains PID82754. Owned5194/8991 test services and the exact disposable sourceclick database were cleaned up. Source remains uncommitted. Evidence: /Users/hendrik/Library/Caches/gnubg-update/bgtutor-source-click-verification/.
