import { expect, test } from './test';

test('match suggestions preview boards and add only the checked cards', async ({
  page,
}, testInfo) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'de'));
  await page.goto('/');
  const importing = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/imports' &&
      response.request().method() === 'POST',
  );
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  const { match } = await (await importing).json();
  const panel = page.getByRole('tabpanel', { name: 'Lernkarten', exact: true });
  await page.getByRole('tab', { name: 'Lernkarten', exact: true }).click();
  await panel.getByLabel('Deckname').fill(`Vorschau ${crypto.randomUUID()}`);
  await panel
    .getByRole('button', { name: 'Deck erstellen', exact: true })
    .click();
  const suggest = panel.getByRole('button', {
    name: 'Positionen vorschlagen',
    exact: true,
  });
  await suggest.click();
  await expect(
    panel.getByText(
      'Für dieses Match ist noch keine abgeschlossene lokale Analyse verfügbar.',
    ),
  ).toBeVisible();

  const queued = await page.request.post(`/api/matches/${match.id}/analysis`, {
    data: { engine_id: 'gnubg', plies: 1 },
  });
  await expect(queued).toBeOK();
  const job = await queued.json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
      { timeout: 35_000 },
    )
    .toBe('completed');
  await suggest.click();
  const items = panel.getByRole('checkbox');
  await expect(items).toHaveCount(10);
  await expect(items.first()).toBeChecked();
  const previews = panel.locator('.card-suggestion-board').getByRole('img');
  await expect(previews).toHaveCount(10);
  const board = page.locator('.board-panel .board');
  const deck = panel.getByLabel('Deck auswählen', { exact: true });

  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(suggest).toBeVisible();
    await expect(board).toBeInViewport({ ratio: 0.99 });
    expect(
      await panel
        .locator('.card-suggestions')
        .evaluate((element) => element.scrollWidth <= element.clientWidth + 1),
    ).toBe(true);
    await page.screenshot({
      path: testInfo.outputPath(`card-suggestions-${width}.png`),
    });
  }

  expect((await board.boundingBox())!.width).toBeGreaterThan(1300);
  expect((await deck.boundingBox())!.width).toBeGreaterThan(600);
  expect((await previews.first().boundingBox())!.width).toBeGreaterThan(220);
  expect(
    (await panel.locator('.card-suggestion-list').boundingBox())!.height,
  ).toBeGreaterThan(750);

  for (const item of (await items.all()).slice(1)) await item.uncheck();
  const write = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname.endsWith('/card-suggestions') &&
      response.request().method() === 'POST',
  );
  await panel
    .getByRole('button', { name: 'Auswahl ins Deck übernehmen' })
    .click();
  const response = await write;
  expect(response.status()).toBe(200);
  expect((await response.json()).added).toBe(1);
  await expect(panel.getByText('1 Karte verarbeitet.')).toBeVisible();
  await suggest.click();
  await expect(panel.getByText('Bereits in diesem Deck')).toHaveCount(1);
  await expect(items.first()).not.toBeChecked();
});
