import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import { useWorkspaceDefaults } from './ConfigurationProvider';
import { useWorkspaceLayout } from './useWorkspaceSession';
import { useBoardReviewText } from './board-review-messages';
import Tooltip from './Tooltip';

const limits = configurationOptions.analysis_board_width;
const desktopDefaultWidth = 420;
const clamp = (value: number, max: number) =>
  Math.round(Math.max(limits.min, Math.min(max, value)));
const boardWidthFields = {
  overview: 'analysis_overview_board_width',
  errors: 'analysis_errors_board_width',
  statistics: 'analysis_statistics_board_width',
  notes: 'analysis_notes_board_width',
  cards: 'analysis_board_width',
} as const;
type BoardWidths = Record<
  (typeof boardWidthFields)[keyof typeof boardWidthFields],
  number | null
>;

const defaultWidths: BoardWidths = {
  analysis_board_width: null,
  analysis_overview_board_width: null,
  analysis_errors_board_width: null,
  analysis_statistics_board_width: null,
  analysis_notes_board_width: null,
};

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
  layout: keyof typeof boardWidthFields,
) {
  const text = useBoardReviewText();
  const legacy = useWorkspaceDefaults();
  const field = boardWidthFields[layout];
  const [preferences, setPreferences] = useWorkspaceLayout(
    'analysis.boardWidths',
    legacy && {
      analysis_board_width: legacy.appearance.analysis_board_width,
      analysis_overview_board_width: legacy.appearance.analysis_overview_board_width,
      analysis_errors_board_width: legacy.appearance.analysis_errors_board_width,
      analysis_statistics_board_width: legacy.appearance.analysis_statistics_board_width,
      analysis_notes_board_width: legacy.appearance.analysis_notes_board_width,
    },
    defaultWidths,
    (value) => {
      if (typeof value !== 'object' || value === null || Array.isArray(value)) return undefined;
      const widths = value as Partial<BoardWidths>;
      const result = { ...defaultWidths };
      for (const key of Object.values(boardWidthFields)) {
        const width = widths[key];
        if (typeof width === 'number' && Number.isFinite(width)) result[key] = clamp(width, limits.max);
      }
      return result;
    },
  );
  const preferred = preferences[field];
  const setPreferred = (width: number | null) =>
    setPreferences((current) => ({ ...current, [field]: width }));
  const [geometry, setGeometry] = useState<{
    max: number;
    defaultWidth: number;
    viewportSpace?: number;
  }>({
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
  }, [active, layout, preferred, cancel]);

  useLayoutEffect(() => {
    const grid = gridRef.current;
    const moves = grid?.querySelector('.moves-panel');
    if (!active || !grid || !moves) return;
    const measure = () => {
      if (!window.matchMedia('(min-width: 961px)').matches) return;
      const gap = parseFloat(getComputedStyle(grid).columnGap) || 0;
      const available =
        grid.clientWidth -
        moves.getBoundingClientRect().width -
        3 * gap -
        8 -
        400;
      const max = Math.max(limits.min, Math.floor(Math.min(available, limits.max)));
      const viewportWidth = window.innerWidth;
      // Page scrolling should not change the saved or automatic geometry.
      const viewportSpace = Math.max(
        0,
        Math.floor(
          window.innerHeight -
            grid.getBoundingClientRect().top -
            window.scrollY -
            gap,
        ),
      );
      const extraWidth = Math.max(0, viewportWidth - 2560);
      const defaultWidth = clamp(
        300 + viewportWidth * 0.22 + extraWidth * 0.23,
        max,
      );
      setGeometry((previous) =>
        previous.max === max &&
        previous.defaultWidth === defaultWidth &&
        previous.viewportSpace === viewportSpace
          ? previous
          : { max, defaultWidth, viewportSpace },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(grid);
    for (const element of grid.parentElement?.children ?? []) {
      if (element !== grid) observer.observe(element);
    }
    const shell = grid.closest('.app-shell');
    const header = shell?.querySelector('.app-header');
    if (header) observer.observe(header);
    if (shell) observer.observe(shell);
    window.addEventListener('resize', measure);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [gridRef, active, layout]);

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
        setPreferred(geometry.max);
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
  return {
    width,
    viewportSpace: geometry.viewportSpace,
    dragging: preview !== undefined,
    handle,
  };
}
