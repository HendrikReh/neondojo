import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import AnalysisWorkspace from './AnalysisWorkspace';
import { I18nProvider } from './i18n';
import type { Match, Position } from './api';
import { AnalysisMatchHeaderView } from './AnalysisMatchHeader';
import type { LibraryMatch } from './library-api';
import type { Locale } from './i18n-core';

const match: Match & { source_available: boolean } = {
  id: '00000000-0000-0000-0000-000000000007',
  white: 'Alice',
  black: 'Bob',
  match_length: 21,
  filename: 'not-a-played-date-2026-10-10.xg',
  format: 'xg',
  position_count: 0,
  created_at: '2026-11-09T12:00:00Z',
  source_available: true,
};
const metadata: LibraryMatch = {
  ...match,
  match_no: 7,
  database_id: '00000000-0000-0000-0000-000000000001',
  display_name: 'Final training',
  game_count: 15,
  played_on: '2026-11-08',
  tournament: 'Autumn Open',
  round: null,
  place: null,
  result_state: 'completed',
  white_score: 21,
  black_score: 17,
  white_pr: 0,
  black_pr: 4.75,
  pr_method: 'gnubg_emg_500_v1',
  pr_profile: 'GNUbg; 2-ply cubeful deterministic',
  analysis_status: 'completed',
  tags: [],
  comment: '',
  version: 0,
};

function renderHeader(
  data: LibraryMatch | null = metadata,
  locale: Locale = 'en',
  error?: unknown,
) {
  vi.stubGlobal('localStorage', { getItem: () => locale });
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(AnalysisMatchHeaderView, {
        match,
        metadata: data ?? undefined,
        loading: data === null && error === undefined,
        error,
        onRetry: () => {},
      }),
    }),
  );
}
function field(html: string, key: string) {
  return (
    html.match(
      new RegExp(`<div[^>]*data-match-field="${key}"[\\s\\S]*?<\\/div>`),
    )?.[0] ?? ''
  );
}

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());

it('integrates the match header with identity and loading fields while omitting source controls', () => {
  const noop = () => {};
  const props = {
    navigation: {
      match,
      games: [],
      positions: [],
      index: 0,
      stats: null,
      game: undefined,
      position: undefined as unknown as Position,
      loading: false,
      error: null,
      selectIndex: noop,
      previous: noop,
      next: noop,
      cancel: noop,
      clearError: noop,
      clear: noop,
      invalidateMatchAnalysis: noop,
      intent: () => 0,
      selectMatch: async () => false,
      selectGame: async () => false,
      openPosition: async () => false,
      openMatch: async () => false,
      resume: async () => {},
    },
    jobs: { jobs: {}, error: null, submitting: false, run: async () => {} },
    theme: 'forest',
    defaultPlies: 2,
    sourceRevision: 0,
    bookmarkRevision: 0,
    configured: true,
    detail: 'overview',
    onDetail: noop,
    onBookmarkChanged: noop,
    onOpenDatabase: noop,
    onFindSimilar: noop,
    onTrackJob: noop,
    onNotesSaved: noop,
    onMoveNotes: noop,
    notesSuspended: false,
    deckRevision: 0,
    onDeckChanged: noop,
    onPosition: noop,
    welcome: null,
  } satisfies Parameters<typeof AnalysisWorkspace>[0];
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(AnalysisWorkspace, props),
    }),
  );
  const header = html.slice(
    html.indexOf('class="analysis-heading'),
    html.indexOf('class="analysis-commandbar'),
  );
  expect(header).toContain(`data-match-id="${match.id}"`);
  expect(header).toContain('<h1');
  expect(header).toContain('Alice');
  expect(header).toContain('Bob');
  for (const field of [
    'id',
    'length',
    'result',
    'date',
    'event',
    'pr-white',
    'pr-black',
  ])
    expect(header).toContain(`data-match-field="${field}"`);
  expect(header).toContain('aria-busy="true"');
  expect(header).not.toContain('SGF');
  expect(header).not.toContain('/source');
  expect(header).not.toContain('XG');
  expect(header).not.toContain('2026');
});

it('shows numeric ID, match score, played date, event and zero PR in White/Black order', () => {
  const html = renderHeader();
  expect(field(html, 'id')).toContain('>7</dd>');
  expect(field(html, 'length')).toContain('>21</dd>');
  expect(field(html, 'result')).toContain('21–17');
  expect(field(html, 'result')).toContain('Final score');
  expect(field(html, 'date')).toContain('Nov 8, 2026');
  expect(field(html, 'event')).toContain('Autumn Open');
  expect(field(html, 'pr-white')).toContain('0.00');
  expect(field(html, 'pr-white')).toContain('Alice');
  expect(field(html, 'pr-black')).toContain('4.75');
  expect(field(html, 'pr-black')).toContain('Bob');
  expect(html.indexOf('data-match-field="pr-white"')).toBeLessThan(
    html.indexOf('data-match-field="pr-black"'),
  );
  expect(html).not.toContain('SGF');
  expect(html).not.toContain('/source');
  expect(html).not.toContain(match.filename);
  expect(html).not.toContain('Nov 9');
});

it('shows n/a for unknown metadata and partial PR without inventing results', () => {
  const html = renderHeader({
    ...metadata,
    white_score: null,
    black_score: null,
    played_on: null,
    tournament: null,
    white_pr: null,
    result_state: 'unknown',
  });
  for (const key of ['result', 'date', 'event', 'pr-white'])
    expect(field(html, key)).toContain('n/a');
  expect(field(html, 'pr-black')).toContain('4.75');
  expect(html).not.toContain('Final score');
  expect(html).not.toContain('2026');
});

it.each(['invalid', '2026-02-30', '2026-13-01', '2026-11-08T00:00:00Z'])(
  'treats invalid played date %s as unavailable',
  (played_on) =>
    expect(field(renderHeader({ ...metadata, played_on }), 'date')).toContain(
      'n/a',
    ),
);

it('keeps an ongoing match score visibly distinct from its final result', () => {
  const html = renderHeader({
    ...metadata,
    white_score: 14,
    black_score: 0,
    result_state: 'ongoing',
  });
  expect(field(html, 'result')).toContain('14–0');
  expect(field(html, 'result')).toContain('Ongoing');
  expect(field(html, 'result')).not.toContain('Final score');
});

it('uses the existing localized Money game label', () => {
  expect(
    field(renderHeader({ ...metadata, match_length: 0 }), 'length'),
  ).toContain('Money game');
  expect(
    field(renderHeader({ ...metadata, match_length: 0 }, 'de'), 'length'),
  ).toContain('Money Game');
});

it('does not present full-match PR for an independently retained position', () => {
  const retained: LibraryMatch & { source_available: false } = {
    ...metadata,
    source_available: false,
  };
  const html = renderHeader(retained);
  expect(field(html, 'pr-white')).toContain('n/a');
  expect(field(html, 'pr-black')).toContain('n/a');
  expect(field(html, 'pr-white')).not.toContain('0.00');
  expect(field(html, 'pr-black')).not.toContain('4.75');
});

it.each([
  ['en', 'Length', 'Played', 'Event', 'PR White', 'PR Black', '4.75'],
  ['de', 'Länge', 'Gespielt', 'Veranstaltung', 'PR Weiß', 'PR Schwarz', '4,75'],
  ['es', 'Longitud', 'Jugado', 'Evento', 'PR Blancas', 'PR Negras', '4,75'],
  ['fr', 'Longueur', 'Joué', 'Événement', 'PR Blancs', 'PR Noirs', '4,75'],
  ['pl', 'Długość', 'Rozegrano', 'Wydarzenie', 'PR Białe', 'PR Czarne', '4,75'],
  ['it', 'Lunghezza', 'Giocato', 'Evento', 'PR Bianchi', 'PR Neri', '4,75'],
] as const)(
  'localizes %s metadata labels and PR',
  (locale, length, played, event, white, black, pr) => {
    const html = renderHeader(metadata, locale);
    expect(field(html, 'length')).toContain(length);
    expect(field(html, 'date')).toContain(played);
    expect(field(html, 'event')).toContain(event);
    expect(field(html, 'pr-white')).toContain(white);
    expect(field(html, 'pr-black')).toContain(black);
    expect(field(html, 'pr-black')).toContain(pr);
  },
);

it('preserves complete long names and events in accessible text and titles', () => {
  const white = 'White player '.repeat(40);
  const tournament = 'Long event '.repeat(50);
  const html = renderHeader({ ...metadata, white, tournament });
  expect(html).toContain(`<h1 title="${white} vs. Bob">`);
  expect(field(html, 'event')).toContain(`title="${tournament}"`);
  expect(field(html, 'event')).toContain(`>${tournament.trim()}</dd>`);
});

it('distinguishes a metadata request error from unavailable stored values and offers retry', () => {
  const html = renderHeader(null, 'en', new Error('Offline'));
  expect(html).toContain('role="alert"');
  expect(html).toContain('Could not load match details.');
  expect(html).toMatch(/<button[^>]*aria-label="Retry"/);
  expect(field(html, 'result')).toContain('—');
  expect(field(html, 'pr-white')).not.toContain('n/a');
  expect(html).not.toContain('aria-busy="true"');
});
