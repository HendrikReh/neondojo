# Application-wide Lucide actions and accessible tooltips

User authorized the action mapping throughout the app. Compact routine actions show an icon with an accessible action name and focus/hover tooltip; important play and primary workflow actions retain visible text. Navigation tabs, move/candidate choices, actual dice/cube artwork and content are not replaced by action icons.

- [x] Add failing browser coverage for compact controls, visible-text Playing actions, pointer/keyboard tooltips, modal top-layer visibility, Escape and unchanged action identity.
- [x] Install lucide-react and centralize semantic icon mapping plus ActionButton/ActionLink components. Localized labels, native props/ref/type/link/disabled semantics retained. Tooltips use manual popovers in the top layer, appear on hover/focus, dismiss on blur/click/Escape and clean up on unmount.
- [x] Migrate Library/Positions actions independently.
- [x] Migrate Analysis/Settings/annotations actions independently.
- [x] Migrate Learning/Deck management actions independently.
- [x] Migrate app shell and Playing actions; important game controls keep text. Replace hand-drawn action icons while preserving preview choices and native game graphics.
- [x] Audit remaining controls, CSS selector interactions, localized access names and dependencies; review scoped changes.
- [x] Build/unit/focused mocked browser checks and desktop/mobile inspection. Use mocked API responses for browser regressions.


Verification notes:

- Shared components reviewed for callback ref cleanup, native form/link semantics, modal Escape, hoverable tooltips and visible analysis labels; findings addressed.
- All 332 unit tests pass. Production TypeScript/Vite build passes.
- All 64 selected mocked browser regressions pass; after the final Settings save-icon correction, all 8 Action/Settings checks pass again. Browser regressions cover compact controls and focus tooltips, modal top-layer visibility, table containment at 1800/2560/3840px, learning deck operations, settings, game resume, Undo/Hint, cube decisions, interruption dialogs and candidate previews on desktop/mobile.
- The saved-game Library fixture now starts in a checker-move phase so its navigation test does not try to click through a mandatory Next roll dialog.
- Two temporary Depth fixture matches created by an older native settings test were deleted by exact ID; no related analysis jobs remain running. Subsequent browser checks exclude native data-writing cases.
