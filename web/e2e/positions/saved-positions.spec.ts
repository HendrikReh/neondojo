import { listMatchPositions } from '../helpers/match-fixtures';
import {
  openPositionBrowser,
  openWorkspace,
  seedLocale,
} from '../helpers/navigation';
import { expect, test, type Locator, type Page } from '../helpers/test';

test.use({ actionTimeout: 10000 });
async function imported(page: Page, games = 1) {
  await seedLocale(page);
  await page.goto('/');
  const suffix = crypto.randomUUID();
  await openWorkspace(page, 'library');
  await page.getByRole('button', { name: 'New database', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByLabel('Name', { exact: true })
    .fill(`Saved study ${suffix}`);
  const created = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/databases' &&
      r.request().method() === 'POST',
  );
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Save', exact: true })
    .click();
  const database = await (await created).json();
  const response = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/imports' &&
      r.request().method() === 'POST',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: `saved-${suffix}.sgf`,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      Array.from(
        { length: games },
        () =>
          `(;GM[6]PW[Study ${suffix}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
      ).join(''),
    ),
  });
  const result = await (await response).json();
  const positions = await listMatchPositions(page, result.match.id);
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return { match: result.match, positions, database };
}
const openCollection = (page: Page, databaseId: string) =>
  openPositionBrowser(page, databaseId);

async function removeSelected(pane: Locator) {
  const actions = pane.locator('.position-selection-summary');
  await actions
    .getByRole('button', { name: 'More selection actions', exact: true })
    .click();
  await actions
    .getByRole('button', { name: 'Remove from collection', exact: true })
    .click();
}

test('imports stay out of the collection until the user deliberately saves a position', async ({
  page,
}) => {
  const f = await imported(page);
  const pane = await openCollection(page, f.database.id);
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(0);
  await expect(pane.locator('.position-empty')).toContainText(
    'No saved positions match these filters.',
  );
  await openWorkspace(page, 'analysis');
  await page
    .getByRole('button', { name: 'Save position', exact: true })
    .click();
  await expect(
    page
      .locator('.position-bookmark-control')
      .getByRole('button', { name: 'Remove from collection', exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
  await openCollection(page, f.database.id);
  const rows = pane.locator('.position-table tbody tr');
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toHaveAttribute(
    'data-position-id',
    f.positions[0].id,
  );
  await expect(pane.locator('.position-preview .board')).toBeVisible();
  await expect(rows.locator('.position-player')).toHaveText(f.match.white);
  await page.reload();
  await openCollection(page, f.database.id);
  await expect(rows).toHaveCount(1);
  await rows.first().locator('.position-player').click();
  await pane
    .locator('.position-preview')
    .getByRole('button', { name: 'Open analysis', exact: true })
    .click();
  await expect(
    page
      .locator('.position-bookmark-control')
      .getByRole('button', { name: 'Remove from collection', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(
    page
      .locator('.position-bookmark-control')
      .getByRole('button', { name: 'Save position', exact: true }),
  ).toBeVisible();
  await openCollection(page, f.database.id);
  await rows
    .first()
    .getByRole('button', { name: 'Remove from collection', exact: true })
    .click();
  await expect(rows).toHaveCount(0);
  const position = await page.request.get(
    `/api/positions/${f.positions[0].id}`,
  );
  expect(position.ok()).toBe(true);
  const match = await page.request.get(`/api/matches/${f.match.id}`);
  expect(match.ok()).toBe(true);
  await openWorkspace(page, 'analysis');
  await page
    .getByRole('button', { name: 'Previous move', exact: true })
    .click();
  await expect(
    page
      .locator('.position-bookmark-control')
      .getByRole('button', { name: 'Save position', exact: true }),
  ).toBeVisible();
});

test('undo puts removed positions back into the collection', async ({
  page,
}) => {
  const f = await imported(page);
  for (const position of f.positions) {
    expect(
      (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
    ).toBe(true);
  }
  const pane = await openCollection(page, f.database.id),
    rows = pane.locator('.position-table tbody tr'),
    notice = pane.locator('.position-undo-notice');
  await expect(rows).toHaveCount(2);
  await expect(notice).toHaveCount(0);
  await rows
    .first()
    .getByRole('button', { name: 'Remove from collection', exact: true })
    .click();
  await expect(rows).toHaveCount(1);
  await expect(notice).toContainText('Removed from collection: 1');
  await notice.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(rows).toHaveCount(2);
  await expect(notice).toHaveCount(0);
  await pane.getByLabel('Select this page', { exact: true }).check();
  const tableTop = (await pane.locator('.position-table').boundingBox())!.y;
  await removeSelected(pane);
  await expect(rows).toHaveCount(0);
  await expect(notice).toContainText('Removed from collection: 2');
  await notice.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(rows).toHaveCount(2);
  // Selecting rows shows its actions in the toolbar without moving the table.
  await pane.getByLabel('Select this page', { exact: true }).uncheck();
  expect((await pane.locator('.position-table').boundingBox())!.y).toBe(
    tableTop,
  );
});

test('selected removal retains original notes and permits saving the same position again', async ({
  page,
}) => {
  const f = await imported(page);
  for (const position of f.positions) {
    expect(
      (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
    ).toBe(true);
  }
  expect(
    (
      await page.request.put(`/api/positions/${f.positions[0].id}/annotation`, {
        data: {
          expected_version: 0,
          tags: ['study-later'],
          comment: 'Keep this explanation',
        },
      })
    ).ok(),
  ).toBe(true);
  const pane = await openCollection(page, f.database.id),
    rows = pane.locator('.position-table tbody tr');
  await expect(rows).toHaveCount(2);
  await pane.getByLabel('Select this page', { exact: true }).check();
  await removeSelected(pane);
  await expect(rows).toHaveCount(0);
  const annotation = await (
    await page.request.get(`/api/positions/${f.positions[0].id}/annotation`)
  ).json();
  expect(annotation.comment).toBe('Keep this explanation');
  expect(annotation.tags).toContain('study-later');
  await openWorkspace(page, 'analysis');
  await expect(
    page
      .locator('.position-bookmark-control')
      .getByRole('button', { name: 'Save position', exact: true }),
  ).toBeVisible();
  await page
    .getByRole('button', { name: 'Save position', exact: true })
    .click();
  await openCollection(page, f.database.id);
  await expect(rows).toHaveCount(1);
  // Notes show below the board without opening the source details.
  await expect(
    pane.locator('.position-preview .position-hit-notes'),
  ).toContainText('Keep this explanation');
});

test('partial removal keeps failed rows selected and makes a retry possible', async ({
  page,
}) => {
  const f = await imported(page);
  for (const position of f.positions) {
    expect(
      (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
    ).toBe(true);
  }
  let fail = true;
  await page.route(`**/api/positions/${f.positions[1].id}/bookmark`, (route) =>
    route.request().method() === 'DELETE' && fail
      ? route.fulfill({
          status: 503,
          json: { error: 'Temporary write failure' },
        })
      : route.continue(),
  );
  const pane = await openCollection(page, f.database.id),
    rows = pane.locator('.position-table tbody tr');
  await expect(rows).toHaveCount(2);
  await pane.getByLabel('Select this page', { exact: true }).check();
  await removeSelected(pane);
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toHaveAttribute(
    'data-position-id',
    f.positions[1].id,
  );
  await expect(rows.first().getByRole('checkbox')).toBeChecked();
  await expect(pane.getByRole('alert')).toContainText(
    'Some positions could not be removed',
  );
  fail = false;
  await removeSelected(pane);
  await expect(rows).toHaveCount(0);
  await expect(pane.getByRole('alert')).toHaveCount(0);
});

test('failed removal on a later loaded page retains selection for immediate retry', async ({
  page,
}) => {
  const f = await imported(page, 30);
  expect(f.positions).toHaveLength(60);
  for (const position of f.positions)
    expect(
      (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
    ).toBe(true);
  const pane = await openCollection(page, f.database.id),
    rows = pane.locator('.position-table tbody tr');
  await expect(rows).toHaveCount(25);
  await rows.first().getByRole('checkbox').check();
  await pane.getByRole('button', { name: 'Next page', exact: true }).click();
  await expect(
    pane.getByRole('button', { name: 'Page 2', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
  await pane.getByRole('button', { name: 'Next page', exact: true }).click();
  await expect(rows).toHaveCount(10);
  const failed = await rows.first().getAttribute('data-position-id');
  await rows.first().getByRole('checkbox').check();
  let fail = true;
  await page.route(`**/api/positions/${failed}/bookmark`, (route) =>
    route.request().method() === 'DELETE' && fail
      ? route.fulfill({
          status: 503,
          json: { error: 'Temporary later-page failure' },
        })
      : route.continue(),
  );
  await removeSelected(pane);
  await expect(pane.getByRole('alert')).toContainText(
    'Some positions could not be removed',
  );
  await expect(pane.locator('.position-selection-summary strong')).toHaveText(
    '1 selected',
  );
  await expect(pane.locator('.position-browser-results')).toHaveAttribute(
    'aria-busy',
    'false',
  );
  // The failed item remains selected while the current page refreshes.
  await expect(pane.getByText('59 positions', { exact: true })).toBeVisible();
  await expect(pane.locator('.position-selection-summary strong')).toHaveText(
    '1 selected',
  );
  fail = false;
  await removeSelected(pane);
  await expect(pane.locator('.position-selection-summary')).toBeHidden();
  await expect(pane.getByRole('alert')).toHaveCount(0);
  expect(
    (await (await page.request.get(`/api/positions/${failed}/bookmark`)).json())
      .saved,
  ).toBe(false);
});
