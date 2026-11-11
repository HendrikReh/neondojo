import { test, expect, type Page } from './test';
const id = '8106b2ea-7fe8-4d57-88c0-111111111111';
const positionId = '8106b2ea-7fe8-4d57-88c0-222222222222';
const match = {
  id,
  source_available: true,
  filename: 'home-fixture.sgf',
  format: 'sgf',
  white: 'Home player',
  black: 'Opponent',
  match_length: 7,
  position_count: 1,
  created_at: '2026-11-09T10:00:00Z',
};
async function fixture(page: Page, hold = false) {
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.addInitScript(
    ({ id }) => {
      localStorage.setItem('bgtutor.locale', 'en');
      if (!sessionStorage.getItem('home-fixture-seeded')) {
        localStorage.setItem(
          'bgtutor.workspace',
          JSON.stringify({ matchId: id, gameNo: 1 }),
        );
        localStorage.setItem('bgtutor.play-session', 'preserve-bot-session');
        sessionStorage.setItem('home-fixture-seeded', 'yes');
      }
    },
    { id },
  );
  await page.route(`**/api/matches/${id}`, async (route) => {
    if (hold) await gate;
    await route.fulfill({ json: match }).catch(() => {});
  });
  await page.route(`**/api/matches/${id}/games`, (route) =>
    route.fulfill({
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
  await page.route(`**/api/matches/${id}/dice?*`, (route) =>
    route.fulfill({ json: { total: 1, rolls: [] } }),
  );
  await page.route('**/api/positions?*', (route) =>
    route.fulfill({
      json: {
        items: [
          {
            id: positionId,
            match_uuid: id,
            ordinal: 0,
            game_no: 1,
            ply: 1,
            player: 0,
            dice: [3, 1],
            board: [
              0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0,
              0, 0, 0, 2, 0,
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
  await page.goto('/');
  return release;
}
for (const target of ['icon', 'title', 'keyboard'])
  test(`${target} returns to welcome and reload stays there`, async ({
    page,
  }) => {
    await fixture(page);
    await expect(page.locator('.analysis-workspace .board')).toBeVisible();
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await page.evaluate(() => {
      document.documentElement.dataset.homeTestDocument = 'original';
    });
    if (target === 'keyboard') {
      await page
        .getByRole('link', { name: 'BG Tutor home', exact: true })
        .focus();
      await page.keyboard.press('Enter');
    } else
      await page
        .locator(target === 'icon' ? '.brand-mark' : '.brand > span:last-child')
        .click();
    await expect(page.locator('.analysis-welcome')).toBeVisible();
    await expect(page.locator('html')).toHaveAttribute(
      'data-home-test-document',
      'original',
    );
    await expect(
      page.getByRole('tab', { name: 'Analysis', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    expect(
      await page.evaluate(() => localStorage.getItem('bgtutor.workspace')),
    ).toBeNull();
    expect(
      await page.evaluate(() => localStorage.getItem('bgtutor.play-session')),
    ).toBe('preserve-bot-session');
    await page.reload();
    await expect(page.locator('.analysis-welcome')).toBeVisible();
  });
test('home cancels pending restoration', async ({ page }) => {
  const release = await fixture(page, true);
  try {
    await page
      .getByRole('link', { name: 'BG Tutor home', exact: true })
      .click();
    await expect(page.locator('.analysis-welcome')).toBeVisible();
    expect(
      await page.evaluate(() => localStorage.getItem('bgtutor.workspace')),
    ).toBeNull();
    release();
    await expect(page.locator('.analysis-workspace .board')).toHaveCount(0);
  } finally {
    release();
  }
});

test('a finishing import cannot reopen a match after Home', async ({
  page,
}) => {
  await fixture(page);
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/imports?*', async (route) => {
    await gate;
    await route.fulfill({ status: 201, json: { match, duplicate: false } });
  });
  const started = page.waitForRequest(
    (r) => new URL(r.url()).pathname === '/api/imports',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'home-import.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from('(;GM[6])'),
  });
  await started;
  try {
    await page
      .getByRole('link', { name: 'BG Tutor home', exact: true })
      .click();
    await expect(page.locator('.analysis-welcome')).toBeVisible();
    release();
    await expect(page.getByLabel('Match file', { exact: true })).toHaveValue(
      '',
    );
    await expect(page.locator('.analysis-welcome')).toBeVisible();
    expect(
      await page.evaluate(() => localStorage.getItem('bgtutor.workspace')),
    ).toBeNull();
  } finally {
    release();
  }
});
