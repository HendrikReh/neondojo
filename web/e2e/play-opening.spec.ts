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
  test(`explicit one-die-per-side opening, same dice, reload and ${tie ? 'bot' : 'human'} first`, async ({
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
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await page
      .getByRole('button', { name: 'Start match', exact: true })
      .click();
    await expect(page.locator('[data-opening-die="pending"]')).toHaveCount(2);
    await expect(
      page.locator('.play-board svg.board [data-opening-die]'),
    ).toHaveCount(2);
    await expect(page.locator('.play-opening [data-opening-die]')).toHaveCount(
      0,
    );
    await expect(page.locator('.play-board [data-play-die-index]')).toHaveCount(
      2,
    );
    await expect(rollDice(page)).not.toBeVisible();
    await page
      .getByRole('button', { name: 'Roll one die each', exact: true })
      .click();
    await expect(
      page.locator('[data-opening-player="0"] [data-opening-die]'),
    ).toHaveAttribute('data-opening-die', tie ? '1' : '6');
    await expect(
      page.locator('[data-opening-player="1"] [data-opening-die]'),
    ).toHaveAttribute('data-opening-die', tie ? '5' : '2');
    await expect(page.locator('.play-opening-winner')).toContainText(
      tie ? 'Bot goes first with 5 and 1' : 'Player goes first with 6 and 2',
    );
    if (tie) await expect(page.locator('.play-opening-ties li')).toHaveCount(2);
    await expect(page.locator('.play-board [data-play-die-index]')).toHaveCount(
      2,
    );
    await expect(rollDice(page)).not.toBeVisible();
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await expect(
      page.getByRole('button', { name: 'Begin the first move', exact: true }),
    ).toBeVisible();
    expect(writes).toEqual([{ version: 0, action: 'opening_roll' }]);
    await page
      .getByRole('button', { name: 'Begin the first move', exact: true })
      .click();
    expect(writes).toEqual([
      { version: 0, action: 'opening_roll' },
      { version: 1, action: 'start_opening_move' },
    ]);
    if (tie) {
      await expect(rollDice(page)).toBeVisible();
      expect(state.history[0].dice).toEqual([5, 1]);
    } else {
      await expect(rollDice(page)).not.toBeVisible();
      await expect(page.locator('[data-play-die-index="0"]')).toHaveAttribute(
        'aria-label',
        'Swap dice order · 6, 2',
      );
    }
  });
