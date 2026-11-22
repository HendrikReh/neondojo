import type { KeyboardEvent } from 'react';
import type { Candidate } from '../api/api';

/** A user-selected checker candidate; XG position references and cube verdicts have none. */
export interface PreviewCandidate {
  notation: string;
  rank: number;
  source: string;
  /** A hypothetical roll for the same position, as in Dice equity; omitted means the recorded roll. */
  dice?: [number, number];
}

export function candidatePreviewNotation(
  candidate: Pick<Candidate, 'notation' | 'path'>,
) {
  return candidate.path ?? candidate.notation;
}

/** Keep arrow-key navigation within the focused candidate list. */
export function navigateCandidatePreview(
  event: KeyboardEvent<HTMLTableSectionElement>,
  onPreview: (index: number, column?: string) => void,
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
  const column = event.target.closest<HTMLButtonElement>(
    '.candidate-preview-button',
  )?.dataset.previewColumn;
  const button = focusCandidateRow(rows[next], column);
  rows[next].scrollIntoView({ block: 'nearest', inline: 'nearest' });
  onPreview(next, button?.dataset.previewColumn);
}

/** A click on any cell leaves keyboard focus on that candidate's native button. */
export function focusCandidateRow(row: HTMLTableRowElement, column?: string) {
  const buttons = row.querySelectorAll<HTMLButtonElement>(
    '.candidate-preview-button',
  );
  const button =
    column === undefined
      ? buttons[0]
      : (Array.from(buttons).find(
          (candidate) => candidate.dataset.previewColumn === column,
        ) ?? buttons[0]);
  button?.focus({ preventScroll: true });
  return button;
}
