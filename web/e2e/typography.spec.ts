import { closeSettings } from './settings-actions';
import {
  openFullMatchAnalysis,
  submitFullMatchAnalysis,
} from './analysis-controls';
import { expect, test, type Locator, type Page } from './test';

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
    ({ id, deck, theme, mode, locale, positionId, jobId }) => {
      if (!localStorage.getItem('bgtutor.locale'))
        localStorage.setItem('bgtutor.locale', locale);
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1, positionId }),
      );
      localStorage.setItem('bgtutor.deck', deck);
      localStorage.setItem(
        `bgtutor.jobs.${positionId}`,
        JSON.stringify({ analysis: jobId }),
      );
      localStorage.setItem(
        'bgtutor.ui-appearance',
        JSON.stringify({ version: 1, theme, mode }),
      );
    },
    {
      id: imported.match.id,
      deck: deck.id,
      theme,
      mode,
      locale,
      positionId: position.id,
      jobId: job.id,
    },
  );
  await page.goto('/');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return { positionId: position.id as string, deckName: deck.name as string };
}

const fontSize = (locator: Locator) =>
  locator.evaluate((element) => parseFloat(getComputedStyle(element).fontSize));
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
    'Study',
    '.learning-heading h1',
    '.learning-card-list',
    '.position-search-row input',
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
        // The checker candidate table grows with its pane on desktop:
        // 12-14 px headings and 13-16 px cells. Other tables use the scale.
        if (id === 'analysis') {
          expect(await fontSize(head)).toBeGreaterThanOrEqual(12);
          expect(await fontSize(head)).toBeLessThanOrEqual(14);
          expect(await fontSize(cell)).toBeGreaterThanOrEqual(13);
          expect(await fontSize(cell)).toBeLessThanOrEqual(16);
        } else {
          await expect(head).toHaveCSS('font-size', '11px');
          await expect(cell).toHaveCSS('font-size', '12px');
        }
        await expect(head).toHaveCSS('font-weight', '600');
        await expect(head).toHaveCSS('text-transform', 'none');
        await expect(cell).toHaveCSS('font-family', family);
        if (id === 'analysis')
          await expect(panel.locator('.moves-panel h3')).toHaveCSS(
            'font-size',
            '14px',
          );
        if (id === 'analysis') {
          // The move button follows its growing candidate cell.
          expect(
            await fontSize(panel.locator('.candidate-preview-button').first()),
          ).toBe(await fontSize(cell));
          await expect(
            panel.locator('.board-view-buttons button').first(),
          ).toHaveCSS('font-size', '13px');
          await expect(panel.locator('.board-focus-toggle')).toHaveCount(0);
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
  const { deckName } = await fixture(page);
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
    .getByRole('button', { name: 'Manage opening books', exact: true })
    .click();
  await expect(page.locator('.library-dialog h2')).toHaveCSS(
    'font-size',
    '20px',
  );
  await page
    .locator('.library-dialog')
    .getByRole('button', { name: 'Cancel', exact: true })
    .click();
  // Closing Opening books returns to Settings, which is then closed itself.
  await expect(page.locator('.library-dialog')).toHaveCount(0);
  await closeSettings(page.locator('.settings-dialog'));
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
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('region', { name: 'Study', exact: true });
  await expect(
    study.locator('.learning-card-list .position-match-name').first(),
  ).toHaveCSS('font-family', /monospace/);
  await study
    .getByRole('button', { name: 'Manage decks', exact: true })
    .click();
  const deckLibrary = page.getByRole('dialog', {
    name: 'Manage decks',
    exact: true,
  });
  await expect(deckLibrary.getByLabel('Deck name')).toHaveCSS(
    'font-size',
    '13px',
  );
  // The list shows 25 decks per page; decks of earlier tests can fill it.
  await deckLibrary.getByRole('searchbox').fill(deckName);
  await deckLibrary
    .getByRole('listitem')
    .filter({ has: page.getByText(deckName, { exact: true }) })
    .getByRole('button', { name: 'Rename deck', exact: true })
    .click();
  dialog = page.getByRole('dialog', { name: 'Rename deck', exact: true });
  await expect(dialog.locator('h2')).toHaveCSS('font-size', '20px');
  await expect(dialog.locator('h2')).toHaveCSS('font-weight', '600');
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await deckLibrary
    .getByRole('button', { name: 'Cancel', exact: true })
    .click();
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

test('analysis statistics and the move history use shared typography', async ({
  page,
}) => {
  await fixture(page);
  const dialog = await openFullMatchAnalysis(page);
  await dialog
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('0');
  await submitFullMatchAnalysis(page, dialog);
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'Match analysis complete',
  );
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  const stats = page.locator('.match-statistics .statistics-games-table');
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
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  const errorsPanel = page.getByRole('tabpanel', { name: 'Cube', exact: true });
  const history = page.locator('.analysis-workspace .moves-panel');
  await expect(
    history.locator('.move-history-details > strong').first(),
  ).toHaveCSS('font-size', '12px');
  await expect(
    history.getByRole('combobox', { name: 'Filter moves by severity' }),
  ).toHaveCSS('font-size', '13px');
  await expect(errorsPanel.locator('.statistics-reset')).toHaveCSS(
    'font-size',
    '13px',
  );
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  for (const locale of ['en', 'de', 'es', 'fr', 'pl', 'it'] as const) {
    if (locale !== 'en') {
      await page.locator('.settings-trigger').click();
      const settings = page.locator('.settings-dialog');
      await settings.locator('select[id$="-language"]').selectOption(locale);
      await settings.locator('button[type="submit"]').click();
      await expect(page.locator('html')).toHaveAttribute('lang', locale);
      await closeSettings(settings, locale);
    }
    for (const viewport of [
      { width: 1800, height: 1169 },
      { width: 2560, height: 1440 },
      { width: 3840, height: 2160 },
    ]) {
      await page.setViewportSize(viewport);
      const labels = await page
        .locator(
          '.match-statistics .statistics-error-matrix [role="columnheader"]',
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
        .locator('.match-statistics .statistics-error-matrix')
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
  const { positionId } = await fixture(page);
  async function reloadWithLocale(locale: string) {
    await page.evaluate(async (locale) => {
      const state = await (await fetch('/api/configuration')).json();
      const response = await fetch('/api/configuration', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expected_revision: state.revision,
          configuration: {
            ...state.current_configuration,
            general: { ...state.current_configuration.general, locale },
          },
        }),
      });
      if (!response.ok) throw new Error(await response.text());
    }, locale);
    const restored = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === `/api/positions/${positionId}` &&
        response.ok(),
    );
    await page.reload();
    await restored;
    await expect(page.locator('html')).toHaveAttribute('lang', locale);
    await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  }
  for (const locale of ['en', 'de', 'es', 'fr', 'pl', 'it']) {
    await reloadWithLocale(locale);
    for (const [id, , title, table] of views) {
      await page.locator(`#workspace-tab-${id}`).click();
      const panel = page.locator(`#workspace-panel-${id}`);
      await expect(panel.locator(title)).toHaveCSS('font-size', '24px');
      const cell = panel.locator(`${table} tbody td`).first();
      if (id === 'analysis') {
        expect(await fontSize(cell)).toBeGreaterThanOrEqual(13);
        expect(await fontSize(cell)).toBeLessThanOrEqual(16);
      } else await expect(cell).toHaveCSS('font-size', '12px');
    }
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  await reloadWithLocale('en');
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
      const cell = panel.locator(`${table} tbody td`).first();
      if (id === 'analysis') {
        expect(await fontSize(cell)).toBeGreaterThanOrEqual(13);
        expect(await fontSize(cell)).toBeLessThanOrEqual(16);
      } else await expect(cell).toHaveCSS('font-size', '12px');
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
