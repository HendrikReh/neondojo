import { use } from 'react';

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
let installed: BgBlitzTheme[] | undefined;

export function loadArchiveThemes() {
  return (archiveRequest ??= import('./archive-themes').then(
    ({ default: themes }) => {
      installed = themes;
      return themes;
    },
  ));
}

const empty: BgBlitzTheme[] = [];
export function useArchiveThemes(required: boolean) {
  if (!required) return empty;
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
  const step = Math.min(
    image.height,
    (theme.background.height / 2 - inset - image.height) / 4,
  );
  return {
    x: theme.columns[column] + theme.checkerOffsetX,
    y: bottom ? theme.background.height - inset - image.height : inset,
    step,
    visible: Math.min(5, Math.abs(count)),
    total: Math.abs(count),
  };
}
