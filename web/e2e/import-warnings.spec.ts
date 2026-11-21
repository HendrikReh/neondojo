import { expect, test } from './test';

// The backend reports resolved source irregularities with each import; the
// XG conversion tests cover when they occur. This checks that both import
// summaries show them in the user's language. The shared fixture keeps the
// configuration per test, so the stored locale seeds this test's profile and
// no earlier test's persisted profile language replaces it after startup.
const warnings = [
  { kind: 'illegal_play_replaced', game_no: 1, source_ply: 19 },
  { kind: 'custom_start_position', game_no: 2, source_ply: null },
  { kind: 'unfinished_game', game_no: 3, source_ply: null },
];
const file = (name: string) => ({
  name,
  mimeType: 'application/octet-stream',
  buffer: Buffer.from(
    `(;GM[6]PW[Warning ${crypto.randomUUID()}]PB[Opponent]MI[length:5][ws:0][bs:0];W[31hefe];B[42qusu])`,
  ),
});

test.beforeEach(async ({ page }) => {
  await page.route('**/api/imports*', async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    await route.fulfill({ response, json: { ...body, warnings } });
  });
});

test('single import notice lists source warnings', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'de'));
  await page.goto('/');
  await page
    .getByLabel('Matchdatei', { exact: true })
    .setInputFiles(file('a.sgf'));
  const notice = page.locator('.notice[role="status"]');
  const items = notice.locator('.import-warnings li');
  await expect(items).toHaveCount(3);
  await expect(items.nth(0)).toContainText('Partie 1, Zug 19');
  await expect(items.nth(1)).toContainText(
    'Partie 2 beginnt mit einer gespeicherten Stellung',
  );
  await expect(items.nth(2)).toContainText('Partie 3 hat kein Ergebnis');
});

test('batch import results list warnings per file', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page
    .getByLabel('Match file', { exact: true })
    .setInputFiles([file('first.sgf'), file('second.sgf')]);
  const results = page.locator('.import-results > ul > li');
  await expect(results).toHaveCount(2);
  for (const result of await results.all())
    await expect(result.locator('.import-warnings li')).toHaveCount(3);
  await expect(results.first()).toContainText(
    'Game 1, move 19: the file marks this move as illegal.',
  );
});
