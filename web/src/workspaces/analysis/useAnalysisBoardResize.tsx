import { useLayoutEffect, useState, type RefObject } from 'react';
import configurationOptions from '../../../../shared/configuration-options.json' with { type: 'json' };
import { useWorkspaceDefaults } from '../../settings/configuration/ConfigurationProvider';
import { useWorkspaceLayout } from '../../shared/hooks/useWorkspaceSession';
import { usePaneSplitter } from '../../shared/hooks/usePaneSplitter';
import { useBoardReviewText } from '../../shared/board/board-review-messages';

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
      analysis_overview_board_width:
        legacy.appearance.analysis_overview_board_width,
      analysis_errors_board_width:
        legacy.appearance.analysis_errors_board_width,
      analysis_statistics_board_width:
        legacy.appearance.analysis_statistics_board_width,
      analysis_notes_board_width: legacy.appearance.analysis_notes_board_width,
    },
    defaultWidths,
    (value) => {
      if (typeof value !== 'object' || value === null || Array.isArray(value))
        return undefined;
      const widths = value as Partial<BoardWidths>;
      const result = { ...defaultWidths };
      for (const key of Object.values(boardWidthFields)) {
        const width = widths[key];
        if (typeof width === 'number' && Number.isFinite(width))
          result[key] = clamp(width, limits.max);
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
  const width = clamp(preferred ?? geometry.defaultWidth, geometry.max);

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
      const max = Math.max(
        limits.min,
        Math.floor(Math.min(available, limits.max)),
      );
      const viewportWidth = window.innerWidth;
      // The footer and the page body's bottom padding sit below the grid, so
      // the grid ends where they still fit in the window.
      const body = grid.closest('.app-body');
      const footer = body?.querySelector(':scope > .app-footer');
      const below =
        (body ? parseFloat(getComputedStyle(body).paddingBottom) || 0 : 0) +
        (footer?.getBoundingClientRect().height ?? 0);
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
    const footer = grid
      .closest('.app-body')
      ?.querySelector(':scope > .app-footer');
    if (footer) observer.observe(footer);
    if (shell) observer.observe(shell);
    window.addEventListener('resize', measure);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [gridRef, active, layout]);

  const { dragging, handle } = usePaneSplitter({
    containerRef: gridRef,
    cssVar: '--analysis-board-width',
    width,
    min: limits.min,
    max: geometry.max,
    direction: 'right',
    active,
    resetKey: `${layout}:${preferred ?? ''}`,
    onCommit: (nextWidth) => setPreferred(nextWidth),
    onReset: () => setPreferred(null),
    ariaLabel: text('resizeBoard'),
    ariaControls: 'analysis-board-panel',
    helpText: text('resizeBoardHelp'),
    className: 'analysis-board-resizer',
    hidden: !active,
  });

  return {
    width,
    viewportSpace: geometry.viewportSpace,
    dragging,
    handle,
  };
}
