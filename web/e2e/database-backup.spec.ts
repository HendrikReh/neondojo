import { expect, test, type FileChooser } from '@playwright/test';
import { defaultConfiguration } from '../src/configuration-model';
import { profileBackend } from './profile-backend';

test('exports a backup and restores it only after confirmation', async ({
  page,
  request,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  const createDatabase = async (name: string) => {
    const response = await request.post('/api/databases', {
      data: { name, description: '' },
    });
    expect(response.ok()).toBeTruthy();
  };
  const importTargetMatch = async (player: string) => {
    const imported = await request.post('/api/imports', {
      multipart: {
        file: {
          name: 'target.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[${player}]PB[Opponent]MI[length:7];W[31hefe])`,
          ),
        },
      },
    });
    expect(imported.status()).toBe(201);
  };

  await createDatabase('Transfer fixture');
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  await page.getByRole('tab', { name: 'Database backup', exact: true }).click();
  const restore = page.getByRole('region', { name: 'Restore backup' });
  const chooseFile = restore.getByRole('button', {
    name: 'Choose backup file',
  });
  const review = restore.getByRole('button', {
    name: 'Review database replacement',
  });
  await expect(
    page.getByRole('heading', { name: 'Database backup' }),
  ).toBeVisible();
  await expect(review).toBeDisabled();
  await expect(
    restore.getByText('Restoring replaces all BG Tutor data', { exact: false }),
  ).toBeVisible();
  await expect(chooseFile).toBeVisible();
  await expect(restore.getByText('No file selected')).toBeVisible();
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(review).toBeInViewport();
    await page.screenshot({
      path: testInfo.outputPath(`database-backup-${width}.png`),
    });
  }
  await page.setViewportSize({ width: 1800, height: 1169 });

  const [download] = await Promise.all([
    page.waitForEvent('download'),
    page.waitForResponse(/\/api\/database-backup$/).then((response) => {
      expect(response.status(), 'Database backup export status').toBe(200);
    }),
    page.getByRole('button', { name: 'Export backup' }).click(),
  ]);
  expect(download.suggestedFilename()).toMatch(/^bgtutor-.*\.bgtutor-backup$/);
  const archive = testInfo.outputPath(download.suggestedFilename());
  await download.saveAs(archive);
  await expect(
    page.getByRole('status').filter({ hasText: 'Backup download started.' }),
  ).toBeVisible();

  await createDatabase('Target fixture');
  async function pickBackup(file: Parameters<FileChooser['setFiles']>[0]) {
    const chooserEvent = page.waitForEvent('filechooser');
    await chooseFile.click();
    await (await chooserEvent).setFiles(file);
  }

  await pickBackup(archive);
  await expect(
    restore.getByText(download.suggestedFilename(), { exact: true }),
  ).toBeVisible();
  await expect(restore.getByRole('status')).toContainText(
    'The backup archive is valid and compatible with this database schema.',
  );
  const contents = restore.getByRole('region', { name: 'Backup contents' });
  await expect(contents.getByText('Created', { exact: true })).toBeVisible();
  await expect(contents.locator('time')).toHaveAttribute(
    'datetime',
    /^\d{4}-\d{2}-\d{2}T/,
  );
  await expect(contents.getByRole('definition')).toHaveText([
    /.+/,
    '0',
    '0',
    '0',
  ]);
  await importTargetMatch('Target player');
  await review.click();
  const dialog = page.getByRole('dialog', { name: 'Restore backup' });
  const comparison = dialog.getByRole('table', { name: 'Compare contents' });
  await expect(comparison.getByRole('columnheader')).toHaveText([
    'Backup',
    'Target database',
    'Difference',
  ]);
  await expect(
    comparison.getByRole('row', { name: /^Matches/ }).getByRole('cell'),
  ).toHaveText(['0', '1', '-1']);
  await expect(
    dialog.getByText('Restoring replaces all BG Tutor data', { exact: false }),
  ).toBeVisible();
  await expect(
    dialog.getByText(download.suggestedFilename(), { exact: true }),
  ).toBeVisible();
  await expect(dialog).toContainText(
    'BG Tutor checks the archive and database constraints again during restore.',
  );
  await page.screenshot({
    path: testInfo.outputPath('database-backup-confirm.png'),
  });
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(review).toBeFocused();
  await expect(dialog).toBeHidden();
  expect(await (await request.get('/api/databases')).json()).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ name: 'Target fixture' }),
    ]),
  );

  await pickBackup({
    name: 'invalid.bgtutor-backup',
    mimeType: 'application/vnd.bgtutor.backup',
    buffer: Buffer.from('Invalid backup'),
  });
  await expect(restore.getByRole('alert')).toContainText(
    'Invalid BG Tutor backup archive',
  );
  await expect(review).toBeDisabled();
  await expect(dialog).toBeHidden();
  const restoreError = dialog.getByRole('alert');
  expect(await (await request.get('/api/databases')).json()).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ name: 'Target fixture' }),
    ]),
  );
  await pickBackup(archive);
  await importTargetMatch('Second target player');
  await review.click();
  await expect(
    comparison.getByRole('row', { name: /^Matches/ }).getByRole('cell'),
  ).toHaveText(['0', '2', '-2']);
  const replace = dialog.getByRole('button', { name: 'Replace database' });
  const confirmRestore = async () => {
    const [response] = await Promise.all([
      page.waitForResponse(/\/api\/database-backup\?replace=true$/),
      replace.click(),
    ]);
    return response;
  };
  let restoredResponse = await confirmRestore();
  if (restoredResponse.status() === 409) {
    await expect(restoreError).toContainText('Database is busy');
    await expect(replace).toBeEnabled();
    restoredResponse = await confirmRestore();
  }
  expect(restoredResponse.status(), 'Database restore status').toBe(204);
  await expect(dialog.getByRole('status')).toContainText('Backup restored.');
  await expect(
    dialog.getByRole('button', { name: 'Reload BG Tutor' }),
  ).toBeFocused();
  const restored = await (await request.get('/api/databases')).json();
  expect(restored).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ name: 'Transfer fixture' }),
    ]),
  );
  expect(restored).not.toEqual(
    expect.arrayContaining([
      expect.objectContaining({ name: 'Target fixture' }),
    ]),
  );
  await dialog.getByRole('button', { name: 'Reload BG Tutor' }).click();
  await expect(
    page.getByRole('tab', { name: 'Extras', exact: true }),
  ).toBeVisible();
  await expect(dialog).toBeHidden();
});

test('only the latest selected archive can enable review', async ({ page }) => {
  let finishFirst!: () => void;
  const first = new Promise<void>((resolve) => {
    finishFirst = resolve;
  });
  await page.route('**/api/database-backup/check', async (route) => {
    if (route.request().postData() === 'first') {
      await first;
      await route
        .fulfill({
          json: {
            status: 'compatible',
            contents: { matches: 99, saved_positions: 99, learning_cards: 99 },
          },
        })
        .catch(() => {});
    } else {
      await route.fulfill({
        json: {
          status: 'incompatible_schema',
          contents: { matches: 1234, saved_positions: 2, learning_cards: 3 },
        },
      });
    }
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  await page.getByRole('tab', { name: 'Database backup', exact: true }).click();
  const restore = page.getByRole('region', { name: 'Restore backup' });
  const input = restore.locator('input[type=file]');
  const review = restore.getByRole('button', {
    name: 'Review database replacement',
  });
  const file = (name: string) => ({
    name: `${name}.bgtutor-backup`,
    mimeType: 'application/vnd.bgtutor.backup',
    buffer: Buffer.from(name),
  });
  const firstStarted = page.waitForRequest('**/api/database-backup/check');
  await input.setInputFiles(file('first'));
  await firstStarted;
  await expect(restore.getByRole('status')).toContainText(
    'Checking backup archive',
  );
  await expect(review).toBeDisabled();
  await input.setInputFiles(file('second'));
  await expect(restore.getByRole('alert')).toContainText(
    'different database schema',
  );
  finishFirst();
  await expect(restore.getByText('second.bgtutor-backup')).toBeVisible();
  await expect(
    restore
      .getByRole('region', { name: 'Backup contents' })
      .getByRole('definition'),
  ).toHaveText(['Unavailable in this archive', '1,234', '2', '3']);
  await expect(review).toBeDisabled();
});

test('dates older backups from a valid BG Tutor filename', async ({ page }) => {
  const archivedDate = '2025-03-01T10:11:12Z';
  await page.route('**/api/database-backup/target-counts', (route) =>
    route.fulfill({
      json: { matches: 40, saved_positions: 3502, learning_cards: 3 },
    }),
  );
  await page.route('**/api/database-backup/check', (route) =>
    route.fulfill({
      json: {
        status: 'compatible',
        contents: {
          matches: 42,
          saved_positions: 2,
          learning_cards: 3,
          ...(route.request().postData() === 'current' && {
            created_at: archivedDate,
          }),
        },
      },
    }),
  );
  const configuration = structuredClone(defaultConfiguration);
  configuration.general.locale = 'de';
  configuration.appearance.mode = 'light';
  await profileBackend(page, { configuration });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Datenbanksicherung', exact: true })
    .click();
  const contents = page.getByRole('region', { name: 'Inhalt der Sicherung' });
  const input = page.locator('input[type=file][accept*=".bgtutor-backup"]');
  const selectArchive = (name: string, body = 'legacy') =>
    input.setInputFiles({
      name,
      mimeType: 'application/vnd.bgtutor.backup',
      buffer: Buffer.from(body),
    });

  const goodName = 'bgtutor-2024-02-29-230405.bgtutor-backup';
  await selectArchive(goodName);
  await expect(contents.locator('time')).toHaveAttribute(
    'datetime',
    '2024-02-29T23:04:05Z',
  );
  await expect(
    contents.getByText('aus Dateiname', { exact: false }),
  ).toBeVisible();

  await page.getByRole('button', { name: 'Wiederherstellung ansehen' }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Sicherung wiederherstellen',
  });
  const dialogContents = dialog.getByRole('region', {
    name: 'Bestände vergleichen',
  });
  await expect(dialogContents.locator('time')).toHaveAttribute(
    'datetime',
    '2024-02-29T23:04:05Z',
  );
  await expect(
    dialogContents.getByText('aus Dateiname', { exact: false }),
  ).toBeVisible();
  await expect(dialogContents.getByRole('columnheader')).toHaveText([
    'Sicherung',
    'Zieldatenbank',
    'Differenz',
  ]);
  await expect(
    dialogContents.getByRole('row', { name: /^Matches/ }).getByRole('cell'),
  ).toHaveText(['42', '40', '+2']);
  await expect(
    dialogContents
      .getByRole('row', { name: /^Gespeicherte Positionen/ })
      .getByRole('cell'),
  ).toHaveText(['2', '3.502', '-3.500']);
  await expect(
    dialogContents.getByRole('row', { name: /^Lernkarten/ }).getByRole('cell'),
  ).toHaveText(['3', '3', '0']);
  await expect(
    dialogContents.getByRole('cell', { name: '+2', exact: true }),
  ).toHaveCSS('color', 'rgb(37, 99, 60)');
  await expect(
    dialogContents.getByRole('cell', { name: '-3.500', exact: true }),
  ).toHaveCSS('color', 'rgb(161, 52, 40)');
  await expect(
    dialogContents.getByRole('cell', { name: '0', exact: true }),
  ).toHaveCSS('color', 'rgb(32, 40, 51)');
  await dialog.getByRole('button', { name: 'Abbrechen', exact: true }).click();

  for (const name of [
    'bgtutor-2025-02-29-230405.bgtutor-backup',
    'bgtutor-2024-02-29-240000.bgtutor-backup',
    'renamed-bgtutor-2024-02-29-230405.bgtutor-backup',
  ]) {
    await selectArchive(name);
    await expect(
      contents.getByText('Im Archiv nicht gespeichert'),
    ).toBeVisible();
    await expect(contents.locator('time')).toHaveCount(0);
  }

  await selectArchive(goodName, 'current');
  await expect(contents.locator('time')).toHaveAttribute(
    'datetime',
    archivedDate,
  );
  await expect(
    contents.getByText('aus Dateiname', { exact: false }),
  ).toHaveCount(0);
});
