import { expect, test } from './test';
import type { Database } from '../src/library-api';

test('duplicate match finder groups copies and deletes the selected ones', async ({
  page,
}) => {
  const id = crypto.randomUUID();
  const create = async (name: string): Promise<Database> => {
    const response = await page.request.post('/api/databases', {
      data: { name },
    });
    expect(response.status()).toBe(201);
    return response.json();
  };
  const first = await create(`Duplicates first ${id}`);
  const second = await create(`Duplicates second ${id}`);
  const record = `PW[Twin ${id}]PB[Opponent]MI[length:7];W[31hefe]`;
  const upload = async (database: Database, name: string, sgf: string) => {
    const response = await page.request.post(
      `/api/imports?database_id=${database.id}`,
      {
        multipart: {
          file: {
            name,
            mimeType: 'application/octet-stream',
            buffer: Buffer.from(sgf),
          },
        },
      },
    );
    expect(response.status()).toBe(201);
  };
  // Different bytes with the same moves, and an identical file elsewhere.
  await upload(first, 'twin.sgf', `(;GM[6]${record})`);
  await upload(first, 'twin edited.sgf', `(;GM[6]GN[Edited]${record})\n`);
  await upload(second, 'twin.sgf', `(;GM[6]${record})`);
  await upload(
    first,
    'other.sgf',
    `(;GM[6]PW[Other ${id}]PB[B]MI[length:7];W[42hdfd])`,
  );
  await page.addInitScript((databaseId) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.database', databaseId);
  }, first.id);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  const workspace = page.locator('.match-database');
  await expect(workspace.locator('.database-table tbody tr')).toHaveCount(3);
  const toggle = workspace.getByRole('button', {
    name: 'Duplicates',
    exact: true,
  });
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  const finder = workspace.locator('.library-duplicates');
  await expect(finder.getByRole('status')).toHaveText('1 group · 2 matches');
  const group = finder.getByRole('region', { name: 'Duplicate group 1' });
  await expect(group.getByRole('heading')).toHaveText('Same moves · 2 matches');
  await expect(group.locator('tbody tr')).toHaveCount(2);
  await expect(group.locator('tbody tr').nth(1)).toContainText('twin edited');

  await finder.getByRole('button', { name: 'Select copies' }).click();
  const batch = workspace.locator('.library-batch-bar');
  await expect(batch.getByRole('status')).toHaveText('1 match selected');
  await expect(group.getByRole('checkbox').nth(0)).not.toBeChecked();
  await expect(group.getByRole('checkbox').nth(1)).toBeChecked();
  await batch.getByRole('button', { name: 'Delete', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(finder.getByRole('status')).toHaveText(
    'No duplicate matches found.',
  );
  await expect(workspace.locator('.match-list tbody tr')).toHaveCount(2);

  // All databases compares across databases; the remaining copies are identical files.
  await workspace
    .locator('.database-collections')
    .getByRole('button', { name: /All databases/ })
    .click();
  await expect(finder.getByRole('status')).toHaveText('1 group · 2 matches');
  await expect(
    finder
      .getByRole('region', { name: 'Duplicate group 1' })
      .getByRole('heading'),
  ).toHaveText('Identical files · 2 matches');
  await expect(
    finder.getByRole('region', { name: 'Duplicate group 1' }),
  ).toContainText(second.name);
});
