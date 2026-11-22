import {
  openFullMatchAnalysis,
  openMatchAnalysisOptions,
  submitFullMatchAnalysis,
} from './analysis-controls';
import { sql } from './stack';
import { test, expect, type Page } from './test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem('neondojo.locale'))
      localStorage.setItem('neondojo.locale', 'de');
  });
});

async function completedFullJob(page: Page, matchId: string) {
  const analysisUrl = `/api/matches/${matchId}/analysis`;
  const existing = await page.request.get(analysisUrl);
  await expect(existing).toBeOK();
  let job = await existing.json();
  if (job?.status === 'completed') return job;

  if (!job || job.status === 'failed') {
    const submitted = await page.request.post(analysisUrl, {
      data: { plies: 1 },
    });
    await expect(submitted).toBeOK();
    job = await submitted.json();
  }
  const jobUrl = `/api/jobs/${job.id}`;
  await expect
    .poll(
      async () => {
        const latest = await page.request.get(jobUrl);
        await expect(latest).toBeOK();
        job = await latest.json();
        return job.status;
      },
      { timeout: 30_000 },
    )
    .toBe('completed');
  return job;
}

async function openReference(page: import('@playwright/test').Page) {
  await page.goto('/');
  const imported = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      new URL(r.url()).pathname === '/api/imports',
  );
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  const match = (await (await imported).json()).match;
  await expect(
    page.getByLabel('Spiel auswählen', { exact: true }),
  ).toBeVisible();
  await expect(page.locator('.board-heading')).toContainText('Spiel 1');
  return match;
}

test('one game owns its move list, board boundaries and native analysis; full match remains optional', async ({
  page,
}) => {
  const match = await openReference(page);
  const picker = page.getByLabel('Spiel auswählen', { exact: true });
  await expect(picker.locator('option')).toHaveCount(10);
  await expect(page.locator('.moves-list .move-history-row')).toHaveCount(8);
  await expect(
    page.getByRole('button', { name: 'Weiter im Verlauf', exact: true }),
  ).toHaveCount(0);
  const requests: string[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/positions')
      requests.push(request.url());
  });
  await picker.selectOption('10');
  await expect(page.locator('.board-heading')).toContainText('Spiel 10');
  expect(requests.length).toBeGreaterThan(0);
  expect(
    requests.every((url) => new URL(url).searchParams.get('game_no') === '10'),
  ).toBe(true);
  expect(
    (await page.locator('.move-no').allTextContents()).every((text) =>
      text.startsWith('10.'),
    ),
  ).toBe(true);
  await page.locator('.moves-list .move-history-row').last().click();
  await expect(
    page.getByRole('button', { name: 'Nächster Zug', exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole('button', { name: 'Nächstes Spiel', exact: true }),
  ).toBeDisabled();
  await picker.selectOption('2');
  await expect(page.locator('.board-heading')).toContainText('Spiel 2');
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  const analysis = page.locator('.match-analysis');
  await openMatchAnalysisOptions(page);
  await expect(page.getByLabel('Angezeigte Analyse', { exact: true })).toBeVisible();
  const depth = page.getByLabel('Match-Analysetiefe', { exact: true });
  await depth.selectOption('1');
  const scoped = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      new URL(r.url()).pathname === `/api/matches/${match.id}/analysis`,
  );
  await page
    .getByRole('button', {
      name: /^(Spiel analysieren|Spiel erneut analysieren)$/,
    })
    .click();
  const response = await scoped;
  expect(response.request().postDataJSON()).toEqual({
    engine_id: 'gnubg',
    plies: 1,
    game_no: 2,
  });
  const job = await response.json();
  expect(job.game_no).toBe(2);
  expect(job.total_games).toBe(1);
  await expect(analysis).toHaveAttribute('data-job-id', job.id);
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByText('Spielanalyse abgeschlossen', { exact: true }),
  ).toBeVisible({ timeout: 30_000 });
  expect(
    (await analysis.locator('tbody .position-link').allTextContents()).every(
      (text) => text.trim().startsWith('2'),
    ),
  ).toBe(true);
  await expect(page.locator('.annotation-panel')).toHaveCount(1);
  await page.screenshot({
    path: 'test-results/game-selection-desktop.png',
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: 'test-results/game-selection-mobile.png',
    fullPage: true,
  });
  await page.setViewportSize({ width: 1440, height: 1100 });
  await openMatchAnalysisOptions(page);
  const full = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      new URL(r.url()).pathname === `/api/matches/${match.id}/analysis`,
  );
  const dialog = await openFullMatchAnalysis(page);
  await dialog.getByLabel('Match-Analysetiefe', { exact: true }).selectOption('1');
  await submitFullMatchAnalysis(page, dialog);
  const fullResponse = await full;
  expect(fullResponse.request().postDataJSON()).toEqual({
    engine_id: 'gnubg',
    plies: 1,
  });
  const fullJob = await fullResponse.json();
  expect(fullJob.game_no).toBeNull();
  expect(fullJob.total_games).toBe(10);
  await expect(analysis).toHaveAttribute('data-job-id', fullJob.id);
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByText('Matchanalyse abgeschlossen', { exact: true }),
  ).toBeVisible({ timeout: 30_000 });
});

test('a note result jumps directly to its game without loading earlier games', async ({
  page,
}) => {
  const match = await openReference(page);
  const position = (
    await (
      await page.request.get(
        `/api/positions?match_id=${match.id}&game_no=8&limit=1`,
      )
    ).json()
  ).items[0];
  const current = await (
    await page.request.get(`/api/positions/${position.id}/annotation`)
  ).json();
  const marker = `game-jump-${crypto.randomUUID()}`;
  expect(
    (
      await page.request.put(`/api/positions/${position.id}/annotation`, {
        data: {
          tags: ['game-jump'],
          comment: marker,
          expected_version: current.version,
        },
      })
    ).status(),
  ).toBe(200);
  await page.getByRole('tab', { name: 'Matchdatenbank', exact: true }).click();
  const search = page.locator('.annotation-search');
  await page.locator('.library-search-toolbar').getByRole('button', { name: 'Notizen', exact: true }).click();
  await search.getByLabel('Text', { exact: true }).fill(marker);
  await search
    .getByRole('button', { name: 'Notizen suchen', exact: true })
    .click();
  const calls: string[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/positions')
      calls.push(request.url());
  });
  await search
    .getByRole('button', { name: 'Position öffnen', exact: true })
    .click();
  await expect(page.getByLabel('Spiel auswählen', { exact: true })).toHaveValue(
    '8',
  );
  await expect(page.locator('.board-heading')).toContainText('Spiel 8');
  await expect(page.locator('[data-annotation-kind=position]')).toHaveAttribute(
    'data-annotation-id',
    position.id,
  );
  expect(calls.length).toBeGreaterThan(0);
  expect(
    calls.every((url) => new URL(url).searchParams.get('game_no') === '8'),
  ).toBe(true);
});

test('a long individual game transparently loads all pages within that game', async ({
  page,
}) => {
  // Two lone checkers repeatedly hit each other. All 126 legal 1-1 rolls fit
  // in one ongoing game, including two internal API pages.
  const positions = [25, 24];
  let source = `(;GM[6]PW[Long game ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0]AE[a:y]AW[y]AB[x]PL[W]`;
  for (let turn = 0; turn < 126; turn++) {
    const player = turn % 2;
    let move = '';
    for (let die = 0; die < 4; die++) {
      const from = positions[player];
      const to =
        from === 25 ? (player === 0 ? 24 : 1) : from + (player === 0 ? -1 : 1);
      if (to === positions[1 - player]) positions[1 - player] = 25;
      positions[player] = to;
      move += String.fromCharCode(96 + from, 96 + to);
    }
    source += `;${player === 0 ? 'W' : 'B'}[11${move}]`;
  }
  source += ')';
  await page.goto('/');
  const calls: string[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/positions')
      calls.push(request.url());
  });
  await page.getByLabel('Matchdatei').setInputFiles({
    name: 'long-game.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(source),
  });
  await expect(page.locator('.moves-list .move-history-row')).toHaveCount(126);
  expect(calls).toHaveLength(2);
  expect(
    calls.every((url) => new URL(url).searchParams.get('game_no') === '1'),
  ).toBe(true);
  await page.locator('.moves-list .move-history-row').last().click();
  await expect(page.locator('.board-navigation')).toContainText('126 / 126');
  await expect(
    page.getByRole('button', { name: 'Nächster Zug', exact: true }),
  ).toBeDisabled();
});

test('changing Cube filters resets the decision count', async ({ page }) => {
  const match = await openReference(page);
  await completedFullJob(page, match.id);
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  await expect(page.locator('.workspace-analysis-command').getByText('Matchanalyse abgeschlossen', { exact: true })).toBeVisible();
  // Opening Cube selects the first matching decision, which stays selected.
  await expect(page.locator('.board-heading')).toContainText('Cube');
  const panel = page.getByRole('tabpanel', { name: 'Cube', exact: true });
  await panel.getByLabel('Nur Auffälligkeiten').uncheck();
  const count = page.locator('.board-error-count');
  await expect(count).toHaveText(/^Entscheidung \d+ von /);
  const total = await count.innerText();
  const player = panel.getByLabel('Analyse nach Spieler filtern');
  await player.selectOption('0');
  await expect(count).not.toHaveText(total);
  await player.selectOption('');
  await expect(count).toHaveText(total);
});

test('a cube line in the move history opens its review and is marked', async ({
  page,
}) => {
  const match = await openReference(page);
  await completedFullJob(page, match.id);
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByText('Matchanalyse abgeschlossen', { exact: true }),
  ).toBeVisible();
  const line = page.locator('.moves-list [data-cube-action]').last();
  await expect(line).toContainText('Passen');
  await line.click();
  await expect(
    page.getByRole('tab', { name: 'Cube', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(line).toHaveAttribute('aria-current', 'true');
  await expect(page.locator('.moves-list .move-history-row.current')).toHaveCount(
    0,
  );
  await expect(page.locator('.board-heading')).toContainText('Cube');
  await page
    .getByRole('button', { name: 'Zurück zum Checkerzug', exact: true })
    .click();
  await expect(line).not.toHaveAttribute('aria-current', 'true');
  await expect(page.locator('.moves-list .move-history-row.current')).toHaveCount(
    1,
  );
});

test('a saved whole-match result stays identified while game navigation loads', async ({
  page,
}) => {
  const match = await openReference(page);
  const fullJob = await completedFullJob(page, match.id);
  expect(fullJob?.status).toBe('completed');
  let release = () => {};
  let requested = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    requested = resolve;
  });
  await page.route('**/api/matches/*/games', async (route) => {
    requested();
    await gate;
    await route.continue();
  });
  await page.reload();
  await started;
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  const analysis = page.locator('.match-analysis');
  try {
    await expect(
      page.getByRole('button', { name: 'Spiel analysieren', exact: true }),
    ).toBeDisabled();
    await expect(analysis).toHaveAttribute('data-job-id', fullJob.id);
    await expect(analysis).toHaveAttribute('aria-label', 'Vollständige Matchanalyse');
    await expect(page.getByLabel('Angezeigte Analyse').locator('option:checked')).toContainText('Ganzes Match');
    await expect(page.locator('.match-analysis-progress:visible')).toContainText('Matchanalyse abgeschlossen');
  } finally {
    release();
  }
  await page.unrouteAll({ behavior: 'wait' });
  // Cube opens on the game's first matching cube decision.
  await expect(page.locator('.board-heading')).toContainText(/Spiel 1|^1 · Cube/);
  await expect(page.getByRole('button', { name: 'Spiel analysieren', exact: true })).toBeEnabled();
});

test('stepping through moves keeps move colors and statistics without reloading them', async ({
  page,
  databaseUrl,
}, info) => {
  await page.goto('/');
  const imported = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      new URL(r.url()).pathname === '/api/imports',
  );
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  const match = (await (await imported).json()).match;
  const games: { game_no: number; position_count: number }[] = await (
    await page.request.get(`/api/matches/${match.id}/games`)
  ).json();
  const game = games.find((entry) => entry.position_count > 11);
  expect(game, 'a game with at least eleven positions').toBeDefined();
  const assessed = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname ===
        `/api/matches/${match.id}/move-assessments` &&
      new URL(r.url()).searchParams.get('game_no') === String(game!.game_no),
  );
  await page
    .getByLabel('Spiel auswählen', { exact: true })
    .selectOption(String(game!.game_no));
  await expect(page.locator('.board-heading')).toContainText(
    `Spiel ${game!.game_no}`,
  );
  await assessed;
  const colored = page.locator(
    '.moves-list .move-history-row[class*="move-assessment-"]',
  );
  await expect(colored.first()).toBeVisible();
  const coloredRows = await colored.count();
  await page.waitForLoadState('networkidle');

  // Record any render in which the move colors are gone.
  await page.evaluate(() => {
    const state = window as unknown as { minColored: number };
    const count = () =>
      document.querySelectorAll(
        '.moves-list .move-history-row[class*="move-assessment-"]',
      ).length;
    state.minColored = count();
    new MutationObserver(() => {
      state.minColored = Math.min(state.minColored, count());
    }).observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ['class'],
    });
  });
  const reads: string[] = [];
  const bodies: Promise<number>[] = [];
  const derived = (url: string) => {
    const path = new URL(url).pathname;
    return (
      path.endsWith('/move-assessments') ||
      path.endsWith('/analysis-statistics')
    );
  };
  page.on('request', (request) => {
    if (derived(request.url()))
      reads.push(new URL(request.url()).pathname.split('/').at(-1)!);
  });
  page.on('requestfinished', (request) => {
    if (derived(request.url()))
      bodies.push(request.sizes().then((sizes) => sizes.responseBodySize));
  });
  async function measure(phase: string) {
    const endpoints = reads.splice(0);
    const bytes = (await Promise.all(bodies.splice(0))).reduce(
      (total, size) => total + size,
      0,
    );
    console.log(
      `Analysis navigation ${phase}: ${endpoints.length} requests, ${bytes} body bytes`,
    );
    return endpoints.sort();
  }
  const visited = new Set<string>();
  async function step(name: 'Nächster Zug' | 'Vorheriger Zug') {
    const before = await page.locator('.position-code code').textContent();
    // Candidates still follow each move.
    const result = page.waitForResponse((r) =>
      /^\/api\/positions\/[^/]+\/analysis-result$/.test(
        new URL(r.url()).pathname,
      ),
    );
    await page.getByRole('button', { name, exact: true }).click();
    await expect(page.locator('.position-code code')).not.toHaveText(
      before ?? '',
    );
    visited.add(new URL((await result).url()).pathname);
  }
  for (let move = 0; move < 10; move++) await step('Nächster Zug');
  for (let move = 0; move < 10; move++) await step('Vorheriger Zug');
  await page.waitForLoadState('networkidle');

  expect(visited.size).toBeGreaterThanOrEqual(10);
  expect(await measure('20 stable moves')).toEqual([]);
  expect(
    await page.evaluate(
      () => (window as unknown as { minColored: number }).minColored,
    ),
  ).toBe(coloredRows);
  await expect(colored).toHaveCount(coloredRows);

  // The replacement keeps the match UUID and the imported analysis ID.
  const revision = Number(
    (
      await sql(
        new URL(databaseUrl),
        `UPDATE matches SET source_revision=source_revision+1 WHERE id='${match.id}' RETURNING source_revision`,
      )
    ).split('\n')[0],
  );
  expect(revision).toBeGreaterThan(0);
  const refreshed = page.waitForResponse(
    async (response) =>
      new URL(response.url()).pathname ===
        `/api/matches/${match.id}/analyses` &&
      (await response.json()).source_revision === revision,
  );
  await step('Nächster Zug');
  await refreshed;
  let revisionEndpoints: string[];
  try {
    await expect.poll(() => bodies.length).toBe(2);
  } finally {
    revisionEndpoints = await measure('source revision');
  }
  expect(revisionEndpoints).toEqual([
    'analysis-statistics',
    'move-assessments',
  ]);
  await expect(colored).toHaveCount(coloredRows);

  const workspace = page.locator('#workspace-panel-analysis');
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await step('Vorheriger Zug');
    await step('Nächster Zug');
    await expect(workspace.locator('.board-heading')).toBeInViewport();
    await expect(
      workspace.locator('.moves-list .move-history-row.current'),
    ).toHaveCount(1);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    await page.screenshot({
      path: info.outputPath(`analysis-navigation-${width}.png`),
    });
  }
});
