# Consistent text navigation

User-approved direction: use the header's quiet text navigation and colored active underline throughout Neondojo. Apply it to navigation and view/source selectors, preserving the established distinction between navigation, actions, input fields and content selection. Existing selected table rows, error colors, board/theme artwork, state and keyboard interactions remain.

One shared `text-navigation` style covers main workspaces, analysis details, candidates/dice/game-roll tabs, analysis sources, opening-book providers, checker/cube decisions, board Initial/Arrows/Final, note scopes, settings categories and database collections. Active text and 2 px underline use the current theme accent. Inactive controls use muted text, hover remains transparent, keyboard focus stays visible, disabled states remain identifiable. Settings theme preview should show the same header. Primary and destructive actions, bookmarks, sort headers and preview tiles are not navigation.

- [x] Inventory current controls and cascading overrides; capture baseline.
- [x] Browser contracts fail against existing filled analysis navigation.
- [x] Add shared style and mark navigation groups; consolidate competing presentation rules without altering behavior.
- [x] Verify active/hover/focus/disabled states in all six themes, keyboard navigation, and containment at1800/2560/3840; run units/build and relevant regressions.
- [x] Review, document, activate frontend only; read-only main smoke with unchanged data/settings/backend process. No commit requested.

Evidence: baseline computed-style check failed on filled Overview tab. All 283 unit tests and build passed. Six theme variants and keyboard navigation passed; 39 related browser regressions passed. Review and visual QA identified crowded labels from retained desktop gap overrides; the measured spacing regression failed before those overrides were removed. Final verification and read-only activation follow.

Final verification: all 12 navigation/desktop checks passed after the spacing fix. Main application was verified read-only in four workspaces at all three desktop sizes, with light/dark themes, settings and real analysis data. No write requests or browser errors; main backend PID, settings, application counts and note/deck/card/review digests stayed unchanged. Only task-owned temporary resources were removed. No commit.
