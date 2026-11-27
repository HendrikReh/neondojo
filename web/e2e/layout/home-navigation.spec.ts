import { DESKTOP_VIEWPORTS, seedLocale } from '../helpers/navigation';
import { playPositionFixture } from '../helpers/play-session-fixture';
import { test, expect, type Page } from '../helpers/test';
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
  position_count: 2,
  created_at: '2026-11-09T10:00:00Z',
};
const position = playPositionFixture(
  { id: positionId, ordinal: 1, ply: 2, notation: '8/5 6/5' },
  id,
);
async function fixture(page: Page, hold = false) {
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.addInitScript(
    ({ id, positionId }) => {
      localStorage.setItem('neondojo.locale', 'en');
      if (!sessionStorage.getItem('home-fixture-seeded')) {
        localStorage.setItem(
          'neondojo.workspace',
          JSON.stringify({ matchId: id, gameNo: 1, positionId }),
        );
        localStorage.setItem(
          'neondojo.play-session',
          '8106b2ea-7fe8-4d57-88c0-333333333333',
        );
        sessionStorage.setItem('home-fixture-seeded', 'yes');
      }
    },
    { id, positionId },
  );
  await page.route(`**/api/positions/${positionId}`, (route) =>
    route.fulfill({ json: position }),
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
          position_count: 2,
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
        items: [{ ...position, id, ply: 1, ordinal: 0 }, position],
        next_cursor: null,
      },
    }),
  );
  await page.goto('/');
  return release;
}
for (const target of ['icon', 'title', 'keyboard'])
  test(`${target} opens Home, retains the move and resumes after reload`, async ({
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
        .getByRole('link', { name: 'NeonDojo home', exact: true })
        .focus();
      await page.keyboard.press('Enter');
    } else
      await page
        .locator(target === 'icon' ? '.brand-mark' : '.brand > span:last-child')
        .click();
    const home = page.getByRole('region', { name: 'Home', exact: true });
    await expect(home.locator('.board')).toBeVisible();
    await expect(home).toContainText('Home player · Opponent');
    if (target === 'icon') {
      for (const { width, height } of DESKTOP_VIEWPORTS) {
        await page.setViewportSize({ width, height });
        await expect(
          home.getByRole('button', { name: 'Resume at game 1, move 2' }),
        ).toBeInViewport();
        await home.screenshot({
          path: test.info().outputPath('home-' + width + '.png'),
        });
      }
    }
    await expect(page.locator('html')).toHaveAttribute(
      'data-home-test-document',
      'original',
    );
    await expect(page.getByRole('tab', { selected: true })).toHaveCount(0);
    expect(
      await page.evaluate(() => localStorage.getItem('neondojo.play-session')),
    ).toBe('8106b2ea-7fe8-4d57-88c0-333333333333');
    await page.reload();
    await home
      .getByRole('button', { name: 'Resume at game 1, move 2' })
      .click();
    await expect(
      page.getByRole('tab', { name: 'Analysis', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    await expect(page.locator('.analysis-workspace .board')).toBeVisible();
    expect(
      JSON.parse(
        (await page.evaluate(() =>
          localStorage.getItem('neondojo.workspace'),
        ))!,
      ),
    ).toEqual({ matchId: id, gameNo: 1, positionId });
  });
test('Home lets a pending restoration finish in the background', async ({
  page,
}) => {
  const release = await fixture(page, true);
  try {
    await page
      .getByRole('link', { name: 'NeonDojo home', exact: true })
      .click();
    await expect(
      page.getByRole('heading', { name: 'Home', exact: true }),
    ).toBeVisible();
    release();
    await expect(
      page.getByRole('button', { name: 'Resume at game 1, move 2' }),
    ).toBeVisible();
    await expect(page.locator('.analysis-workspace .board')).toBeHidden();
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
      .getByRole('link', { name: 'NeonDojo home', exact: true })
      .click();
    await expect(page.locator('.home-page')).toBeVisible();
    release();
    await expect(page.getByLabel('Match file', { exact: true })).toHaveValue(
      '',
    );
    await expect(page.locator('.home-page')).toBeVisible();
    expect(
      await page.evaluate(() => localStorage.getItem('neondojo.workspace')),
    ).toContain(positionId);
  } finally {
    release();
  }
});

for (const stored of [
  { matchId: '8106b2ea-7fe8-4d57-88c0-999999999991', gameNo: 1 },
  {
    matchId: '8106b2ea-7fe8-4d57-88c0-999999999992',
    gameNo: 1,
    positionId: '8106b2ea-7fe8-4d57-88c0-999999999993',
  },
]) {
  test(`a deleted ${stored.positionId ? 'position' : 'match'} is forgotten on startup instead of reported`, async ({
    page,
  }) => {
    await page.addInitScript((value) => {
      localStorage.setItem('neondojo.locale', 'en');
      localStorage.setItem('neondojo.workspace', JSON.stringify(value));
    }, stored);
    await page.goto('/');
    await expect(
      page.getByRole('heading', { name: 'Your next move starts here.' }),
    ).toBeVisible();
    await expect
      .poll(() =>
        page.evaluate(() => localStorage.getItem('neondojo.workspace')),
      )
      .toBeNull();
    await expect(page.getByRole('alert')).toHaveCount(0);
  });
}

test('first visit opens Home and its quick starts reach the intended controls', async ({
  page,
}) => {
  await seedLocale(page);
  await page.goto('/');
  const home = page.getByRole('region', { name: 'Home', exact: true });
  await expect(
    home.getByRole('heading', { name: 'Import your first match' }),
  ).toBeVisible();
  const chooser = page.waitForEvent('filechooser');
  await home.getByRole('button', { name: 'Import match', exact: true }).click();
  await (await chooser).setFiles([]);
  await home.getByRole('button', { name: 'Find a position' }).click();
  await expect(
    page.getByRole('button', { name: 'All analyzed decisions' }),
  ).toHaveAttribute('aria-pressed', 'true');
  const logo = page.getByRole('link', { name: 'NeonDojo home' });
  await logo.click();
  await logo.focus();
  await page.keyboard.press('Tab');
  await expect(
    page.getByRole('tab', { name: 'Playing', exact: true }),
  ).toBeFocused();
  await home.getByRole('button', { name: 'Play against the computer' }).click();
  await expect(page.locator('.play-setup-dialog')).toBeVisible();
});

test('the card shortcut uses today’s allowance and opens the recommended deck', async ({
  page,
}) => {
  const deck = {
    id: '8106b2ea-7fe8-4d57-88c0-444444444444',
    name: 'Home cards',
    card_count: 8,
    due_count: 8,
    version: 1,
  };
  await seedLocale(page);
  await page.route('**/api/study/learning-summary', (route) =>
    route.fulfill({ json: { today_count: 3, due_count: 8, next_deck: deck } }),
  );
  await page.route('**/api/decks', (route) => route.fulfill({ json: [deck] }));
  await page.goto('/');
  const next = page.waitForRequest((request) =>
    request.url().includes('/decks/' + deck.id + '/next'),
  );
  await page.getByRole('button', { name: 'Flashcards for today · 3' }).click();
  await next;
  await expect(
    page.getByRole('tab', { name: 'Flashcards & Anki' }),
  ).toHaveAttribute('aria-selected', 'true');
});

test('an import from Home opens its match during a pending restore', async ({
  page,
}) => {
  const release = await fixture(page, true);
  try {
    await page.getByRole('link', { name: 'NeonDojo home' }).click();
    await page.route(/\/api\/imports(?:\?|$)/, (route) =>
      route.fulfill({
        status: 201,
        json: { match, duplicate: false, warnings: [] },
      }),
    );
    await page.getByLabel('Match file', { exact: true }).setInputFiles({
      name: 'during-restore.sgf',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from('(;GM[6])'),
    });
    await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  } finally {
    release();
  }
});
