import { expect, test, type Page } from './test';

test.use({ viewport: { width: 1800, height: 1169 }, actionTimeout: 10000 });

async function openMatch(
  page: Page,
  theme = 'graphite',
  mode = 'light',
  locale = 'en',
) {
  await page.addInitScript(
    ({ theme, mode, locale }) => {
      localStorage.setItem('bgtutor.locale', locale);
      localStorage.setItem(
        'bgtutor.ui-appearance',
        JSON.stringify({ version: 1, theme, mode }),
      );
    },
    { theme, mode, locale },
  );
  const response = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'history-reference.mat',
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(
          '7 point match\n\n Game 1\n Hendrik : 0                    Mochy : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/18 18/13                31: 8/5 6/5\n 3) 42: 8/4 6/4                    65: 24/18 18/13\n',
        ),
      },
    },
  });
  expect(response.ok()).toBe(true);
  const imported = await response.json();
  const analyzed = await page.request.post(
    `/api/matches/${imported.match.id}/analysis`,
    {
      data: { engine_id: 'gnubg', plies: 0 },
    },
  );
  expect(analyzed.status()).toBe(202);
  const job = await analyzed.json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
    )
    .toBe('completed');
  const positions = (
    await (
      await page.request.get(
        `/api/positions?match_id=${imported.match.id}&game_no=1&limit=100`,
      )
    ).json()
  ).items;
  await page.route('**/api/matches/*/move-assessments?*', (route) => {
    const query = new URL(route.request().url()).searchParams;
    return route.fulfill({
      json: {
        match_id: imported.match.id,
        game_no: 1,
        job_id: query.get('job_id'),
        analysis_id: query.get('analysis_id'),
        items: positions.slice(0, 4).map((position, index) => ({
          position_id: position.id,
          severity: ['none', 'doubtful', 'bad', 'very_bad'][index],
          equity_loss: [0, 0.045, 0.092, 0.188][index],
          source: 'native',
          engine: 'Reference GNUbg',
          depth: '2-ply',
          depth_plies: 2,
          job_id: null,
        })),
      },
    });
  });
  await page.addInitScript(
    (id) =>
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      ),
    imported.match.id,
  );
  await page.goto('/');
  await expect(page.locator('.moves-list .move-history-row')).toHaveCount(6);
  return { positions, match: imported.match };
}

for (const theme of ['graphite', 'club', 'paper'])
  for (const mode of ['light', 'dark']) {
    test(`${theme} ${mode}: underlined navigation and two-line loss rows match the reference`, async ({
      page,
    }) => {
      await openMatch(page, theme, mode);
      const tab = page.getByRole('tab', { name: 'Analysis', exact: true });
      await expect(tab).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
      await expect(tab).toHaveCSS('border-bottom-width', '2px');
      const accent = await page.evaluate(() => {
        const probe = document.createElement('span');
        probe.style.color = 'var(--ui-accent)';
        document.body.append(probe);
        const color = getComputedStyle(probe).color;
        probe.remove();
        return color;
      });
      await expect(tab).toHaveCSS('color', accent);
      await expect(tab).toHaveCSS('border-bottom-color', accent);
      await tab.hover();
      await expect(tab).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
      const rows = page.locator('.moves-list .move-history-row');
      await expect(rows.first().locator('.move-history-context')).toContainText(
        'Hendrik',
      );
      await expect(rows.nth(1).locator('.move-history-context')).toContainText(
        'Mochy',
      );
      await expect(rows.nth(1).locator('.move-dice')).toHaveText('4–2');
      for (const [index, value] of [
        '0.000',
        '0.045',
        '0.092',
        '0.188',
      ].entries())
        await expect(rows.nth(index).locator('.move-loss-value')).toHaveText(
          value,
        );
      await expect(
        rows.nth(4).locator('.move-assessment-unavailable'),
      ).toHaveText('—Not evaluated');
      await expect(rows.nth(4).locator('.move-assessment-marker')).toHaveCount(
        0,
      );
      const assessedRow = rows.nth(2);
      await assessedRow.focus();
      const tooltip = page.getByRole('tooltip');
      const assessment = /Reference GNUbg.*2-ply/;
      await expect(tooltip).toBeVisible();
      await expect(tooltip).toContainText(assessment);
      await expect(assessedRow).toHaveAccessibleDescription(assessment);
      await assessedRow.click();
      await expect(assessedRow).toHaveAttribute('aria-current', 'step');
      await expect(page.locator('.board-heading')).toContainText('Move 3');
      await expect(rows.nth(2)).toHaveCSS(
        'background-color',
        await rows.nth(2).evaluate((element) => {
          const probe = document.createElement('span');
          probe.style.backgroundColor = 'var(--ui-selected)';
          element.append(probe);
          const value = getComputedStyle(probe).backgroundColor;
          probe.remove();
          return value;
        }),
      );
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
    });
  }

test('header keyboard navigation and history navigation retain selection, focus and real source context', async ({
  page,
}) => {
  await openMatch(page);
  const tab = page.getByRole('tab', { name: 'Analysis', exact: true });
  await tab.focus();
  // The header order is Playing, Analysis, Study, Positions, …
  await page.keyboard.press('ArrowRight');
  await expect(
    page.getByRole('tab', { name: 'Study', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('End');
  await expect(
    page.getByRole('tab', { name: 'Extras', exact: true }),
  ).toBeFocused();
  const playing = page.getByRole('tab', { name: 'Playing', exact: true });
  await page.keyboard.press('Home');
  await expect(playing).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(
    page.getByRole('tab', { name: 'Extras', exact: true }),
  ).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(playing).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(tab).toBeFocused();
  await expect(tab).toHaveAttribute('aria-selected', 'true');
  await page.locator('.moves-list .move-history-row').nth(1).focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.moves-list .move-history-row').nth(2)).toHaveAttribute(
    'aria-current',
    'step',
  );
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Settings', exact: true })
    .getByRole('button', { name: 'Close', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: 'Settings', exact: true }),
  ).toBeFocused();
});

test('German loss badges use decimal commas and remain readable at laptop and4K sizes', async ({
  page,
}, info) => {
  await openMatch(page, 'graphite', 'light', 'de');
  await expect(page.locator('.move-loss-value').nth(1)).toHaveText('0,045');
  for (const viewport of [
    { width: 1800, height: 1169 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ]) {
    await page.setViewportSize(viewport);
    const row = page.locator('.moves-list .move-history-row').nth(1);
    await expect(row.locator('.move-history-context')).toBeVisible();
    expect((await row.boundingBox())!.height).toBeLessThanOrEqual(52);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: info.outputPath(`reference-${viewport.width}.png`),
      animations: 'disabled',
    });
  }
});
