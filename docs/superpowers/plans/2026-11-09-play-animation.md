# Play Animation Implementation Plan

**Goal:** Always show dice rolls and checker movement, including the bot opening turn.
**Architecture:** Pure native-record frame planning and checker transitions; an owned, cancellable React playback queue; SVG overlays for both board renderers; cosmetic dice faces/rolling state passed through existing dice components. Engine writes are unchanged.

- [x] Write failing frame-plan tests for bot opening/same dice, human history suppression, hits/bar/off, ties, reverse steps and both colors; observe red.
- [x] Implement frame planning, geometry and cancellable presentation queue with reduced-motion and synchronized audio.
- [x] Integrate action playback before final state acceptance, animate local steps/undo/reset, lock decision controls until playback, and show dice motion in built-in/archive/opening renderers.
- [x] Verify unit/type/build, animation browser cases, existing play/sound/source/opening regressions and native games. Review for cancellation, stale state, replay errors and mirrored geometry.
- [x] Publish tested frontend and verify live behavior/data preservation. Clean owned test resources; leave source uncommitted with ongoing work.


Observed verification so far: six new unit cases (initial five failed before implementation), including all archive theme movement endpoints; 309 frontend unit tests and production build passed. Nine dedicated browser animation cases passed: both opening bot colors/directions/themes, actual nonzero checker trajectories and dice-before-move sequencing, immediate human roll animation, reload/workspace cancellation, hit/bar/off and reverse motions. All 28 existing play/source/opening/audio browser regressions also passed. Optional video capture was removed because Playwright FFmpeg was absent; browser requestAnimationFrame samples verify real movement instead. Mocked API inspections use page fetch so route interception applies. Independent scoped code review approved with no findings. Native matches are tested using the supported reduced-motion fade path; focused browser cases force normal motion.


Completed native checks: both 3-point matches passed (White/right/2-ply, Black/left/0-ply), three opening-seed cases and three cube/profile/reload/next-game cases passed. Native audio smoke was updated to wait for queued human animation before counting sounds; this reflects the new intentional audio timing. Both normal-motion native smoke cases then passed. No application change was needed for that assertion timing.

Published the tested frontend on8787; all nine animation cases passed again against the activated bundle, including nonzero movement trajectories and the bot's original dice before its opening moves. Live index equals staged index; exact play/settings rows and position/review counts compare unchanged. Backend PID remains82754. Owned5194/8991 test servers and the exact disposable animation DB were removed. Build, 309 units, format and diff checks passed. Source remains uncommitted. Evidence: /Users/hendrik/Library/Caches/gnubg-update/bgtutor-animation-verification/.
