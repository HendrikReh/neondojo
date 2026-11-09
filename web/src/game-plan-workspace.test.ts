import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { beforeEach, expect, it, vi } from 'vitest';
import type { Position } from './api';

const hooks = vi.hoisted(() => ({ slots: [] as unknown[], index: 0 }));
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useState(initial: unknown) {
    const index = hooks.index++;
    if (!(index in hooks.slots))
      hooks.slots[index] = typeof initial === 'function' ? initial() : initial;
    return [
      hooks.slots[index],
      (next: unknown) => {
        hooks.slots[index] =
          typeof next === 'function' ? next(hooks.slots[index]) : next;
      },
    ];
  },
  useRef(initial: unknown) {
    return (hooks.slots[hooks.index++] ??= { current: initial });
  },
  useMemo(factory: () => unknown) {
    return factory();
  },
  useCallback(callback: unknown) {
    return callback;
  },
  useEffect() {},
}));
vi.mock('./i18n', () => ({
  useI18n: () => ({
    locale: 'en',
    t: String,
    formatNumber: String,
    formatDecision: String,
    formatError: String,
  }),
}));
import AnalysisWorkspace from './AnalysisWorkspace';
import Board from './Board';
import BoardReviewControls from './BoardReviewControls';
import AnalysisPanel from './AnalysisPanel';
import GamePlanPanel from './GamePlanPanel';
import { displayedBoardPosition } from './displayed-board';
import { projectMove, type BoardView } from './move-preview';

type Element = ReactElement<Record<string, unknown>>;
function elements(node: ReactNode): Element[] {
  if (Array.isArray(node)) return node.flatMap(elements);
  if (!isValidElement<Record<string, unknown>>(node)) return [];
  return [node, ...elements(node.props.children as ReactNode)];
}
function props(player: 0 | 1 = 0) {
  const board = Array<number>(26).fill(0);
  board[player === 0 ? 13 : 12] = player === 0 ? 1 : -1;
  board[player === 0 ? 1 : 24] = player === 0 ? -1 : 1;
  const position: Position = {
    id: 'snapshot',
    match_uuid: 'match',
    ordinal: 1,
    game_no: 1,
    ply: 1,
    player,
    board,
    dice: [6, 1],
    notation: '13/6',
    position_id: 'board-id',
    gnubg_match_id: '',
  };
  const noop = () => {};
  const game = {
    game_no: 1,
    position_count: 1,
    white_score: 0,
    black_score: 0,
    crawford: false,
  };
  return {
    navigation: {
      match: {
        id: 'match',
        source_available: false,
        white: 'Alice',
        black: 'Bob',
        filename: 'removed.sgf',
        format: 'sgf',
        match_length: 7,
        position_count: 1,
        created_at: '',
      },
      game,
      games: [game],
      positions: [position],
      position,
      index: 0,
      loading: false,
      error: null,
      stats: null,
      selectIndex: noop,
      previous: noop,
      next: noop,
      cancel: noop,
      clearError: noop,
      clear: noop,
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
    configured: true,
    detail: 'overview',
    onDetail: noop,
    onBookSettings: noop,
    onOpenDatabase: noop,
    onTrackJob: noop,
    onNotesSaved: noop,
    onMoveNotes: noop,
    notesSuspended: false,
    deckRevision: 0,
    onDeckChanged: noop,
    onPosition: noop,
    welcome: null,
    bookmarkRevision: 0,
    onBookmarkChanged: noop,
  } satisfies Parameters<typeof AnalysisWorkspace>[0];
}
function render(input: Parameters<typeof AnalysisWorkspace>[0]) {
  hooks.index = 0;
  return elements(AnalysisWorkspace(input));
}
function panel(tree: Element[]) {
  const element = tree.find((element) => element.type === GamePlanPanel);
  expect(element, 'game plan panel beside the displayed board').toBeDefined();
  return element!;
}
beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
});

it.each([0, 1] as const)(
  'classifies the exact board in every view for mover %s, including retained snapshots',
  (player) => {
    const input = props(player);
    let tree = render(input);
    const controls = tree.find(
      (element) => element.type === BoardReviewControls,
    )!;
    const onView = controls.props.onView as (view: BoardView) => void;
    for (const view of ['before', 'arrows', 'after', 'before'] as const) {
      onView(view);
      tree = render(input);
      const board = tree.find((element) => element.type === Board)!;
      const displayed = displayedBoardPosition(
        board.props.position as Position,
        board.props.preview as ReturnType<typeof projectMove>,
        board.props.previewMode as BoardView,
      );
      expect(panel(tree).props.board).toBe(displayed.board);
      expect(panel(tree).props).toMatchObject({
        white: 'Alice',
        black: 'Bob',
        active: true,
      });
      if (view === 'after')
        expect(panel(tree).props.board).not.toBe(
          input.navigation.position.board,
        );
      else
        expect(panel(tree).props.board).toBe(input.navigation.position.board);
    }
  },
);

it('classifies successful selected candidates and falls back with the board on invalid notation', () => {
  const input = props();
  let tree = render(input);
  const analysis = tree.find((element) => element.type === AnalysisPanel)!;
  (analysis.props.onPreview as (candidate: unknown) => void)({
    rank: 2,
    notation: '13/7 7/6',
    source: 'GNUbg',
  });
  tree = render(input);
  const controls = tree.find(
    (element) => element.type === BoardReviewControls,
  )!;
  (controls.props.onView as (view: BoardView) => void)('after');
  tree = render(input);
  expect((panel(tree).props.board as number[])[6]).toBe(1);
  input.navigation.position = {
    ...input.navigation.position,
    notation: 'unsupported',
    id: 'new-occurrence',
  };
  tree = render(input);
  expect(panel(tree).props.board).toBe(input.navigation.position.board);
  expect(
    tree.find((element) => element.type === Board)?.props.previewMode,
  ).toBe('before');
});

it('passes workspace visibility without depending on engine configuration or analysis availability', () => {
  const input = { ...props(), active: false, configured: false };
  expect(panel(render(input)).props.active).toBe(false);
  expect(panel(render({ ...input, active: true })).props.active).toBe(true);
});
