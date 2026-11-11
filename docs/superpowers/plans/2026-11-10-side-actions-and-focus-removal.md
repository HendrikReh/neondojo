# Right-side play actions and analysis simplification

- [x] Propose Lucide Outline as the common icon family, with a concrete action-to-icon mapping and consistent dimensions/stroke/text/accessibility rules. Keep the app-wide migration a proposal.
- [x] Add failing browser assertions that Playing actions sit to the right of the actual board on desktop and mobile, retain Undo behavior and Hint access, and that Analysis has no Board Focus toggle while move history remains visible.
- [x] Give only the live Playing board a responsive board/actions grid. Stack Hint and Undo in a sticky side rail, retaining text labels and large targets, using a compact mobile arrangement. Preview boards and required dialogs keep their current sizing/semantics.
- [x] Remove Analysis focus state, toggle, layout rules and translations; update the obsolete typography assertion and documentation.
- [x] Build and run focused mocked browser regressions, unit checks and visual inspection; review the scoped changes. Do not run database-mutating typography/import fixtures against the user's database.

Verification: observed both missing-removal and wrong-placement browser failures before implementation. Production build passed; 330 unit tests and 36 scoped mocked browser tests passed. Inspected desktop/mobile screenshots. Review found no important issues and additionally checked five locales at 390px with no document overflow. No Board Focus selectors/state/messages remain in src. Typography assertion updated but its database-mutating integration fixture was intentionally not run against user data. Formatting checks passed.
