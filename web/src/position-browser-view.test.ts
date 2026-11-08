import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { I18nProvider } from './i18n';
import {
  PositionBrowserResults,
  PositionBrowserPreview,
} from './PositionBrowserResults';
import { positionMessages } from './position-browser-messages';
import type { PositionBrowserHit } from './position-browser-api';
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const hit: PositionBrowserHit = {
  position: {
    id: 'one',
    match_uuid: 'match',
    ordinal: 1,
    game_no: 2,
    ply: 3,
    player: 0,
    dice: [6, 1],
    board: Array(26).fill(0),
    position_id: 'board',
    gnubg_match_id: '',
    notation: 'SECRET SOLUTION',
  },
  match_info: {
    id: 'match',
    white: 'Ada',
    black: 'Grace',
    filename: 'match.mat',
    format: 'mat',
    match_length: 7,
    position_count: 5,
    created_at: '',
  },
  database_id: 'db',
  database_name: 'Independent database',
  display_name: 'Match title',
  tournament: null,
  round: null,
  white_score: 0,
  black_score: 2,
  crawford: false,
  cube_value: 1,
  cube_owner: null,
  tags: ['prime'],
  notes: [
    { scope: 'position', tags: ['prime'], comment_excerpt: 'Relevant comment' },
  ],
  card_ready: false,
  native_error: null,
};
function render(child: ReturnType<typeof createElement>) {
  return renderToStaticMarkup(createElement(I18nProvider, { children: child }));
}
test('results expose contextual selection, real origin and actionable missing analysis', () => {
  const html = render(
    createElement(PositionBrowserResults, {
      items: [hit],
      selected: new Set<string>(),
      onPreview: () => {},
      onToggle: () => {},
      onOpen: () => {},
      busy: false,
    }),
  );
  expect(html).toContain('Select game 2, ply 3, Ada');
  expect(html).toContain('Select this page');
  expect(html).toContain('Independent database');
  expect(html).toContain('Relevant comment');
  expect(html).toContain('Open analysis');
  expect(html).not.toContain('SECRET SOLUTION');
});
test('one preview shows only the stored board before the move and no invented answer', () => {
  const html = render(
    createElement(PositionBrowserPreview, {
      hit,
      theme: 'forest',
      onOpen: () => {},
    }),
  );
  expect(html.match(/class="board-shell /g)).toHaveLength(1);
  expect(html).toContain('data-preview-mode="before"');
  expect(html).toContain('Stored position before the move');
  expect(html).toContain('Local analysis is required');
  expect(html).not.toContain('SECRET SOLUTION');
});
test('native error is paired with its actual engine depth and job', () => {
  const html = render(
    createElement(PositionBrowserResults, {
      items: [
        {
          ...hit,
          native_error: {
            engine: 'gnubg',
            plies: 2,
            job_id: 'native-job',
            equity_loss: 0.04,
            severity: 'bad',
          },
        },
      ],
      selected: new Set<string>(),
      onPreview: () => {},
      onToggle: () => {},
      onOpen: () => {},
      busy: false,
    }),
  );
  expect(html).toContain('gnubg · 2 ply · job native-job');
  expect(html).toContain('0.040');
});
test('all six position catalogs preserve keys and arguments', () => {
  const source = positionMessages.en;
  for (const catalog of Object.values(positionMessages)) {
    expect(Object.keys(catalog)).toEqual(Object.keys(source));
    for (const key of Object.keys(source) as (keyof typeof source)[]) {
      expect(catalog[key]).not.toBe('');
      const args = (value: string) =>
        [...value.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
      expect(args(catalog[key])).toEqual(args(source[key]));
    }
  }
});
