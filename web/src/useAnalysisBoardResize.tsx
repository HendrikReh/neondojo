import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import { useOptionalConfiguration } from './ConfigurationProvider';
import { useProfileValue } from './useProfileValue';
import { useBoardReviewText } from './board-review-messages';
import Tooltip from './Tooltip';

const limits = configurationOptions.analysis_board_width;
const desktopDefaultWidth = 420;
const wideDefaultWidth = 600;
const clamp = (value: number, max: number) =>
  Math.round(Math.max(limits.min, Math.min(max, value)));
type Drag = {
  pointer: number;
  target: HTMLElement;
  startX: number;
  startWidth: number;
  width: number;
};

export function useAnalysisBoardResize(
  gridRef: RefObject<HTMLDivElement | null>,
  active: boolean,
  overview: boolean,
) {
  const text = useBoardReviewText();
  const profile = useOptionalConfiguration();
  const [preferences, setPreferences] = useProfileValue(
    ({
      appearance: { analysis_board_width, analysis_overview_board_width },
    }) => ({
      analysis_board_width,
      analysis_overview_board_width,
    }),
    (c, widths) => ({
      ...c,
      appearance: { ...c.appearance, ...widths },
    }),
    { analysis_board_width: null, analysis_overview_board_width: null },
  );
  const field = overview
    ? 'analysis_overview_board_width'
    : 'analysis_board_width';
  const preferred = preferences[field];
  const setPreferred = (width: number | null) =>
    setPreferences((current) => ({ ...current, [field]: width }));
  const [geometry, setGeometry] = useState({
    max: limits.max,
    defaultWidth: desktopDefaultWidth,
  });
  const [preview, setPreview] = useState<number>();
  const drag = useRef<Drag | undefined>(undefined);
  const width = clamp(
    preview ?? preferred ?? geometry.defaultWidth,
    geometry.max,
  );
  const cancel = useCallback(() => {
    const previous = drag.current;
    drag.current = undefined;
    setPreview(undefined);
    if (previous?.target.hasPointerCapture(previous.pointer))
      previous.target.releasePointerCapture(previous.pointer);
  }, []);

  useLayoutEffect(() => {
    cancel();
  }, [active, overview, profile?.generation, preferred, cancel]);

  useLayoutEffect(() => {
    const grid = gridRef.current;
    const board = grid?.querySelector('.board-panel');
    const moves = grid?.querySelector('.moves-panel');
    if (!active || !grid || !board || !moves) return;
    const measure = () => {
      if (!window.matchMedia('(min-width: 961px)').matches) return;
      const gap = parseFloat(getComputedStyle(grid).columnGap) || 0;
      const available =
        grid.clientWidth -
        moves.getBoundingClientRect().width -
        3 * gap -
        8 -
        400;
      // The board artwork must also fit above the navigation and game-plan controls.
      const maxHeight = parseFloat(getComputedStyle(board).maxHeight);
      const heightLimit = (maxHeight - 220) * 1.4 + 20;
      const max = Math.max(
        limits.min,
        Math.min(limits.max, Math.floor(available), Math.floor(heightLimit)),
      );
      const viewportWidth = window.innerWidth;
      const standardWidth = Math.min(desktopDefaultWidth, viewportWidth * 0.24);
      // Grow slowly beyond the reference desktop width, within the existing
      // limits for window height and space beside the board.
      const wideGrowth = Math.max(0, viewportWidth - 1800) * 0.105;
      const defaultWidth = clamp(
        Math.min(wideDefaultWidth, standardWidth + wideGrowth),
        max,
      );
      setGeometry((previous) =>
        previous.max === max && previous.defaultWidth === defaultWidth
          ? previous
          : { max, defaultWidth },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(grid);
    window.addEventListener('resize', measure);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [gridRef, active, overview]);

  useLayoutEffect(
    () => () => {
      const previous = drag.current;
      drag.current = undefined;
      if (previous?.target.hasPointerCapture(previous.pointer))
        previous.target.releasePointerCapture(previous.pointer);
    },
    [],
  );

  const handle = (
    <Tooltip
      as="span"
      role="separator"
      tabIndex={0}
      aria-label={text('resizeBoard')}
      aria-orientation="vertical"
      aria-controls="analysis-board-panel"
      aria-valuemin={limits.min}
      aria-valuemax={geometry.max}
      aria-valuenow={width}
      text={text('resizeBoardHelp')}
      className="analysis-board-resizer"
      hidden={!active}
      onPointerDown={(event) => {
        if (event.button !== 0 || !event.isPrimary || drag.current) return;
        event.preventDefault();
        event.stopPropagation();
        event.currentTarget.focus();
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = {
          pointer: event.pointerId,
          target: event.currentTarget,
          startX: event.clientX,
          startWidth: width,
          width,
        };
      }}
      onPointerMove={(event) => {
        const current = drag.current;
        if (!current || current.pointer !== event.pointerId) return;
        current.width = clamp(
          current.startWidth + event.clientX - current.startX,
          geometry.max,
        );
        setPreview(current.width);
      }}
      onPointerUp={(event) => {
        const current = drag.current;
        if (!current || current.pointer !== event.pointerId) return;
        drag.current = undefined;
        setPreview(undefined);
        if (current.width !== current.startWidth) setPreferred(current.width);
        if (event.currentTarget.hasPointerCapture(event.pointerId))
          event.currentTarget.releasePointerCapture(event.pointerId);
      }}
      onPointerCancel={cancel}
      onLostPointerCapture={cancel}
      onDoubleClick={() => {
        cancel();
        setPreferred(null);
      }}
      onKeyDown={(event) => {
        if (
          ![
            'ArrowLeft',
            'ArrowRight',
            'Home',
            'End',
            'Enter',
            'Escape',
          ].includes(event.key)
        )
          return;
        event.preventDefault();
        event.stopPropagation();
        if (event.key === 'Escape') {
          cancel();
          return;
        }
        if (drag.current) return;
        if (event.key === 'Enter') {
          setPreferred(null);
          return;
        }
        const step = event.shiftKey ? 50 : 10;
        const next =
          event.key === 'Home'
            ? limits.min
            : event.key === 'End'
              ? geometry.max
              : width + (event.key === 'ArrowLeft' ? -step : step);
        const nextWidth = clamp(next, geometry.max);
        if (nextWidth !== width) setPreferred(nextWidth);
      }}
    />
  );
  return { width, dragging: preview !== undefined, handle };
}
