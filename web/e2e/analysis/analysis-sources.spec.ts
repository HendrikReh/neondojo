import { waitForJobCompletion } from '../helpers/match-fixtures';
import { seedLocale } from '../helpers/navigation';
import {
  calculateDiceEquity,
  openPositionCalculation,
} from '../helpers/analysis-controls';
import { expect, test } from '../helpers/test';
import { readFileSync } from 'node:fs';

test('the catalog shows saved match candidates without recalculating the position', async ({
  page,
}) => {
  await seedLocale(page);
  const standaloneJobs: string[] = [];
  page.on('request', (request) => {
    if (
      request.method() === 'POST' &&
      new URL(request.url()).pathname === '/api/jobs'
    )
      standaloneJobs.push(request.url());
  });
  await page.goto('/');
  const imported = page.waitForResponse(
    (response) => new URL(response.url()).pathname === '/api/imports',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'saved-match-analysis.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]FF[4]PW[Saved ${crypto.randomUUID()}]PB[Opponent]MI[length:21][ws:0][bs:0];B[63lolr])`,
    ),
  });
  const { match } = await (await imported).json();
  const started = await page.request.post(`/api/matches/${match.id}/analysis`, {
    data: { engine_id: 'gnubg', plies: 1 },
  });
  expect(started.ok()).toBeTruthy();
  const job = await started.json();
  await waitForJobCompletion(page, job.id);
  await page.reload();
  await expect(page.getByLabel('Displayed analysis')).toBeVisible();
  const panel = page.locator('.analysis-panel');
  await expect(
    panel.locator('.candidate-comparison-table tbody tr'),
  ).toHaveCount(14);
  await expect(panel.locator('tr[data-candidate-played="true"]')).toContainText(
    /13\/10|13\/7/,
  );
  await expect(panel.locator('tr[data-candidate-best="true"]')).toBeVisible();
  await expect(
    page.getByLabel('Displayed analysis').locator('option:checked'),
  ).toContainText('Entire match');
  await expect(panel).toContainText(/GNU Backgammon/);
  expect(standaloneJobs).toEqual([]);
});

test('imported GNUbg analysis remains available independently from new engine jobs', async ({
  page,
}) => {
  await seedLocale(page);
  await page.goto('/');
  // Export-native A annotations record candidates and their original evaluation depth.
  const imported = page.waitForResponse(
    (r) => new URL(r.url()).pathname === '/api/imports',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'retained-analysis.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      readFileSync(
        '../backend/tests/fixtures/analysis-game1.sgf',
        'utf8',
      ).replace(/PW\[[^\]]*\]/, `PW[Source ${crypto.randomUUID()}]`),
    ),
  });
  const response = await (await imported).json();
  expect(response.match).toBeTruthy();
  await expect(page.getByLabel('Displayed analysis')).toBeVisible();
  await selectImported(page);
  const importedEngine = page.locator(
    '.source-analysis:visible .candidate-metadata .engine-label-details',
  );
  const tooltip = page.getByRole('tooltip');
  await expect(importedEngine).toHaveText('GNU Backgammon 1.05.002');
  await importedEngine.hover();
  await expect(tooltip).toContainText('1-ply');
  await page.keyboard.press('Escape');
  await expect(page.locator('.source-analysis:visible')).toContainText('0.014');
  await (
    await openPositionCalculation(page)
  )
    .getByRole('button', { name: 'Analyze position', exact: true })
    .click();
  const choice = page.getByLabel('Displayed analysis');
  await expect(choice.locator('option[value^="job:"]')).toHaveCount(1);
  await choice.selectOption(
    (await choice.locator('option[value^="job:"]').getAttribute('value')) ?? '',
  );
  await expect(
    page.locator('.analysis-panel .candidates tbody tr').first(),
  ).toBeVisible();
  await selectImported(page);
  await importedEngine.focus();
  await expect(tooltip).toContainText('1-ply');
  await page.keyboard.press('Escape');
  const catalog = await (
    await page.request.get(
      `/api/matches/${response.match.id}/games/1/decisions`,
    )
  ).json();
  let cube;
  for (const move of catalog.items.filter(
    (m: { kind: string }) => m.kind === 'cube',
  )) {
    const data = await (
      await page.request.get(`/api/moves/${move.id}/imported-analysis`)
    ).json();
    if (data.length) {
      cube = move;
      break;
    }
  }
  expect(cube).toBeTruthy();
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await page
    .getByRole('button', { name: 'Cube and move notes', exact: true })
    .click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Move', { exact: true }).selectOption(cube.id);
  await expect(dialog.locator('.imported-move-analysis')).toContainText(
    'GNU Backgammon',
  );
  await expect(dialog.locator('.imported-move-analysis')).toContainText('MWC');
});

test('opening-book preferences and a real opening lookup expose source and depth', async ({
  page,
}) => {
  await seedLocale(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Analysis', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Manage opening books', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'Opening books',
    exact: true,
  });
  await expect(
    dialog.getByLabel('Opening-book source', { exact: true }),
  ).toBeVisible();
  await expect(dialog).toContainText('BackgammonDB');
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Manage opening books', exact: true }),
  ).toBeFocused();
  await page.keyboard.press('Escape');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'book-hit.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Book ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await openBook(page);
  await expect(page.locator('.source-analysis:visible')).toContainText(
    'BackgammonDB',
  );
  const bookEngine = page.locator(
    '.source-analysis:visible .candidate-metadata .engine-label-details',
  );
  const tooltip = page.getByRole('tooltip');
  await expect(bookEngine).toHaveText(/^GNU Backgammon [\d.]+$/);
  await bookEngine.hover();
  await expect(tooltip).toContainText('BackgammonDB · 4-ply');
  await page.keyboard.press('Escape');
  await expect(
    page.locator('.source-analysis:visible .candidates tbody tr'),
  ).toHaveCount(8);
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await openBook(page);
  await page
    .locator('.source-analysis')
    .getByRole('button', { name: 'Cube', exact: true })
    .click();
  await expect(page.locator('.source-analysis:visible')).toContainText(
    'before this roll',
  );
  const submitted = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/jobs' &&
      r.request().method() === 'POST',
  );
  await calculateDiceEquity(page);
  const diceJob = await (await submitted).json();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  const completed = await (
    await page.request.get(`/api/jobs/${diceJob.id}`)
  ).json();
  expect(completed.status).toBe('completed');
  await page.getByRole('tab', { name: 'Candidate moves', exact: true }).click();
  await openBook(page);
  await page.getByRole('tab', { name: 'Dice equity', exact: true }).click();
  await page.locator('.distribution-caption .engine-label-details').focus();
  await expect(tooltip).toHaveText(
    `${completed.result.engine} · ${completed.result.plies}-ply`,
  );
});

test('opening-book navigation explicitly applies or preserves an engine draft', async ({
  page,
}) => {
  await seedLocale(page);
  await page.route('**/api/opening-book', (route) =>
    route.fulfill({
      json: {
        config: {
          version: 0,
          provider: 'automatic',
          path: '/local/book.sqlite',
          xg_path: '/local/XG.ob',
        },
        status: {
          available: false,
          path: '/local/book.sqlite',
          entries: 0,
          contexts: [],
          engine_version: null,
          compression: null,
          license: null,
          source_url: null,
          attribution: null,
          error: null,
        },
      },
    }),
  );
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.getByRole('button', { name: 'Analysis', exact: true }).click();
  const profile = settings.getByLabel('Default analysis profile', {
    exact: true,
  });
  await profile.selectOption('2');
  await settings
    .getByRole('button', { name: 'Manage opening books', exact: true })
    .click();
  const guard = page.getByRole('dialog', { name: 'Unapplied settings' });
  await expect(guard).toBeVisible();
  await guard
    .getByRole('button', { name: 'Keep editing', exact: true })
    .click();
  await expect(profile).toHaveValue('2');
  await settings
    .getByRole('button', { name: 'Manage opening books', exact: true })
    .click();
  await guard
    .getByRole('button', { name: 'Apply and continue', exact: true })
    .click();
  const books = page.getByRole('dialog', {
    name: 'Opening books',
    exact: true,
  });
  await expect(
    books.getByLabel('Opening-book source', { exact: true }),
  ).toBeVisible();
  await books.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(profile).toHaveValue('2');
  await expect(
    settings.getByRole('button', { name: 'Manage opening books', exact: true }),
  ).toBeFocused();
  await profile.selectOption('1');
  await settings
    .getByRole('button', { name: 'Manage opening books', exact: true })
    .click();
  await guard
    .getByRole('button', { name: 'Discard draft', exact: true })
    .click();
  await expect(books).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(books).not.toBeVisible();
  await expect(profile).toHaveValue('2');
  await settings
    .getByRole('button', { name: 'Manage opening books', exact: true })
    .click();
  await books
    .getByRole('combobox', { name: 'Opening-book source', exact: true })
    .selectOption('off');
  await books.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(books).not.toBeVisible();
  await expect(settings.getByText('Off', { exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(settings).not.toBeVisible();
});

test('XG lookup is an attributed position reference, separate from rolled candidates', async ({
  page,
}) => {
  await seedLocale(page);
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'xg-book-reference.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[XG reference ${crypto.randomUUID()}]PB[Opponent]MI[length:0][ws:0][bs:0]RU[Jacoby];W[31hefe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await openBook(page);
  await page
    .locator('.source-book-selector')
    .getByRole('button', { name: 'XG position reference', exact: true })
    .click();
  const reference = page.locator('.xg-position-reference');
  await expect(reference).toContainText('3.70');
  await expect(reference).toContainText('before the roll');
  await expect(reference.locator('.xg-book-author')).not.toHaveCount(0);
  await expect(reference.locator('.xg-book-evaluation')).not.toHaveCount(0);
  await expect(reference.locator('.candidates')).toHaveCount(0);
  await expect(
    reference.getByRole('button', { name: 'Cube', exact: true }),
  ).toHaveCount(0);
  await expect(reference).toContainText('player on roll');
});

test('a late imported source response cannot replace a newer position', async ({
  page,
}) => {
  await seedLocale(page);
  let release = () => {},
    started = () => {},
    first = true;
  const gate = new Promise<void>((resolve) => {
      release = resolve;
    }),
    waiting = new Promise<void>((resolve) => {
      started = resolve;
    });
  await page.route('**/api/positions/*/analysis-result?*', async (route) => {
    if (!first) return route.continue();
    first = false;
    const response = await route.fetch();
    started();
    await gate;
    await route.fulfill({ response });
  });
  await page.goto('/');
  const imported = page.waitForResponse(
    (r) => new URL(r.url()).pathname === '/api/imports',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'late-source.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      readFileSync(
        '../backend/tests/fixtures/analysis-game1.sgf',
        'utf8',
      ).replace(/PW\[[^\]]*\]/, `PW[Late source ${crypto.randomUUID()}]`),
    ),
  });
  const { match } = await (await imported).json();
  await waiting;
  const positions = await (
    await page.request.get(`/api/positions?match_id=${match.id}&game_no=1`)
  ).json();
  const records = await (
    await page.request.get(
      `/api/positions/${positions.items[1].id}/imported-analysis`,
    )
  ).json();
  // The table shows a combined move by its end points (13/8 8/7 as 13/7*),
  // so the newer position is identified by its best candidate's equity.
  const best = Math.max(
    ...records
      .find((r: { kind: string }) => r.kind === 'checker')
      .candidates.map((candidate: { equity: number }) => candidate.equity),
  );
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(
    page.locator('.source-analysis:visible .candidates tbody tr').first(),
  ).toContainText(best.toFixed(3));
  release();
  await page.unrouteAll({ behavior: 'wait' });
  await expect(
    page.locator('.source-analysis:visible .candidates tbody tr').first(),
  ).toContainText(best.toFixed(3));
  await expect(page.locator('.board-heading')).toContainText('Move 2');
});

async function selectImported(page: import('@playwright/test').Page) {
  const match = page.locator('.analysis-heading[data-match-id]');
  await expect(match).toBeVisible();
  const matchId = await match.getAttribute('data-match-id');
  await page
    .getByLabel('Displayed analysis')
    .selectOption(`imported:${matchId}`);
}
async function openBook(page: import('@playwright/test').Page) {
  const details = page.locator('.analysis-book-reference');
  if (
    !(await details.getAttribute('open')) &&
    (await details.getAttribute('open')) !== ''
  )
    await details.locator('summary').click();
}
