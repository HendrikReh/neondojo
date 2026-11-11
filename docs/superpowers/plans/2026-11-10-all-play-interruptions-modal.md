# Consistent board-centered interruptions

- [x] Extract existing native board-centered mandatory modal into shared PlayBoardDialog; Cube keeps the same values/sound/feedback behavior.
- [x] Derive one interruption at a time: Cube response; staged blocking Coach/Training/Tutor/Competition acknowledgment; opening roll/result; pre-roll (Roll/Double); forced pass; next game; terminal result acknowledgment. Hide these during actual dice/checker playback and pending roll. Ordinary hints that allow checker moves remain inline; deliberate history review stays a view.
- [x] Keep errors, hints and all pending-choice/retry/reveal/commit controls inside the active modal. Explicit retry returns to editable board when action becomes null; pre-roll stays modal until actual roll/double. End-of-match result has explicit View match acknowledgment so learning review remains accessible.
- [x] Add browser regressions for center/modality, Coach before next roll, versioned pending IDs, retries, forced pass and game boundaries. Update old board-action test helpers/contracts to modal pre-roll; retain actual checker/dice interactions during movable turns.
- [x] Review, build/unit/browser validation, screenshots, production-bundle smoke and documentation.

Verification: production build passed; 330 unit tests passed; focused modal/mode/roll suite 37 passed; animation/opening/dice/history/sounds/play suite 58 passed (five overlapping interruption cases); production-bundle interruption smoke 7 passed. Desktop and 390px screenshots inspected. Independent read-only review found no important issues. Formatting checks passed. User database left unchanged by mocked browser fixtures.
