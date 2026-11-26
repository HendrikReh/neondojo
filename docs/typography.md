# UI typography

All workspaces share the tokens in `web/src/styles/typography.css`, which
`web/src/styles/stylesheets.ts` loads for the whole app. Use the same token for the same text role,
including dialogs and previews. Workspace-specific CSS can adjust spacing
without introducing a different type scale.

| Role                  | Size  | Token                       | Weight                |
| --------------------- | ----- | --------------------------- | --------------------- |
| Page title            | 24 px | `--font-size-page`          | 600                   |
| Panel/section heading | 14 px | `--font-size-panel`         | 600                   |
| Preview subject       | 16 px | `--font-size-subject`       | 600                   |
| Dialog title          | 20 px | `--font-size-dialog`        | 600                   |
| Active study question | 18 px | `--font-size-question`      | 600                   |
| Body text             | 13 px | `--font-size-body`          | 400                   |
| Field values          | 13 px | `--font-size-control`       | 400                   |
| Ordinary button       | 13 px | inherits body size          | 500                   |
| Field label           | 12 px | `--font-size-label`         | 500                   |
| Table content         | 12 px | `--font-size-table`         | 400; emphasis 500/600 |
| Table heading         | 11 px | `--font-size-table-heading` | 600                   |
| Secondary text        | 11 px | `--font-size-meta`          | 400                   |
| Compact metadata      | 10 px | `--font-size-micro`         | 400                   |

The shared sans-serif stack `--font-ui` is `Inter, ui-sans-serif,
-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif`. It uses an
available system font; the app does not download Inter. On macOS the browser
audit resolves it to SF. The weight tokens `--font-weight-normal`, `-medium`,
`-semibold` and `-bold` are 400, 500, 600 and 700.
Table headings use normal casing and tracking. Italics are not a default UI
role.

Position IDs and code use the shared monospace stack `--font-mono`, including
the exact position-search input. Board SVG lettering and BGBlitz theme artwork retain
their own fonts. The brand mark is the text-free logo
[`web/public/neondojo-logo.svg`](../web/public/neondojo-logo.svg), which also
serves as the favicon; the name beside it uses the shared stack. Table actions
inherit the table size; compact metadata and icon controls can use smaller
roles without changing ordinary form controls.

The checker candidate table in Analysis grows continuously with its pane on
desktop: headings scale from 12 to 14 px and cells from 13 to 16 px
(`web/src/styles/desktop-analysis.css`). On ultra-wide displays (`min-width: 3000px`),
the same stylesheet also steps up the Statistics tables, the Notes comment
editor, and the Study cards controls using the larger shared tokens.

The November 2026 rendered audit found serif page titles ranging from 25–30 px,
a 21 px sans-serif Study title, differing table weights/sizes and form controls
between 11–14 px. These were normalized to the shared roles. Note editors already
inherited the UI family; their size is now the shared 13 px field size.

`web/e2e/typography.spec.ts` checks computed styles in populated Analysis,
Match database, Positions and Study views (using the `graphite` light theme,
since interface themes only redefine color tokens; `web/e2e/ui-themes.spec.ts`
verifies all six light/dark theme palettes). It covers every UI language (`en`,
`de`, `es`, `fr`, `pl`, `it`), dialogs, previews, notes, statistics, cube
review, monospace position IDs and the board’s graphical dice pips.
Desktop checks use the logical viewports 1800×1169, 2560×1440 and 3840×2160.
Different text roles remain visually distinct to preserve the dense analysis
hierarchy.
