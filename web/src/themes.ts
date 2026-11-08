import catalog from './generated-themes.json';

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

// Generated from validated, local ZIPs by tools/prepare_themes.py.
export const archiveThemes = catalog as BgBlitzTheme[];
export const themes = [
  { id: 'forest', name: 'Forest' },
  { id: 'walnut', name: 'Walnut' },
  { id: 'slate', name: 'Slate' },
  ...archiveThemes.map(({ id, name }) => ({ id, name })),
];
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
