import { expect, test } from './test';
import { POSITION_COLUMNS } from '../src/position-table-columns';
test.use({ actionTimeout: 10000 });
test('header omits import while the match database retains working file import', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await expect(
    page.getByRole('banner').getByRole('button', { name: /Import match/ }),
  ).toHaveCount(0);
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(
    page
      .locator('.match-database')
      .getByRole('button', { name: /Import match/ })
      .first(),
  ).toBeVisible();
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'database-import.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Header ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31hefe])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
});

async function fixture(
  page: import('@playwright/test').Page,
  games = 1,
  matchLength = 7,
) {
  const suffix = crypto.randomUUID();
  const database = await (
    await page.request.post('/api/databases', {
      data: { name: `Positions ${suffix}`, description: 'Browser regression' },
    })
  ).json();
  const sgf = Array.from(
    { length: games },
    () =>
      `(;GM[6]PW[Browser ${suffix}]PB[Opponent]MI[length:${matchLength}][ws:0][bs:0];W[31hefe];B[42qusu])`,
  ).join('');
  const response = await page.request.post(
    `/api/imports?database_id=${database.id}`,
    {
      multipart: {
        file: {
          name: `browser-${suffix}.sgf`,
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(sgf),
        },
      },
    },
  );
  expect(response.status()).toBe(201);
  const imported = await response.json();
  const positions = (
    await (
      await page.request.get(
        `/api/positions?match_id=${imported.match.id}&limit=100`,
      )
    ).json()
  ).items;
  for (const position of positions) {
    expect(
      (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
    ).toBe(true);
  }
  return { database, match: imported.match, positions };
}
async function openBrowser(
  page: import('@playwright/test').Page,
  databaseId: string,
) {
  await page.goto('/');
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  const pane = page.getByRole('tabpanel', { name: 'Positions', exact: true });
  await pane
    .getByRole('combobox', { name: 'Database', exact: true })
    .selectOption(databaseId);
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  return pane;
}

test('deck memberships refresh on later result pages and retain existing column widths', async ({
  page,
}) => {
  const legacyWidths = Object.fromEntries(
    POSITION_COLUMNS.filter((column) => column.id !== 'decks').map((column) => [
      column.id,
      column.id === 'player' ? 221 : 180,
    ]),
  );
  await page.addInitScript((widths) => {
    localStorage.setItem('bgtutor.locale', 'en');
    if (!localStorage.getItem('bgtutor.positions.columns'))
      localStorage.setItem(
        'bgtutor.positions.columns',
        JSON.stringify({ version: 1, widths }),
      );
  }, legacyWidths);
  const f = await fixture(page, 14, 43);
  const target = f.positions[26];
  const initial = await (
    await page.request.get(
      `/api/position-browser?limit=100&filters=${encodeURIComponent(JSON.stringify({ schema_version: 1, database_id: f.database.id }))}`,
    )
  ).json();
  const priorDecks: { id: string; name: string }[] = initial.items.find(
    (hit: { position: { id: string } }) => hit.position.id === target.id,
  ).decks;
  const job = await (
    await page.request.post('/api/jobs', {
      data: { position_id: target.id, kind: 'analysis', plies: 0 },
    })
  ).json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
    )
    .toBe('completed');
  const pane = await openBrowser(page, f.database.id);
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(25);
  await pane
    .getByRole('button', { name: 'Load more positions', exact: true })
    .click();
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(3);
  const row = pane.locator(`tr[data-position-id="${target.id}"]`);
  await row.locator('.position-player').click();
  await row.getByRole('checkbox').check();
  await expect(row.locator('.position-row-decks')).toHaveText(
    priorDecks.length ? priorDecks.map((deck) => deck.name).join(', ') : '—',
  );
  if (!priorDecks.length)
    await expect(pane.locator('.position-preview-decks')).toContainText(
      'Not in a deck yet',
    );
  const suffix = crypto.randomUUID(),
    names = [`Zulu ${suffix}`, `Alpha ${suffix}`];
  for (const name of names) {
    await pane.getByLabel('New deck name', { exact: true }).fill(name);
    await pane
      .getByRole('button', { name: 'Create deck', exact: true })
      .click();
    await expect(
      pane
        .getByRole('combobox', { name: 'Choose deck', exact: true })
        .locator('option:checked'),
    ).toHaveText(name);
    await pane
      .getByRole('button', { name: 'Add selected positions', exact: true })
      .click();
    await expect(row.locator('.position-row-decks')).toContainText(name);
    await expect(row.getByRole('checkbox')).toBeChecked();
    await expect(row).toHaveClass(/position-preview-selected/);
    await expect(pane.locator('.position-pagination')).toContainText('Page 2');
    await expect(pane.locator('.position-preview-decks')).toContainText(name);
  }
  const expected = [...priorDecks.map((deck) => deck.name), ...names]
    .sort()
    .join(', ');
  await expect(row.locator('.position-row-decks')).toHaveText(expected);
  await expect(row).toHaveAttribute('title', new RegExp('Decks: Alpha'));
  await pane
    .getByRole('button', { name: 'Add selected positions', exact: true })
    .click();
  await expect(pane.locator('[data-transfer-status="duplicate"]')).toHaveCount(
    1,
  );
  await expect(row.locator('.position-row-decks')).toHaveText(expected);
  expect((await row.boundingBox())!.height).toBeLessThanOrEqual(40);
  const player = pane.locator('th[data-column="player"]'),
    decks = pane.locator('th[data-column="decks"]');
  expect((await player.boundingBox())!.width).toBeCloseTo(221, 0);
  expect((await decks.boundingBox())!.width).toBeCloseTo(144, 0);
  await decks.getByRole('separator').focus();
  await page.keyboard.press('ArrowRight');
  const resized = (await decks.boundingBox())!.width;
  expect(resized).toBeGreaterThan(144);
  await pane.locator('button[data-sort="decks"]').click();
  await expect(decks).toHaveAttribute('aria-sort', 'ascending');
  await expect(
    pane
      .locator('.position-table tbody tr')
      .first()
      .locator('.position-row-decks'),
  ).toHaveText(expected);
  // Persisted membership and a newly added column coexist with the old layout profile.
  await page.reload();
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await pane
    .getByRole('combobox', { name: 'Database', exact: true })
    .selectOption(f.database.id);
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(25);
  await expect(
    pane.locator(
      `tr[data-position-id="${f.positions[0].id}"] .position-row-decks`,
    ),
  ).toHaveText(expected);
  expect((await player.boundingBox())!.width).toBeCloseTo(221, 0);
  expect((await decks.boundingBox())!.width).toBeCloseTo(resized, 0);
});

test('position notes filter exact decisions, search definitions persist, and pagination stays contextual', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  const f = await fixture(page, 15);
  const annotated = await page.request.put(
    `/api/positions/${f.positions[0].id}/annotation`,
    {
      data: {
        expected_version: 0,
        tags: ['only-here'],
        comment: 'Position browser needle 50%_literal',
      },
    },
  );
  expect(annotated.ok()).toBe(true);
  const pane = await openBrowser(page, f.database.id),
    rows = pane.locator('.position-table tbody tr');
  await expect(rows).toHaveCount(25);
  await pane
    .getByRole('button', { name: 'Load more positions', exact: true })
    .click();
  await expect(rows).toHaveCount(5);
  await pane.locator('.position-filter-details summary').click();
  await pane.getByLabel('Exact tag', { exact: true }).fill('only-here');
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toHaveAttribute(
    'data-position-id',
    f.positions[0].id,
  );
  await expect(rows.first()).toContainText(
    'Position browser needle 50%_literal',
  );
  await pane.locator('.position-saved-searches summary').click();
  const name = `Needle ${crypto.randomUUID()}`;
  await pane.getByLabel('Search name', { exact: true }).fill(name);
  const savedResponse = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/position-filters' &&
      r.request().method() === 'POST',
  );
  await pane
    .getByRole('button', { name: 'Save as new search', exact: true })
    .click();
  const saved = await (await savedResponse).json();
  expect(saved.definition.tag).toBe('only-here');
  const renamed = name + ' renamed';
  await pane.getByLabel('Search name', { exact: true }).fill(renamed);
  await pane
    .getByRole('button', { name: 'Update / rename', exact: true })
    .click();
  await expect(
    pane.getByRole('combobox', { name: 'Choose saved search', exact: true }),
  ).toContainText(renamed);
  await page.reload();
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await pane.locator('.position-saved-searches summary').click();
  await pane
    .getByRole('combobox', { name: 'Choose saved search', exact: true })
    .selectOption(saved.id);
  await pane.getByRole('button', { name: 'Load search', exact: true }).click();
  await expect(
    pane.getByRole('combobox', { name: 'Database', exact: true }),
  ).toHaveValue(f.database.id);
  await expect(rows).toHaveCount(1);
  await pane.locator('.position-filter-details summary').click();
  await expect(pane.getByLabel('Exact tag', { exact: true })).toHaveValue(
    'only-here',
  );
  await pane
    .getByRole('button', { name: 'Clear filters', exact: true })
    .click();
  await pane
    .getByRole('combobox', { name: 'Database', exact: true })
    .selectOption(f.database.id);
  await pane.getByLabel('First die', { exact: true }).fill('4');
  await pane.getByLabel('Second die', { exact: true }).fill('2');
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  await expect(rows).toHaveCount(15);
  await expect(rows.first()).toContainText('Opponent');
  await rows.first().locator('.position-player').click();
  await pane
    .locator('.position-preview')
    .getByRole('button', { name: 'Open analysis', exact: true })
    .click();
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  await expect(page.locator('.position-code code')).toHaveText(
    f.positions[1].position_id,
  );
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await pane
    .getByRole('button', { name: 'Delete search', exact: true })
    .click();
  await expect(
    pane.getByRole('combobox', { name: 'Choose saved search', exact: true }),
  ).not.toContainText(renamed);
});

test('found positions flow through real analysis, partial card transfer, Anki export and a fresh study session', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  const f = await fixture(page),
    pane = await openBrowser(page, f.database.id),
    rows = pane.locator('.position-table tbody tr');
  await expect(rows).toHaveCount(2);
  await pane.getByLabel('Select this page', { exact: true }).check();
  const deckName = `Position deck ${crypto.randomUUID()}`;
  await pane.getByLabel('New deck name', { exact: true }).fill(deckName);
  await pane.getByRole('button', { name: 'Create deck', exact: true }).click();
  await expect(
    pane.getByRole('combobox', { name: 'Choose deck', exact: true }),
  ).toContainText(deckName);
  await pane
    .getByRole('button', { name: 'Add selected positions', exact: true })
    .click();
  await expect(
    pane.locator('[data-transfer-status="analysis_required"]'),
  ).toHaveCount(2);
  await expect(
    pane.getByRole('button', {
      name: 'Download entire deck as Anki',
      exact: true,
    }),
  ).toBeDisabled();
  await rows.first().locator('.position-player').click();
  await pane
    .locator('.position-preview')
    .getByRole('button', { name: 'Open analysis', exact: true })
    .click();
  await expect(page.locator('.position-code code')).toHaveText(
    f.positions[0].position_id,
  );
  await page
    .getByRole('button', { name: 'Analyze position', exact: true })
    .click();
  await expect(
    page.locator('.analysis-panel .candidates tbody tr').first(),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await expect(rows).toHaveCount(2);
  await expect(rows.first().locator('.position-analysis-needed')).toHaveCount(
    0,
  );
  await pane
    .getByRole('button', { name: 'Add selected positions', exact: true })
    .click();
  await expect(pane.locator('[data-transfer-status="added"]')).toHaveCount(1);
  await expect(
    pane.locator('[data-transfer-status="analysis_required"]'),
  ).toHaveCount(1);
  const download = page.waitForEvent('download');
  await pane
    .getByRole('button', { name: 'Download entire deck as Anki', exact: true })
    .click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/\.apkg$/);
  const path = await file.path();
  expect(path).toBeTruthy();
  const { readFileSync } = await import('node:fs');
  expect(readFileSync(path!).subarray(0, 2).toString()).toBe('PK');
  await pane
    .getByRole('button', { name: 'Study this deck', exact: true })
    .click();
  const learning = page.getByRole('region', {
    name: 'Study & Anki',
    exact: true,
  });
  await expect(learning.locator('.study-card')).toContainText(deckName);
  await expect(
    learning.getByRole('button', { name: 'Show answer', exact: true }),
  ).toBeVisible();
  await expect(learning.locator('.study-answer')).toHaveCount(0);
  await learning
    .getByRole('button', { name: 'Show answer', exact: true })
    .click();
  await expect(learning.locator('.study-answer')).toBeVisible();
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await pane
    .getByRole('button', { name: 'Study this deck', exact: true })
    .click();
  await expect(
    learning.getByRole('button', { name: 'Show answer', exact: true }),
  ).toBeVisible();
  await expect(learning.locator('.study-answer')).toHaveCount(0);
});

test('a delayed old search cannot replace a newly loaded saved search', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  const f = await fixture(page);
  const saved = await (
    await page.request.post('/api/position-filters', {
      data: {
        name: `Race ${crypto.randomUUID()}`,
        definition: {
          schema_version: 1,
          sort: 'match',
          database_id: f.database.id,
        },
      },
    })
  ).json();
  let release = () => {},
    started = () => {},
    finished = () => {},
    first = true;
  const gate = new Promise<void>((r) => {
      release = r;
    }),
    waiting = new Promise<void>((r) => {
      started = r;
    }),
    fulfilled = new Promise<void>((r) => {
      finished = r;
    });
  await page.route('**/api/position-browser?**', async (route) => {
    if (!first) return route.continue();
    first = false;
    const response = await route.fetch();
    started();
    await gate;
    await route.fulfill({ response }).catch(() => {});
    finished();
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await waiting;
  const pane = page.getByRole('tabpanel', { name: 'Positions', exact: true });
  await pane.locator('.position-saved-searches summary').click();
  await pane
    .getByRole('combobox', { name: 'Choose saved search', exact: true })
    .selectOption(saved.id);
  await pane.getByRole('button', { name: 'Load search', exact: true }).click();
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(2);
  release();
  await fulfilled;
  await page.evaluate(
    () =>
      new Promise<void>((r) =>
        requestAnimationFrame(() => requestAnimationFrame(() => r())),
      ),
  );
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(2);
  await expect(pane.locator('.position-table tbody tr').first()).toContainText(
    f.database.name,
  );
});

test('saved-search conflicts retain drafts and deleted database scopes never broaden silently', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  const f = await fixture(page),
    replacement = await fixture(page);
  const saved = await (
    await page.request.post('/api/position-filters', {
      data: {
        name: `Conflict ${crypto.randomUUID()}`,
        definition: {
          schema_version: 1,
          sort: 'match',
          database_id: f.database.id,
        },
      },
    })
  ).json();
  const pane = await openBrowser(page, f.database.id);
  await pane.locator('.position-saved-searches summary').click();
  await pane
    .getByRole('combobox', { name: 'Choose saved search', exact: true })
    .selectOption(saved.id);
  await pane.getByRole('button', { name: 'Load search', exact: true }).click();
  expect(
    (
      await page.request.put(`/api/position-filters/${saved.id}`, {
        data: {
          name: saved.name + ' external',
          definition: saved.definition,
          expected_version: saved.version,
        },
      })
    ).ok(),
  ).toBe(true);
  await pane
    .getByLabel('Search name', { exact: true })
    .fill('My retained draft');
  await pane
    .getByRole('button', { name: 'Update / rename', exact: true })
    .click();
  await expect(pane.getByRole('alert')).toContainText('409');
  await expect(pane.getByLabel('Search name', { exact: true })).toHaveValue(
    'My retained draft',
  );
  expect(
    (
      await page.request.delete(
        `/api/databases/${f.database.id}?version=${f.database.version}&delete_matches=true`,
      )
    ).ok(),
  ).toBe(true);
  await page.reload();
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await pane.locator('.position-saved-searches summary').click();
  await pane
    .getByRole('combobox', { name: 'Choose saved search', exact: true })
    .selectOption(saved.id);
  await pane.getByRole('button', { name: 'Load search', exact: true }).click();
  await expect(
    pane.getByRole('combobox', { name: 'Database', exact: true }),
  ).toHaveValue(f.database.id);
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(0);
  await expect(pane.getByText(/deleted database/)).toBeVisible();
  await pane
    .getByRole('combobox', { name: 'Database', exact: true })
    .selectOption(replacement.database.id);
  await pane
    .getByRole('button', { name: 'Update / rename', exact: true })
    .click();
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(2);
});

test('positions keep a readable active tab and contain wide results on mobile', async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  const f = await fixture(page);
  const pane = await openBrowser(page, f.database.id);
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(2);
  const tab = page.getByRole('tab', { name: 'Positions', exact: true });
  await tab.hover();
  const activeHover = await tab.evaluate((element) => {
    const probe = document.createElement('span');
    probe.style.color = 'var(--ui-accent)';
    element.append(probe);
    const color = getComputedStyle(probe).color;
    probe.remove();
    return color;
  });
  await expect.soft(tab).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  await expect.soft(tab).toHaveCSS('color', activeHover);
  await expect.soft(tab).toHaveCSS('border-bottom-color', activeHover);
  await page.setViewportSize({ width: 390, height: 844 });
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    )
    .toBe(true);
  const tableScroll = pane.locator('.position-table-scroll');
  expect(
    await tableScroll.evaluate((el) => el.scrollWidth > el.clientWidth),
  ).toBe(true);
  await pane.locator('.position-preview').scrollIntoViewIfNeeded();
  await expect(pane.locator('.position-preview .board')).toBeVisible();
});
