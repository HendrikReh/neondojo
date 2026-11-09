# User-resizable match columns

**Goal:** Let desktop users resize every match-table column and retain their layout locally.

**Design:** Add a focusable vertical resize handle at the right edge of each header, separate from sorting. Dragging freezes the current rendered widths and changes only the chosen column. Custom widths use pixels and may grow beyond the viewport inside the existing horizontal scroller; untouched default layout remains responsive. Store a versioned, validated complete width map in localStorage on commit. Provide a reset button to restore responsive defaults. Pointer cancellation/Escape restore the preceding layout, and keyboard arrows adjust widths. Bound widths to usable column-specific minimums and a finite maximum. No backend or database changes.

- [x] Add failing unit tests for validated persistence, immutable one-column updates and limits; add browser coverage for drag, sorting isolation, keyboard control, cancel, reload persistence, reset and contained horizontal scrolling.
- [x] Implement a small column model/storage module and a resize hook with pointer capture, cleanup and ResizeObserver measurements.
- [x] Attach handles to sortable and control headers, apply widths through colgroup/table, and add the localized reset action while retaining existing sort and batch behavior.
- [x] Run frontend/build/format checks and desktop browser regressions using read-only mocked library records. Inspect the main library without changing its records/settings, update documentation and leave prior uncommitted work intact.

No commit is requested. Screenshots and test artifacts remain outside Git.

Verification: 113 frontend tests, TypeScript/Vite build, Prettier and 68 affected browser tests passed. The two unchanged deep native analysis browser scenarios were excluded. Five new interaction cases cover drag/sort isolation, no-op clicks, keyboard bounds, Escape and pointer cancellation, reload persistence, reset on changed viewports, bounded snapshots on a 5120px viewport, and accessible width values after reset. Reviewer found the oversized-snapshot persistence issue; its regression failed before the fix and passed afterward.

A read-only main-library smoke check resized White from 166.86px to 247px, retained 35px rows and issued no write requests. Source/user-data, settings and completed-job fingerprints were unchanged. One internal moves_cataloged flag advanced during normal lazy catalog loading; reproducing its previous value in a read-only checksum query exactly recovered the baseline match fingerprint, confirming all other match fields and source bytes/SGF were unchanged. The disposable backend/database were removed; screenshots remain outside Git.
