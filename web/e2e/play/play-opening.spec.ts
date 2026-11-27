import { gotoWorkspace } from '../helpers/navigation';
import {
  mockPlaySession,
  playSessionFixture,
} from '../helpers/play-session-fixture';
import { eitherOrder } from '../helpers/play-turn-fixtures';
import { rollDice } from '../helpers/play-board-actions';
import { test, expect } from '../helpers/test';
import type { PlaySession } from '../../src/workspaces/play/play-api';
const id = '200389f1-9b5b-4b38-9869-3fdc922030b1';
const pending = (): PlaySession =>
  playSessionFixture({ id, phase: 'opening_roll' });
for (const tie of [false, true])
  test(`automatic opening animation with ${tie ? 'ties and bot' : 'human'} first`, async ({
    page,
  }) => {
    let state = pending();
    const { writes } = await mockPlaySession(page, {
      session: state,
      seedSession: false,
      onAction: (action, current) => {
        if (action.action === 'opening_roll')
          state = {
            ...current,
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
              ...current.position,
              player: tie ? 1 : 0,
              dice: tie ? [5, 1] : [6, 2],
            },
          };
        else if (action.action === 'start_opening_move')
          state = {
            ...current,
            version: 2,
            phase: tie ? 'roll' : 'move',
            position: {
              ...current.position,
              player: 0,
              dice: tie ? [] : [6, 2],
            },
            history: tie
              ? [{ player: 1, dice: [5, 1], notation: '13/8 24/23' }]
              : [],
            legal_turns: tie ? [] : eitherOrder([6, 2], '24/18 13/11'),
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
        return state;
      },
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
    await gotoWorkspace(page, 'play');
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
  const { writes, setState } = await mockPlaySession(page, {
    session: pending(),
    mockCollection: false,
    open: true,
    onAction: (request, saved, route) => {
      if (request.action === 'opening_roll') {
        setState({
          ...saved,
          version: 1,
          phase: 'opening_result',
          opening: { white_die: 6, black_die: 1, winner: 0, rolls: [[6, 1]] },
          position: { ...saved.position, dice: [6, 1] },
        });
        return route.abort();
      }
      expect(request).toEqual({ version: 1, action: 'start_opening_move' });
      return {
        ...saved,
        version: 2,
        phase: 'move',
        legal_turns: eitherOrder([6, 1], '13/7 6/5'),
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
    },
  });
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
