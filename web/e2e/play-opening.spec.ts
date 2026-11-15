import { rollDice } from './play-board-actions';
import { test, expect } from './test';
import type { PlaySession } from '../src/play-api';
const id = '200389f1-9b5b-4b38-9869-3fdc922030b1';
const board = [
  0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2,
  0,
];
function pending(): PlaySession {
  return {
    id,
    version: 0,
    human_player: 0,
    human_name: 'Player',
    match_length: 7,
    bearoff_direction: 'right',
    engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 2 },
    position: {
      id,
      match_uuid: id,
      ordinal: 0,
      game_no: 1,
      ply: 0,
      player: 0,
      dice: [],
      board,
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cInlAAAAAAAE',
      notation: '',
    },
    phase: 'opening_roll',
    can_double: false,
    opening: null,
    legal_moves: [],
    history: [],
    scores: [0, 0],
    game_no: 1,
    winner: null,
  };
}
for (const tie of [false, true])
  test(`automatic opening animation with ${tie ? 'ties and bot' : 'human'} first`, async ({
    page,
  }) => {
    let state = pending();
    const writes: unknown[] = [];
    await page.route('**/api/play/sessions', (route) =>
      route.fulfill({
        json: route.request().method() === 'POST' ? state : [state],
      }),
    );
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: state }),
    );
    await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
      const action = route.request().postDataJSON();
      writes.push(action);
      if (action.action === 'opening_roll')
        state = {
          ...state,
          version: 1,
          phase: 'opening_result',
          opening: tie
            ? {
                white_die: 1,
                black_die: 5,
                winner: 1,
                rolls: [
                  [4, 4],
                  [2, 2],
                  [1, 5],
                ],
              }
            : { white_die: 6, black_die: 2, winner: 0, rolls: [[6, 2]] },
          position: {
            ...state.position,
            player: tie ? 1 : 0,
            dice: tie ? [5, 1] : [6, 2],
          },
        };
      else if (action.action === 'start_opening_move')
        state = {
          ...state,
          version: 2,
          phase: tie ? 'roll' : 'move',
          position: { ...state.position, player: 0, dice: tie ? [] : [6, 2] },
          history: tie
            ? [{ player: 1, dice: [5, 1], notation: '13/8 24/23' }]
            : [],
          legal_moves: tie
            ? []
            : [
                {
                  notation: '24/18 13/11',
                  steps: [
                    { from: 24, to: 18 },
                    { from: 13, to: 11 },
                  ],
                },
              ],
        };
      else
        throw new Error(
          'Unexpected action before the first checker play: ' +
            JSON.stringify(action),
        );
      await route.fulfill({ json: state });
    });
    await page.addInitScript(() => {
      document.addEventListener('DOMContentLoaded', () => {
        const seen = document.createElement('meta');
        seen.name = 'opening-animation-frames';
        document.head.append(seen);
        const frames: string[] = [];
        new MutationObserver(() => {
          const board = document.querySelector('.play-board');
          if (!board) return;
          const kind = board.getAttribute('data-play-animation');
          const values = [
            ...board.querySelectorAll(
              '[data-opening-die]:has([data-rolling="false"])',
            ),
          ]
            .map((die) => die.getAttribute('data-opening-die'))
            .join('-');
          const frame = values && !values.includes('pending') ? values : kind;
          if (!frame || frames.at(-1) === frame) return;
          frames.push(frame);
          seen.content = frames.join(',');
        }).observe(document.body, {
          subtree: true,
          attributes: true,
          childList: true,
        });
      });
    });
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await page
      .getByRole('button', { name: 'Start match', exact: true })
      .click();
    const board = page.locator('.play-board');
    await expect(board.locator('.play-die-rolling')).toHaveCount(2);
    await expect(page.locator('dialog:modal')).toHaveCount(0);
    await expect(board).toHaveAttribute(
      'data-play-phase',
      tie ? 'roll' : 'move',
    );
    const frames = await page
      .locator('meta[name="opening-animation-frames"]')
      .getAttribute('content');
    expect(frames).toContain(tie ? '4-4,dice,2-2,dice,1-5' : '6-2');
    if (tie) expect(frames).toContain('checker');
    expect(writes).toEqual([
      { version: 0, action: 'opening_roll' },
      { version: 1, action: 'start_opening_move' },
    ]);
    if (tie) {
      await expect(rollDice(page)).toBeVisible();
      expect(state.history[0].dice).toEqual([5, 1]);
    } else {
      await expect(rollDice(page)).not.toBeVisible();
      await expect(
        board.getByRole('button', {
          name: 'Swap dice order · 6, 2',
          exact: true,
        }),
      ).toBeVisible();
    }
  });

test('a lost opening-roll response resumes from the committed dice after reload', async ({
  page,
}) => {
  let saved = pending();
  const writes: unknown[] = [];
  await page.addInitScript(
    (id) => localStorage.setItem('bgtutor.play-session', id),
    id,
  );
  await page.route(`**/api/play/sessions/${id}`, (route) =>
    route.fulfill({ json: saved }),
  );
  await page.route(`**/api/play/sessions/${id}/actions`, (route) => {
    const request = route.request().postDataJSON();
    writes.push(request);
    if (request.action === 'opening_roll') {
      saved = {
        ...saved,
        version: 1,
        phase: 'opening_result',
        opening: { white_die: 6, black_die: 1, winner: 0, rolls: [[6, 1]] },
        position: { ...saved.position, dice: [6, 1] },
      };
      return route.abort();
    }
    expect(request).toEqual({ version: 1, action: 'start_opening_move' });
    saved = {
      ...saved,
      version: 2,
      phase: 'move',
      legal_moves: [
        {
          notation: '13/7 6/5',
          steps: [
            { from: 13, to: 7 },
            { from: 6, to: 5 },
          ],
        },
      ],
    };
    return route.fulfill({ json: saved });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const error = page.locator('.play-workspace').getByRole('alert');
  await expect(error).toContainText('Reload the saved state before retrying');
  expect(writes).toEqual([{ version: 0, action: 'opening_roll' }]);
  await expect(page.locator('dialog:modal')).toHaveCount(0);
  await error.getByRole('button', { name: 'Reload saved state' }).click();
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-play-phase',
    'move',
  );
  await expect(
    page.getByRole('button', { name: 'Swap dice order · 6, 1', exact: true }),
  ).toBeVisible();
  expect(writes).toEqual([
    { version: 0, action: 'opening_roll' },
    { version: 1, action: 'start_opening_move' },
  ]);
});
