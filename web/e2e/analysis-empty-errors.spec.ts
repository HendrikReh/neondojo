import { readFileSync } from 'node:fs';
import type { ReviewDecision } from '../src/review-decisions-api';
import { expect, test } from './test';

const decision: ReviewDecision = {
  id: 'imported:review-example',
  game_no: 1,
  decision_no: 1,
  checker_no: 1,
  player: 0,
  kind: 'cube',
  played: '8/5 6/5',
  best: '8/5 6/5',
  equity_loss: 0,
  evaluation_status: 'evaluated',
  severity: 'none',
  source: 'imported',
  engine: 'eXtreme Gammon',
  depth: 'XG Roller+',
};

for (const scenario of ['missing', 'filtered', 'clean'] as const) {
  test(`empty error navigation explains the ${scenario} state`, async ({
    page,
  }) => {
    await page.addInitScript(() =>
      localStorage.setItem('neondojo.locale', 'en'),
    );
    page.setDefaultTimeout(10000);

    const forced: ReviewDecision = {
      ...decision,
      id: 'imported:forced',
      evaluation_status: 'forced',
    };
    const evaluated =
      scenario === 'filtered'
        ? { ...decision, equity_loss: 0.1, severity: 'bad' }
        : decision;
    await page.route('**/api/matches/*/review-decisions?*', (route) => {
      const query = new URL(route.request().url()).searchParams;
      const errorsOnly = query.get('errors_only') === 'true';
      const numbered = query.has('page');
      const filtered = query.get('player') === '1';
      const items = errorsOnly
        ? scenario === 'filtered' && !filtered
          ? [evaluated]
          : []
        : scenario === 'missing'
          ? [forced]
          : filtered || (scenario === 'clean' && !numbered && !query.has('after'))
            ? []
            : [evaluated];
      const next_cursor =
        scenario === 'clean' && !numbered && !errorsOnly && !query.has('after')
          ? 'next-review-page'
          : null;
      return route.fulfill({
        json: {
          items,
          next_cursor,
          ...(numbered ? { total: items.length } : {}),
        },
      });
    });
    await page.goto('/');
    await page.getByLabel('Match file', { exact: true }).setInputFiles({
      name: 'empty-errors.sgf',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from(
        readFileSync('../backend/tests/fixtures/analysis-game1.sgf', 'utf8').replace(
          /PW\[[^\]]*\]/, `PW[Review ${crypto.randomUUID()}]`,
        ),
      ),
    });
    await page.getByRole('tab', { name: 'Cube', exact: true }).click();
    const panel = page.locator('.match-analysis');
    const empty = panel.locator('.analysis-empty');
    const board = page.getByRole('group', { name: 'Review errors' });
    const boardEmpty = board.locator('.analysis-empty');

    if (scenario === 'missing') {
      const selection = page.getByLabel('Displayed analysis', { exact: true });
      const start = page.getByRole('button', {
        name: 'Analyze game',
        exact: true,
      });
      await expect(selection).toBeVisible();
      await expect(start).toBeEnabled();
      await expect(empty).toContainText('No evaluated decisions are available');
      await expect(boardEmpty).toHaveText(await empty.innerText(), {
        useInnerText: true,
      });
      await expect(empty.getByRole('button')).toHaveCount(0);
      await expect(boardEmpty.getByRole('button')).toHaveCount(0);
    } else if (scenario === 'filtered') {
      const player = panel.getByLabel('Filter analysis by player', {
        exact: true,
      });
      await player.selectOption('1');
      await expect(empty).toContainText('No decisions match these filters');
      await expect(boardEmpty).toHaveText(await empty.innerText(), {
        useInnerText: true,
      });
      await boardEmpty
        .getByRole('button', { name: 'Reset selection filters' })
        .click();
      await expect(player).toHaveValue('');
      await expect(board.locator('.board-error-count')).toHaveText('Errors: 1');
      const errorsOnly = panel.getByRole('checkbox');
      await errorsOnly.uncheck();
      await player.selectOption('1');
      await expect(empty).toContainText('No decisions match these filters');
      await empty
        .getByRole('button', { name: 'Reset selection filters' })
        .click();
      await expect(errorsOnly).not.toBeChecked();
      await expect(board.locator('.board-error-count')).toHaveText('Decisions: 1');
    } else {
      await expect(empty).toContainText(
        'No inaccuracies in the evaluated decisions',
      );
      await expect(boardEmpty).toHaveText(await empty.innerText(), {
        useInnerText: true,
      });
      for (const [width, height] of [
        [1800, 1169],
        [2560, 1440],
        [3840, 2160],
      ]) {
        await page.setViewportSize({ width, height });
        await expect(empty).toBeInViewport();
        await expect(boardEmpty).toBeInViewport();
        await page.screenshot({
          path: test.info().outputPath(`empty-errors-${width}.png`),
        });
      }
      await boardEmpty
        .getByRole('button', { name: 'Show all decisions' })
        .click();
      await expect(board.locator('.board-error-count')).toHaveText('Decisions: 1');
    }
  });
}
