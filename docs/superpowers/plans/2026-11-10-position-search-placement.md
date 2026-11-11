# Position search beside bookmark

User approved on 2026-11-10: move the position-search action above the board, directly to the right of Save position, with German tooltip “Diese Stellung suchen”. Preserve its existing Match Database destination/search flow.

- [x] Add failing desktop/mobile browser checks for adjacency above the board, 44px targets, the exact hover/focus tooltip, removal from position-code controls, and working navigation/current-position search.
- [x] In AnalysisWorkspace.tsx group the bookmark and existing search action in board-position-actions. Update position-bookmark.css to align the group and keep it together on mobile. Update the six existing app.prepareSearch translations.
- [x] Verify placement/search cases, existing saved-position behavior, frontend tests/build and formatting. Inspect screenshots and the updated local production app. Preserve unrelated uncommitted work.


Verified: the desktop assertion failed before implementation because the search control was absent from the board toolbar. All four new placement/tooltip/destination/error-independence browser checks and four existing saved-position cases now pass against the local production app. Both buttons keep 44px targets, align with a 6px gap above the board at 1800px and 390px, and remain together when the controls wrap. German hover/focus tooltip is exactly “Diese Stellung suchen”; other interface languages use matching localized labels. Current-position transfer and exact library search still work. All 341 frontend tests and production build pass. Desktop/mobile screenshots inspected. Existing bundle-size advisory remains; no backend changes or restart required.
