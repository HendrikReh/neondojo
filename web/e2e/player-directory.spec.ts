import { expect, test } from './test';

test('edits an imported player profile and opens its match', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  await page.setViewportSize({ width: 1800, height: 1169 });
  const alias = `Spieler ${crypto.randomUUID()}`;
  const imported = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'player.sgf',
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(
          `(;GM[6]PW[${alias}]PB[Gegner]MI[length:7];W[31hefe])`,
        ),
      },
    },
  });
  expect(imported.status()).toBe(201);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Spielerdatenbank' }).click();
  await page.getByRole('searchbox', { name: 'Spieler suchen' }).fill(alias);
  await page.getByRole('button', { name: new RegExp(alias) }).click();
  await expect(page.getByText('Keine PR verfügbar.')).toBeVisible();
  await page.getByRole('textbox', { name: 'Vorname' }).fill('Anna');
  await page.getByRole('textbox', { name: 'Nachname' }).fill('Beispiel');
  await page.getByRole('button', { name: 'Namen speichern' }).click();
  await expect(
    page.getByRole('heading', { name: 'Anna Beispiel' }),
  ).toBeVisible();
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(page.locator('.player-card')).toBeVisible();
    expect(
      await page
        .locator('.player-directory')
        .evaluate((element) => element.scrollWidth <= element.clientWidth),
    ).toBe(true);
    await page.locator('.player-directory').screenshot({
      path: test.info().outputPath(`player-directory-${width}.png`),
    });
  }
  await page
    .getByRole('button', { name: `${alias} – Gegner`, exact: true })
    .click();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
});
