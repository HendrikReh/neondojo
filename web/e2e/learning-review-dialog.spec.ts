import { test, expect, type Page, type Locator } from './test';
import {
  historyFixture,
  historySessionId,
} from './play-history-review-helpers';
import {
  asMoneyReport,
  detail,
  id,
  moneySummary,
} from './study-report-fixture';
async function study(
  page: Page,
  long = false,
  locale = 'en',
  name = 'Hendrik · GNUbg',
) {
  await page.addInitScript(
    (locale) => localStorage.setItem('neondojo.locale', locale),
    locale,
  );
  await page.route('**/api/decks', (r) => r.fulfill({ json: [] }));
  await page.route('**/api/study/reports?group_by=session', (r) =>
    r.fulfill({
      json: { items: [moneySummary(id, name)], next_cursor: null },
    }),
  );
  const report = asMoneyReport(detail());
  if (long)
    report.review.games[0].decisions = Array.from(
      { length: 150 },
      (_, index) => ({
        ...report.review.games[0].decisions[0],
        decision_no: index + 1,
      }),
    );
  await page.route(`**/api/study/reports/${id}`, (r) =>
    r.fulfill({ json: report }),
  );
  await page.goto('/');
  await page
    .getByRole('tab', {
      name: locale === 'de' ? 'Lernen' : 'Study',
      exact: true,
    })
    .click();
  await page
    .getByRole('tab', {
      name: locale === 'de' ? 'Auswertungen' : 'Reports',
      exact: true,
    })
    .click();
  const trigger = page.getByRole('button', {
    name: locale === 'de' ? 'Bericht öffnen' : 'Open report',
    exact: true,
  });
  await trigger.click();
  return {
    trigger,
    dialog: page.getByRole('dialog', {
      name: locale === 'de' ? 'Lernauswertung' : 'Learning review',
      exact: true,
    }),
  };
}
async function drag(page: Page, dialog: Locator) {
  const title = (await dialog
    .locator('.play-dialog-drag-handle')
    .boundingBox())!;
  const before = (await dialog.boundingBox())!;
  await page.mouse.move(title.x + title.width / 2, title.y + title.height / 2);
  await page.mouse.down();
  await page.mouse.move(
    title.x + title.width / 2 + 90,
    title.y + title.height / 2 + 40,
    { steps: 8 },
  );
  await page.mouse.up();
  const after = (await dialog.boundingBox())!;
  expect(after.x - before.x).toBeCloseTo(90, 0);
  expect(after.y - before.y).toBeCloseTo(40, 0);
}
test('Study uses the draggable modal, traps focus and returns to the trigger after Escape and close', async ({
  page,
}) => {
  const { dialog, trigger } = await study(page);
  await expect(dialog).toBeVisible();
  expect(await dialog.evaluate((el) => el.matches(':modal'))).toBe(true);
  await expect(dialog).toContainText('4.5');
  await drag(page, dialog);
  for (let i = 0; i < 6; i++) {
    await page.keyboard.press('Tab');
    expect(
      await dialog.evaluate((el) => el.contains(document.activeElement)),
    ).toBe(true);
  }
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await trigger.click();
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
});
for (const width of [1440, 390])
  test(`review decisions scroll inside the viewport at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 850 });
    const { dialog } = await study(page, true);
    await expect(dialog).toBeVisible();
    const bounds = (await dialog.boundingBox())!;
    expect(bounds.x).toBeGreaterThanOrEqual(15);
    expect(bounds.y).toBeGreaterThanOrEqual(15);
    expect(bounds.x + bounds.width).toBeLessThanOrEqual(width - 15);
    expect(bounds.y + bounds.height).toBeLessThanOrEqual(835);
    const scroll = dialog.locator('.learning-review-decisions');
    await expect(scroll).toBeVisible();
    expect(
      await scroll.evaluate((el) => el.scrollHeight > el.clientHeight),
    ).toBe(true);
    const rating = dialog.locator('.play-first-pr');
    const before = (await rating.boundingBox())!;
    await scroll.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    expect(await scroll.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
    expect((await rating.boundingBox())!.y).toBeCloseTo(before.y, 0);
    await expect(rating).toBeInViewport();
    await expect(
      dialog.getByRole('button', { name: 'Close', exact: true }),
    ).toBeInViewport();
    await page.screenshot({
      path: test.info().outputPath(`learning-review-${width}.png`),
      fullPage: true,
    });
  });
test('end-of-match generation opens the same report modal and reopening never recomputes it', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  saved.phase = 'match_over';
  saved.scores = [7, 3];
  saved.winner = 0;
  let reviews = 0;
  await page.route(
    `**/api/play/sessions/${historySessionId}/review`,
    async (r) => {
      reviews++;
      Object.assign(saved, {
        review: detail().review,
        decisions: detail().decisions,
        version: saved.version + 1,
      });
      await r.fulfill({ json: saved });
    },
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page
    .getByRole('button', { name: 'Back to board', exact: true })
    .click();
  const trigger = page.getByRole('button', {
    name: 'Learning review',
    exact: true,
  });
  await trigger.click();
  const dialog = page.getByRole('dialog', {
    name: 'Learning review',
    exact: true,
  });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText('4.5');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
  await trigger.click();
  await expect(dialog).toBeVisible();
  expect(reviews).toBe(1);
  await page.keyboard.press('Escape');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(dialog).toHaveCount(0);
});
test('failed review generation keeps the action available for retry', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  saved.phase = 'match_over';
  saved.scores = [7, 3];
  let fail = true;
  await page.route(`**/api/play/sessions/${historySessionId}/review`, (r) =>
    fail
      ? r.fulfill({ status: 503, json: { error: 'Engine unavailable' } })
      : r.fulfill({ json: { ...saved, review: detail().review } }),
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page
    .getByRole('button', { name: 'Back to board', exact: true })
    .click();
  const trigger = page.getByRole('button', {
    name: 'Learning review',
    exact: true,
  });
  await trigger.click();
  await expect(trigger).toBeEnabled();
  await expect(
    page.getByRole('dialog', { name: 'Learning review', exact: true }),
  ).toHaveCount(0);
  fail = false;
  await trigger.click();
  await expect(
    page.getByRole('dialog', { name: 'Learning review', exact: true }),
  ).toBeVisible();
});

test('a review finishing in the background does not reopen a modal on return', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  saved.phase = 'match_over';
  saved.scores = [7, 3];
  let release = () => {};
  const delayed = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(
    `**/api/play/sessions/${historySessionId}/review`,
    async (r) => {
      await delayed;
      Object.assign(saved, { review: detail().review });
      await r.fulfill({ json: saved });
    },
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page
    .getByRole('button', { name: 'Back to board', exact: true })
    .click();
  const response = page.waitForResponse(
    (r) => r.url().endsWith('/review') && r.request().method() === 'POST',
  );
  await page
    .getByRole('button', { name: 'Learning review', exact: true })
    .click();
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  release();
  await response;
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Learning review', exact: true }),
  ).toBeEnabled();
  await expect(
    page.getByRole('dialog', { name: 'Learning review', exact: true }),
  ).toHaveCount(0);
});

test('the review summary visibly identifies the captured play mode', async ({
  page,
}) => {
  const { dialog } = await study(page);
  await expect(dialog.getByText('Tutor', { exact: true })).toBeVisible();
});

for (const [width, height] of [
  [568, 320],
  [667, 375],
])
  test(`short German report remains readable at ${width}x${height}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height });
    const { dialog } = await study(page, true, 'de');
    await expect(dialog).toBeVisible();
    const scroll = dialog.locator('.play-learning-review');
    expect(
      await scroll.evaluate((el) => el.clientHeight),
    ).toBeGreaterThanOrEqual(100);
    expect(
      await scroll.evaluate((el) => el.scrollHeight > el.clientHeight),
    ).toBe(true);
    await scroll.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    expect(await scroll.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
    await expect(
      dialog.getByRole('button', { name: 'Schließen', exact: true }),
    ).toBeInViewport();
    await page.screenshot({
      path: test.info().outputPath('short-review.png'),
      fullPage: true,
    });
  });

test('a long match name leaves readable decision space on a narrow portrait screen', async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 568 });
  const { dialog } = await study(page, true, 'de', 'W'.repeat(240));
  await expect(dialog).toBeVisible();
  const decisions = dialog.locator('.learning-review-decisions');
  expect(
    await decisions.evaluate((el) => el.clientHeight),
  ).toBeGreaterThanOrEqual(80);
  await expect(
    dialog.locator('.learning-review-summary p').last(),
  ).toBeInViewport();
  await expect(
    dialog.getByRole('button', { name: 'Schließen', exact: true }),
  ).toBeInViewport();
});

for (const width of [1440, 390])
  test(`review sits beside the export action with matching proportions at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    const { saved } = await historyFixture(page);
    saved.phase = 'match_over';
    saved.scores = [7, 3];
    saved.review = detail().review;
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await page
      .getByRole('button', { name: 'Back to board', exact: true })
      .click();
    const actions = page.locator('.play-history-actions');
    const download = actions.getByRole('link', {
      name: 'Download SGF',
      exact: true,
    });
    const review = actions.getByRole('button', {
      name: 'Learning review',
      exact: true,
    });
    await expect(download).toBeVisible();
    await expect(review).toBeVisible();
    const d = (await download.boundingBox())!,
      r = (await review.boundingBox())!;
    expect(d.width).toBe(44);
    expect(d.height).toBe(44);
    expect(r.width).toBe(d.width);
    expect(r.height).toBe(d.height);
    expect(r.y).toBeCloseTo(d.y, 0);
    // A finished point match offers its AI assessment between the two icons.
    const assessment = actions.getByRole('button', {
      name: 'Create AI assessment',
      exact: true,
    });
    await expect(assessment).toBeVisible();
    const a = (await assessment.boundingBox())!;
    // The labeled button is lower than the icons and centered on their row.
    expect(a.y + a.height / 2).toBeCloseTo(d.y + d.height / 2, 0);
    expect(a.x - d.x - d.width).toBeCloseTo(8, 0);
    expect(r.x - a.x - a.width).toBeCloseTo(8, 0);
    expect(r.x + r.width).toBeLessThanOrEqual(width);
    for (const button of [download, review]) {
      const icon = (await button.locator('.action-icon').boundingBox())!;
      expect(icon.width).toBe(20);
      expect(icon.height).toBe(20);
    }
    await expect(
      page
        .locator('.play-table')
        .getByRole('button', { name: 'Learning review', exact: true }),
    ).toHaveCount(0);
    await expect(download).toHaveAttribute('download', 'match.sgf');
    await review.click();
    const dialog = page.getByRole('dialog', {
      name: 'Learning review',
      exact: true,
    });
    await expect(dialog).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(review).toBeFocused();
    await page.screenshot({
      path: test.info().outputPath(`history-review-action-${width}.png`),
      fullPage: true,
    });
  });
