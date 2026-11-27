import { seedLocale } from '../helpers/navigation';
import { readFileSync } from 'node:fs';
import { expect, test } from '../helpers/test';

// A real MAT import through GNUbg: two faulty moves end their games, and the
// public 1988 transcript already contains a header score that contradicts the
// previous result. The backend tests cover where each note comes from; this
// checks where the user meets them.
function faultyTranscript() {
  let text = readFileSync('../backend/tests/fixtures/WC1988-final.txt', 'utf8');
  for (const [from, to] of [
    ['  3) 21: 24/21 ', '  3) 21: 24/20 '],
    ['  2) 41: 24/20 21/20 ', '  2) 41: 24/20 xx/20 '],
  ]) {
    expect(text.split(from)).toHaveLength(2);
    text = text.replace(from, to);
  }
  // A distinct file per run, so the import is never a duplicate.
  return Buffer.from(`; [Note "${crypto.randomUUID()}"]\r\n${text}`);
}

test('MAT import notes appear in the notice, notes, header and library', async ({
  page,
}) => {
  test.setTimeout(90_000);
  await seedLocale(page, 'de');
  await page.goto('/');
  await page.getByLabel('Matchdatei', { exact: true }).setInputFiles({
    name: 'wc1988-fehler.txt',
    mimeType: 'text/plain',
    buffer: faultyTranscript(),
  });
  const notice = page.locator('.notice[role="status"] .import-warnings li');
  await expect(notice).toHaveCount(3, { timeout: 60_000 });
  // The added comment line shifts every source line by one.
  await expect(notice.nth(0)).toHaveText(
    'Partie 2, Zug 3 (Zeile 50): „21: 24/20“ ist kein legaler Zug. Die Partie endet davor.',
  );
  await expect(notice.nth(1)).toHaveText(
    'Partie 3, Zug 2 (Zeile 58): „41: 24/20 xx/20“ ist nicht lesbar. Die Partie endet davor.',
  );
  await expect(notice.nth(2)).toHaveText(
    'Partie 11 (Zeile 237) beginnt bei 12:8, das vorige Ergebnis ergibt aber 12:6. Verwendet wird der aufgezeichnete Stand.',
  );

  await expect(page.locator('.board-heading')).toContainText('Spiel 1');
  const headerNotes = page.locator('[data-match-field="import-notes"]');
  await expect(headerNotes).toContainText('Importhinweise');
  await expect(headerNotes.locator('dd')).toHaveText('3');

  await page.getByRole('tab', { name: 'Notizen', exact: true }).click();
  await page.getByRole('tab', { name: 'Match', exact: true }).click();
  const notes = page.locator('.notes-panel .import-notes');
  await expect(
    notes.getByRole('heading', { name: 'Importhinweise' }),
  ).toBeVisible();
  await expect(notes.locator('li')).toHaveCount(3);
  // The user's own comment stays a separate, empty editor.
  await expect(
    page.locator('.annotation-panel').getByLabel('Kommentar', { exact: true }),
  ).toHaveValue('');

  const picker = page.getByLabel('Spiel auswählen', { exact: true });
  await picker.selectOption('2');
  await expect(page.locator('.board-heading')).toContainText('Spiel 2');
  await page.getByRole('tab', { name: 'Spiel', exact: true }).click();
  await expect(notes.locator('li')).toHaveCount(1);
  await expect(notes.locator('li')).toContainText('Partie 2, Zug 3');
  await picker.selectOption('4');
  await expect(page.locator('.board-heading')).toContainText('Spiel 4');
  await expect(notes).toHaveCount(0);

  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  const row = page
    .locator('.database-table tbody tr')
    .filter({ hasText: 'Ray Glaeser' })
    .first();
  await expect(
    row.getByRole('img', { name: 'Importhinweise: 3' }).first(),
  ).toBeVisible();
});
