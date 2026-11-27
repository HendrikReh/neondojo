import { createDatabase, importSgfMatch } from '../helpers/match-fixtures';
import { gotoWorkspace, seedStorage } from '../helpers/navigation';
import { expect, test } from '../helpers/test';
import type { Database } from '../../src/workspaces/library/library-api';

test('duplicate match finder groups copies and deletes the selected ones', async ({
  page,
}) => {
  const id = crypto.randomUUID();
  const first = await createDatabase(page, `Duplicates first ${id}`);
  const second = await createDatabase(page, `Duplicates second ${id}`);
  // Duplicates group by match length and moves; an unusual length keeps other
  // specs' one-move matches out of this group.
  const record = `PW[Twin ${id}]PB[Opponent]MI[length:17];W[31hefe]`;
  const upload = (database: Database, filename: string, sgf: string) =>
    importSgfMatch(page, { databaseId: database.id, filename, sgf });
  // Different bytes with the same moves, and an identical file elsewhere.
  await upload(first, 'twin.sgf', `(;GM[6]${record})`);
  await upload(first, 'twin edited.sgf', `(;GM[6]GN[Edited]${record})\n`);
  await upload(second, 'twin.sgf', `(;GM[6]${record})`);
  await upload(
    first,
    'other.sgf',
    `(;GM[6]PW[Other ${id}]PB[B]MI[length:7];W[42hdfd])`,
  );
  await seedStorage(page, { locale: 'en', databaseId: first.id });
  await gotoWorkspace(page, 'library');
  const workspace = page.locator('.match-database');
  // Other specs on this worker may own matches in other databases, so the
  // test selects its own database explicitly.
  await workspace
    .locator('.database-collections')
    .getByRole('button', { name: new RegExp(`Duplicates first ${id}`) })
    .click();
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
  // Other specs' matches may form further groups across all databases, so the
  // check follows this test's group instead of the overall counts.
  const twins = finder
    .getByRole('region', { name: /^Duplicate group \d+$/ })
    .filter({ hasText: `Twin ${id}` });
  await expect(twins).toHaveCount(1);
  await expect(twins.getByRole('heading')).toHaveText(
    'Identical files · 2 matches',
  );
  await expect(twins).toContainText(second.name);
});
