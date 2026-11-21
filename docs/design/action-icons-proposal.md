# Consistent action icons

> **Status: implemented.** `web/src/action-icons.tsx` includes all icons in the table through `lucide-react` (`close` and `cancel` share `x`) and draws them at 20 px with a stroke width of 1.5 by default (`aria-hidden="true"`, `focusable="false"`). `ActionButton` and `ActionLink` (`web/src/ActionButton.tsx`) show popover tooltips on hover and keyboard focus (`web/src/useActionTooltip.tsx`), and `ActionButton` names any declared keyboard shortcut in the tooltip and exposes it via `aria-keyshortcuts`. Compact icon buttons have a 44 × 44 px target in `web/src/action-controls.css`. `web/e2e/action-icons.spec.ts` checks compact icon tooltips, dialog focus tooltips and Escape handling, hover tooltip reachability, and icon-with-text game actions. The mapping now covers 60 named actions (including `roll`, `double`, `take`, `drop`, `pass`, `resign`, `pause`, `filter`, `clear`, `bookmark`, `unbookmark`, `similar`, `duplicates`, `import-folder`, and `system-database`); `web/src/action-icons.tsx` is authoritative.

NeonDojo uses [Lucide Outline](https://lucide.dev/icons/) with a stroke width of 1.5 on a 24 × 24 SVG grid. The stroke width scales with the displayed icon size. Compact routine actions use an icon with a tooltip, while important game actions and primary form actions use an icon with text. Tooltips appear on hover and keyboard focus.

| User action | Lucide icon | Meaning |
| --- | --- | --- |
| Save | [save](https://lucide.dev/icons/save) | Save changes |
| Undo / Step back | [undo-2](https://lucide.dev/icons/undo-2) | Revert the last unconfirmed step |
| Hint | [lightbulb](https://lucide.dev/icons/lightbulb) | Help with the current decision |
| Delete | [trash](https://lucide.dev/icons/trash) | Delete the selected item |
| Add | [plus](https://lucide.dev/icons/plus) | Create or add a new item |
| Export | [download](https://lucide.dev/icons/download) | Download a file from NeonDojo |
| Import | [upload](https://lucide.dev/icons/upload) | Bring a file into NeonDojo |
| Resume game | [play](https://lucide.dev/icons/play) | Resume a saved game |
| Open | [folder-open](https://lucide.dev/icons/folder-open) | Open an existing match or file |
| Search | [search](https://lucide.dev/icons/search) | Search the collection |
| Edit | [pencil](https://lucide.dev/icons/pencil) | Change existing details |
| Copy | [copy](https://lucide.dev/icons/copy) | Create a copy |
| Refresh | [refresh-cw](https://lucide.dev/icons/refresh-cw) | Reload the current state |
| Settings | [settings](https://lucide.dev/icons/settings) | Open the configuration |
| Close / Cancel | [x](https://lucide.dev/icons/x) | Close the current view |
| Confirm | [check](https://lucide.dev/icons/check) | Confirm a selection or change |

Display rules: include Lucide icons through `ActionIcon`, at 20 px by default in buttons. Use rounded line caps and inherit the text color of the surrounding element. Show icon and text together on primary actions; use icon-only buttons only for well-known, space-critical actions with an understandable accessible name and a tooltip. Click targets are at least 44 × 44 px. Use red for destructive actions and the accent color for the main action currently available, and keep disabled actions recognizable. Do not mix with emoji or filled icons.
