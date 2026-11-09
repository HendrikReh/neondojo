# Navigation style

BG Tutor uses the same text-navigation language in its workspaces, analysis
subviews, board views, sources, opening books, decision kinds, note scopes,
settings categories and database collections.

Navigation has a transparent background, muted inactive text and no rounded
button frame. The current item uses the theme accent, semibold text and a 2 px
underline. Hover remains transparent; keyboard focus has a visible accent
outline. Disabled choices retain native disabled behavior and reduced opacity.
The settings appearance preview depicts the same navigation.

Use `text-navigation` on the container and keep buttons as direct children.
Existing `aria-selected="true"`, `aria-pressed="true"`, `aria-current="page"` or
`active` state drives the underline. Keep existing keyboard handlers and ARIA
relationships. `text-navigation--compact` reduces vertical padding in dense
analysis panels; `text-navigation--stacked` arranges sidebar choices vertically.
The main header retains its 20 px gap; other horizontal groups use 16 px.

The shared rules live in `web/src/text-navigation.css`, imported after the other
styles. Local CSS manages surrounding layout and should not reintroduce filled
navigation states or competing padding/gap rules.

Action buttons, field controls, bookmarks, sortable table headers, selected rows,
error badges, candidate previews and artwork/theme tiles retain their existing
visual roles. This keeps navigation, actions and content selection recognizable.

`web/e2e/text-navigation.spec.ts` verifies active, hover, disabled and keyboard
focus states in all six light/dark theme variants, actual spacing between labels,
and the established desktop viewports. Related workflow tests cover settings,
notes, sources, board views and header navigation.
