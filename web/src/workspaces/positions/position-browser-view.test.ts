import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { I18nProvider } from '../../shared/i18n/i18n';
import {
  PositionBrowserResults,
  PositionBrowserPreview,
} from './PositionBrowserResults';
import { placeholders as args } from '../../shared/i18n/i18n-core';
import { positionMessages } from './position-browser-messages';
import type { PositionBrowserHit } from './position-browser-api';
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const hit: PositionBrowserHit = {
  source_available: true,
  match_no: 7,
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
    source_available: true,
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
  decks: [],
  native_error: null,
};
function render(child: ReturnType<typeof createElement>) {
  return renderToStaticMarkup(createElement(I18nProvider, { children: child }));
}
test('compact results expose contextual selection and keep hidden fields in the row summary', () => {
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
  expect(html).not.toMatch(/<button\b[^>]*aria-label="Open analysis"[^>]*>/);
  expect(html).not.toContain('SECRET SOLUTION');
  expect(html).toContain('data-column="player"');
  expect(html).not.toContain('data-column="opponent"');
  expect(html).toContain('class="position-player">Ada');
  expect(html).toContain('Opponent: Grace');
  expect(html).toContain('data-sort="player"');
  expect(html).not.toContain('data-sort="opponent"');
  expect(html).toContain('data-sort="match_id"');
});

test('saved rows offer removal without deleting the original position', () => {
  const html = render(
    createElement(PositionBrowserResults, {
      items: [hit],
      selected: new Set<string>(),
      onPreview: () => {},
      onToggle: () => {},
      onOpen: () => {},
      onRemove: () => {},
      busy: false,
    }),
  );
  expect(html).toContain('Remove from collection');
  expect(html).not.toMatch(/<button\b[^>]*aria-label="Open analysis"[^>]*>/);
  expect(html).not.toContain('Delete position');
});

test('position results and preview list existing decks without exposing an answer', () => {
  const member = {
    ...hit,
    decks: [
      { id: 'deck-a', name: 'Contact & timing' },
      { id: 'deck-b', name: 'Race study' },
    ],
  };
  const html = render(
    createElement(PositionBrowserResults, {
      items: [member],
      selected: new Set<string>(),
      onPreview: () => {},
      onToggle: () => {},
      onOpen: () => {},
      busy: false,
    }),
  );
  expect(html).not.toContain('data-column="decks"');
  expect(html).toContain('Decks: Contact &amp; timing, Race study');
  const preview = render(
    createElement(PositionBrowserPreview, {
      hit: member,
      theme: 'forest',
      onOpen: () => {},
    }),
  );
  expect(preview).toContain('Contact &amp; timing, Race study');
  expect(preview).not.toContain('SECRET SOLUTION');
});

test('the preview explicitly distinguishes a position with no deck membership', () => {
  const html = render(
    createElement(PositionBrowserPreview, {
      hit: { ...hit, decks: [] },
      theme: 'forest',
      onOpen: () => {},
    }),
  );
  expect(html).toContain('Not in a deck yet');
});

test('a black-on-roll record names the actual player and opponent correctly', () => {
  const html = render(
    createElement(PositionBrowserResults, {
      items: [{ ...hit, position: { ...hit.position, player: 1 } }],
      selected: new Set<string>(),
      onPreview: () => {},
      onToggle: () => {},
      onOpen: () => {},
      busy: false,
    }),
  );
  expect(html).toContain('class="position-player">Grace');
  expect(html).toContain('Opponent: Ada');
  expect(html).toContain('Player on roll: Grace');
});

test('recorded moves are available only after opting in, without engine answers', () => {
  const html = render(
    createElement(PositionBrowserResults, {
      items: [hit],
      selected: new Set<string>(),
      onPreview: () => {},
      onToggle: () => {},
      onOpen: () => {},
      busy: false,
      showPlayed: true,
    }),
  );
  expect(html).toContain('SECRET SOLUTION');
  expect(html).toContain('Played move');
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
  expect(html).toMatch(/<button\b[^>]*aria-label="Open analysis"[^>]*>/);
  expect(html).not.toContain('SECRET SOLUTION');
});
test('native error is paired with its actual engine depth and job', () => {
  const html = render(
    createElement(PositionBrowserResults, {
      items: [
        {
          ...hit,
          native_error: {
            analysis_id: 'job:native-job',
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
test('both position catalogs preserve keys and arguments', () => {
  const source = positionMessages.en;
  for (const catalog of Object.values(positionMessages)) {
    expect(Object.keys(catalog)).toEqual(Object.keys(source));
    for (const key of Object.keys(source) as (keyof typeof source)[]) {
      expect(catalog[key]).not.toBe('');
      expect(args(catalog[key])).toEqual(args(source[key]));
    }
  }
});

test('retained preview explains that the source is gone while individual analysis remains available', () => {
  const html = render(
    createElement(PositionBrowserPreview, {
      hit: {
        ...hit,
        source_available: false,
        match_info: { ...hit.match_info, source_available: false },
      },
      theme: 'forest',
      onOpen: () => {},
    }),
  );
  expect(html).toContain('Original match removed; saved position retained');
  expect(html).toMatch(/<button\b[^>]*aria-label="Open analysis"[^>]*>/);
  expect(html).toContain('class="board-shell ');
});

test('imported error provenance retains its original depth and never claims a native job', () => {
  const html = render(
    createElement(PositionBrowserResults, {
      items: [
        {
          ...hit,
          native_error: {
            analysis_id: 'imported:match',
            imported_analysis_id: 'record',
            engine: 'XG',
            plies: null,
            depth: 'XGRoller++',
            job_id: null,
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
  expect(html).toContain('Imported · XG · XGRoller++');
  expect(html).not.toContain('null ply');
  expect(html).not.toContain('job null');
});
