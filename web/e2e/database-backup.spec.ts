import { expect, test, type FileChooser } from '@playwright/test';

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
  await review.click();
  const dialog = page.getByRole('dialog', { name: 'Restore backup' });
  await expect(
    dialog.getByText('Restoring replaces all BG Tutor data', { exact: false }),
  ).toBeVisible();
  await expect(
    dialog.getByText(download.suggestedFilename(), { exact: true }),
  ).toBeVisible();
  await expect(dialog).toContainText(
    'checks the backup file and its database schema after you confirm',
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
  await review.click();
  await dialog.getByRole('button', { name: 'Replace database' }).click();
  const restoreError = dialog.getByRole('alert');
  await expect(restoreError).toContainText('Invalid BG Tutor backup archive');
  await expect(restoreError).toBeFocused();
  expect(await (await request.get('/api/databases')).json()).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ name: 'Target fixture' }),
    ]),
  );
  await page.keyboard.press('Escape');
  await expect(review).toBeFocused();

  await pickBackup(archive);
  await review.click();
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
