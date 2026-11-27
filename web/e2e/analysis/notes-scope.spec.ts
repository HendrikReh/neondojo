import { seedLocale } from '../helpers/navigation';
import { expect, test, type Page } from '../helpers/test';
import { closeSettings } from '../helpers/settings-actions';
import type { Locale } from '../../src/shared/i18n/i18n-core';

test.beforeEach(async ({ page }) => {
  await seedLocale(page, 'de', { preserveLocale: true });
});

async function importNotesMatch(page: Page) {
  const marker = crypto.randomUUID();
  await page.goto('/');
  await page.locator('input[type=file][accept*=".mat"]').setInputFiles({
    name: `notes-scope-${marker}.sgf`,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Notes ${marker}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31xufe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByRole('tab', { name: 'Notizen', exact: true }).click();
  return marker;
}

test('one notes editor retains separate drafts while changing scope and position', async ({
  page,
}) => {
  const marker = await importNotesMatch(page);
  const editor = page.locator('.annotation-panel');
  const comment = editor.getByLabel('Kommentar', { exact: true });
  const tagInput = editor.getByLabel('Tags', { exact: true });
  const matchTab = page.getByRole('tab', { name: 'Match', exact: true });
  const positionTab = page.getByRole('tab', { name: 'Position', exact: true });

  await expect(editor).toHaveCount(1);
  await expect(positionTab).toHaveAttribute('aria-selected', 'true');
  await expect(comment).toBeEnabled();
  const firstPositionId = await editor.getAttribute('data-annotation-id');
  await comment.fill(`position-draft-${marker}`);
  await tagInput.fill('Position tag draft');

  await matchTab.click();
  await expect(editor).toHaveCount(1);
  await expect(editor).toHaveAttribute('data-annotation-kind', 'match');
  await expect(comment).toBeEnabled();
  const matchId = await editor.getAttribute('data-annotation-id');
  await expect(comment).toHaveValue('');
  await comment.fill(`match-draft-${marker}`);
  await tagInput.fill('Match tag draft');

  await page.getByRole('button', { name: 'Nächster Zug', exact: true }).click();
  await expect(matchTab).toHaveAttribute('aria-selected', 'true');
  await expect(comment).toHaveValue(`match-draft-${marker}`);
  await positionTab.click();
  await expect(comment).toBeEnabled();
  await expect(comment).toHaveValue('');
  await expect(tagInput).toHaveValue('');
  await expect(editor).not.toHaveAttribute(
    'data-annotation-id',
    firstPositionId!,
  );
  await comment.fill('Second position draft');

  await page
    .getByRole('button', { name: 'Vorheriger Zug', exact: true })
    .click();
  await expect(positionTab).toHaveAttribute('aria-selected', 'true');
  await expect(comment).toBeEnabled();
  await expect(comment).toHaveValue(`position-draft-${marker}`);
  await expect(tagInput).toHaveValue('Position tag draft');
  await editor
    .getByRole('button', { name: 'Notizen speichern', exact: true })
    .click();
  await expect(
    editor.getByText('Notizen gespeichert.', { exact: true }),
  ).toBeVisible();

  const savedPosition = await (
    await page.request.get(`/api/positions/${firstPositionId}/annotation`)
  ).json();
  expect(savedPosition.comment).toBe(`position-draft-${marker}`);
  expect(savedPosition.tags).toEqual(['position tag draft']);
  const unsavedMatch = await (
    await page.request.get(`/api/matches/${matchId}/annotation`)
  ).json();
  expect(unsavedMatch.comment).toBe('');

  await matchTab.click();
  await expect(comment).toBeEnabled();
  await expect(comment).toHaveValue(`match-draft-${marker}`);
  await expect(tagInput).toHaveValue('Match tag draft');
  await editor
    .getByRole('button', { name: 'Notizen speichern', exact: true })
    .click();
  await expect(
    editor.getByText('Notizen gespeichert.', { exact: true }),
  ).toBeVisible();
  const savedMatch = await (
    await page.request.get(`/api/matches/${matchId}/annotation`)
  ).json();
  expect(savedMatch.comment).toBe(`match-draft-${marker}`);
  expect(savedMatch.tags).toEqual(['match tag draft']);

  await positionTab.click();
  await expect(comment).toBeEnabled();
  await expect(comment).toHaveValue(`position-draft-${marker}`);
  await page.getByRole('button', { name: 'Nächster Zug', exact: true }).click();
  await expect(comment).toBeEnabled();
  await expect(comment).toHaveValue('Second position draft');
  await expect(editor).toHaveCount(1);
});

test('notes tabs support keyboard navigation and localized scope labels', async ({
  page,
}) => {
  await importNotesMatch(page);
  const tablist = page.locator('.notes-scope-tabs');
  const matchTab = page.getByRole('tab', { name: 'Match', exact: true });
  const positionTab = page.getByRole('tab', { name: 'Position', exact: true });
  await expect(positionTab).toBeVisible();
  await positionTab.focus();
  await expect(
    page.getByRole('tab', { name: 'Spiel', exact: true }),
  ).toBeEnabled();
  await page.keyboard.press('ArrowLeft');
  await expect(
    page.getByRole('tab', { name: 'Spiel', exact: true }),
  ).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(matchTab).toBeFocused();
  await expect(matchTab).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('End');
  await expect(positionTab).toBeFocused();
  await expect(positionTab).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Home');
  await expect(matchTab).toBeFocused();
  const labels = [
    ['en', 'Match', 'Position'],
    ['de', 'Match', 'Position'],
  ];
  for (const [locale, match, position] of labels) {
    await page.locator('.settings-trigger').click();
    const dialog = page.getByRole('dialog');
    await dialog.locator('select').first().selectOption(locale);
    await dialog.locator('button[type=submit]').click();
    // Apply keeps Settings open; Close is already labeled in the new language.
    await closeSettings(dialog, locale as Locale);
    await expect(
      tablist.getByRole('tab', { name: match, exact: true }),
    ).toBeVisible();
    await expect(
      tablist.getByRole('tab', { name: position, exact: true }),
    ).toBeVisible();
    await expect(
      page.locator('.notes-panel').getByRole('tabpanel'),
    ).toHaveCount(1);
    await expect(page.locator('.annotation-panel')).toHaveCount(1);
  }
});
