# UI typography

Database, Analysis, Positions and Study & Anki share the tokens in
`web/src/typography.css`. Use the same token for the same text role, including
dialogs and previews. Workspace-specific CSS can adjust spacing without
introducing a different type scale.

| Role                       | Size  | Weight                |
| -------------------------- | ----- | --------------------- |
| Page title                 | 24 px | 600                   |
| Panel/section heading      | 14 px | 600                   |
| Preview subject            | 16 px | 600                   |
| Dialog title               | 20 px | 600                   |
| Active study question      | 18 px | 600                   |
| Body text and field values | 13 px | 400                   |
| Ordinary button            | 13 px | 500                   |
| Field label                | 12 px | 500                   |
| Table content              | 12 px | 400; emphasis 500/600 |
| Table heading              | 11 px | 600                   |
| Secondary text             | 11 px | 400                   |
| Compact metadata           | 10 px | 400                   |

The shared sans-serif stack is `Inter, ui-sans-serif, -apple-system,
BlinkMacSystemFont, 'Segoe UI', sans-serif`. It uses an available system font;
the app does not download Inter. On macOS the browser audit resolves it to SF.
Normal, medium, semibold and bold weights are 400, 500, 600 and 700.
Table headings use normal casing and tracking. Italics are not a default UI
role.

Position IDs and code use the shared monospace stack, including the exact
position-search input. Board SVG lettering and BGBlitz theme artwork retain
their own fonts. The brand mark is the text-free logo
[`web/public/neondojo-logo.svg`](../web/public/neondojo-logo.svg), which also
serves as the favicon; the name beside it uses the shared stack. Table actions
inherit the table size; compact metadata and icon controls can use smaller
roles without changing ordinary form controls.

The checker candidate table in Analysis is the one table that grows with its
pane on desktop: headings scale from 12 to 14 px and cells from 13 to 16 px.

The November 2026 rendered audit found serif page titles ranging from 25–30 px,
a 21 px sans-serif Study title, differing table weights/sizes and form controls
between 11–14 px. These were normalized to the shared roles. Note editors already
inherited the UI family; their size is now the shared 13 px field size.

`web/e2e/typography.spec.ts` checks computed styles with populated views, including
all six light/dark theme variants, every UI language, dialogs, previews, notes,
statistics, errors and deliberate monospace/board exceptions. Desktop checks
use logical viewport sizes 1800×1169, 2560×1440 and 3840×2160. Different text roles
remain visually distinct to preserve the dense analysis hierarchy.
