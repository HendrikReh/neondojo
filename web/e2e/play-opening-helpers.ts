import { expect, type Page } from '@playwright/test';
import type { PlaySession } from '../src/workspaces/play/play-api';

export function watchOpening(page: Page) {
  const wait = (action: string) =>
    page.waitForResponse(
      (response) =>
        response.url().endsWith('/actions') &&
        response.request().method() === 'POST' &&
        response.request().postDataJSON()?.action === action,
      { timeout: 200_000 },
    );
  const rolled = wait('opening_roll');
  const continued = wait('start_opening_move');
  return async (expectedRolls?: number[][]): Promise<PlaySession> => {
    const response = await rolled;
    expect(response.status(), await response.text()).toBe(200);
    const result = (await response.json()) as PlaySession;
    expect(result.phase).toBe('opening_result');
    const opening = result.opening!;
    expect(opening.white_die).not.toBe(opening.black_die);
    expect(opening.winner).toBe(opening.white_die > opening.black_die ? 0 : 1);
    expect(opening.rolls.at(-1)).toEqual([
      opening.white_die,
      opening.black_die,
    ]);
    expect(opening.rolls.slice(0, -1).every(([a, b]) => a === b)).toBe(true);
    if (expectedRolls) expect(opening.rolls).toEqual(expectedRolls);
    const final = await continued;
    expect(final.status(), await final.text()).toBe(200);
    const state = (await final.json()) as PlaySession;
    const pair = [opening.white_die, opening.black_die].sort((a, b) => b - a);
    if (opening.winner === state.human_player) {
      expect(state.phase).toBe('move');
      expect(state.position.dice).toEqual(pair);
      expect(state.history).toEqual(result.history);
    } else {
      expect(state.history[result.history.length]).toMatchObject({
        player: opening.winner,
        dice: pair,
      });
    }
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-phase',
      state.phase,
    );
    await expect(page.locator('.play-workspace')).toHaveAttribute(
      'aria-busy',
      'false',
    );
    return state;
  };
}

export async function finishNextGame(
  page: Page,
  nextGame: () => Promise<unknown>,
) {
  const opening = watchOpening(page);
  await nextGame();
  return opening();
}
