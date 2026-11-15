import type {
  AnalysisCatalogEntry,
  PositionAnalysisResult,
} from './analysis-catalog-api';
import type { Position } from './api';
import { candidatePreviewNotation } from './candidate-preview';
import {
  candidateEquityDifference,
  candidateEquityRanks,
  type CandidatePosition,
  type ComparisonCandidate,
} from './candidate-comparison';
import { projectMove } from './move-preview';

export const comparisonEngines = ['xg', 'gnubg', 'bgsage'] as const;
export type ComparisonEngine = (typeof comparisonEngines)[number];
export type XgComparisonEngine = Exclude<ComparisonEngine, 'xg'>;
const xgName = /\b(?:extreme gammon|xg)\b/i;

export function positionComparisonOptions(
  entries: AnalysisCatalogEntry[],
  position: Pick<Position, 'id' | 'game_no'>,
  engine: ComparisonEngine,
) {
  return entries
    .filter((entry) => {
      if (entry.status !== 'completed') return false;
      if (entry.scope === 'position' && entry.position_id !== position.id)
        return false;
      if (entry.scope === 'game' && entry.game_no !== position.game_no)
        return false;
      if (engine === 'xg')
        return entry.source === 'imported' && xgName.test(entry.engine);
      if (entry.source !== 'native') return false;
      const provider = entry.job?.engine_snapshot?.engine_id;
      return provider
        ? provider === engine
        : (engine === 'gnubg'
            ? /^(GNU Backgammon|GNUbg)\b/i
            : /^Open Sage\b/i
          ).test(entry.engine);
    })
    .sort(
      (a, b) =>
        (b.job?.plies ?? -1) - (a.job?.plies ?? -1) ||
        b.created_at.localeCompare(a.created_at),
    );
}

export function xgCheckerCandidates(
  result: PositionAnalysisResult | null | undefined,
  player: number,
): ComparisonCandidate[] {
  return (
    result?.records.find(
      (record) =>
        record.kind === 'checker' &&
        record.player === player &&
        xgName.test(record.engine) &&
        record.candidates.length > 0,
    )?.candidates ?? []
  );
}

interface PositionComparisonRow {
  key: string;
  notation: string;
  played: boolean;
  xgEquityRank?: number;
  candidates: (ComparisonCandidate | undefined)[];
}

export function positionComparisonRows(
  position: CandidatePosition,
  columns: ComparisonCandidate[][],
): PositionComparisonRow[] {
  const boards = new Map<string, string>();
  function keyFor(notation: string) {
    const text = notation.trim();
    const cached = boards.get(text);
    if (cached) return cached;
    const projected = projectMove(position, text);
    const key = projected.ok
      ? `board:${projected.board.join(',')}`
      : `notation:${text}`;
    boards.set(text, key);
    return key;
  }
  const played = keyFor(position.notation);
  const equityRanks = candidateEquityRanks(columns[0] ?? []);
  const rows = new Map<string, PositionComparisonRow>();
  columns.forEach((candidates, column) => {
    const ordered =
      column === 0 && equityRanks.size ? equityRanks.keys() : candidates;
    for (const candidate of ordered) {
      const key = keyFor(candidatePreviewNotation(candidate));
      let row = rows.get(key);
      if (!row) {
        row = {
          key,
          notation: candidate.notation,
          played: key === played,
          xgEquityRank: equityRanks.get(candidate),
          candidates: Array<ComparisonCandidate | undefined>(
            columns.length,
          ).fill(undefined),
        };
        rows.set(key, row);
      }
      row.candidates[column] ??= candidate;
    }
  });
  return [...rows.values()];
}

export function sortPositionComparisonRows(
  rows: readonly PositionComparisonRow[],
  engine: XgComparisonEngine,
): PositionComparisonRow[] {
  const index = comparisonEngines.indexOf(engine);
  return rows
    .map((row) => {
      const difference = candidateEquityDifference(
        row.candidates[index],
        row.candidates[0],
      );
      return {
        row,
        magnitude: difference === null ? -1 : Math.abs(difference),
      };
    })
    .sort((a, b) => b.magnitude - a.magnitude)
    .map(({ row }) => row);
}
