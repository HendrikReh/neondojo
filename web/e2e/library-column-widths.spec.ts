import { fulfillJson } from './api-fixtures';
import { expect, test, type Page } from './test';

const localWidths = (page: Page) =>
  page.evaluate(() => JSON.parse(localStorage.getItem('neondojo.layout.library.columns.v1') ?? 'null'));

async function openLibrary(page: Page, width = 1800, height = 1169) {
  await page.setViewportSize({ width, height });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.route('**/api/library/matches?*', (route) =>
    fulfillJson<'LibraryPage'>(route, {
      items: [
        {
          id: '00000000-0000-0000-0000-000000000007',
          match_no: 7,
          database_id: '00000000-0000-0000-0000-000000000001',
          filename: 'resize.sgf',
          display_name: 'resize.sgf',
          format: 'sgf',
          white: 'Resize Alice',
          black: 'Resize Bob',
          match_length: 7,
          position_count: 460,
          game_count: 10,
          created_at: '2026-11-09T12:00:00Z',
          played_on: null,
          tournament: null,
          round: null,
          place: null,
          result_state: 'completed',
          white_score: 7,
          black_score: 5,
          white_pr: 3.2,
          black_pr: 4.2,
          pr_method: null,
          pr_profile: null,
          analysis_status: 'imported',
          import_warning_count: 0,
          play_assisted: false,
          play_paused: false,
          source_available: true,
          tags: ['Study'],
          comment: 'Review the cube decision',
          version: 0,
        },
      ],
      next_cursor: null,
    }),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(1);
}
const column = (page: Page, id: string) =>
  page.locator(`.database-table th[data-column="${id}"]`);
const widthOf = async (page: Page, id: string) =>
  (await column(page, id).boundingBox())!.width;
async function drag(page: Page, id: string, delta: number) {
  const handle = column(page, id).getByRole('separator');
  await expect(handle).toBeVisible();
  const rect = (await handle.boundingBox())!;
  const x = rect.x + rect.width / 2,
    y = rect.y + rect.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + delta, y, { steps: 8 });
}

test('drag resizes only one column without sorting and persists across reload', async ({
  page,
}) => {
  await openLibrary(page);
  await expect(column(page, 'white').getByRole('separator')).toBeVisible();
  await column(page, 'white').getByRole('separator').click();
  await expect.poll(() => localWidths(page)).toBeNull();
  const initial = await widthOf(page, 'white'),
    black = await widthOf(page, 'black');
  await drag(page, 'white', 100);
  await page.mouse.up();
  await expect
    .poll(() => widthOf(page, 'white'))
    .toBeCloseTo(Math.round(initial) + 100, 0);
  expect(Math.abs((await widthOf(page, 'black')) - black)).toBeLessThanOrEqual(
    1,
  );
  await expect(page.locator('button[data-sort="created_at"]')).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(page.locator('button[data-sort="white"]')).toHaveAttribute(
    'aria-pressed',
    'false',
  );
  await expect.poll(() => localWidths(page)).not.toBeNull();
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect
    .poll(() => widthOf(page, 'white'))
    .toBeCloseTo(Math.round(initial) + 100, 0);
  await page
    .getByRole('button', { name: 'Reset column widths', exact: true })
    .click();
  await expect.poll(() => widthOf(page, 'white')).toBeCloseTo(initial, 0);
  await expect.poll(() => localWidths(page)).toBeNull();
  await drag(page, 'white', 80);
  await column(page, 'white')
    .getByRole('separator')
    .dispatchEvent('pointercancel', { pointerId: 1 });
  await page.mouse.up();
  await expect.poll(() => widthOf(page, 'white')).toBeCloseTo(initial, 0);
  await expect.poll(() => localWidths(page)).toBeNull();
  await expect(page.locator('.library-table-resizing')).toHaveCount(0);
  await page.locator('button[data-sort="white"]').click();
  await expect(column(page, 'white')).toHaveAttribute('aria-sort', 'ascending');
});

test('keyboard bounds, cancellation and controls remain usable', async ({
  page,
}) => {
  await openLibrary(page);
  const handle = column(page, 'white').getByRole('separator');
  await expect(handle).toBeVisible();
  const initial = await widthOf(page, 'white');
  await handle.focus();
  await page.keyboard.press('ArrowRight');
  await expect
    .poll(() => widthOf(page, 'white'))
    .toBeCloseTo(Math.round(initial) + 10, 0);
  await page.keyboard.press('Home');
  expect(await widthOf(page, 'white')).toBe(60);
  await page.keyboard.press('End');
  expect(await widthOf(page, 'white')).toBe(2400);
  expect(
    await page
      .locator('.database-table-wrap')
      .evaluate((el) => el.scrollWidth > el.clientWidth),
  ).toBe(true);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page
    .getByRole('button', { name: 'Reset column widths', exact: true })
    .click();
  await drag(page, 'white', 80);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect.poll(() => widthOf(page, 'white')).toBeCloseTo(initial, 0);
  await expect.poll(() => localWidths(page)).toBeNull();
  await expect(page.locator('.database-table tbody tr')).toContainText(
    'Resize Alice',
  );
  await page.locator('.database-table tbody tr').getByRole('checkbox').check();
  await expect(page.locator('.library-batch-bar')).toBeVisible();
});

test('custom wide-monitor widths stay contained and reset to current window defaults', async ({
  page,
}) => {
  await openLibrary(page, 3840, 2160);
  const handle = column(page, 'white').getByRole('separator');
  await expect(handle).toBeVisible();
  const initial = await widthOf(page, 'white');
  await drag(page, 'white', 120);
  await page.mouse.up();
  await page.setViewportSize({ width: 1800, height: 1169 });
  await expect
    .poll(() => widthOf(page, 'white'))
    .toBeCloseTo(Math.round(initial) + 120, 0);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page
    .getByRole('button', { name: 'Reset column widths', exact: true })
    .click();
  expect(await widthOf(page, 'white')).toBeLessThan(initial);
  const sizes = await page.locator('.database-table-wrap').evaluate((el) => ({
    available: el.clientWidth,
    table: el.querySelector('table')!.getBoundingClientRect().width,
  }));
  expect(Math.abs(sizes.available - sizes.table)).toBeLessThanOrEqual(1);
  expect(
    (await page.locator('.database-table tbody tr').boundingBox())!.height,
  ).toBeLessThanOrEqual(56);
});

test('captured defaults on an extra-wide display stay reloadable within width bounds', async ({
  page,
}) => {
  await openLibrary(page, 5120, 2160);
  await expect(column(page, 'white').getByRole('separator')).toBeVisible();
  const initial = await widthOf(page, 'white');
  await drag(page, 'white', 70);
  await page.mouse.up();
  await expect.poll(() => localWidths(page)).not.toBeNull();
  const saved = await localWidths(page);
  expect(saved.tags).toBeLessThanOrEqual(2400);
  expect(saved.comment).toBe(220);
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect
    .poll(() => widthOf(page, 'white'))
    .toBeCloseTo(Math.round(initial) + 70, 0);
});

test('reset refreshes accessible widths even when the total table size is unchanged', async ({
  page,
}) => {
  await openLibrary(page);
  await expect(column(page, 'white').getByRole('separator')).toBeVisible();
  await drag(page, 'white', 60);
  await page.mouse.up();
  await drag(page, 'black', -60);
  await page.mouse.up();
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page
    .getByRole('button', { name: 'Reset column widths', exact: true })
    .click();
  for (const id of ['white', 'black']) {
    await expect(column(page, id).getByRole('separator')).toHaveAttribute(
      'aria-valuenow',
      String(Math.round(await widthOf(page, id))),
    );
  }
});
