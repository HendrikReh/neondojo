import { useId, useLayoutEffect, useRef, useState } from 'react';
import Tooltip from './shared/ui/Tooltip';
import { useCardSearchText } from './card-search-messages';

const storageKey = 'neondojo.cardPreviewWidth';
const defaultWidth = 400;
const minWidth = 320;
const listMinWidth = 440;
const dividerWidth = 20;

function readWidth() {
  try {
    const stored = Number(localStorage.getItem(storageKey));
    if (Number.isFinite(stored) && stored >= minWidth) return stored;
  } catch {
    // The preview can still be resized if browser storage is unavailable.
  }
  return defaultWidth;
}

export function useCardPreviewResize(visible: boolean) {
  const text = useCardSearchText();
  const previewId = useId();
  const layout = useRef<HTMLDivElement>(null);
  const drag = useRef<{ pointerId: number; x: number; width: number }>(null);
  const [preferred, setPreferred] = useState(readWidth);
  const [pendingWidth, setPendingWidth] = useState<number>();
  const [maxWidth, setMaxWidth] = useState(defaultWidth);
  const clamp = (width: number) =>
    Math.max(minWidth, Math.min(maxWidth, Math.round(width)));
  const width = clamp(pendingWidth ?? preferred);

  useLayoutEffect(() => {
    if (!visible || !layout.current) {
      drag.current = null;
      setPendingWidth(undefined);
      return;
    }
    const node = layout.current;
    const measure = () => {
      setMaxWidth(
        Math.max(minWidth, node.clientWidth - listMinWidth - dividerWidth),
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [visible]);

  function save(next: number) {
    setPreferred(next);
    try {
      localStorage.setItem(storageKey, String(next));
    } catch {
      // Keep the chosen width for the mounted preview.
    }
  }

  function cancel() {
    drag.current = null;
    setPendingWidth(undefined);
  }

  const divider = (
    <Tooltip
      as="span"
      role="separator"
      tabIndex={0}
      aria-label={text('resizePreview')}
      aria-orientation="vertical"
      aria-controls={previewId}
      aria-valuemin={minWidth}
      aria-valuemax={maxWidth}
      aria-valuenow={width}
      text={text('resizePreviewHelp')}
      className="learning-preview-divider"
      data-dragging={pendingWidth !== undefined}
      onPointerDown={(event) => {
        if (!event.isPrimary || event.button !== 0 || drag.current) return;
        event.preventDefault();
        event.currentTarget.focus({ preventScroll: true });
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = { pointerId: event.pointerId, x: event.clientX, width };
        setPendingWidth(width);
      }}
      onPointerMove={(event) => {
        const start = drag.current;
        if (start?.pointerId === event.pointerId)
          setPendingWidth(clamp(start.width + start.x - event.clientX));
      }}
      onPointerUp={(event) => {
        const start = drag.current;
        if (start?.pointerId !== event.pointerId) return;
        const next = clamp(start.width + start.x - event.clientX);
        if (next !== start.width) save(next);
        cancel();
      }}
      onLostPointerCapture={cancel}
      onDoubleClick={() => save(defaultWidth)}
      onKeyDown={(event) => {
        if (event.key === 'Escape' && drag.current) {
          event.preventDefault();
          const { pointerId } = drag.current;
          cancel();
          event.currentTarget.releasePointerCapture(pointerId);
          return;
        }
        if (drag.current) return;
        const step = event.shiftKey ? 50 : 10;
        const widths: Record<string, number> = {
          ArrowLeft: width + step,
          ArrowRight: width - step,
          Home: minWidth,
          End: maxWidth,
          Enter: defaultWidth,
        };
        if (!Object.hasOwn(widths, event.key)) return;
        event.preventDefault();
        event.stopPropagation();
        save(clamp(widths[event.key]));
      }}
    />
  );

  return { layout, previewId, width, divider };
}
