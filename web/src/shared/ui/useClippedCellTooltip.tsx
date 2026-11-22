import type { PointerEvent } from 'react';
import { createPortal } from 'react-dom';
import { clippedTooltipText } from '../format/overflow-title';
import { useActionTooltip } from './useActionTooltip';

/**
 * One delegated tooltip for a table body: hovering a cell shows the complete
 * text of its clipped `data-tooltip-overflow` labels, and nothing for a cell
 * whose text fits. Rows and cells mount no tooltip state of their own.
 */
export function useClippedCellTooltip() {
  const tip = useActionTooltip(clippedTooltipText);
  return {
    onPointerOver(event: PointerEvent<HTMLElement>) {
      const cell = (event.target as Element).closest('td');
      if (!cell || !event.currentTarget.contains(cell)) return;
      if (cell !== tip.anchor.current) {
        tip.dismiss();
        tip.anchor.current = cell;
      }
      tip.onPointerEnter();
    },
    onPointerLeave: tip.onPointerLeave,
    onPointerDown: tip.dismiss,
    tooltip:
      tip.content && tip.anchor.current
        ? createPortal(
            tip.content,
            tip.anchor.current.closest('dialog, .app-shell') ?? document.body,
          )
        : null,
  };
}
