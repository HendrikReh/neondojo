import { fulfillJson } from './api-fixtures';
import { gotoWorkspace, seedStorage } from './navigation';
import {
  initialBoard,
  playPositionFixture,
  playSessionFixture,
} from './play-session-fixture';
import { eitherOrder } from './play-turn-fixtures';
import { type Page, type Route } from '@playwright/test';
import { applySteps } from '../../src/shared/board/play-state';
import { nativeSteps } from '../../src/shared/board/play-sound-events';
import type { PlayReviewHistory } from '../../src/workspaces/play/play-api';

export const historySessionId = '7c390ba7-76a1-44f0-a041-ae982f6d1363';
const id = historySessionId;

export async function historyFixture(
  page: Page,
  human = 0,
  reply?: (route: Route, data: PlayReviewHistory) => Promise<unknown>,
) {
  const history = [
    { player: human, dice: [3, 1], notation: '8/5 6/5' },
    { player: 1 - human, dice: [4, 2], notation: '8/4 6/4' },
  ];
  let board = initialBoard();
  const items = history.map((turn, index) => {
    const before = playPositionFixture(
      {
        ordinal: index,
        ply: index + 1,
        player: turn.player,
        dice: turn.dice,
        board: [...board],
        notation: turn.notation,
      },
      id,
    );
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
  const saved = playSessionFixture({
    id,
    version: 7,
    human_player: human,
    human_name: 'Human',
    bearoff_direction: human ? 'left' : 'right',
    position: {
      ...items[1].after,
      player: human,
      dice: [3, 1],
      board: [...board],
      notation: '',
    },
    legal_turns: eitherOrder([3, 1], '13/10 6/5', human),
    legal_moves: [
      { notation: '13/10 6/5', steps: nativeSteps('13/10 6/5', human) },
    ],
    history,
  });
  const writes: unknown[] = [];
  let reads = 0;
  await seedStorage(page, {
    locale: 'en',
    playSessionId: id,
    ...(human ? { theme: 'bgblitz:bg-earth' } : {}),
  });
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
  await gotoWorkspace(page, 'play');
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
