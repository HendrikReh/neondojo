import { seedLocale } from '../helpers/navigation';
import { test, expect, type Page } from '../helpers/test';
async function transfer(page: Page, filename?: string) {
  return page.evaluateHandle((name) => {
    const data = new DataTransfer();
    if (name)
      data.items.add(
        new File(['fixture match'], name, { type: 'application/octet-stream' }),
      );
    else data.setData('text/plain', 'a dragged note');
    return data;
  }, filename);
}
test.beforeEach(async ({ page }) => {
  await seedLocale(page, 'de');
  await page.goto('/');
});
test('drop zone aligns with both buttons, highlights nested file drags and resets on leave', async ({
  page,
}) => {
  const zone = page.locator('.match-drop-zone');
  await expect(zone).toBeVisible();
  await expect(zone).toContainText('Match-Dateien oder Ordner hier ablegen');
  await expect(zone).toContainText('MAT · TXT · XG · SGF · BGF');
  const buttons = page.locator('.welcome-actions > button');
  const first = await buttons.first().boundingBox(),
    last = await buttons.last().boundingBox(),
    box = await zone.boundingBox();
  expect(Math.abs(box!.x - first!.x)).toBeLessThan(0.5);
  expect(Math.abs(box!.x + box!.width - last!.x - last!.width)).toBeLessThan(
    0.5,
  );
  const data = await transfer(page, 'test.sgf');
  await zone.dispatchEvent('dragenter', { dataTransfer: data });
  await expect(zone).toHaveAttribute('data-drag-active', 'true');
  await expect(zone).toContainText('Zum Importieren loslassen');
  const icon = zone.locator('svg');
  await icon.dispatchEvent('dragenter', { dataTransfer: data });
  await icon.dispatchEvent('dragleave', { dataTransfer: data });
  await expect(zone).toHaveAttribute('data-drag-active', 'true');
  await zone.dispatchEvent('dragleave', { dataTransfer: data });
  await expect(zone).toHaveAttribute('data-drag-active', 'false');
  await zone.dispatchEvent('dragenter', { dataTransfer: data });
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await expect(zone).toHaveAttribute('data-drag-active', 'false');
});
// Drops do not branch on the extension; match-import.test.ts covers which
// extensions are accepted. One text and one binary format suffice here.
for (const extension of ['mat', 'xg'])
  test(`dropped ${extension} uses the existing importer once and clears highlighting`, async ({
    page,
  }) => {
    const writes: { body: string; databaseId: string | null }[] = [];
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    await page.route(/\/api\/imports(?:\?|$)/, async (route) => {
      writes.push({
        body: route.request().postDataBuffer()!.toString(),
        databaseId: new URL(route.request().url()).searchParams.get(
          'database_id',
        ),
      });
      await gate;
      await route.fulfill({
        status: 422,
        json: { error: 'Drop fixture rejected' },
      });
    });
    const zone = page.locator('.match-drop-zone'),
      data = await transfer(page, `fixture.${extension}`);
    try {
      await zone.dispatchEvent('dragenter', { dataTransfer: data });
      await zone.dispatchEvent('drop', { dataTransfer: data });
      await expect.poll(() => writes.length).toBe(1);
      expect(writes[0].databaseId).toBeNull();
      expect(writes[0].body).toContain(`filename="fixture.${extension}"`);
      await expect(zone).toHaveAttribute('data-drag-active', 'false');
      await expect(zone).toHaveAttribute('aria-busy', 'true');
      await zone.dispatchEvent('drop', { dataTransfer: data });
      expect(writes).toHaveLength(1);
      release();
      await expect(zone).toHaveAttribute('aria-busy', 'false');
      await expect(page.getByRole('alert')).toContainText(
        'Drop fixture rejected',
      );
      await expect(zone).toContainText(
        'Match-Dateien oder Ordner hier ablegen',
      );
    } finally {
      release();
    }
  });
test('text drags stay neutral and mobile layout stays inside the viewport', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const zone = page.locator('.match-drop-zone'),
    data = await transfer(page);
  await zone.dispatchEvent('dragenter', { dataTransfer: data });
  await zone.dispatchEvent('dragover', { dataTransfer: data });
  await expect(zone).toHaveAttribute('data-drag-active', 'false');
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(390);
});
