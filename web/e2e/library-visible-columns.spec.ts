import { expect, test, type Page } from './test';
import { LIBRARY_COLUMNS } from '../src/library-columns';
test.use({ actionTimeout: 10000, viewport: { width: 1800, height: 1169 } });

async function fixture(page: Page) {
  const suffix = crypto.randomUUID();
  const database = await (
    await page.request.post('/api/databases', {
      data: { name: `Columns ${suffix}` },
    })
  ).json();
  const matches = [];
  for (let index = 0; index < 2; index++) {
    const source = `(;GM[6]PW[Columns ${index} ${suffix}]PB[Opponent]MI[length:7];W[31hefe])`;
    const response = await page.request.post(
      `/api/imports?database_id=${database.id}`,
      {
        multipart: {
          file: {
            name: `columns-${index}.sgf`,
            mimeType: 'application/octet-stream',
            buffer: Buffer.from(source),
          },
        },
      },
    );
    expect(response.status()).toBe(201);
    const match = (await response.json()).match;
    expect(
      (
        await page.request.put(`/api/matches/${match.id}/annotation`, {
          data: {
            expected_version: 0,
            tags: ['column-tag'],
            comment: `Rare comment ${index}`,
          },
        })
      ).ok(),
    ).toBe(true);
    matches.push(match);
  }
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.database', id);
  }, database.id);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(2);
  return { database, matches };
}
const head = (page: Page, id: string) =>
  page.locator(`.database-table th[data-column="${id}"]`);
async function picker(page: Page) {
  const details = page.locator('.library-column-picker');
  if (!(await details.evaluate((el) => (el as HTMLDetailsElement).open)))
    await details.locator('summary').click();
  return details;
}

test('optional columns persist without changing filters, sorting or batch selection', async ({
  page,
}) => {
  await fixture(page);
  await expect(head(page, 'tournament')).toHaveCount(0);
  await expect(head(page, 'comment')).toHaveCount(0);
  await expect(head(page, 'tags')).toBeVisible();
  const row = page.locator('.database-table tbody tr').first();
  await expect(row.locator('.library-tags-cell')).toContainText('column-tag');
  await row.focus();
  await expect(page.getByRole('tooltip')).toContainText('Rare comment');
  await row.getByRole('checkbox').check();
  const requests: string[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/library/matches')
      requests.push(request.url());
  });
  const menu = await picker(page);
  await menu.getByRole('checkbox', { name: 'Event', exact: true }).check();
  await menu.getByRole('checkbox', { name: 'Comment', exact: true }).check();
  await expect(head(page, 'tournament')).toBeVisible();
  await expect(head(page, 'comment')).toBeVisible();
  await expect(row.locator('.library-comment-cell')).toContainText(
    'Rare comment',
  );
  await menu
    .getByRole('checkbox', { name: 'Imported on', exact: true })
    .uncheck();
  await expect(page.locator('.library-hidden-sort')).toContainText('Imported');
  await expect(row.getByRole('checkbox')).toBeChecked();
  expect(requests).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(menu.locator('summary')).toBeFocused();
  await expect(menu).not.toHaveAttribute('open', '');
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(head(page, 'comment')).toBeVisible();
  await expect(head(page, 'tournament')).toBeVisible();
  await expect(head(page, 'created_at')).toHaveCount(0);
  await (
    await picker(page)
  )
    .getByRole('button', { name: 'Default columns', exact: true })
    .click();
  await expect(head(page, 'comment')).toHaveCount(0);
  await expect(head(page, 'tournament')).toHaveCount(0);
  await expect(head(page, 'created_at')).toBeVisible();
});

test('hidden columns retain their widths and legacy notes widths migrate to tags', async ({
  page,
}) => {
  const legacy = {
    select: 32,
    id: 60,
    white: 211,
    white_pr: 56,
    black: 180,
    black_pr: 56,
    match_length: 86,
    score: 76,
    played_at: 100,
    created_at: 100,
    tournament: 160,
    position_count: 96,
    analysis_status: 126,
    notes: 199,
    actions: 88,
  };
  await page.addInitScript((widths) => {
    if (!localStorage.getItem('bgtutor.library.columns'))
      localStorage.setItem(
        'bgtutor.library.columns',
        JSON.stringify({ version: 1, widths }),
      );
  }, legacy);
  await fixture(page);
  expect((await head(page, 'white').boundingBox())!.width).toBeCloseTo(211, 0);
  expect((await head(page, 'tags').boundingBox())!.width).toBeCloseTo(199, 0);
  let menu = await picker(page);
  await menu.getByRole('checkbox', { name: 'Comment', exact: true }).check();
  await page.keyboard.press('Escape');
  const handle = head(page, 'comment').getByRole('separator');
  await handle.focus();
  await page.keyboard.press('ArrowRight');
  const width = (await head(page, 'comment').boundingBox())!.width;
  expect(width).toBeGreaterThan(220);
  menu = await picker(page);
  await menu.getByRole('checkbox', { name: 'Comment', exact: true }).uncheck();
  await page.keyboard.press('Escape');
  await head(page, 'white').getByRole('separator').focus();
  await page.keyboard.press('ArrowRight');
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(head(page, 'comment')).toHaveCount(0);
  menu = await picker(page);
  await menu.getByRole('checkbox', { name: 'Comment', exact: true }).check();
  await page.keyboard.press('Escape');
  expect((await head(page, 'comment').boundingBox())!.width).toBeCloseTo(
    width,
    0,
  );
  await page
    .getByRole('button', { name: 'Reset column widths', exact: true })
    .click();
  await expect(head(page, 'comment')).toBeVisible();
  await expect(head(page, 'tournament')).toHaveCount(0);
});

test('opening and selection remain available with both player columns hidden', async ({
  page,
}) => {
  await page.addInitScript(
    (widths) =>
      localStorage.setItem(
        'bgtutor.library.columns',
        JSON.stringify({ version: 1, widths }),
      ),
    Object.fromEntries(
      LIBRARY_COLUMNS.map((column) => [column.id, column.defaultWidth]),
    ),
  );
  await fixture(page);
  const menu = await picker(page);
  await menu.getByRole('checkbox', { name: 'White', exact: true }).uncheck();
  await page.keyboard.press('Escape');
  const row = page.locator('.database-table tbody tr').first();
  await expect(row.locator('.match-card')).toContainText('Opponent');
  await (
    await picker(page)
  )
    .getByRole('checkbox', { name: 'Black', exact: true })
    .uncheck();
  await page.keyboard.press('Escape');
  await row.getByRole('checkbox').check();
  await expect(page.locator('.library-batch-bar')).toBeVisible();
  const actions = await row
    .locator('.library-row-actions')
    .evaluate((element) => ({
      available: element.clientWidth,
      required: element.scrollWidth,
    }));
  expect(actions.required).toBeLessThanOrEqual(actions.available);
  const id = await row.getAttribute('id');
  await row.locator('.library-row-open').click();
  await expect(page.locator('.analysis-heading')).toHaveAttribute(
    'data-match-id',
    id!.replace('library-row-', ''),
  );
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  const options = await picker(page);
  const boxes = options.getByRole('checkbox');
  for (let index = 0; index < (await boxes.count()); index++)
    if (
      (await boxes.nth(index).isChecked()) &&
      (await boxes.nth(index).isEnabled())
    )
      await boxes.nth(index).uncheck();
  await expect(options.locator('input:checked')).toHaveCount(1);
  await expect(options.locator('input:checked')).toBeDisabled();
  await options
    .getByRole('button', { name: 'Show all columns', exact: true })
    .click();
  await expect(head(page, 'comment')).toBeVisible();
  await expect(head(page, 'tournament')).toBeVisible();
});
