import { fulfillJson } from './api-fixtures';
import { eitherOrder } from './play-turn-fixtures';
import { type Page, type Route } from '@playwright/test';
import { applySteps } from '../src/shared/board/play-state';
import { nativeSteps } from '../src/shared/board/play-sound-events';
import type {
  PlaySession,
  PlayReviewHistory,
} from '../src/workspaces/play/play-api';
export const historySessionId = '7c390ba7-76a1-44f0-a041-ae982f6d1363';
const id = historySessionId;
const initial = [
  0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2,
  0,
];
export async function historyFixture(
  page: Page,
  human = 0,
  reply?: (route: Route, data: PlayReviewHistory) => Promise<unknown>,
) {
  const history = [
    { player: human, dice: [3, 1], notation: '8/5 6/5' },
    { player: 1 - human, dice: [4, 2], notation: '8/4 6/4' },
  ];
  let board = [...initial];
  const items = history.map((turn, index) => {
    const before = {
      id,
      match_uuid: id,
      ordinal: index,
      game_no: 1,
      ply: index + 1,
      player: turn.player,
      dice: turn.dice,
      board: [...board],
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cInlAAAAAAAE',
      notation: turn.notation,
    };
    board = applySteps(
      board,
      nativeSteps(turn.notation, turn.player),
      turn.player,
    );
    return {
      index,
      game_no: 1,
      move_no: index + 1,
      kind: 'checker',
      before,
      after: { ...before, board: [...board], dice: [] },
    };
  });
  const saved: PlaySession = {
    id,
    version: 7,
    human_player: human,
    human_name: 'Human',
    match_length: 7,
    bearoff_direction: human ? 'left' : 'right',
    engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 3 },
    position: {
      ...items[1].after,
      player: human,
      dice: [3, 1],
      board: [...board],
      notation: '',
    },
    phase: 'move',
    can_double: false,
    legal_turns: eitherOrder([3, 1], '13/10 6/5', human),
    legal_moves: [
      { notation: '13/10 6/5', steps: nativeSteps('13/10 6/5', human) },
    ],
    history,
    scores: [0, 0],
    game_no: 1,
    winner: null,
    opening: null,
  };
  const writes: unknown[] = [];
  let reads = 0;
  await page.addInitScript(
    ({ id, human }) => {
      localStorage.setItem('neondojo.locale', 'en');
      localStorage.setItem('neondojo.play-session', id);
      if (human) localStorage.setItem('neondojo.theme', 'bgblitz:bg-earth');
    },
    { id, human },
  );
  await page.route(`**/api/play/sessions/${id}`, (r) =>
    fulfillJson<'PlaySession'>(r, saved),
  );
  await page.route(`**/api/play/sessions/${id}/history?*`, (r) => {
    reads++;
    const data = { session_id: id, version: 7, items };
    return reply
      ? reply(r, data)
      : fulfillJson<'PlayHistorySnapshots'>(r, data);
  });
  await page.route(`**/api/play/sessions/${id}/actions`, (r) => {
    writes.push(r.request().postDataJSON());
    return fulfillJson<'PlaySession'>(r, saved);
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  return {
    saved,
    items,
    writes,
    readCount: () => reads,
    point: (n: number) =>
      page.locator(`[data-play-point="${human ? 25 - n : n}"] .play-point`),
    rows: page.locator('.play-history .move-history-row'),
  };
}
