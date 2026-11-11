# Move preview in mandatory learning feedback

Reuse the Hint view's read-only candidate preview for modal Coach/Tutor/Training feedback. Preserve the selected pending action and its confirmation identity; previewing another candidate never replaces it. Tutor warnings and Competition confirmations must not reveal candidate moves before permission to see analysis.

- [x] Add failing browser coverage for Coach/Tutor previews, both colors, desktop/mobile, unchanged versioned Continue/Play anyway request, hidden Tutor warning then Show solution and Try again, and neutral Competition confirmation.
- [x] Extract PlayMovePreview (board/views/candidate selection/layout), leaving Hint and feedback as consumers of the same component. Retain mode filtering and original controls inside the feedback panel.
- [x] Widen only revealed checker-feedback modals; use shared responsive analysis-dialog styling and preserve text-only Cube/neutral dialogs.
- [x] Run build, unit and focused browser regressions; inspect desktop/mobile output, review changes and update documentation.

Verification: new missing-preview test failed before implementation. Production build, 330 unit tests and all 64 focused browser tests against the built app8787 passed. Covered both colors, desktop/mobile, correct pending-choice identity after previewing a different candidate, Tutor reveal/retry and switching to Competition. Desktop Coach and mobile Tutor screenshots inspected; scoped review found no important issues. Formatting checks passed. Browser API fixtures did not change the user's matches.
