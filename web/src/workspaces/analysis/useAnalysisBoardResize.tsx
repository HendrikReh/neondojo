import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';
import configurationOptions from '../../../../shared/configuration-options.json' with { type: 'json' };
import { useWorkspaceDefaults } from '../../settings/configuration/ConfigurationProvider';
import { useWorkspaceLayout } from '../../shared/hooks/useWorkspaceSession';
import { useBoardReviewText } from '../../shared/board/board-review-messages';
import Tooltip from '../../shared/ui/Tooltip';

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
  frame?: number;
};

// A drag writes the width straight to the grid's CSS variable and the
// separator's value once per frame. React state changes only when the drag
// starts and ends, so pointer moves do not re-render the analysis workspace.
function showWidth(grid: HTMLElement | null, drag: Drag, width: number) {
  grid?.style.setProperty('--analysis-board-width', `${width}px`);
  drag.target.setAttribute('aria-valuenow', String(width));
}

function stopDrag(grid: HTMLElement | null, drag: Drag, width: number) {
  if (drag.frame !== undefined) cancelAnimationFrame(drag.frame);
  showWidth(grid, drag, width);
  if (drag.target.hasPointerCapture(drag.pointer))
    drag.target.releasePointerCapture(drag.pointer);
}

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
  const [dragging, setDragging] = useState(false);
  const drag = useRef<Drag | undefined>(undefined);
  const width = clamp(preferred ?? geometry.defaultWidth, geometry.max);
  const cancel = useCallback(() => {
    const previous = drag.current;
    drag.current = undefined;
    setDragging(false);
    if (previous) stopDrag(gridRef.current, previous, previous.startWidth);
  }, [gridRef]);

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
      // The footer and the page body's bottom padding sit below the grid, so
      // the grid ends where they still fit in the window.
      const body = grid.closest('.app-body');
      const footer = body?.querySelector(':scope > .app-footer');
      const footerStyle = footer && getComputedStyle(footer);
      const below =
        (body ? parseFloat(getComputedStyle(body).paddingBottom) || 0 : 0) +
        (footer && footerStyle
          ? footer.getBoundingClientRect().height +
            (parseFloat(footerStyle.marginTop) || 0) +
            (parseFloat(footerStyle.marginBottom) || 0)
          : 0);
      // Page scrolling should not change the saved or automatic geometry.
      const viewportSpace = Math.max(
        0,
        Math.floor(
          window.innerHeight -
            grid.getBoundingClientRect().top -
            window.scrollY -
            gap -
            below,
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
    const footer = grid.closest('.app-body')?.querySelector(':scope > .app-footer');
    if (footer) observer.observe(footer);
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
      if (previous?.frame !== undefined) cancelAnimationFrame(previous.frame);
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
        setDragging(true);
      }}
      onPointerMove={(event) => {
        const current = drag.current;
        if (!current || current.pointer !== event.pointerId) return;
        current.width = clamp(
          current.startWidth + event.clientX - current.startX,
          geometry.max,
        );
        current.frame ??= requestAnimationFrame(() => {
          current.frame = undefined;
          if (drag.current === current)
            showWidth(gridRef.current, current, current.width);
        });
      }}
      onPointerUp={(event) => {
        const current = drag.current;
        if (!current || current.pointer !== event.pointerId) return;
        drag.current = undefined;
        stopDrag(gridRef.current, current, current.width);
        setDragging(false);
        if (current.width !== current.startWidth) setPreferred(current.width);
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
    dragging,
    handle,
  };
}
