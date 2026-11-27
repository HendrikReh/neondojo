import { fulfillPlaySession } from '../helpers/api-fixtures';
import { openWorkspace } from '../helpers/navigation';
import { test, expect } from '../helpers/test';
import { playMenu } from '../helpers/play-board-actions';
import {
  historyFixture,
  historySessionId as id,
} from '../helpers/play-history-review-helpers';

test('forfeit confirmation preserves a draft on cancel and records a match loss on confirm', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  const { saved, writes, point } = await historyFixture(page);
  await page.route(`**/api/play/sessions/${id}/actions`, (route) => {
    writes.push(route.request().postDataJSON());
    Object.assign(saved, {
      version: 8,
      phase: 'match_over',
      match_forfeited: true,
      scores: [0, 7],
      winner: 1,
      legal_moves: [],
    });
    return fulfillPlaySession(route, saved);
  });
  await page.reload();
  await openWorkspace(page, 'play');
  await point(13).click();

  const menu = playMenu(page);
  const trigger = page.locator('.play-menu').getByRole('button', {
    name: 'Forfeit match',
    exact: true,
  });
  const dialog = page.getByRole('dialog', {
    name: 'Forfeit match',
    exact: true,
  });
  await menu.click();
  await trigger.click();
  await expect(dialog).toContainText('recorded as a loss');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(menu).toBeFocused();
  await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
  expect(writes).toEqual([]);

  await menu.click();
  await trigger.click();
  await dialog
    .getByRole('button', { name: 'Forfeit match', exact: true })
    .click();
  const result = page.getByRole('alertdialog', { name: 'Match finished' });
  await expect(result).toContainText('Match forfeited. You lost.');
  await expect(result.getByRole('button', { name: 'Next game' })).toHaveCount(
    0,
  );
  expect(writes).toEqual([{ action: 'forfeit_match', version: 7 }]);

  await page.reload();
  await openWorkspace(page, 'play');
  await expect(result).toContainText('Match forfeited. You lost.');
  await expect(trigger).toHaveCount(0);
});

test('forfeit can be cancelled from the between-games dialog', async ({
  page,
}) => {
  const { saved, writes } = await historyFixture(page);
  Object.assign(saved, { phase: 'game_over', scores: [1, 0], winner: 0 });
  await page.reload();
  await openWorkspace(page, 'play');
  const game = page.getByRole('alertdialog', { name: 'Game finished' });
  const trigger = game.getByRole('button', {
    name: 'Forfeit match',
    exact: true,
  });
  await trigger.click();
  const dialog = page.getByRole('dialog', {
    name: 'Forfeit match',
    exact: true,
  });
  await dialog.getByRole('button', { name: 'Continue playing' }).click();
  await expect(game.getByRole('button', { name: 'Next game' })).toBeVisible();
  await expect(trigger).toBeFocused();
  expect(writes).toEqual([]);
});
