import { rollDice } from './play-board-actions';
import { expect, type Page } from '@playwright/test';
import type { PlaySession } from '../src/play-api';

/** Drive the actual API through the visible opening, never bypassing its dice. */
export async function finishOpening(
  page: Page,
  state: PlaySession,
  reloadResult = false,
): Promise<PlaySession> {
  const baseHistory = state.history.length;
  if (state.phase === 'opening_roll') {
    await expect(
      page.getByRole('button', { name: 'Roll one die each', exact: true }),
    ).toBeVisible();
    await expect(rollDice(page)).not.toBeVisible();
    const response = page.waitForResponse(
      (r) => r.url().endsWith('/actions') && r.request().method() === 'POST',
      { timeout: 200000 },
    );
    await page
      .getByRole('button', { name: 'Roll one die each', exact: true })
      .click();
    const result = await response;
    expect(result.status(), await result.text()).toBe(200);
    state = await result.json();
  }
  if (state.phase !== 'opening_result') return state;
  const result = state.opening;
  expect(result).toBeTruthy();
  if (!result) throw new Error('Opening result missing');
  expect(result.white_die).not.toBe(result.black_die);
  expect(result.winner).toBe(result.white_die > result.black_die ? 0 : 1);
  expect(result.rolls.at(-1)).toEqual([result.white_die, result.black_die]);
  expect(
    result.rolls.slice(0, -1).every((roll) => roll[0] === roll[1]),
  ).toBeTruthy();
  expect(state.history.length).toBe(baseHistory);
  await expect(
    page.locator('[data-opening-player="0"] [data-opening-die]'),
  ).toHaveAttribute('data-opening-die', String(result.white_die));
  await expect(
    page.locator('[data-opening-player="1"] [data-opening-die]'),
  ).toHaveAttribute('data-opening-die', String(result.black_die));
  await expect(rollDice(page)).not.toBeVisible();
  if (reloadResult) {
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const after = await (
      await page.request.get(`/api/play/sessions/${state.id}`)
    ).json();
    expect(after).toEqual(state);
  }
  const response = page.waitForResponse(
    (r) => r.url().endsWith('/actions') && r.request().method() === 'POST',
    { timeout: 200000 },
  );
  await page
    .getByRole('button', { name: 'Begin the first move', exact: true })
    .click();
  const continued = await response;
  expect(continued.status(), await continued.text()).toBe(200);
  state = await continued.json();
  const pair = [result.white_die, result.black_die].sort((a, b) => b - a);
  if (result.winner === state.human_player) {
    expect(state.phase).toBe('move');
    expect(state.position.dice).toEqual(pair);
    expect(state.history.length).toBe(baseHistory);
    await expect(rollDice(page)).not.toBeVisible();
  } else {
    const first = state.history[baseHistory];
    expect(first.player).toBe(result.winner);
    expect(first.dice).toEqual(pair);
  }
  return state;
}
