import type { Position } from '../api/api';
import {
  applySteps,
  matchingMoves,
  type CheckerStep,
  type CheckerTurn,
} from './play-state';

export type BoardView = 'before' | 'arrows' | 'after';
export type CheckerPosition = Pick<Position, 'board' | 'player' | 'dice'>;
export interface MovePreviewStep {
  from: number | 'bar';
  to: number | 'off';
  die: number;
  hit: boolean;
}
export type MovePreview =
  | { ok: true; board: number[]; steps: MovePreviewStep[]; forced: boolean }
  | { ok: false; reason: string };

/** Restore the submitted path from the stored absolute checker steps. */
export function projectPracticeSteps(
  position: CheckerPosition,
  steps: CheckerStep[],
  turns: CheckerTurn[] | undefined,
): MovePreview {
  const { player } = position;
  if (!turns) return { ok: false, reason: 'Checker paths are unavailable.' };
  const turn = matchingMoves(turns, steps).find(
    (move) => move.steps.length === steps.length,
  );
  if (!turn) {
    return steps.length === 0 && turns.length === 0
      ? { ok: true, board: [...position.board], steps: [], forced: true }
      : {
          ok: false,
          reason: 'The submitted steps do not describe a complete legal roll.',
        };
  }
  const opponent = player === 0 ? -1 : 1;
  let board = position.board;
  const previewSteps = steps.map(({ from, to }, index): MovePreviewStep => {
    const hit = board[to] === opponent;
    board = applySteps(board, [{ from, to }], player);
    return {
      from: from === 0 || from === 25 ? 'bar' : from,
      to: to < 0 || to > 25 ? 'off' : to,
      die: turn.dice[index],
      hit,
    };
  });
  return { ok: true, board, steps: previewSteps, forced: false };
}

interface Run {
  points: number[];
  hits: boolean[];
}
interface LocalStep {
  from: number;
  to: number;
  die: number;
  hit: boolean;
}

// Work in mover-relative coordinates: our bar is 25, our off is a sentinel 0.
// The board's slot 0 is always the opponent's bar, never our bear-off tray.
function legalStep(board: number[], from: number, die: number) {
  if (board[from] <= 0 || (board[25] > 0 && from !== 25)) return null;
  const to = Math.max(0, from - die);
  if (to > 0) {
    if (board[to] < -1) return null;
  } else if (
    from > 6 ||
    board.slice(7).some((count) => count > 0) ||
    (die > from && board.slice(from + 1, 7).some((count) => count > 0))
  )
    return null;
  const after = [...board];
  const hit = to > 0 && after[to] === -1;
  after[from]--;
  if (to > 0) {
    if (hit) {
      after[to] = 0;
      after[0]--;
    }
    after[to]++;
  }
  return { after, step: { from, to, die, hit } };
}

function anyStep(board: number[], die: number) {
  for (let from = 1; from <= 25; from++) {
    if (legalStep(board, from, die)) return true;
  }
  return false;
}

function maximumPlays(
  board: number[],
  dice: number[],
  memo: Map<string, number>,
): number {
  if (!dice.length) return 0;
  const key = `${board.join(',')}/${dice.join(',')}`;
  const cached = memo.get(key);
  if (cached !== undefined) return cached;
  let best = 0;
  for (let index = 0; index < dice.length; index++) {
    if (index > 0 && dice[index] === dice[index - 1]) continue;
    const remaining = dice.filter((_, i) => i !== index);
    for (let from = 1; from <= 25; from++) {
      const next = legalStep(board, from, dice[index]);
      if (!next) continue;
      best = Math.max(best, 1 + maximumPlays(next.after, remaining, memo));
      if (best === dice.length) {
        memo.set(key, best);
        return best;
      }
    }
  }
  memo.set(key, best);
  return best;
}

function parseRuns(notation: string): Run[] | null {
  if (!notation || notation.length > 160) return null;
  const runs: Run[] = [];
  let segments = 0;
  for (const token of notation.split(/\s+/)) {
    const match =
      /^((?:bar|[1-9]|1[0-9]|2[0-4])(?:\/(?:off|[1-9]|1[0-9]|2[0-4])\*?)+)(?:\(([1-4])\))?$/i.exec(
        token,
      );
    if (!match) return null;
    const parts = match[1].toLowerCase().split('/');
    const points = parts.map((part) =>
      part === 'bar' ? 25 : part === 'off' ? 0 : Number(part.replace('*', '')),
    );
    // "off*" cannot be a hit and off must terminate the path.
    if (
      parts.some((part) => part === 'off*') ||
      points.some((point, i) => i > 0 && point >= points[i - 1])
    )
      return null;
    const copies = Number(match[2] ?? 1);
    segments += (points.length - 1) * copies;
    if (segments > 4) return null;
    for (let copy = 0; copy < copies; copy++) {
      runs.push({
        points,
        hits: parts.slice(1).map((part) => part.endsWith('*')),
      });
    }
  }
  return runs;
}

function joinRuns(runs: Run[]): Run[] {
  const joined: Run[] = [];
  for (const run of runs) {
    const previous = joined.find(
      (path) => path.points.at(-1) === run.points[0],
    );
    if (previous) {
      previous.points.push(...run.points.slice(1));
      previous.hits.push(...run.hits);
    } else {
      joined.push({ points: [...run.points], hits: [...run.hits] });
    }
  }
  return joined;
}

/** Shorten the display of a checker path, keeping every point where it hits. */
export function compactCheckerNotation(
  notation: string,
  position?: CheckerPosition,
) {
  const runs = parseRuns(notation.trim());
  if (!runs) return notation;
  let paths = joinRuns(runs);
  if (paths.every(({ points }) => points.length === 2)) return notation;

  if (position) {
    // Some sources store individual steps without hit markers.
    const preview = projectMove({ ...position, notation });
    if (!preview.ok) return notation;
    const relative = (point: number) =>
      position.player === 0 ? point : 25 - point;
    paths = joinRuns(
      preview.steps.map(({ from, to, hit }) => ({
        points: [
          from === 'bar' ? 25 : relative(from),
          to === 'off' ? 0 : relative(to),
        ],
        hits: [hit],
      })),
    );
  }

  const counts = new Map<string, number>();
  for (const { points, hits } of paths) {
    const path = points
      .flatMap((point, index) => {
        const hit = hits[index - 1];
        if (index > 0 && index < points.length - 1 && !hit) return [];
        const name = point === 25 ? 'bar' : point === 0 ? 'off' : String(point);
        return hit ? `${name}*` : name;
      })
      .join('/');
    counts.set(path, (counts.get(path) ?? 0) + 1);
  }
  return [...counts]
    .map(([path, count]) => (count > 1 ? `${path}(${count})` : path))
    .join(' ');
}

function expandRuns(
  board: number[],
  dice: number[],
  runs: Run[],
  required: number,
  higherDie: number | null,
  steps: LocalStep[] = [],
): { board: number[]; steps: LocalStep[] } | null {
  if (!runs.length) {
    const won = !board.some((count) => count > 0);
    if (steps.length !== required && !won) return null;
    if (steps.length === 1 && higherDie !== null && steps[0].die !== higherDie)
      return null;
    return { board, steps };
  }
  if (!dice.length) return null;
  for (let runIndex = 0; runIndex < runs.length; runIndex++) {
    const run = runs[runIndex];
    const [from, target] = run.points;
    for (let dieIndex = 0; dieIndex < dice.length; dieIndex++) {
      if (dieIndex > 0 && dice[dieIndex] === dice[dieIndex - 1]) continue;
      const next = legalStep(board, from, dice[dieIndex]);
      if (!next || next.step.to < target) continue;
      const reached = next.step.to === target;
      // A hidden hit changes the position; require it to be an explicit endpoint.
      if (
        (!reached && next.step.hit) ||
        (reached && run.hits[0] && !next.step.hit)
      )
        continue;
      const nextRuns = [...runs];
      if (reached && run.points.length === 2) nextRuns.splice(runIndex, 1);
      else
        nextRuns[runIndex] = reached
          ? { points: run.points.slice(1), hits: run.hits.slice(1) }
          : { points: [next.step.to, ...run.points.slice(1)], hits: run.hits };
      const result = expandRuns(
        next.after,
        dice.filter((_, i) => i !== dieIndex),
        nextRuns,
        required,
        higherDie,
        [...steps, next.step],
      );
      if (result) return result;
    }
  }
  return null;
}

/** Project a complete legal roll without modifying a stored position or its IDs. */
export function projectMove(
  position: CheckerPosition & Pick<Position, 'notation'>,
  notation = position.notation,
): MovePreview {
  const invalid = (reason: string): MovePreview => ({ ok: false, reason });
  const { board, player, dice } = position;
  if (
    board.length !== 26 ||
    board.some((count) => !Number.isInteger(count) || Math.abs(count) > 15) ||
    board[0] > 0 ||
    board[25] < 0 ||
    board.reduce((sum, count) => sum + Math.max(0, count), 0) > 15 ||
    board.reduce((sum, count) => sum + Math.max(0, -count), 0) > 15 ||
    (player !== 0 && player !== 1)
  )
    return invalid('The position cannot be projected safely.');
  if (
    dice.length !== 2 ||
    dice.some((die) => !Number.isInteger(die) || die < 1 || die > 6)
  ) {
    return invalid('A checker preview requires two valid dice.');
  }
  const local =
    player === 0
      ? [...board]
      : [...board].reverse().map((count) => -count || 0);
  if (!local.some((count) => count > 0))
    return invalid('The moving player has no checkers.');
  const available = (
    dice[0] === dice[1] ? Array<number>(4).fill(dice[0]) : [...dice]
  ).sort((a, b) => b - a);
  const required = maximumPlays(local, available, new Map());
  const text = notation.trim();
  if (/^(?:cannot\s+move|dance)$/i.test(text)) {
    return required === 0
      ? { ok: true, board: [...board], steps: [], forced: true }
      : invalid('A checker can move; this is not a forced pass.');
  }
  const runs = parseRuns(text);
  if (!runs) return invalid('This notation is not a supported checker move.');
  const high = Math.max(...dice);
  const higherDie = dice[0] !== dice[1] && anyStep(local, high) ? high : null;
  const expanded = expandRuns(local, available, runs, required, higherDie);
  if (!expanded)
    return invalid('The notation does not describe a complete legal roll.');
  const point = (value: number) => (player === 0 ? value : 25 - value);
  return {
    ok: true,
    board:
      player === 0
        ? expanded.board
        : [...expanded.board].reverse().map((count) => -count || 0),
    steps: expanded.steps.map((step) => ({
      from: step.from === 25 ? 'bar' : point(step.from),
      to: step.to === 0 ? 'off' : point(step.to),
      die: step.die,
      hit: step.hit,
    })),
    forced: false,
  };
}
