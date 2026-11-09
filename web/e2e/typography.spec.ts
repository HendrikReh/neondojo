import { expect, test, type Page } from '@playwright/test';

test.use({ viewport: { width: 1800, height: 1169 }, actionTimeout: 10000 });

async function fixture(
  page: Page,
  theme = 'graphite',
  mode = 'light',
  locale = 'en',
) {
  const imported = await (
    await page.request.post('/api/imports', {
      multipart: {
        file: {
          name: 'typography.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[Typography ${crypto.randomUUID()}]PB[Mochy]MI[length:23][ws:3][bs:4];W[31hefe];B[42qusu])`,
          ),
        },
      },
    })
  ).json();
  const position = (
    await (
      await page.request.get(
        `/api/positions?match_id=${imported.match.id}&limit=1`,
      )
    ).json()
  ).items[0];
  const job = await (
    await page.request.post('/api/jobs', {
      data: { position_id: position.id, kind: 'analysis', plies: 0 },
    })
  ).json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
    )
    .toBe('completed');
  expect(
    (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
  ).toBe(true);
  const deck = await (
    await page.request.post('/api/decks', {
      data: { name: `Typography ${crypto.randomUUID()}` },
    })
  ).json();
  expect(
    (
      await page.request.post(`/api/decks/${deck.id}/positions`, {
        data: { position_ids: [position.id] },
      })
    ).ok(),
  ).toBe(true);
  await page.addInitScript(
    ({ id, deck, theme, mode, locale }) => {
      if (!localStorage.getItem('bgtutor.locale'))
        localStorage.setItem('bgtutor.locale', locale);
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      );
      localStorage.setItem('bgtutor.deck', deck);
      localStorage.setItem(
        'bgtutor.ui-appearance',
        JSON.stringify({ version: 1, theme, mode }),
      );
    },
    { id: imported.match.id, deck: deck.id, theme, mode, locale },
  );
  await page.goto('/');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return deck;
}

const views = [
  [
    'analysis',
    'Analysis',
    '.analysis-heading h1',
    '.candidates',
    '.analysis-commandbar select',
  ],
  [
    'database',
    'Match database',
    '.database-heading h1',
    '.database-table',
    '.library-search-toolbar input',
  ],
  [
    'positions',
    'Positions',
    '.position-browser-heading h1',
    '.position-table',
    '.position-search-row input',
  ],
  [
    'learning',
    'Study & Anki',
    '.learning-heading h1',
    '.learning-card-list',
    '.create-deck input',
  ],
] as const;

for (const theme of ['graphite', 'club', 'paper'])
  for (const mode of ['light', 'dark']) {
    test(`${theme} ${mode}: equivalent text roles share family, size, weight and style across workspaces`, async ({
      page,
    }) => {
      await fixture(page, theme, mode);
      const family = await page
        .locator('.app-shell')
        .evaluate((el) => getComputedStyle(el).fontFamily);
      for (const [id, name, title, table, control] of views) {
        await page.getByRole('tab', { name, exact: true }).click();
        const panel = page.locator(`#workspace-panel-${id}`);
        const heading = panel.locator(title);
        await expect(heading).toHaveCSS('font-family', family);
        await expect(heading).toHaveCSS('font-size', '24px');
        await expect(heading).toHaveCSS('font-weight', '600');
        await expect(heading).toHaveCSS('font-style', 'normal');
        await expect(panel.locator(control).first()).toHaveCSS(
          'font-size',
          '13px',
        );
        await expect(panel.locator(control).first()).toHaveCSS(
          'font-family',
          family,
        );
        for (const field of await panel
          .locator(
            'input:visible:not([type="checkbox"]):not([type="radio"]):not([type="range"]), select:visible, textarea:visible',
          )
          .all()) {
          await expect(field).toHaveCSS('font-size', '13px');
          await expect(field).toHaveCSS('font-style', 'normal');
        }
        const head = panel.locator(`${table} thead th`).first();
        const cell = panel.locator(`${table} tbody td`).first();
        await expect(head).toHaveCSS('font-size', '11px');
        await expect(head).toHaveCSS('font-weight', '600');
        await expect(head).toHaveCSS('text-transform', 'none');
        await expect(cell).toHaveCSS('font-size', '12px');
        await expect(cell).toHaveCSS('font-family', family);
        if (id === 'analysis')
          await expect(panel.locator('.moves-panel h3')).toHaveCSS(
            'font-size',
            '14px',
          );
        if (id === 'analysis') {
          await expect(panel.locator('.preview-best')).toHaveCSS(
            'font-size',
            '13px',
          );
          await expect(
            panel.locator('.board-view-buttons button').first(),
          ).toHaveCSS('font-size', '13px');
          await expect(panel.locator('.board-focus-toggle')).toHaveCSS(
            'font-size',
            '13px',
          );
        }
        if (id === 'database')
          await expect(panel.locator('.position-search input')).toHaveCSS(
            'font-family',
            /monospace/,
          );
        if (id === 'database')
          await expect(panel.locator('.library-width-reset')).toHaveCSS(
            'font-size',
            '13px',
          );
        if (id === 'positions')
          await expect(panel.locator('.position-preview h2')).toHaveCSS(
            'font-size',
            '14px',
          );
        if (id === 'learning')
          await expect(panel.locator('.learning-card-preview h4')).toHaveCSS(
            'font-size',
            '16px',
          );
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth,
          ),
        ).toBe(true);
      }
    });
  }

test('comment editors, modal titles and study questions follow their roles while code and board fonts remain distinct', async ({
  page,
}) => {
  await fixture(page);
  const family = await page
    .locator('.app-shell')
    .evaluate((el) => getComputedStyle(el).fontFamily);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .locator('.settings-categories')
    .getByRole('button', { name: 'Analysis', exact: true })
    .click();
  await page
    .locator('.settings-dialog')
    .getByRole('button', { name: 'Opening books', exact: true })
    .click();
  await expect(page.locator('.library-dialog h2')).toHaveCSS(
    'font-size',
    '20px',
  );
  await page
    .locator('.library-dialog')
    .getByRole('button', { name: 'Cancel', exact: true })
    .click();
  await page
    .locator('.settings-dialog')
    .getByRole('button', { name: 'Cancel', exact: true })
    .click();
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await expect(page.locator('textarea:visible').first()).toHaveCSS(
    'font-family',
    family,
  );
  await expect(page.locator('textarea:visible').first()).toHaveCSS(
    'font-size',
    '13px',
  );
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  let dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await expect(dialog.locator('h2')).toHaveCSS('font-size', '20px');
  await expect(dialog.locator('h2')).toHaveCSS('font-family', family);
  await expect(dialog.locator('.settings-field select').first()).toHaveCSS(
    'font-size',
    '13px',
  );
  await expect(dialog.locator('.settings-section:visible > legend')).toHaveCSS(
    'font-size',
    '14px',
  );
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('tab', { name: 'Study & Anki', exact: true }).click();
  const study = page.getByRole('region', { name: 'Study & Anki', exact: true });
  await expect(study.locator('.learning-card-list small').first()).toHaveCSS(
    'font-family',
    /monospace/,
  );
  await study.getByRole('button', { name: 'Rename deck', exact: true }).click();
  dialog = page.getByRole('dialog', { name: 'Rename deck', exact: true });
  await expect(dialog.locator('h2')).toHaveCSS('font-size', '20px');
  await expect(dialog.locator('h2')).toHaveCSS('font-weight', '600');
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await study.getByRole('button', { name: 'Study now', exact: true }).click();
  await expect(study.locator('.study-card > h3')).toHaveCSS(
    'font-family',
    family,
  );
  await expect(study.locator('.study-card > h3')).toHaveCSS(
    'font-size',
    '18px',
  );
  const boardFont = await study
    .locator('.board .die-value')
    .first()
    .evaluate((el) => getComputedStyle(el).fontFamily);
  expect(boardFont).not.toBe(family);
  expect(boardFont).toBe('Arial, sans-serif');
});

test('analysis statistics and move errors use the shared table roles', async ({
  page,
}) => {
  await fixture(page);
  await page
    .getByLabel('Analysis scope', { exact: true })
    .selectOption('match');
  await page
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('0');
  await page
    .getByRole('button', { name: 'Analyze entire match', exact: true })
    .click();
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'Match analysis complete',
  );
  const stats = page.locator('.overview-statistics .statistics-games-table');
  await expect(stats.locator('tbody td').first()).toHaveCSS(
    'font-size',
    '12px',
  );
  await expect(stats.locator('.position-link').first()).toHaveCSS(
    'font-size',
    '12px',
  );
  await expect(stats.locator('thead th').first()).toHaveCSS(
    'font-size',
    '11px',
  );
  await page
    .locator('.overview-errors .match-analysis-filters input[type="checkbox"]')
    .uncheck();
  const errors = page.locator('.overview-errors .match-moves-table');
  await expect(errors.locator('tbody td').first()).toHaveCSS(
    'font-size',
    '12px',
  );
  await expect(errors.locator('thead th').first()).toHaveCSS(
    'font-size',
    '11px',
  );
  await expect(page.locator('.overview-errors .statistics-reset')).toHaveCSS(
    'font-size',
    '13px',
  );
  for (const locale of ['en', 'de', 'es', 'fr', 'pl', 'it']) {
    if (locale !== 'en') {
      await page.locator('.settings-trigger').click();
      const settings = page.locator('.settings-dialog');
      await settings.locator('select[id$="-language"]').selectOption(locale);
      await settings.locator('button[type="submit"]').click();
      await expect(settings).not.toBeVisible();
      await expect(page.locator('html')).toHaveAttribute('lang', locale);
    }
    for (const viewport of [
      { width: 1800, height: 1169 },
      { width: 2560, height: 1440 },
      { width: 3840, height: 2160 },
    ]) {
      await page.setViewportSize(viewport);
      const labels = await page
        .locator(
          '.overview-statistics .statistics-error-matrix [role="columnheader"]',
        )
        .evaluateAll((nodes) =>
          nodes.map((node) => {
            const text = node.firstChild!;
            const words = [...(text.textContent ?? '').matchAll(/\S+/gu)];
            return {
              wordHeights: words.map((word) => {
                const range = document.createRange();
                range.setStart(text, word.index!);
                range.setEnd(text, word.index! + word[0].length);
                return range.getBoundingClientRect().height;
              }),
              line: parseFloat(getComputedStyle(node).lineHeight),
              fits: node.scrollWidth <= node.clientWidth,
            };
          }),
        );
      expect(labels.length).toBeGreaterThan(0);
      for (const label of labels) {
        for (const height of label.wordHeights)
          expect(height).toBeLessThanOrEqual(label.line + 1);
        expect(label.fits).toBe(true);
      }
      for (const matrix of await page
        .locator('.overview-statistics .statistics-error-matrix')
        .all()) {
        const columns = await matrix.evaluate((el) => {
          const rows = [...el.querySelectorAll('[role="row"]')];
          return rows.map((row) =>
            [...row.children].map((cell) => {
              const r = cell.getBoundingClientRect();
              return { x: r.x, width: r.width };
            }),
          );
        });
        const header = columns[0];
        for (const row of columns.slice(1))
          for (const [i, cell] of row.entries()) {
            expect(Math.abs(cell.x - header[i].x)).toBeLessThan(1);
            expect(Math.abs(cell.width - header[i].width)).toBeLessThan(1);
          }
      }
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
    }
  }
});

test('the shared role scale stays stable across desktop widths and every locale', async ({
  page,
}, info) => {
  await fixture(page);
  for (const locale of ['en', 'de', 'es', 'fr', 'pl', 'it']) {
    await page.evaluate(
      (locale) => localStorage.setItem('bgtutor.locale', locale),
      locale,
    );
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('lang', locale);
    for (const [id, , title, table] of views) {
      await page.locator(`#workspace-tab-${id}`).click();
      const panel = page.locator(`#workspace-panel-${id}`);
      await expect(panel.locator(title)).toHaveCSS('font-size', '24px');
      await expect(panel.locator(`${table} tbody td`).first()).toHaveCSS(
        'font-size',
        '12px',
      );
    }
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await page.evaluate(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.reload();
  for (const viewport of [
    { width: 1800, height: 1169 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ]) {
    await page.setViewportSize(viewport);
    for (const [id, name, title, table] of views) {
      await page.getByRole('tab', { name, exact: true }).click();
      const panel = page.locator(`#workspace-panel-${id}`);
      await expect(panel.locator(title)).toHaveCSS('font-size', '24px');
      await expect(panel.locator(`${table} tbody td`).first()).toHaveCSS(
        'font-size',
        '12px',
      );
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      await page.screenshot({
        path: info.outputPath(`${id}-${viewport.width}.png`),
        animations: 'disabled',
      });
    }
  }
});
