import { DESKTOP_VIEWPORTS } from '../helpers/navigation';
import type { components } from '../../src/generated-api';
import { fulfillPositionAnalysisResult } from '../helpers/api-fixtures';
import { openPositionCalculation } from '../helpers/analysis-controls';
import { closeSettings } from '../helpers/settings-actions';
import { test, expect } from '../helpers/test';
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
    notation: '13/9',
    equity: 0.1,
    probabilities: [0.52, 0.1, 0.01, 0.48, 0.08, 0.01],
  },
];
for (const source of ['engine', 'file', 'book'] as const)
  test(`entire ${source} candidate rows preview from every column and keyboard`, async ({
    page,
  }) => {
    await page.addInitScript(
      ({ matchId }) => {
        localStorage.setItem('neondojo.locale', 'en');
        localStorage.setItem(
          'neondojo.workspace',
          JSON.stringify({ matchId, gameNo: 1 }),
        );
      },
      { matchId },
    );
    await page.route(
      (url) => url.pathname.startsWith('/api/'),
      (route) => {
        if (
          route.request().method() === 'GET' ||
          /^\/api\/(?:configuration|profiles)(?:\/|$)/.test(
            new URL(route.request().url()).pathname,
          )
        )
          return route.fallback();
        return route.fulfill({
          status: 422,
          json: { error: 'Fixture blocks writes' },
        });
      },
    );
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
    const analysis = { engine: 'GNU Backgammon', plies: 2, candidates };
    const job = {
      id: jobId,
      position_uuid: positionId,
      match_uuid: null,
      game_no: null,
      kind: 'analysis',
      status: 'completed',
      plies: 2,
      progress: 100,
      attempts: 1,
      total_games: 0,
      completed_games: 0,
      engine_snapshot: null,
      error: null,
      created_at: match.created_at,
      updated_at: match.created_at,
      result: analysis,
    };
    const records = [
      {
        id: 'file-record',
        match_uuid: matchId,
        position_uuid: positionId,
        source_format: 'sgf',
        kind: 'checker',
        engine: 'GNU Backgammon',
        engine_version: null,
        game_no: 1,
        source_move_no: 1,
        checker_ply: 1,
        player: 0,
        played: '8/5 6/5',
        method: 'evaluation',
        evaluated: true,
        depth: '2-ply',
        depth_plies: 2,
        raw: null,
        candidates: candidates.map((candidate) => ({
          ...candidate,
          notation: candidate.rank === 2 ? '13/10 10/9' : candidate.notation,
          probabilities: {
            win: candidate.probabilities[0],
            win_gammon: candidate.probabilities[1],
            win_backgammon: candidate.probabilities[2],
            lose_gammon: candidate.probabilities[4],
            lose_backgammon: candidate.probabilities[5],
          },
          equity_kind: 'cubeful',
          depth: '2-ply',
          raw: {},
        })),
        cube: null,
      },
    ];
    const entry = {
      id: source === 'file' ? `imported:${matchId}` : `job:${jobId}`,
      source: source === 'file' ? 'imported' : 'native',
      scope: source === 'file' ? 'match' : 'position',
      engine: 'GNU Backgammon',
      depth: '2-ply',
      game_no: source === 'file' ? null : 1,
      position_id: source === 'file' ? null : positionId,
      created_at: match.created_at,
      status: 'completed',
      complete: true,
      checker_count: 1,
      cube_count: 0,
      job: source === 'file' ? null : job,
    };
    await page.route(`**/api/matches/${matchId}/analyses?*`, (route) =>
      route.fulfill({
        json: {
          match_id: matchId,
          items: [entry],
          selected: entry,
          selected_id: entry.id,
          next_cursor: null,
        },
      }),
    );
    await page.route(`**/api/positions/${positionId}/jobs?*`, (route) =>
      route.fulfill({ json: [job] }),
    );
    await page.route(`**/api/jobs/${jobId}`, (route) =>
      route.fulfill({ json: job }),
    );
    await page.route(
      `**/api/positions/${positionId}/analysis-result?*`,
      (route) =>
        fulfillPositionAnalysisResult(route, {
          analysis_id: entry.id,
          native_checker_equity_kind: source === 'file' ? null : 'cubeless',
          position_id: positionId,
          engine: entry.engine,
          depth: entry.depth,
          records:
            source === 'file'
              ? (records as components['schemas']['ImportedAnalysisRecord'][])
              : [],
          decision: null,
          analysis: source === 'file' ? null : analysis,
        }),
    );
    await page.route(`**/api/positions/${positionId}/opening-book`, (r) =>
      r.fulfill({
        json: {
          provider: 'backgammondb',
          hit: true,
          provenance: {
            provider: 'backgammondb',
            engine: 'GNUbg',
            evaluation_kind: 'evaluation',
            equity_kind: 'normalized_cubeful',
            context: 'money',
          },
          cube: null,
          checker: { plies: 3, candidates },
        },
      }),
    );
    await page.goto('/');
    await expect(page.locator('.analysis-workspace .board')).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'Analysis options', exact: true }),
    ).toBeVisible();
    await expect(
      page.locator('.analysis-workspace .board-focus-toggle'),
    ).toHaveCount(0);
    await expect(
      page.locator('.analysis-workspace .moves-panel'),
    ).toBeVisible();
    await expect(page.locator('.analysis-workspace')).not.toHaveClass(
      /board-focus/,
    );
    await expect(
      page.getByLabel('Displayed analysis', { exact: true }),
    ).toHaveValue(entry.id);
    const candidateTab = page.getByRole('tabpanel', {
      name: 'Candidate moves',
    });
    if (source === 'book')
      await candidateTab.locator('.analysis-book-reference > summary').click();
    const panel =
      source === 'book'
        ? candidateTab.locator('.analysis-book-reference')
        : candidateTab;
    const rows = panel.locator('.candidates tbody tr');
    await expect(rows).toHaveCount(2);
    await expect(
      panel.getByRole('columnheader', { name: 'Loss', exact: true }),
    ).toBeVisible();
    await expect(rows.first()).toHaveAttribute('data-candidate-best', 'true');
    await expect(rows.first()).toHaveAttribute('data-candidate-played', 'true');
    await expect(rows.last().locator('td').nth(3)).toHaveText('0.100');
    await expect(rows.first().locator('[data-probability="win"]')).toHaveText([
      '55.0%',
      '9.0%',
      '1.0%',
    ]);
    await expect(rows.first().locator('[data-probability="loss"]')).toHaveText([
      '45.0%',
      '7.0%',
      '1.0%',
    ]);
    await expect(
      rows.last().locator('td[data-outcome="total"] [data-probability="win"]'),
    ).toHaveText('52.0%');
    await expect(panel.locator('.candidate-metadata')).toContainText(
      { book: 'Normalized cubeful', file: 'Cubeful', engine: 'Cubeless' }[
        source
      ],
    );
    await expect(panel.locator('.candidate-metadata')).toContainText('Player');
    await expect(
      page.locator('.board a[href*="bgblitz.com/themes.html"]'),
    ).toHaveCount(0);
    for (let column = 0; column < 8; column++) {
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
      await expect(
        page.locator('.analysis-workspace .board-shell .move-arrows'),
      ).toBeVisible();
    }
    const first = rows
      .first()
      .getByRole('button', { name: /^Preview move 1:/ });
    await first.click();
    await expect(first).toHaveAttribute('aria-pressed', 'true');
    await page.mouse.move(0, 0);
    await expect(first).toHaveCSS('border-top-color', 'rgba(0, 0, 0, 0)');
    await expect(first).toHaveCSS('outline-style', 'none');
    await first.press('Tab');
    await page.keyboard.press('Shift+Tab');
    await expect(first).toBeFocused();
    await expect(first).toHaveCSS('outline-style', 'solid');
    await page.keyboard.press('Enter');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 1',
    );
    const second = rows
      .last()
      .getByRole('button', { name: 'Preview move 2: 13/9', exact: true });
    await second.focus();
    await page.keyboard.press(' ');
    await expect(rows.last().locator('.candidate-marker-preview')).toHaveText(
      'Preview',
    );
    await expect(rows.first().locator('.candidate-marker-best')).toHaveText(
      'Best',
    );
    await expect(rows.first().locator('.candidate-marker-played')).toHaveText(
      'Played',
    );
    await expect(rows.first()).not.toHaveClass(/best-move/);
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 2',
    );
    await expect(page.locator('.board-review-selection')).toContainText('13/9');
    await expect(
      page.locator(
        '.analysis-workspace .board-shell .move-arrows [data-from="13"][data-to="10"]',
      ),
    ).toBeVisible();
    await expect(
      page.locator(
        '.analysis-workspace .board-shell .move-arrows [data-from="10"][data-to="9"]',
      ),
    ).toBeVisible();
    await rows.first().locator('td').first().click();
    await first.focus();
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.board-review-selection')).toContainText(
      'Candidate 2',
    );
    await expect(second).toBeFocused();
    await expect(
      page
        .locator('.analysis-workspace .board-shell .move-arrows [data-from]')
        .first(),
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
      page
        .locator('.analysis-workspace .board-shell .move-arrows [data-from]')
        .first(),
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
    const board = page.locator('.analysis-workspace .board-shell');
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
    const calculation = await openPositionCalculation(page);
    await calculation.getByLabel('Search depth', { exact: true }).focus();
    await page.keyboard.press('ArrowLeft');
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.keyboard.press('ArrowLeft');
    await expect(board).toHaveAttribute('data-preview-mode', 'after');
    await closeSettings(
      page.getByRole('dialog', { name: 'Settings', exact: true }),
    );
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
    for (const { width, height } of DESKTOP_VIEWPORTS) {
      await page.setViewportSize({ width, height });
      await expect(rows.last().locator('td').last()).toBeInViewport();
      await page.screenshot({
        path: test.info().outputPath(`candidates-${source}-${width}.png`),
      });
    }
    await expect(board.locator('[data-point="13"]')).toHaveAttribute(
      'data-count',
      '5',
    );
  });
