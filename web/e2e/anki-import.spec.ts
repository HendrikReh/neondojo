import { execFile } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { dirname } from 'node:path';
import { promisify } from 'node:util';
import { expect, test } from './test';

const run = promisify(execFile);

test('imports an external Anki package and keeps reviews on reimport', async ({
  page,
}, info) => {
  const python = process.env.BGTUTOR_ANKI_PYTHON;
  test.skip(!python, 'Set BGTUTOR_ANKI_PYTHON to an interpreter with Anki');
  if (!python) return;
  const fixture = info.outputPath('external.apkg');
  await mkdir(dirname(fixture), { recursive: true });
  await run(python, [
    '-I',
    '-c',
    `
import base64
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
from anki.collection import Collection, ExportAnkiPackageOptions
from anki.lang import set_lang

set_lang('en')
with TemporaryDirectory() as temporary:
    collection = Collection(str(Path(temporary) / 'collection.anki2'))
    try:
        deck = collection.decks.id('External study::Cube')
        note = collection.new_note(collection.models.by_name('Basic'))
        pixel = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==')
        image = collection.media.write_data('diagram.png', pixel)
        note['Front'] = f'External practice question<img src="{image}">'
        note['Back'] = 'External practice answer'
        note.tags = ['Cube::Reference']
        collection.add_note(note, deck)
        collection.export_anki_package(out_path=sys.argv[1], options=ExportAnkiPackageOptions(legacy=False, with_media=True), limit=None)
    finally:
        collection.close()
`,
    fixture,
  ]);
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('tabpanel', { name: 'Flashcards & Anki' });
  const open = study.getByRole('button', {
    name: 'Import Anki package',
    exact: true,
  });
  async function upload() {
    const chooser = page.waitForEvent('filechooser');
    await open.click();
    await (await chooser).setFiles(fixture);
    await expect(
      study
        .getByRole('status')
        .filter({ hasText: 'Cards imported 1 · Decks 1' }),
    ).toBeVisible();
  }
  await upload();
  const decks = await (await page.request.get('/api/decks')).json();
  const deck = decks.find(
    (item: { name: string }) => item.name === 'External study::Cube',
  );
  expect(deck).toBeTruthy();
  const cardsUrl = `/api/decks/${deck.id}/cards`;
  const {
    items: [card],
  } = await (await page.request.get(cardsUrl)).json();
  expect(card).toMatchObject({ kind: 'anki', review_count: 0 });
  expect(card.browse.card_tags).toContain('cube::reference');
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deck.id);
  await study.getByRole('button', { name: 'Study now', exact: true }).click();
  await expect(
    study.frameLocator('iframe').getByText('External practice question'),
  ).toBeVisible();
  await expect
    .poll(() =>
      study
        .frameLocator('iframe')
        .locator('img')
        .evaluate((image) => (image as HTMLImageElement).naturalWidth),
    )
    .toBe(1);
  const revealed = page.waitForResponse(
    (response) => new URL(response.url()).pathname.endsWith('/reveal'),
    { timeout: 8_000 },
  );
  await study.getByRole('button', { name: 'Show answer', exact: true }).click();
  expect((await revealed).status()).toBe(200);
  await expect(
    study.frameLocator('iframe').getByText('External practice answer'),
  ).toBeVisible();
  await study.getByRole('button', { name: 'Good', exact: true }).click();
  await expect(
    study.getByText('No cards are due in this deck right now.'),
  ).toBeVisible();
  await study
    .getByRole('button', { name: 'Back to card search', exact: true })
    .click();
  await upload();
  const { items } = await (await page.request.get(cardsUrl)).json();
  expect(items).toHaveLength(1);
  expect(items[0]).toMatchObject({ id: card.id, review_count: 1 });
});
