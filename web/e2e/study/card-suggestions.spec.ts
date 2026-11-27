import { waitForJobCompletion } from '../helpers/match-fixtures';
import { DESKTOP_VIEWPORTS, seedLocale } from '../helpers/navigation';
import { expect, test, type Page } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { defaultConfiguration } from '../../src/settings/configuration/configuration-model';

async function openCards(page: Page) {
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
  return { match, panel };
}

test('match suggestions preview boards and add only the checked cards', async ({
  page,
}, testInfo) => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.general.locale = 'de';
  configuration.analysis.error_thresholds.error = 0.033;
  const profiles = await profileBackend(page, { configuration });
  await seedLocale(page, 'de');
  const { match, panel } = await openCards(page);
  await panel.getByLabel('Deckname').fill(`Vorschau ${crypto.randomUUID()}`);
  await panel
    .getByRole('button', { name: 'Deck erstellen', exact: true })
    .click();
  const suggest = panel.getByRole('button', {
    name: 'Positionen vorschlagen',
    exact: true,
  });
  const minimum = panel.getByRole('spinbutton', { name: 'Mindestverlust' });
  await expect(minimum).toHaveValue('0,033');
  configuration.analysis.error_thresholds.error = 0.03;
  profiles.updateRemote(configuration);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(minimum).toHaveValue('0,03');
  const suggestionRequest = page.waitForRequest('**/card-suggestions?*');
  await suggest.click();
  expect(
    new URL((await suggestionRequest).url()).searchParams.get('min_loss'),
  ).toBe('0.03');
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
  await waitForJobCompletion(page, job.id, 35_000);
  await suggest.click();
  const items = panel.getByRole('checkbox');
  await expect(items).toHaveCount(10);
  await expect(items.first()).toBeChecked();
  const previews = panel
    .locator('.card-suggestion-board')
    .getByRole('img', { name: /^Spiel / });
  await expect(previews).toHaveCount(10);
  const board = page.locator('.board-panel .board');
  const deck = panel.getByLabel('Deck auswählen', { exact: true });

  for (const { width, height } of DESKTOP_VIEWPORTS) {
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

  await minimum.fill('0');
  await page.reload();
  await page.getByRole('tab', { name: 'Lernkarten', exact: true }).click();
  await expect(minimum).toHaveValue('0');
  const generation = page.waitForRequest(
    (request) =>
      request.method() === 'POST' &&
      /\/api\/decks\/[^/]+\/cards$/.test(new URL(request.url()).pathname),
  );
  await panel
    .getByRole('button', { name: 'Fehler aus Spiel 1 übernehmen' })
    .click();
  expect((await generation).postDataJSON()).toMatchObject({
    min_loss: 0,
    game_no: 1,
  });
});

test('Add Cards preserves a previous generation cutoff and an existing session entry', async ({
  page,
}) => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.general.locale = 'de';
  configuration.learning.generation.min_loss = 0.125;
  configuration.analysis.error_thresholds.error = 0.031;
  await profileBackend(page, { configuration });
  await seedLocale(page, 'de');
  const { panel } = await openCards(page);
  const minimum = panel.getByRole('spinbutton', { name: 'Mindestverlust' });
  await expect(minimum).toHaveValue('0,125');

  // A previous release could have saved the old 0.04 default in this tab.
  await page.evaluate(() =>
    sessionStorage.setItem(
      'neondojo.session.analysis.card-min-loss.v1',
      JSON.stringify('0.04'),
    ),
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Lernkarten', exact: true }).click();
  await expect(minimum).toHaveValue('0,04');
  await minimum.fill('');
  await page.reload();
  await page.getByRole('tab', { name: 'Lernkarten', exact: true }).click();
  await expect(minimum).toHaveValue('');
  await expect(
    panel.getByRole('button', { name: 'Positionen vorschlagen' }),
  ).toBeDisabled();
});
