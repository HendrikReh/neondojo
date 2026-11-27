import { test, expect } from './test';
import { fulfillJson } from './api-fixtures';
import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/settings/configuration/configuration-model';

const position = {
  id: '11111111-1111-4111-8111-111111111111',
  match_uuid: '22222222-2222-4222-8222-222222222222',
  ordinal: 3,
  game_no: 1,
  ply: 3,
  player: 0,
  dice: [],
  board: [
    0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    2, 0,
  ],
  position_id: '4HPwATDgc/ABMA',
  gnubg_match_id: 'cIngAAAAAAAE',
  notation: '',
};
const hit = (decision_no: number, loss: number) => ({
  reference: { job_id: 'job', game_no: 1, decision_no },
  kind: 'cube_response',
  position: { ...position, id: '00000000-0000-0000-0000-000000000000' },
  players: ['Alice', 'Bob'],
  engine: 'GNU Backgammon 1.08.003',
  plies: 2,
  depth: '2-ply',
  source: 'native',
  game_plan: 'attacking',
  mistake_kind: 'wrong_take',
  played: 'take',
  best: 'drop',
  equity_loss: loss,
  match_length: 7,
  away: [2, 4],
  crawford: false,
  cube_value: 1,
  cube_owner: null,
  practicable: true,
});

test('all analyzed decisions are filtered on the server, previewed and practiced', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.route('**/api/study/mistakes?*', (route) =>
    route.fulfill({
      json: {
        players: ['Alice', 'Bob'],
        groups: [],
        analyzed_decisions: 0,
        analyzed_matches: 0,
      },
    }),
  );
  const searches: URLSearchParams[] = [];
  await page.route('**/api/study/decisions?*', (route) => {
    const params = new URL(route.request().url()).searchParams;
    searches.push(params);
    const page_no = Number(params.get('page') ?? 1);
    return route.fulfill({
      json: {
        items: page_no === 1 ? [hit(3, 0.125), hit(9, 0.06)] : [hit(12, 0.05)],
        total: 51,
        page: page_no,
        limit: 50,
      },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  const scope = page.getByRole('group', { name: 'Position source' });
  await expect(
    scope.getByRole('button', { name: 'Saved positions' }),
  ).toHaveAttribute('aria-pressed', 'true');
  await scope.getByRole('button', { name: 'All analyzed decisions' }).click();
  const search = page.getByRole('region', { name: 'Analyzed decisions' });
  await expect(search).toContainText(
    'Choose a player to search their analyzed decisions.',
  );
  const submit = search.getByRole('button', { name: 'Search decisions' });
  await expect(submit).toBeDisabled();
  // The disabled button names what is still missing.
  await expect(submit).toHaveAccessibleDescription(
    'Choose a player to search their analyzed decisions.',
  );
  await search
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  await search
    .getByRole('combobox', { name: 'Decision type' })
    .selectOption('cube_response');
  await search.locator('.position-filter-details > summary').click();
  await search
    .getByRole('spinbutton', { name: 'Player needs (away)' })
    .fill('2');
  await submit.click();
  await expect(search).not.toContainText(
    'Choose a player to search their analyzed decisions.',
  );
  await expect(search).toContainText('51 decisions found');
  expect(Object.fromEntries(searches[0])).toEqual({
    player: 'Alice',
    kind: 'cube_response',
    min_loss: '0.02',
    away: '2',
    limit: '50',
    page: '1',
  });
  const table = search.getByRole('table', { name: 'Analyzed decisions' });
  const first = table.getByRole('row').nth(1);
  await expect(first).toContainText('Alice – Bob');
  await expect(first).toContainText('1.3');
  await expect(first).toContainText('Wrong take');
  await expect(first).toContainText('2-away / 4-away');
  await expect(first).toContainText('0.125');
  const preview = search.getByRole('complementary', {
    name: 'Decision preview',
  });
  await expect(preview.locator('[data-cube-context]')).toContainText(
    'Bob doubles to 2',
  );
  await expect(preview).toContainText(
    'Played: Take · Best: Pass · Loss: 0.125',
  );
  // A cube decision has no saved checker position: it opens in its game by
  // its number and cannot be bookmarked, but it can still join a deck.
  await expect(
    preview.getByRole('button', { name: 'Open in Analysis' }),
  ).toBeVisible();
  await expect(
    preview.getByRole('button', { name: 'Add to deck', exact: true }),
  ).toBeVisible();
  await expect(
    preview.locator('.saved-position-preview-actions button'),
  ).toHaveCount(2);

  // The whole row selects, and the arrow keys move the preview.
  const second = table.getByRole('row').nth(2);
  await second.getByText('0.060').click();
  await expect(second).toHaveAttribute('aria-selected', 'true');
  await expect(preview).toContainText('Loss: 0.060');
  await page.keyboard.press('ArrowUp');
  await expect(first).toBeFocused();
  await expect(preview).toContainText('Loss: 0.125');
  await page.keyboard.press('ArrowDown');
  await expect(preview).toContainText('Loss: 0.060');

  // Filters, results and the chosen row survive a visit to the collection.
  await scope.getByRole('button', { name: 'Saved positions' }).click();
  await expect(search).toBeHidden();
  await scope.getByRole('button', { name: 'All analyzed decisions' }).click();
  await expect(
    search.getByRole('combobox', { name: 'Player', exact: true }),
  ).toHaveValue('Alice');
  await expect(search).toContainText('51 decisions found');
  await expect(second).toHaveAttribute('aria-selected', 'true');

  // The loss heading reverses the order on the server and returns to page 1.
  const lossHeading = table.getByRole('columnheader', { name: 'Loss' });
  await expect(lossHeading).toHaveAttribute('aria-sort', 'descending');
  await lossHeading.getByRole('button').click();
  await expect(lossHeading).toHaveAttribute('aria-sort', 'ascending');
  expect(searches.at(-1)?.get('direction')).toBe('asc');
  expect(searches.at(-1)?.get('page')).toBe('1');
  await lossHeading.getByRole('button').click();
  await expect(lossHeading).toHaveAttribute('aria-sort', 'descending');
  expect(searches.at(-1)?.has('direction')).toBe(false);

  await search
    .getByRole('navigation', { name: 'Analyzed decisions' })
    .getByRole('button', { name: 'Page 2', exact: true })
    .click();
  await expect(table.getByRole('row').nth(1)).toContainText('1.12');
  expect(searches.at(-1)?.get('page')).toBe('2');
  expect(searches.at(-1)?.get('away')).toBe('2');

  await search.getByRole('button', { name: 'Practice results' }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
    exact: true,
  });
  await expect(dialog).toContainText('1 / 1');
  await expect(dialog.getByRole('button', { name: 'Take' })).toHaveAttribute(
    'aria-keyshortcuts',
    'T',
  );
  await page.keyboard.press('Escape');
  await expect(
    search.getByRole('button', { name: 'Practice results' }),
  ).toBeFocused();
});

test('Clear filters resets the decision filters but keeps the player and refreshes results', async ({
  page,
}) => {
  const config = structuredClone(defaultConfiguration);
  config.analysis.error_thresholds.error = 0.037;
  const profiles = await profileBackend(page, { configuration: config });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.route('**/api/study/mistakes?*', (route) =>
    route.fulfill({
      json: {
        players: ['Alice', 'Bob'],
        groups: [],
        analyzed_decisions: 0,
        analyzed_matches: 0,
      },
    }),
  );
  const searches: Record<string, string>[] = [];
  await page.route('**/api/study/decisions?*', (route) => {
    searches.push(
      Object.fromEntries(new URL(route.request().url()).searchParams),
    );
    return route.fulfill({
      json: { items: [hit(3, 0.125)], total: 1, page: 1, limit: 50 },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await page
    .getByRole('group', { name: 'Position source' })
    .getByRole('button', { name: 'All analyzed decisions' })
    .click();
  const search = page.getByRole('region', { name: 'Analyzed decisions' });
  const player = search.getByRole('combobox', { name: 'Player', exact: true });
  await player.selectOption('Alice');
  const kind = search.getByRole('combobox', { name: 'Decision type' });
  await kind.selectOption('cube_response');
  const minLoss = search.getByRole('spinbutton', { name: 'Equity loss from' });
  await expect(minLoss).toHaveValue('0.037');
  await minLoss.fill('');
  const maxLoss = search.getByRole('spinbutton', { name: 'Equity loss to' });
  await maxLoss.fill('0.5');
  await search.getByRole('button', { name: 'Search decisions' }).click();
  await expect.poll(() => searches.at(-1)?.max_loss).toBe('0.5');

  expect(searches.at(-1)).not.toHaveProperty('min_loss');
  config.analysis.error_thresholds.error = 0.048;
  profiles.updateRemote(config);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(minLoss).toHaveValue('');

  await search
    .getByRole('button', { name: 'Clear filters', exact: true })
    .click();
  await expect(player).toHaveValue('Alice');
  await expect(kind).toHaveValue('');
  await expect(maxLoss).toHaveValue('');
  await expect
    .poll(() => searches.at(-1))
    .toEqual({ player: 'Alice', min_loss: '0.048', limit: '50', page: '1' });
  await expect(minLoss).toHaveValue('0.048');
  config.analysis.error_thresholds.error = 0.055;
  profiles.updateRemote(config);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(minLoss).toHaveValue('0.055');
  await expect.poll(() => searches.at(-1)?.min_loss).toBe('0.055');
  await minLoss.fill('0');
  await search.getByRole('button', { name: 'Search decisions' }).click();
  await expect.poll(() => searches.at(-1)?.min_loss).toBe('0');
});

test('decision search has a visible heading, a column picker and resizable columns', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.route('**/api/study/mistakes?*', (route) =>
    route.fulfill({
      json: {
        players: ['Alice', 'Bob'],
        groups: [],
        analyzed_decisions: 0,
        analyzed_matches: 0,
      },
    }),
  );
  await page.route('**/api/study/decisions?*', (route) =>
    route.fulfill({
      json: {
        items: [hit(3, 0.125), hit(9, 0.06)],
        total: 2,
        page: 1,
        limit: 50,
      },
    }),
  );
  const search = page.getByRole('region', { name: 'Analyzed decisions' });
  const run = async () => {
    await page.getByRole('tab', { name: 'Positions', exact: true }).click();
    await page
      .getByRole('group', { name: 'Position source' })
      .getByRole('button', { name: 'All analyzed decisions' })
      .click();
    await search
      .getByRole('combobox', { name: 'Player', exact: true })
      .selectOption('Alice');
    await search.getByRole('button', { name: 'Search decisions' }).click();
    await expect(search.getByRole('status')).toHaveText('2 decisions found');
  };
  await page.goto('/');
  await run();
  await expect(
    search.getByRole('heading', { level: 1, name: 'Analyzed decisions' }),
  ).toBeVisible();
  const headers = search.locator('.decision-search-table th');
  const visibleColumns = () =>
    headers.evaluateAll((cells) =>
      cells.map((cell) => cell.getAttribute('data-column')),
    );
  const all = [
    'match',
    'move',
    'kind',
    'score',
    'cube',
    'played',
    'best',
    'loss',
    'source',
  ];
  await expect.poll(visibleColumns).toEqual(all);
  const scroll = search.locator('.position-table-scroll');
  expect(await scroll.evaluate((el) => el.scrollWidth)).toBe(
    await scroll.evaluate((el) => el.clientWidth),
  );

  const picker = search.locator('.library-column-picker');
  await picker.locator('summary').click();
  await picker.getByRole('checkbox', { name: 'Cube', exact: true }).uncheck();
  await picker.getByRole('checkbox', { name: 'Loss', exact: true }).uncheck();
  await expect
    .poll(visibleColumns)
    .toEqual(all.filter((id) => id !== 'cube' && id !== 'loss'));
  // The results stay sorted by loss and say so while the column is hidden.
  await expect(search.locator('.library-hidden-sort')).toHaveText(
    'Sort: Loss (Descending)',
  );
  await expect(
    search.getByRole('table').getByRole('row').nth(1).locator('td'),
  ).toHaveCount(7);

  await page.reload();
  await run();
  await expect
    .poll(visibleColumns)
    .toEqual(all.filter((id) => id !== 'cube' && id !== 'loss'));
  await picker.locator('summary').click();
  await picker
    .getByRole('button', { name: 'Default columns', exact: true })
    .click();
  await expect.poll(visibleColumns).toEqual(all);

  const reset = search.getByRole('button', { name: 'Reset column widths' });
  await expect(reset).toBeDisabled();
  const match = search.locator('th[data-column="match"]');
  const before = (await match.boundingBox())!.width;
  const handle = match.getByRole('separator');
  const box = (await handle.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2, {
    steps: 5,
  });
  await page.mouse.up();
  await expect
    .poll(async () => (await match.boundingBox())!.width)
    .toBeGreaterThan(before + 40);
  await expect(reset).toBeEnabled();
  await reset.click();
  await expect
    .poll(async () => Math.round((await match.boundingBox())!.width))
    .toBe(Math.round(before));
});

test('a pre-filled player lists its decisions at once and hits join a deck one by one or by page', async ({
  page,
}, testInfo) => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.analysis.error_thresholds.error = 0.035;
  await profileBackend(page, { configuration });
  let releaseConfiguration!: () => void;
  const waiting = new Promise<void>((resolve) => {
    releaseConfiguration = resolve;
  });
  await page.route('**/api/configuration', async (route) => {
    await waiting;
    await route.fallback();
  });
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.route('**/api/study/mistakes?*', (route) =>
    route.fulfill({
      json: {
        players: ['Alice', 'Bob'],
        resolved_player: 'Alice',
        groups: [],
        analyzed_decisions: 0,
        analyzed_matches: 0,
      },
    }),
  );
  const searches: URLSearchParams[] = [];
  const jobId = '33333333-3333-4333-8333-333333333333';
  const checker = { job_id: jobId, game_no: 1, decision_no: 4 };
  const cube = { job_id: jobId, game_no: 1, decision_no: 7 };
  await page.route('**/api/study/decisions?*', (route) => {
    searches.push(new URL(route.request().url()).searchParams);
    return route.fulfill({
      json: {
        items: [
          {
            ...hit(4, 0.091),
            reference: checker,
            kind: 'checker',
            position: { ...position, dice: [3, 1] },
            mistake_kind: 'checker',
            played: '24/23 13/10',
            best: '8/5 6/5',
          },
          { ...hit(7, 0.06), reference: cube },
        ],
        total: 2,
        page: 1,
        limit: 50,
      },
    });
  });
  await page.route(`**/api/positions/${position.id}/bookmark`, (route) =>
    route.fulfill({ json: { saved: false } }),
  );
  const deck = {
    id: '44444444-4444-4444-8444-444444444444',
    name: 'Openings',
    card_count: 3,
    due_count: 0,
    created_at: '2026-11-20T10:00:00Z',
    version: 1,
  };
  await page.route('**/api/decks', (route) =>
    route.request().method() === 'GET'
      ? route.fulfill({ json: [deck] })
      : route.fallback(),
  );
  await page.route(`**/api/decks/${deck.id}/next`, (route) =>
    fulfillJson<'NextCard'>(route, {
      card: null,
      counts: { total: deck.card_count, due: 0, new: 0 },
    }),
  );
  await page.route(`**/api/decks/${deck.id}/cards?*`, (route) =>
    fulfillJson<'CardPage'>(route, { items: [], total: 0 }),
  );
  const added: unknown[] = [];
  await page.route(`**/api/decks/${deck.id}/decisions`, (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    const body = route.request().postDataJSON() as {
      decisions: (typeof checker)[];
    };
    added.push(body);
    const first = added.length === 1;
    deck.card_count += 1;
    return route.fulfill({
      json: {
        added: 1,
        updated: 0,
        total: deck.card_count,
        items: body.decisions.map((reference, index) => ({
          reference,
          status: first || index === 1 ? 'added' : 'duplicate',
          card_id: `55555555-5555-4555-8555-55555555555${index}`,
        })),
      },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await page
    .getByRole('group', { name: 'Position source' })
    .getByRole('button', { name: 'All analyzed decisions' })
    .click();
  const search = page.getByRole('region', { name: 'Analyzed decisions' });
  await expect(
    search.getByRole('combobox', { name: 'Player', exact: true }),
  ).toHaveValue('Alice');
  // My player profile's decisions are listed without a click, once.
  await expect(search).toContainText('2 decisions found');
  expect(searches).toHaveLength(1);
  expect(searches[0].get('player')).toBe('Alice');
  expect(searches[0].get('page')).toBe('1');
  expect(searches[0].get('min_loss')).toBe('0.02');
  releaseConfiguration();
  await expect.poll(() => searches.at(-1)?.get('min_loss')).toBe('0.035');
  await expect(
    search.getByRole('spinbutton', { name: 'Equity loss from' }),
  ).toHaveValue('0.035');
  await expect(search).not.toContainText('Select “Search decisions”');
  await page.screenshot({ path: testInfo.outputPath('decision-auto.png') });

  const preview = search.getByRole('complementary', {
    name: 'Decision preview',
  });
  const addToDeck = preview.getByRole('button', {
    name: 'Add to deck',
    exact: true,
  });
  await addToDeck.click();
  const dialog = page.getByRole('dialog', { name: 'Add to deck', exact: true });
  await expect(
    dialog.getByRole('combobox', { name: 'Choose deck', exact: true }),
  ).toHaveValue(deck.id);
  await expect(dialog).toContainText('3 cards in deck');
  await dialog
    .getByRole('button', { name: 'Add 1 decision', exact: true })
    .click();
  await expect(dialog).toBeHidden();
  expect(added).toEqual([{ decisions: [checker] }]);
  await expect(preview.getByRole('status')).toHaveText(
    'Added · Openings · 4 cards in deck',
  );
  await expect(addToDeck).toBeFocused();

  // The whole page, cube decisions included, joins a deck in one request.
  const addPage = search.getByRole('button', {
    name: 'Add page to deck',
    exact: true,
  });
  await expect(addPage).toHaveAttribute('data-action-icon', 'deck');
  await addPage.click();
  const pageDialog = page.getByRole('dialog', {
    name: 'Add page to deck',
    exact: true,
  });
  await pageDialog
    .getByRole('button', { name: 'Add 2 decisions', exact: true })
    .click();
  await expect(pageDialog).toBeHidden();
  expect(added[1]).toEqual({ decisions: [checker, cube] });
  await expect(
    search.getByRole('status').filter({ hasText: 'Added 1' }),
  ).toHaveText(
    'Added 1 · updated 0 · already in deck 1 · not available 0 · Openings · 5 cards in deck',
  );
  await expect(addPage).toBeFocused();
  await page.screenshot({ path: testInfo.outputPath('decision-added.png') });
  await search.getByRole('button', { name: 'Study this deck' }).click();
  await expect(
    page.getByRole('tab', { name: 'Flashcards & Anki' }),
  ).toHaveAttribute('aria-selected', 'true');
  const study = page.getByRole('region', { name: 'Study', exact: true });
  await expect(
    study.getByText('No cards are due in this deck right now.', {
      exact: true,
    }),
  ).toBeVisible();
  await expect(study.getByLabel('Choose deck')).toHaveValue(deck.id);
  await expect(study.getByRole('alert')).toHaveCount(0);
});
