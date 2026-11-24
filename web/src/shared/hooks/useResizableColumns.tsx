import Tooltip from '../ui/Tooltip';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
} from 'react';
import { useLatest } from './useLatest';
import {
  MAX_COLUMN_WIDTH,
  captureColumnWidths,
  type ColumnLayout,
  type ColumnWidths,
} from './column-widths';
import './resizable-columns.css';
import { useWorkspaceLayout } from './useWorkspaceSession';

type Drag<Id extends string> = {
  column: Id;
  startX: number;
  base: ColumnWidths<Id>;
  latest: ColumnWidths<Id>;
  before?: ColumnWidths<Id>;
  pointer: number;
  target: HTMLElement;
  frame?: number;
};
// A drag writes widths straight to the table's <col data-column> elements and
// the separator once per frame. React state changes only when the drag starts
// and ends, so pointer moves do not re-render the whole table.
function showWidths<Id extends string>(
  table: HTMLTableElement | null,
  drag: Drag<Id>,
  widths: ColumnWidths<Id>,
  total: number | undefined,
) {
  for (const col of table?.querySelectorAll<HTMLElement>('col[data-column]') ??
    []) {
    const width = widths[col.dataset.column as Id];
    if (width !== undefined) col.style.width = `${width}px`;
  }
  if (table && total !== undefined) {
    table.style.width = `${total}px`;
    table.style.minWidth = `${total}px`;
  }
  drag.target.setAttribute('aria-valuenow', String(widths[drag.column]));
}
export type ResizableColumnsOptions<Id extends string> = {
  visible: boolean;
  activeColumns?: readonly Id[];
  resizeLabel: (label: string) => string;
  legacyWidths?: Record<string, number> | null;
};

export function useResizableColumns<Id extends string>(
  layout: ColumnLayout<Id>,
  options: ResizableColumnsOptions<Id>,
) {
  const tableRef = useRef<HTMLTableElement>(null);
  const [legacyLocalWidths] = useState(layout.read);
  const fillWidths = (values: Record<string, number>) =>
    captureColumnWidths(
      layout,
      (id) =>
        values[id] ??
        layout.columns.find((column) => column.id === id)?.defaultWidth ??
        layout.minimum(id),
    );
  const [savedWidths, saveWidths] = useWorkspaceLayout<ColumnWidths<Id> | null>(
    layout.storageKey.replace(/^neondojo\./, ''),
    options.legacyWidths === undefined
      ? undefined
      : options.legacyWidths && fillWidths(options.legacyWidths),
    legacyLocalWidths ?? null,
    (value) => {
      if (value === null) return null;
      if (!value || typeof value !== 'object' || Array.isArray(value))
        return undefined;
      const widths = value as Record<string, unknown>;
      return captureColumnWidths(layout, (id) =>
        typeof widths[id] === 'number' ? widths[id] : undefined,
      );
    },
  );
  const [dragWidths, setWidths] = useState<ColumnWidths<Id>>();
  const [measured, setMeasured] = useState<ColumnWidths<Id>>();
  const [dragging, setDragging] = useState(false);
  const active = useRef<Drag<Id> | undefined>(undefined);
  const widths = dragging ? dragWidths : (savedWidths ?? undefined);
  const current = useLatest({ layout, options, widths });
  const activeKey = JSON.stringify(
    options.activeColumns ?? layout.columns.map((column) => column.id),
  );
  const custom = !!widths;

  const updateWidths = useCallback((value: ColumnWidths<Id> | undefined) => {
    current.current.widths = value;
    setWidths(value);
  }, []);
  const capture = useCallback((): ColumnWidths<Id> | undefined => {
    const table = tableRef.current;
    if (!table) return;
    const { layout, options, widths } = current.current;
    const headers = new Map(
      Array.from(table.querySelectorAll<HTMLElement>('th[data-column]')).map(
        (header) => [header.dataset.column, header],
      ),
    );
    return captureColumnWidths(
      layout,
      (column) => headers.get(column)?.getBoundingClientRect().width,
      { widths, activeColumns: options.activeColumns },
    );
  }, []);
  useLayoutEffect(() => {
    const table = tableRef.current;
    if (!table || !options.visible) return;
    const measure = () => {
      if (active.current) return;
      const value = capture();
      if (value)
        setMeasured((previous) =>
          previous &&
          current.current.layout.columns.every(
            (column) => previous[column.id] === value[column.id],
          )
            ? previous
            : value,
        );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(table);
    return () => observer.disconnect();
  }, [capture, options.visible, activeKey, layout.storageKey, custom]);
  const show = useCallback(
    (drag: Drag<Id>, widths: ColumnWidths<Id>) =>
      showWidths(
        tableRef.current,
        drag,
        widths,
        current.current.layout.total(
          widths,
          current.current.options.activeColumns,
        ),
      ),
    [tableRef],
  );
  const cancel = useCallback(() => {
    const drag = active.current;
    if (!drag) return;
    active.current = undefined;
    if (drag.frame !== undefined) cancelAnimationFrame(drag.frame);
    // React last rendered the drag's base widths.
    show(drag, drag.base);
    updateWidths(drag.before);
    setDragging(false);
    if (drag.target.hasPointerCapture(drag.pointer))
      drag.target.releasePointerCapture(drag.pointer);
  }, [show, updateWidths]);
  useEffect(() => {
    cancel();
  }, [options.visible, activeKey, layout.storageKey, cancel]);
  useEffect(
    () => () => {
      const drag = active.current;
      active.current = undefined;
      if (drag?.frame !== undefined) cancelAnimationFrame(drag.frame);
      if (drag?.target.hasPointerCapture(drag.pointer))
        drag.target.releasePointerCapture(drag.pointer);
    },
    [],
  );
  function begin(event: PointerEvent<HTMLElement>, column: Id) {
    if (
      event.button !== 0 ||
      !event.isPrimary ||
      !options.visible ||
      active.current
    )
      return;
    const base = capture();
    if (!base) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.focus();
    event.currentTarget.setPointerCapture(event.pointerId);
    active.current = {
      column,
      startX: event.clientX,
      base,
      latest: base,
      before: current.current.widths,
      pointer: event.pointerId,
      target: event.currentTarget,
    };
    updateWidths(base);
    setDragging(true);
  }
  function move(event: PointerEvent<HTMLElement>) {
    const drag = active.current;
    if (!drag || drag.pointer !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    drag.latest = layout.resize(
      drag.base,
      drag.column,
      drag.base[drag.column] + event.clientX - drag.startX,
    );
    drag.frame ??= requestAnimationFrame(() => {
      drag.frame = undefined;
      if (active.current === drag) show(drag, drag.latest);
    });
  }
  function finish(event: PointerEvent<HTMLElement>) {
    const drag = active.current;
    if (!drag || drag.pointer !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    active.current = undefined;
    if (drag.frame !== undefined) cancelAnimationFrame(drag.frame);
    show(drag, drag.latest);
    setDragging(false);
    if (drag.latest[drag.column] === drag.base[drag.column])
      updateWidths(drag.before);
    else persist(drag.latest);
    if (event.currentTarget.hasPointerCapture(event.pointerId))
      event.currentTarget.releasePointerCapture(event.pointerId);
  }
  function key(event: KeyboardEvent<HTMLElement>, column: Id) {
    if (event.key === 'Escape' && active.current) {
      event.preventDefault();
      event.stopPropagation();
      cancel();
      return;
    }
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    event.stopPropagation();
    const base = capture();
    if (!base || active.current || !options.visible) return;
    const delta =
      (event.shiftKey ? 50 : 10) * (event.key === 'ArrowLeft' ? -1 : 1);
    const value =
      event.key === 'Home'
        ? layout.minimum(column)
        : event.key === 'End'
          ? MAX_COLUMN_WIDTH
          : base[column] + delta;
    const next = layout.resize(base, column, value);
    updateWidths(next);
    persist(next);
  }
  function handle(column: Id, label: string) {
    const resizeLabel = options.resizeLabel(label);
    return (
      <Tooltip
        as="span"
        role="separator"
        tabIndex={0}
        aria-orientation="vertical"
        aria-label={resizeLabel}
        text={resizeLabel}
        aria-valuemin={layout.minimum(column)}
        aria-valuemax={MAX_COLUMN_WIDTH}
        aria-valuenow={(widths ?? measured)?.[column] ?? layout.minimum(column)}
        className="table-column-resizer library-column-resizer"
        onPointerDown={(event) => begin(event, column)}
        onPointerMove={move}
        onPointerUp={finish}
        onPointerCancel={cancel}
        onLostPointerCapture={cancel}
        onKeyDown={(event) => key(event, column)}
        onClick={(event) => event.stopPropagation()}
        onDoubleClick={(event) => event.stopPropagation()}
      />
    );
  }
  function persist(value: ColumnWidths<Id> | undefined) {
    saveWidths(value ?? null);
  }
  function reset() {
    cancel();
    updateWidths(undefined);
    persist(undefined);
  }
  const total = widths && layout.total(widths, options.activeColumns);
  return {
    tableRef,
    widths,
    dragging,
    handle,
    reset,
    custom,
    tableStyle:
      total === undefined ? undefined : { width: total, minWidth: total },
  };
}
