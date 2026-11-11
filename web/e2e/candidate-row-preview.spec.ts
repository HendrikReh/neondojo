import { test, expect } from './test';
const matchId = '8216b2ea-7fe8-4d57-88c0-111111111111',
  positionId = '8216b2ea-7fe8-4d57-88c0-222222222222',
  jobId = '8216b2ea-7fe8-4d57-88c0-333333333333';
const candidates = [
  {
    rank: 1,
    notation: '8/5 6/5',
    equity: 0.2,
    probabilities: [0.55, 0.1, 0.01, 0.45, 0.08, 0.01],
  },
  {
    rank: 2,
    notation: '13/10 6/5',
    equity: 0.1,
    probabilities: [0.52, 0.1, 0.01, 0.48, 0.08, 0.01],
  },
];
for (const source of ['engine', 'file', 'book'] as const)
  test(`entire ${source} candidate rows preview from every column and keyboard`, async ({
    page,
  }) => {
    await page.addInitScript(
      ({ matchId, positionId, jobId }) => {
        localStorage.setItem('bgtutor.locale', 'en');
        localStorage.setItem(
          'bgtutor.workspace',
          JSON.stringify({ matchId, gameNo: 1 }),
        );
        localStorage.setItem(
          `bgtutor.jobs.${positionId}`,
          JSON.stringify({ analysis: jobId }),
        );
      },
      { matchId, positionId, jobId },
    );
    await page.route('**/api/**', async (route) => {
      if (
        /\/api\/(?:configuration|profiles)(?:\/|$)/.test(
          new URL(route.request().url()).pathname,
        )
      )
        return route.fallback();
      if (route.request().method() !== 'GET')
        return route.fulfill({
          status: 422,
          json: { error: 'Fixture blocks writes' },
        });
      return route.continue();
    });
    const match = {
      id: matchId,
      source_available: true,
      filename: 'rows.sgf',
      format: 'sgf',
      white: 'Player',
      black: 'Opponent',
      match_length: 7,
      position_count: 1,
      created_at: '2026-11-09T10:00:00Z',
    };
    await page.route(`**/api/matches/${matchId}`, (r) =>
      r.fulfill({ json: match }),
    );
    await page.route(`**/api/matches/${matchId}/games`, (r) =>
      r.fulfill({
        json: [
          {
            game_no: 1,
            position_count: 1,
            white_score: 0,
            black_score: 0,
            crawford: false,
          },
        ],
      }),
    );
    await page.route(`**/api/matches/${matchId}/dice?*`, (r) =>
      r.fulfill({ json: { total: 1, rolls: [] } }),
    );
    await page.route('**/api/positions?*', (r) =>
      r.fulfill({
        json: {
          items: [
            {
              id: positionId,
              match_uuid: matchId,
              ordinal: 0,
              game_no: 1,
              ply: 1,
              player: 0,
              dice: [3, 1],
              board: [
                0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5,
                0, 0, 0, 0, 2, 0,
              ],
              position_id: '4HPwATDgc/ABMA',
              gnubg_match_id: 'cInlAAAAAAAE',
              notation: '8/5 6/5',
            },
          ],
          next_cursor: null,
        },
      }),
    );
    await page.route(`**/api/jobs/${jobId}`, (r) =>
      r.fulfill({
        json: {
          id: jobId,
          position_uuid: positionId,
          match_uuid: null,
          kind: 'analysis',
          status: 'completed',
          plies: 2,
          progress: 100,
          attempts: 1,
          created_at: '2026-11-09T10:00:00Z',
          updated_at: '2026-11-09T10:00:00Z',
          result: { engine: 'GNU Backgammon', plies: 2, candidates },
        },
      }),
    );
    await page.route(`**/api/positions/${positionId}/imported-analysis`, (r) =>
      r.fulfill({
        json: [
          {
            id: 'file-record',
            match_uuid: matchId,
            position_uuid: positionId,
            source_format: 'sgf',
            kind: 'checker',
            engine: 'GNU Backgammon',
            depth: '2-ply',
            candidates: candidates.map((c) => ({
              ...c,
              equity_kind: 'cubeful',
              depth: '2-ply',
              raw: {},
            })),
            cube: null,
          },
        ],
      }),
    );
    await page.route(`**/api/positions/${positionId}/opening-book`, (r) =>
      r.fulfill({
        json: {
          provider: 'backgammondb',
          hit: true,
          cube: null,
          checker: { plies: 3, candidates },
        },
      }),
    );
    await page.goto('/');
    await expect(page.locator('.analysis-workspace .board')).toBeVisible();
    await expect(
      page.locator(
        '.analysis-controls [data-action-icon="analysis"] .action-label',
      ),
    ).toBeVisible();
    await expect(
      page.locator('.analysis-workspace .moves-panel'),
    ).toBeVisible();
    await expect(page.locator('.analysis-workspace')).not.toHaveClass(
      /board-focus/,
    );
    if (source !== 'engine')
      await page
        .getByRole('tab', {
          name: source === 'file' ? 'Imported analysis' : 'Opening book',
          exact: true,
        })
        .click();
    const panel =
      source === 'engine'
        ? page.locator('.analysis-panel')
        : page.locator('.source-analysis');
    const rows = panel.locator('.candidates tbody tr');
    await expect(rows).toHaveCount(2);
    for (let column = 0; column < 4; column++) {
      const rank = (column % 2) + 1;
      await rows
        .nth(rank - 1)
        .locator('td')
        .nth(column)
        .click();
      await expect(page.locator('.board-review-selection')).toContainText(
        `Candidate ${rank}`,
      );
      await expect(rows.nth(rank - 1)).toHaveAttribute(
        'data-preview-selected',
        'true',
      );
      await expect(rows.nth(2 - rank)).toHaveAttribute(
        'data-preview-selected',
        'false',
      );
      await expect(page.locator('.board-shell .move-arrows')).toBeVisible();
    }
    const first = rows
      .first()
      .getByRole('button', { name: /^Preview move 1:/ });
    await first.focus();
    await page.keyboard.press('Enter');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    const second = rows
      .last()
      .getByRole('button', { name: /^Preview move 2:/ });
    await second.focus();
    await page.keyboard.press(' ');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 2',
    );
    await rows.first().locator('td').first().click();
    await first.focus();
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 2',
    );
    await expect(second).toBeFocused();
    await expect(
      page.locator('.board-shell .move-arrows [data-from]').first(),
    ).toHaveAttribute('data-from', '13');
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 2',
    );
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    await expect(first).toBeFocused();
    await expect(
      page.locator('.board-shell .move-arrows [data-from]').first(),
    ).toHaveAttribute('data-from', '8');
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    await page.keyboard.press('Control+ArrowDown');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    await rows.last().locator('td').last().click();
    await expect(second).toBeFocused();
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    await rows.last().locator('td').nth(2).click();
    const positionCodes = await page
      .locator('.position-code code')
      .allTextContents();
    const board = page.locator('.board-shell');
    await page.keyboard.press('ArrowLeft');
    await expect(board).toHaveAttribute('data-preview-mode', 'before');
    await expect(
      page.getByRole('button', { name: 'Initial', exact: true }),
    ).toHaveAttribute('aria-pressed', 'true');
    await expect(board.locator('[data-point="13"]')).toHaveAttribute(
      'data-count',
      '5',
    );
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    await expect(board).toHaveAttribute('data-preview-mode', 'before');
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 2',
    );
    await expect(board).toHaveAttribute('data-preview-mode', 'before');
    await page.keyboard.press('ArrowRight');
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await expect(
      page.getByRole('button', { name: 'Final', exact: true }),
    ).toHaveAttribute('aria-pressed', 'true');
    await expect(board.locator('[data-point="13"]')).toHaveAttribute(
      'data-count',
      '4',
    );
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 2',
    );
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await expect(board.locator('[data-point="13"]')).toHaveAttribute(
      'data-count',
      '5',
    );
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 2',
    );
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await expect(board.locator('[data-point="13"]')).toHaveAttribute(
      'data-count',
      '4',
    );
    await expect(second).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await page.keyboard.press('Control+ArrowLeft');
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await second.evaluate((el) => el.blur());
    await page.keyboard.press('ArrowLeft');
    await expect(board).toHaveAttribute('data-preview-mode', 'before');
    await page.keyboard.press('ArrowRight');
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await page.locator('.analysis-controls select').first().focus();
    await page.keyboard.press('ArrowLeft');
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.keyboard.press('ArrowLeft');
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await page
      .getByRole('dialog', { name: 'Settings', exact: true })
      .getByRole('button', { name: 'Cancel', exact: true })
      .click();
    expect(await page.locator('.position-code code').allTextContents()).toEqual(
      positionCodes,
    );
    await rows.last().locator('td').first().click();
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    await expect(
      page.getByRole('button', { name: 'Preview best move', exact: true }),
    ).toHaveCount(0);
    await second.press('Enter');
    await expect(board.locator('[data-point="13"]')).toHaveAttribute(
      'data-count',
      '4',
    );
    await page
      .getByRole('button', { name: 'Reset preview', exact: true })
      .click();
    const playedMove = page.locator('.moves-list [aria-current="step"]');
    await expect(playedMove).toBeFocused();
    await expect(playedMove).toBeInViewport();
    await expect(playedMove).toHaveClass(/\bcurrent\b/);
    await expect(playedMove).toHaveCSS('outline-style', 'solid');
    await expect(playedMove).toContainText('8/5 6/5');
    await expect(page.locator('.board-review-selection')).toHaveCount(0);
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await expect(board.locator('[data-point="13"]')).toHaveAttribute(
      'data-count',
      '5',
    );
  });
