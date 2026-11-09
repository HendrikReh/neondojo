export const MAX_COLUMN_WIDTH = 2400;

export type ColumnSpec<Id extends string = string> = {
  id: Id;
  className: string;
  min: number;
  defaultWidth?: number;
};
export type ColumnWidths<Id extends string = string> = Record<Id, number>;
export type ColumnLayout<Id extends string = string> = {
  columns: readonly ColumnSpec<Id>[];
  storageKey: string;
  minimum: (column: Id) => number;
  read: (storage?: Pick<Storage, 'getItem'>) => ColumnWidths<Id> | undefined;
  save: (
    widths?: ColumnWidths<Id>,
    storage?: Pick<Storage, 'setItem' | 'removeItem'>,
  ) => void;
  resize: (
    widths: ColumnWidths<Id>,
    column: Id,
    value: number,
  ) => ColumnWidths<Id>;
  total: (widths: ColumnWidths<Id>, activeColumns?: readonly Id[]) => number;
};

function boundedWidth(column: ColumnSpec, value: number) {
  return Math.max(column.min, Math.min(MAX_COLUMN_WIDTH, Math.round(value)));
}

export function createColumnLayout<const Columns extends readonly ColumnSpec[]>(
  columns: Columns,
  storageKey: string,
  legacyDefaults: Partial<ColumnWidths<Columns[number]['id']>> = {},
): ColumnLayout<Columns[number]['id']> {
  type Id = Columns[number]['id'];
  type Widths = ColumnWidths<Id>;
  const specs = new Map<Id, ColumnSpec<Id>>(
    columns.map((column) => [column.id, column]),
  );
  const valid = (widths: unknown): widths is Widths =>
    typeof widths === 'object' &&
    widths !== null &&
    !Array.isArray(widths) &&
    columns.every((column) => {
      const value = (widths as Widths)[column.id as Id];
      return (
        Number.isInteger(value) &&
        value >= column.min &&
        value <= MAX_COLUMN_WIDTH
      );
    });
  return {
    columns,
    storageKey,
    minimum: (column) => specs.get(column)!.min,
    read(storage) {
      try {
        const raw = (storage ?? localStorage).getItem(storageKey);
        if (!raw || raw.length > 4096) return;
        const profile = JSON.parse(raw);
        if (
          profile?.version !== 1 ||
          typeof profile.widths !== 'object' ||
          profile.widths === null ||
          Array.isArray(profile.widths)
        )
          return;
        const widths = { ...legacyDefaults, ...profile.widths };
        if (!valid(widths)) return;
        return Object.fromEntries(
          columns.map((column) => [column.id, widths[column.id as Id]]),
        ) as Widths;
      } catch {
        return;
      }
    },
    save(widths, storage) {
      try {
        const target = storage ?? localStorage;
        if (widths) {
          if (!valid(widths)) return;
          target.setItem(storageKey, JSON.stringify({ version: 1, widths }));
        } else target.removeItem(storageKey);
      } catch {
        /* Resizing remains usable when local persistence is unavailable. */
      }
    },
    resize(widths, column, value) {
      if (!Number.isFinite(value)) return widths;
      return { ...widths, [column]: boundedWidth(specs.get(column)!, value) };
    },
    total(widths, activeColumns) {
      const active = activeColumns && new Set(activeColumns);
      return columns.reduce(
        (sum, column) =>
          sum +
          (!active || active.has(column.id as Id)
            ? widths[column.id as Id]
            : 0),
        0,
      );
    },
  };
}

export function captureColumnWidths<Id extends string>(
  layout: ColumnLayout<Id>,
  measure: (column: Id) => number | undefined,
  {
    widths,
    activeColumns,
  }: { widths?: ColumnWidths<Id>; activeColumns?: readonly Id[] } = {},
): ColumnWidths<Id> | undefined {
  const active = activeColumns && new Set(activeColumns);
  const entries: [Id, number][] = [];
  for (const column of layout.columns) {
    let width: number;
    if (!active || active.has(column.id)) {
      const measured = measure(column.id);
      if (measured === undefined || !Number.isFinite(measured) || measured <= 0)
        return;
      width = measured;
    } else width = widths?.[column.id] ?? column.defaultWidth ?? column.min;
    entries.push([column.id, boundedWidth(column, width)]);
  }
  return Object.fromEntries(entries) as ColumnWidths<Id>;
}
