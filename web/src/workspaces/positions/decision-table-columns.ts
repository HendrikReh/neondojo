import { createColumnLayout } from '../../shared/hooks/column-widths';

/** Columns of the analyzed-decision table, in display order. */
export const DECISION_COLUMNS = [
  { id: 'match', className: 'decision-col-match', min: 90, defaultWidth: 200 },
  { id: 'move', className: 'decision-col-move', min: 60, defaultWidth: 120 },
  { id: 'kind', className: 'decision-col-kind', min: 80, defaultWidth: 150 },
  { id: 'score', className: 'decision-col-score', min: 70, defaultWidth: 150 },
  { id: 'cube', className: 'decision-col-cube', min: 44, defaultWidth: 70 },
  {
    id: 'played',
    className: 'decision-col-played',
    min: 60,
    defaultWidth: 110,
  },
  { id: 'best', className: 'decision-col-best', min: 60, defaultWidth: 110 },
  { id: 'loss', className: 'decision-col-loss', min: 60, defaultWidth: 100 },
  {
    id: 'source',
    className: 'decision-col-source',
    min: 90,
    defaultWidth: 220,
  },
] as const;

export const DECISION_COLUMN_LAYOUT = createColumnLayout(
  DECISION_COLUMNS,
  'neondojo.decisions.columns',
);

export type DecisionColumn = (typeof DECISION_COLUMNS)[number]['id'];

export const DECISION_COLUMN_IDS: readonly DecisionColumn[] =
  DECISION_COLUMNS.map(({ id }) => id);
