import { readFileSync } from 'node:fs';
import builtInThemes from '../../../shared/builtin-board-themes.json' with { type: 'json' };
import type { BgBlitzTheme } from '../../src/shared/board/themes';

export const archiveBoardThemes: BgBlitzTheme[] = JSON.parse(
  readFileSync(
    new URL('../../src/generated-themes.json', import.meta.url),
    'utf8',
  ),
);
export const installedBoardThemes = [...builtInThemes, ...archiveBoardThemes];
