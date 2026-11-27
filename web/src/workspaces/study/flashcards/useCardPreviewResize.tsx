import { useId, useLayoutEffect, useRef, useState } from 'react';
import { usePaneSplitter } from '../../../shared/hooks/usePaneSplitter';
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
  const [preferred, setPreferred] = useState(readWidth);
  const [maxWidth, setMaxWidth] = useState(defaultWidth);
  const width = Math.max(minWidth, Math.min(maxWidth, Math.round(preferred)));

  useLayoutEffect(() => {
    if (!visible || !layout.current) return;
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

  const { handle: divider } = usePaneSplitter({
    containerRef: layout,
    cssVar: '--card-preview-width',
    width,
    min: minWidth,
    max: maxWidth,
    direction: 'left',
    active: visible,
    onCommit: save,
    onReset: () => save(defaultWidth),
    ariaLabel: text('resizePreview'),
    ariaControls: previewId,
    helpText: text('resizePreviewHelp'),
    className: 'learning-preview-divider',
    preventFocusScroll: true,
  });

  return { layout, previewId, width, divider };
}
