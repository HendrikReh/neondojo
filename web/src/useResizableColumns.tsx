import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
} from 'react';
import {
  MAX_COLUMN_WIDTH,
  captureColumnWidths,
  type ColumnLayout,
  type ColumnWidths,
} from './column-widths';
import './resizable-columns.css';

type Drag<Id extends string> = {
  column: Id;
  startX: number;
  base: ColumnWidths<Id>;
  latest: ColumnWidths<Id>;
  before?: ColumnWidths<Id>;
  pointer: number;
  target: HTMLElement;
};
export type ResizableColumnsOptions<Id extends string> = {
  visible: boolean;
  activeColumns?: readonly Id[];
  resizeLabel: (label: string) => string;
  widths?: Partial<ColumnWidths<Id>> | null;
  onWidthsChange?: (widths: ColumnWidths<Id> | undefined) => void;
  configurationGeneration?: number;
};

export function useResizableColumns<Id extends string>(
  layout: ColumnLayout<Id>,
  options: ResizableColumnsOptions<Id>,
) {
  const tableRef = useRef<HTMLTableElement>(null);
  const [localWidths, setWidths] = useState(layout.read);
  const [measured, setMeasured] = useState<ColumnWidths<Id>>();
  const [dragging, setDragging] = useState(false);
  const active = useRef<Drag<Id> | undefined>(undefined);
  const widths =
    options.onWidthsChange && !dragging
      ? options.widths
        ? (Object.fromEntries(
            layout.columns.map((column) => [
              column.id,
              options.widths?.[column.id] ?? column.defaultWidth ?? column.min,
            ]),
          ) as ColumnWidths<Id>)
        : undefined
      : localWidths;
  const current = useRef({ layout, options, widths });
  current.current = { layout, options, widths };
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
  const cancel = useCallback(() => {
    const drag = active.current;
    if (!drag) return;
    active.current = undefined;
    updateWidths(drag.before);
    setDragging(false);
    if (drag.target.hasPointerCapture(drag.pointer))
      drag.target.releasePointerCapture(drag.pointer);
  }, [updateWidths]);
  useEffect(() => {
    cancel();
  }, [
    options.visible,
    activeKey,
    layout.storageKey,
    options.configurationGeneration,
    cancel,
  ]);
  useEffect(
    () => () => {
      const drag = active.current;
      active.current = undefined;
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
    updateWidths(drag.latest);
  }
  function finish(event: PointerEvent<HTMLElement>) {
    const drag = active.current;
    if (!drag || drag.pointer !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    active.current = undefined;
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
      <span
        role="separator"
        tabIndex={0}
        aria-orientation="vertical"
        aria-label={resizeLabel}
        title={resizeLabel}
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
    if (current.current.options.onWidthsChange)
      current.current.options.onWidthsChange(value);
    else layout.save(value);
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
