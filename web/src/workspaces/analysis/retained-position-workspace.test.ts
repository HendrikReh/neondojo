import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Match, Position } from '../../shared/api/api';
import type { Locale } from '../../shared/i18n/i18n-core';

vi.mock('../../shared/board/Board', () => ({
  default: () => createElement('div', { 'data-retained-board': true }),
}));
vi.mock('./AnalysisPanel', () => ({
  default: vi.fn(() =>
    createElement('div', { 'data-retained-evaluation': true }),
  ),
}));
vi.mock('./notes/NotesPanel', () => ({
  default: () => createElement('div', { 'data-retained-notes': true }),
}));
vi.mock('../../PositionBookmarkButton', () => ({
  default: () => createElement('div', { 'data-retained-bookmark': true }),
}));
vi.mock('../study/flashcards/LearningPanel', () => ({
  AddCardsPanel: vi.fn(() =>
    createElement('div', { 'data-retained-cards': true }),
  ),
}));
vi.mock('./match/MatchAnalysisPanel', () => ({
  default: () => createElement('div', { 'data-full-match-analysis': true }),
}));
vi.mock('../../shared/review/useMoveAssessments', () => ({
  useMoveAssessments: vi.fn(() => ({
    assessments: {},
    loading: false,
    error: null,
  })),
}));
import AnalysisWorkspace from './AnalysisWorkspace';
import AnalysisPanel from './AnalysisPanel';
import { AddCardsPanel } from '../study/flashcards/LearningPanel';
import { I18nProvider } from '../../shared/i18n/i18n';
import { useMoveAssessments } from '../../shared/review/useMoveAssessments';

const match: Match & { source_available: boolean } = {
  id: 'collection-host',
  white: 'Alice',
  black: 'Bob',
  match_length: 21,
  filename: 'original.xg',
  format: 'xg',
  position_count: 1,
  created_at: '2026-11-09T12:00:00Z',
  source_available: false,
};
const position: Position = {
  id: 'retained-position',
  match_uuid: match.id,
  game_no: 4,
  ordinal: 3,
  ply: 3,
  player: 0,
  dice: [3, 2],
  board: Array(26).fill(0),
  notation: '13/10 13/11',
  position_id: '',
  gnubg_match_id: '',
};
function props(
  detail: Parameters<typeof AnalysisWorkspace>[0]['detail'] = 'overview',
  sourceAvailable = false,
) {
  const noop = () => {};
  const game = {
    game_no: 4,
    white_score: 5,
    black_score: 3,
    crawford: false,
    position_count: 1,
  };
  const selectedMatch = { ...match, source_available: sourceAvailable };
  return {
    navigation: {
      match: selectedMatch,
      game,
      games: [game],
      positions: [position],
      position,
      index: 0,
      gameLoaded: true,
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
    jobs: {
      jobs: {},
      refresh: () => {},
      removeDice: async () => {},
      ...{ completedAnalysis: undefined },
      error: null,
      submitting: false,
      restoring: false,
      failure: undefined,
      run: async () => {},
    },
    theme: 'forest',
    defaultPlies: 2,
    openingBookRevision: 0,
    bookmarkRevision: 0,
    configured: true,
    detail,
    onDetail: noop,
    onBookmarkChanged: noop,
    onOpenDatabase: noop,
    onTrackJob: noop,
    onNotesSaved: noop,
    onMoveNotes: noop,
    notesSuspended: false,
    deckRevision: 0,
    onDeckChanged: noop,
    onPosition: noop,
    welcome: null,
  } satisfies Parameters<typeof AnalysisWorkspace>[0];
}
function render(
  detail?: Parameters<typeof AnalysisWorkspace>[0]['detail'],
  sourceAvailable = false,
  locale: Locale = 'en',
) {
  vi.stubGlobal('localStorage', { getItem: () => locale });
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(
        AnalysisWorkspace,
        props(detail, sourceAvailable),
      ),
    }),
  );
}
beforeEach(() => vi.clearAllMocks());
afterEach(() => vi.unstubAllGlobals());

it('keeps individual review and saved-game navigation while unmounting match analysis', () => {
  const html = render();
  expect(html).toContain('Original match removed; saved position retained');
  for (const feature of ['board', 'evaluation', 'notes', 'cards', 'bookmark'])
    expect(html).toContain(`data-retained-${feature}="true"`);
  expect(html).toContain('class="game-selector"');
  expect(html).not.toContain('data-full-match-analysis');
  expect(html).not.toContain('class="workspace-analysis-command"');
  expect(html).not.toContain('class="next-error"');
  expect(html).not.toContain('id="inspector-tab-evaluation"');
  expect(html).not.toContain('id="inspector-tab-statistics"');
  expect(html).not.toContain('id="inspector-tab-errors"');
  expect(useMoveAssessments).toHaveBeenCalledWith(
    expect.objectContaining({ matchId: undefined }),
  );
});

it.each(['statistics', 'errors'] as const)(
  'falls back from the unavailable %s detail to a usable overview',
  (detail) => {
    const html = render(detail);
    expect(html).toMatch(
      /id="inspector-tab-overview"[^>]*aria-selected="true"/,
    );
    expect(html).toContain('analysis-overview');
    expect(html).not.toContain('data-full-match-analysis');
  },
);

it.each(['notes', 'cards'] as const)(
  'preserves the selected %s view',
  (detail) => {
    const html = render(detail);
    expect(html).toMatch(
      new RegExp(`id="inspector-tab-${detail}"[^>]*aria-selected="true"`),
    );
  },
);

it('keeps full-match controls and waits for a catalog selection before requesting assessments', () => {
  const html = render('statistics', true);
  expect(html).toContain('data-full-match-analysis="true"');
  expect(html).toContain('id="inspector-tab-statistics"');
  expect(html).toContain('id="inspector-tab-errors"');
  expect(html).toContain('class="workspace-analysis-command"');
  expect(html).not.toContain('Original match removed; saved position retained');
  expect(useMoveAssessments).toHaveBeenCalledWith(
    expect.objectContaining({ matchId: undefined, analysisId: undefined }),
  );
});

it('withholds the observed-roll source for a retained position', () => {
  render();
  expect(
    vi.mocked(AnalysisPanel).mock.calls[0][0].observedMatchId,
  ).toBeUndefined();
});

it('limits card creation to the current retained position', () => {
  render();
  expect(vi.mocked(AddCardsPanel).mock.calls[0][0]).toMatchObject({
    positionId: position.id,
    allowBatch: false,
  });
});

it('preserves card batch creation for an available source', () => {
  render('overview', true);
  expect(vi.mocked(AddCardsPanel).mock.calls[0][0]).toMatchObject({
    allowBatch: true,
  });
});

it('provides the loaded game as the observed-roll source', () => {
  render('overview', true);
  expect(vi.mocked(AnalysisPanel).mock.calls[0][0]).toMatchObject({
    observedMatchId: match.id,
    gameNo: position.game_no,
  });
});

it.each([
  ['en', 'Original match removed; saved position retained'],
  ['de', 'Originalmatch gelöscht; gemerkte Position bleibt erhalten'],
  ['es', 'Match original eliminado; posición guardada conservada'],
  ['fr', 'Match original supprimé ; position enregistrée conservée'],
  ['pl', 'Oryginalny mecz usunięty; zapisana pozycja zachowana'],
  ['it', 'Match originale eliminato; posizione salvata conservata'],
] as const)('explains retained context in %s', (locale, text) => {
  expect(render('overview', false, locale)).toContain(text);
});
