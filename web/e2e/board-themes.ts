import { readFileSync } from 'node:fs';
import builtInThemes from '../../shared/builtin-board-themes.json' with { type: 'json' };

const archives: { id: string; name: string }[] = JSON.parse(
  readFileSync(
    new URL('../src/generated-themes.json', import.meta.url),
    'utf8',
  ),
);
export const installedBoardThemes = [...builtInThemes, ...archives];
