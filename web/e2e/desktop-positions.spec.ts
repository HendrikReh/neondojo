import { expect, test, type Page } from './test';
import {
  expectCompactPositionRow,
  showPositionColumns,
} from './position-row-layout';

test.use({ actionTimeout: 10000 });

const widthsKey = 'neondojo.positions.columns';
const libraryKey = 'neondojo.library.columns';
async function fixture(page: Page, width = 1800, height = 1169) {
  await page.setViewportSize({ width, height });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  const database = await (
    await page.request.post('/api/databases', {
      data: { name: `Mochy research ${crypto.randomUUID()}` },
    })
  ).json();
  const records = [];
  for (const [white, black, turn] of [
    ['Mochy', 'Opponent A', 'W'],
    ['Opponent B', 'Mochy', 'B'],
    ['Mochy', 'Opponent C', 'B'],
    ['Opponent D', 'Mochy', 'W'],
  ]) {
    const source = `(;GM[6]PW[${white}]PB[${black}]MI[length:7][ws:0][bs:0];${turn}[31${turn === 'W' ? 'hefe' : 'qtst'}])`;
    const response = await page.request.post(
      `/api/imports?database_id=${database.id}`,
      {
        multipart: {
          file: {
            name: `${white}-${black}.sgf`,
            mimeType: 'application/octet-stream',
            buffer: Buffer.from(source),
          },
        },
      },
    );
    expect(response.status()).toBe(201);
    const imported = await response.json();
    const positions = await (
      await page.request.get(`/api/positions?match_id=${imported.match.id}`)
    ).json();
    expect(
      (
        await page.request.put(
          `/api/positions/${positions.items[0].id}/bookmark`,
        )
      ).ok(),
    ).toBe(true);
    records.push(positions.items[0]);
  }
  await page.goto('/');
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  const pane = page.getByRole('tabpanel', { name: 'Positions', exact: true });
  await pane
    .getByRole('combobox', { name: 'Database', exact: true })
    .selectOption(database.id);
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  const rows = pane.locator('.position-table tbody tr');
  await expect(rows).toHaveCount(4);
  return { pane, rows, database, records };
}
const column = (page: Page, id: string) =>
  page.locator(`.position-table th[data-column="${id}"]`);
const widthOf = async (page: Page, id: string) =>
  (await column(page, id).boundingBox())!.width;

test('compact position rows retain a right-hand board preview at desktop sizes', async ({
  page,
}) => {
  const { pane, rows, records } = await fixture(page);
  await showPositionColumns(pane, 'Tags / Comment');
  await expect(
    pane.getByRole('textbox', { name: 'Player on roll', exact: true }),
  ).toBeVisible();
  await expect(rows.first().locator('[title]')).toHaveCount(0);
  await expect(
    rows.getByRole('button', { name: 'Open analysis', exact: true }),
  ).toHaveCount(0);
  await expect(
    pane
      .locator('.position-preview')
      .getByRole('button', { name: 'Open analysis', exact: true }),
  ).toBeVisible();
  await rows.first().focus();
  await expect(page.getByRole('tooltip')).toContainText('Player on roll:');
  await expect(page.getByRole('tooltip')).not.toContainText('Played move:');
  await page.keyboard.press('Escape');
  await expect(page.getByRole('tooltip')).toHaveCount(0);
  await expectCompactPositionRow(rows.first());
  await expect(pane.locator('.position-played-move')).toHaveCount(0);
  // Select by the exact stored occurrence, rather than by a duplicate board ID.
  const second = pane.locator(`tr[data-position-id="${records[1].id}"]`);
  await second.locator('.position-player').click();
  await expect(pane.locator('.position-preview-players')).toContainText(
    'Player on roll: Mochy',
  );
  await expect(pane.locator('.position-preview-players')).toContainText(
    'Opponent: Opponent B',
  );
  await expect(second).toHaveClass(/position-preview-selected/);
  await expect(pane.locator('.position-preview .board')).toBeVisible();
  await expect(pane.locator('.position-preview-played')).toHaveCount(0);
  await second.focus();
  await page.keyboard.press('ArrowDown');
  const focused = pane.locator('.position-table tr:focus');
  await expect(focused).toHaveClass(/position-preview-selected/);
  for (const [width, height] of [
    [1440, 1100],
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expectCompactPositionRow(rows.first());
    await expectCompactPositionRow(rows.last());
    expect(
      (await pane.locator('th[data-column="notes"]').boundingBox())!.width,
    ).toBeGreaterThanOrEqual(120);
    const preview = (await pane.locator('.position-preview').boundingBox())!;
    const table = (await pane.locator('.position-table-scroll').boundingBox())!;
    expect(preview.x).toBeGreaterThanOrEqual(table.x + table.width);
    expect(preview.x + preview.width).toBeLessThanOrEqual(width);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.screenshot({
    path: '/tmp/neondojo-desktop-positions-1800.png',
    fullPage: true,
  });
  const positionPanel = page.locator('#workspace-panel-positions');
  const previewBoard = positionPanel.locator('.position-preview .board');
  const selectedRow = positionPanel.locator('tr.position-preview-selected');
  const selectedId = await selectedRow.getAttribute('data-position-id');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await expect(previewBoard).toHaveCount(0);
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await expect(previewBoard).toBeVisible();
  await expect(selectedRow).toHaveAttribute('data-position-id', selectedId!);
});

test('position widths persist independently and optional played-move widths survive toggling', async ({
  page,
}) => {
  const { pane } = await fixture(page);
  await page.evaluate(
    (key) => localStorage.setItem(key, 'match-layout-sentinel'),
    libraryKey,
  );
  const initial = await widthOf(page, 'player');
  const handle = column(page, 'player').getByRole('separator');
  await handle.click();
  expect(
    await page.evaluate((key) => localStorage.getItem(key), widthsKey),
  ).toBeNull();
  const rect = (await handle.boundingBox())!;
  await page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2);
  await page.mouse.down();
  await page.mouse.move(
    rect.x + rect.width / 2 + 90,
    rect.y + rect.height / 2,
    { steps: 8 },
  );
  await page.mouse.up();
  await expect
    .poll(() => widthOf(page, 'player'))
    .toBeCloseTo(Math.round(initial) + 90, 0);
  expect(
    await page.evaluate((key) => localStorage.getItem(key), libraryKey),
  ).toBe('match-layout-sentinel');
  await pane.getByLabel('Show played moves', { exact: true }).check();
  await expect(pane.locator('.position-played-move')).toHaveCount(4);
  await column(page, 'played_move').getByRole('separator').focus();
  await page.keyboard.press('Shift+ArrowRight');
  const playedWidth = await widthOf(page, 'played_move');
  await pane.getByLabel('Show played moves', { exact: true }).uncheck();
  await column(page, 'player').getByRole('separator').focus();
  await page.keyboard.press('ArrowRight');
  await page.reload();
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await expect
    .poll(() => widthOf(page, 'player'))
    .toBeCloseTo(Math.round(initial) + 100, 0);
  await expect(
    pane.getByLabel('Show played moves', { exact: true }),
  ).not.toBeChecked();
  await pane.getByLabel('Show played moves', { exact: true }).check();
  await expect
    .poll(() => widthOf(page, 'played_move'))
    .toBeCloseTo(playedWidth, 0);
  await pane
    .getByRole('button', { name: 'Reset column widths', exact: true })
    .click();
  expect(
    await page.evaluate((key) => localStorage.getItem(key), widthsKey),
  ).toBeNull();
  expect(
    await page.evaluate((key) => localStorage.getItem(key), libraryKey),
  ).toBe('match-layout-sentinel');
});

test('Mochy research includes both colors, excludes opponents, and saves header sorting', async ({
  page,
}) => {
  const { pane, rows, records } = await fixture(page);
  await showPositionColumns(pane, 'Opponent');
  await pane
    .getByRole('textbox', { name: 'Player on roll', exact: true })
    .fill('mochy');
  await pane.locator('.position-filter-details summary').click();
  await pane
    .getByLabel('Checker position ID', { exact: true })
    .fill(records[0].position_id);
  await pane.getByLabel('First die', { exact: true }).fill('3');
  await pane.getByLabel('Second die', { exact: true }).fill('1');
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  await expect(rows).toHaveCount(2);
  await expect(rows.locator('.position-player')).toHaveText(['Mochy', 'Mochy']);
  await expect(rows.locator('.position-opponent')).toHaveText([
    'Opponent A',
    'Opponent B',
  ]);
  await pane.locator('button[data-sort="opponent"]').click();
  await expect(column(page, 'opponent')).toHaveAttribute(
    'aria-sort',
    'ascending',
  );
  await pane.locator('button[data-sort="opponent"]').click();
  await expect(column(page, 'opponent')).toHaveAttribute(
    'aria-sort',
    'descending',
  );
  await expect(rows.locator('.position-opponent')).toHaveText([
    'Opponent B',
    'Opponent A',
  ]);
  await pane.getByLabel('Show played moves', { exact: true }).check();
  await expect(rows.first().locator('.position-played-move')).toHaveText(
    records[1].notation,
  );
  await rows.first().locator('.position-player').click();
  await expect(pane.locator('.position-preview-played')).toContainText(
    records[1].notation,
  );
  await pane
    .getByRole('button', { name: 'Saved search actions', exact: true })
    .click();
  await pane
    .getByRole('button', { name: 'Save current search…', exact: true })
    .click();
  const saveDialog = page.getByRole('dialog', {
    name: 'Save search',
    exact: true,
  });
  await saveDialog
    .getByLabel('Search name', { exact: true })
    .fill(`Mochy ${crypto.randomUUID()}`);
  const savedResponse = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/position-filters' &&
      r.request().method() === 'POST',
  );
  await saveDialog
    .getByRole('button', { name: 'Save as new search', exact: true })
    .click();
  const saved = await (await savedResponse).json();
  expect(saved.definition).toMatchObject({
    player: 'mochy',
    sort: 'opponent',
    direction: 'desc',
    position_id: records[0].position_id,
    die1: 1,
    die2: 3,
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await pane
    .getByRole('combobox', { name: 'Choose saved search', exact: true })
    .selectOption(saved.id);
  await expect(rows.locator('.position-opponent')).toHaveText([
    'Opponent B',
    'Opponent A',
  ]);
  await expect(column(page, 'opponent')).toHaveAttribute(
    'aria-sort',
    'descending',
  );
  await expect(
    pane.getByLabel('Show played moves', { exact: true }),
  ).toBeChecked();
  await rows.first().focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('.board-heading')).toContainText('Mochy');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
});
