import type { KeyboardEvent } from 'react';

/** A user-selected checker candidate; XG position references and cube verdicts have none. */
export interface PreviewCandidate {
  notation: string;
  rank: number;
  source: string;
}

/** Checker comparison and its loss within a single analysis source. */
export interface PreviewComparison {
  best?: PreviewCandidate;
  loss: number | null;
  lossUnit?: 'mwc';
  /** The local engine tab can show a saved game result while no hint exists. */
  useSavedGame?: boolean;
}

/** Keep arrow-key navigation within the focused candidate list. */
export function navigateCandidatePreview(
  event: KeyboardEvent<HTMLTableSectionElement>,
  onPreview: (index: number) => void,
) {
  if (
    !['ArrowUp', 'ArrowDown'].includes(event.key) ||
    event.altKey ||
    event.ctrlKey ||
    event.metaKey ||
    event.shiftKey ||
    !(event.target instanceof Element)
  )
    return;
  const row = event.target.closest<HTMLTableRowElement>(
    'tr.candidate-preview-row',
  );
  const rows = Array.from(
    event.currentTarget.querySelectorAll<HTMLTableRowElement>(
      'tr.candidate-preview-row',
    ),
  );
  const current = row ? rows.indexOf(row) : -1;
  if (current < 0) return;
  event.preventDefault();
  event.stopPropagation();
  const next = Math.max(
    0,
    Math.min(rows.length - 1, current + (event.key === 'ArrowDown' ? 1 : -1)),
  );
  focusCandidateRow(rows[next]);
  rows[next].scrollIntoView({ block: 'nearest', inline: 'nearest' });
  onPreview(next);
}

/** A click on any cell leaves keyboard focus on that candidate's native button. */
export function focusCandidateRow(row: HTMLTableRowElement) {
  row
    .querySelector<HTMLButtonElement>('.candidate-preview-button')
    ?.focus({ preventScroll: true });
}
