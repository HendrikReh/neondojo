# Live Tutor Threshold Implementation Plan

**Goal:** Edit the running match's Tutor warning threshold next to its mode selector.

**Design:** Extend the existing versioned mode update with an optional `tutor_threshold`, preserving old clients. Validate the same finite range (0, 1] as creation; retain the threshold when absent. Recompute only the current pending Tutor warning, preserving its identity, reveal state, original attempts and explicit confirmation. Never invoke the engine or advance a position. Global new-match defaults remain independent.

Show an always-visible numeric field next to the mode selector, initialized from the saved session. Valid changed values save on blur (including Enter). Invalid inputs restore the saved value; unchanged values cause no request. Disable controls during requests, animations and after match completion. Preserve partial checker steps; errors leave the saved threshold authoritative and the draft editable. Translate the new action/help in existing supported locales.

- [x] Add failing backend coverage for valid/invalid thresholds, stale revisions, checkpoint/attempt preservation, pending warning re-evaluation, reload and unchanged defaults.
- [x] Extend `PlayModeRequest` / `change_mode`, regenerate OpenAPI and TypeScript, verify native tests.
- [x] Add failing browser coverage for inline editing, validation, reload, partial checker steps and failed writes.
- [x] Extend `changePlayMode`, `PlayWorkspace`, mode messages and CSS; verify focused browser tests.
- [x] Review changes, run builds/unit/native checks, update documentation and restart/smoke-check local application.

Verified: 28 native backend play tests, 317 frontend unit tests, 31 focused browser cases and 5 actual GNUbg browser lifecycles passed. Native Tutor lifecycle raises/lowers the live threshold while preserving pending decision and position. Production build, generated API/types, formatting, clippy and diff checks passed. Read-only code review approved; screenshot inspected. Restarted and smoke-checked the app at 127.0.0.1:8787; removed isolated test service/database.

Follow-up: Remove Apply button and unused translations/styles. Verify blur, Enter, unchanged/invalid inputs, failed-save retry, reload and partial-step preservation in the browser.

Automatic-save follow-up verified: 21 mode browser tests and 317 frontend unit tests passed; production build, type checks, formatting and diff checks passed. Screenshot confirms button removed. Local port 8787 serves the updated bundle; no backend restart required for this frontend change.

Save-status follow-up: A polite live region beside the input displays saving while its request is pending, then success for three seconds. Editing or changing sessions clears the message; failed requests clear saving without reporting success. Added translations for all six locales. Verified six focused threshold browser cases (including German/English timing and failed-save retry), production build/type check, formatting, diff check, and 317 frontend unit tests. Inspected German screenshot.
