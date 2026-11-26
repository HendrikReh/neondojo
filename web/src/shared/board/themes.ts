import { use } from 'react';
import { checkerCounts } from './boardMath';
import { isChunkLoadError, RetryableChunkError } from '../chunk-load';

export interface ThemeImage {
  src: string;
  width: number;
  height: number;
}
export interface BgBlitzTheme {
  id: string;
  name: string;
  source: string;
  background: ThemeImage;
  columns: number[];
  checkerOffsetX: number;
  checkerOffsetY: number;
  checkers: { white: ThemeImage[]; black: ThemeImage[] };
  dice: { white: ThemeImage[]; black: ThemeImage[] };
  diceLeftX: number;
  diceRightX: number;
  cubes: Record<string, ThemeImage>;
  cubeLeftX: number;
  cubeRightX: number;
  cubeOffsetY: number;
  cubeCenteredY: number;
  numbers: Record<string, ThemeImage[]>;
  numbersX: number;
  numbersY: number[];
}

let archiveRequest: Promise<BgBlitzTheme[]> | undefined;
let archiveFailure: unknown;
let installed: BgBlitzTheme[] | undefined;

export function loadArchiveThemes() {
  archiveFailure = undefined;
  return (archiveRequest ??= import('./archive-themes').then(
    ({ default: themes }) => {
      installed = themes;
      return themes;
    },
    (error: unknown) => {
      archiveRequest = undefined;
      // React replays a suspended render. Keep the failure visible until a retry.
      archiveFailure = isChunkLoadError(error)
        ? new RetryableChunkError(error, () => {
            archiveFailure = undefined;
          })
        : error;
      throw archiveFailure;
    },
  ));
}

const empty: BgBlitzTheme[] = [];
export function useArchiveThemes(required: boolean) {
  if (!required) return empty;
  if (archiveFailure !== undefined) throw archiveFailure;
  return installed ?? use(loadArchiveThemes());
}

export { default as builtInThemes } from '../../../../shared/builtin-board-themes.json' with { type: 'json' };
export type Theme = string;
export const pointColumns = [
  ...Array.from({ length: 12 }, (_, i) => ({
    point: 13 + i,
    column: i + 1 + (i >= 6 ? 1 : 0),
    bottom: false,
  })),
  ...Array.from({ length: 12 }, (_, i) => ({
    point: 12 - i,
    column: i + 1 + (i >= 6 ? 1 : 0),
    bottom: true,
  })),
];

export function stackGeometry(
  theme: BgBlitzTheme,
  image: ThemeImage,
  column: number,
  count: number,
  bottom: boolean,
) {
  // The artwork has a frame around the playing surface. XML supplies the
  // explicit column origins and vertical correction for each theme.
  const inset = theme.background.width * 0.028 + theme.checkerOffsetY;
  const { width, height } = image;
  return {
    x: theme.columns[column] + theme.checkerOffsetX,
    y: bottom ? theme.background.height - inset - height : inset,
    width,
    height,
    step: height,
    visible: Math.min(5, Math.abs(count)),
    total: Math.abs(count),
  };
}

// X22 uses 84-pixel pieces on points roughly 90 pixels apart on its 1490px board.
const nativeScale = 680 / 1490;
const nativeCheckerSize = 84 * nativeScale;
export const nativePointPitch = 90 * nativeScale;
const fieldWidth = 13 * nativePointPitch; // twelve points plus the bar
export const nativeField = {
  x: (680 - fieldWidth) / 2,
  y: 28,
  width: fieldWidth,
  height: 414,
};

export function nativePointX(index: number) {
  return (
    nativeField.x + (index + 0.5 + (index >= 6 ? 1 : 0)) * nativePointPitch
  );
}

export function pointStackGeometry(
  point: number,
  count: number,
  theme?: BgBlitzTheme,
) {
  const mapping = pointColumns.find((entry) => entry.point === point);
  const bottom = mapping?.bottom ?? (point === -1 || point === 25);
  const off = point === -1 || point === 26;
  const column = mapping?.column ?? (off ? 14 : 7);
  if (theme) {
    const image = theme.checkers[count < 0 ? 'black' : 'white'][0];
    return {
      ...stackGeometry(theme, image, column, count, bottom),
      column,
      bottom,
    };
  }
  const index = column - 1 - (column > 7 ? 1 : 0);
  const centerX = off ? 706 : column === 7 ? 340 : nativePointX(index);
  return {
    x: centerX - nativeCheckerSize / 2,
    y: bottom
      ? nativeField.y + nativeField.height - nativeCheckerSize
      : nativeField.y,
    width: nativeCheckerSize,
    height: nativeCheckerSize,
    step: nativeCheckerSize,
    visible: Math.min(5, Math.abs(count)),
    total: Math.abs(count),
    bottom,
    column,
  };
}

export function checkerCenter(
  board: number[],
  point: number,
  player: number,
  theme?: BgBlitzTheme,
) {
  const off = point === -1 || point === 26;
  const count = off
    ? 15 - checkerCounts(board)[player]
    : Math.abs(board[point]);
  const stack = pointStackGeometry(
    point,
    (player === 0 ? 1 : -1) * Math.max(1, count),
    theme,
  );
  return {
    x: stack.x + stack.width / 2,
    y:
      stack.y +
      stack.height / 2 +
      (stack.bottom ? -1 : 1) * (stack.visible - 1) * stack.step,
  };
}
