import type { GameMoveAssessments } from '../../src/shared/review/useMoveAssessments';
import { importSgfMatch } from '../helpers/match-fixtures';
import { DESKTOP_VIEWPORTS, seedLocale } from '../helpers/navigation';
import { expect, test } from '../helpers/test';

test('move history filters the selected game by the saved grade without moving the board', async ({
  page,
}, testInfo) => {
  await seedLocale(page);
  const { match } = await importSgfMatch(page, {
    filename: 'move-history-filter.sgf',
    game: {
      white: `History ${crypto.randomUUID()}`,
      moves: ';W[31hefe];B[42qusu];W[31hefe];B[42qusu]',
    },
  });
  const { items: positions } = await (
    await page.request.get(
      `/api/positions?match_id=${match.id}&game_no=1&limit=100`,
    )
  ).json();
  expect(positions).toHaveLength(4);
  const assessment = {
    source: 'imported',
    engine: 'GNU Backgammon',
    equity_loss: 0,
  } as const;
  // Move marks follow the displayed analysis, so the match needs one.
  const entry = {
    id: `imported:${match.id}`,
    source: 'imported',
    scope: 'match',
    engine: 'GNU Backgammon',
    depth: '2-ply',
    game_no: null,
    position_id: null,
    created_at: match.created_at,
    status: 'completed',
    complete: true,
    checker_count: 3,
    cube_count: 0,
    job: null,
  };
  await page.route(`**/api/matches/${match.id}/analyses?*`, (route) =>
    route.fulfill({
      json: {
        match_id: match.id,
        items: [entry],
        selected: entry,
        selected_id: entry.id,
        next_cursor: null,
        source_revision: 0,
      },
    }),
  );
  await page.route(`**/api/matches/${match.id}/move-assessments?*`, (route) =>
    route.fulfill({
      json: {
        match_id: match.id,
        game_no: 1,
        job_id: null,
        analysis_id: entry.id,
        items: [
          { ...assessment, position_id: positions[0].id, severity: 'none' },
          {
            ...assessment,
            position_id: positions[1].id,
            severity: 'bad',
            equity_loss: 0.04,
          },
          {
            ...assessment,
            position_id: positions[2].id,
            severity: 'blunder',
            equity_loss: 0.1,
          },
        ],
      } satisfies GameMoveAssessments,
    }),
  );
  await page.addInitScript((matchId) => {
    localStorage.setItem(
      'neondojo.workspace',
      JSON.stringify({ matchId, gameNo: 1 }),
    );
  }, match.id);
  await page.goto('/');

  const history = page.locator('.analysis-workspace .moves-panel');
  const rows = history.locator('.move-history-row');
  const numbers = rows.locator('.move-no');
  const filter = history.getByRole('combobox', {
    name: 'Filter moves by severity',
  });
  const board = page.locator('.board-heading');
  await expect(filter.locator('option:checked')).toHaveText('All moves');
  await expect(filter.locator('option')).toHaveText([
    'All moves',
    'All errors',
    'Questionable',
    'Error',
    'Blunder',
    'Large Blunder',
  ]);
  await expect(numbers).toHaveText(['1.1', '1.2', '1.3', '1.4']);
  await expect(rows.last()).toContainText('—');
  await expect(board).toContainText('Move 1');

  await filter.selectOption({ label: 'All errors' });
  await expect(numbers).toHaveText(['1.2', '1.3']);
  await expect(board).toContainText('Move 1');
  await filter.selectOption({ label: 'Blunder' });
  await expect(numbers).toHaveText(['1.3']);
  await rows.first().click();
  await expect(board).toContainText('Move 3');

  await filter.selectOption({ label: 'Questionable' });
  await expect(rows).toHaveCount(0);
  await expect(history.getByRole('status')).toHaveText(
    'No moves with this error grade.',
  );
  await filter.selectOption({ label: 'All moves' });
  await expect(numbers).toHaveText(['1.1', '1.2', '1.3', '1.4']);
  await expect(rows.nth(2)).toHaveAttribute('aria-current', 'step');

  await filter.selectOption({ label: 'Questionable' });
  await page.reload();
  await expect(filter.locator('option:checked')).toHaveText('Questionable');
  await expect(history.getByRole('status')).toHaveText(
    'No moves with this error grade.',
  );
  await filter.selectOption({ label: 'All moves' });

  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  const errors = page.getByRole('tabpanel', { name: 'Cube', exact: true });
  await expect(errors.getByRole('table')).toHaveCount(0);
  await expect(
    page.getByRole('group', { name: 'Review errors' }),
  ).toBeVisible();
  await expect(numbers).toHaveText(['1.1', '1.2', '1.3', '1.4']);
  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    await expect(filter).toBeInViewport({ ratio: 1 });
    await expect(rows.nth(2)).toBeInViewport({ ratio: 1 });
    await page.screenshot({
      path: testInfo.outputPath(`move-history-${width}.png`),
    });
  }
});
