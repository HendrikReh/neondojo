# Shared learning review dialog

User approved implementation on 2026-11-10. Keep existing persisted reports and archive APIs. Use the same native draggable PlayBoardDialog as Hint for both Playing and Study.

- [x] Add failing Playwright tests for report dialogs from both entry points, successful generation auto-open, generation failure, reopening without writes, drag, Escape/close focus restoration, loading/retry, stale-response cancellation, long-list internal scrolling and small viewports. Update existing inline-report expectations to require explicit opening and closing.
- [x] Extend PlayBoardDialog with an optional board anchor (viewport center in Study) and an explicit return-focus ref. Existing anchored Hint/interruption behavior stays the default.
- [x] Add LearningReviewDialog.tsx as the shared loading/error/report shell. Separate PlayLearningReview summary from the scrollable decision/attempt content. Style a wide, viewport-bounded dialog with a fixed heading and summary and an internally scrollable list.
- [x] Replace inline Playing report with an open/generate action. Open after successful generation; allow reopening saved results without recomputation; close when hiding/resetting Playing. Replace Study inline detail with the same dialog and keep request cancellation/error retry.
- [x] Verify frontend unit tests, production build, report/Hint/drag/interruption browser regressions, screenshot desktop/mobile, scoped review, and deployed local app. Record results and update README.


Verification on 2026-11-10:

- The two entry-point modal tests failed against the previous inline implementation, then passed with the shared dialog. Additional red-to-green regressions cover late background completion, visibly captured play mode, short German landscape screens at 568×320 and 667×375, and a valid 240-character match context at 320×568.
- All 15 report/Study Playwright cases pass: generation, retry, no recomputation, drag, native modality, focus trap/return, Escape/close, stale detail replies, archive reload, internal scrolling and size containment. Tests share the persisted-report fixture.
- All 45 relevant existing Hint, dialog-drag, Playing-feedback and interruption browser cases pass. Existing board anchoring and mandatory game acknowledgments are preserved.
- All 56 Vitest files / 332 frontend tests pass. Production TypeScript/Vite build passes; existing bundle-size advisory remains. Changed files pass Prettier and git diff --check.
- Review findings resolved: suppress automatic dialog opening when generation completes outside Playing; scroll the complete report under a fixed header on very short screens; bound context height at all viewport sizes and make overflow reachable by keyboard. Follow-up review found no remaining actionable issues.
- Actual local production app verified using the existing saved report in German through both Study and Playing: no runtime errors or API writes. Desktop/mobile screenshots inspected at /tmp/bgtutor-learning-review-dialog-desktop.png and /tmp/bgtutor-learning-review-dialog-mobile.png. Updated frontend is served by the existing backend; no backend changes or restart required.
- Existing PostgreSQL persistence and archive APIs preserved. All unrelated uncommitted work remains in the current feature branch; no push or merge.
