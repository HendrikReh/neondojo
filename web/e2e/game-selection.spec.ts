import { openMatchCalculation } from './analysis-controls';
import { test, expect, type Page } from './test';

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem('bgtutor.locale'))
      localStorage.setItem('bgtutor.locale', 'de');
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
      data: { plies: 0 },
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
    .getByLabel('Partiedatei')
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
  await expect(page.locator('.moves-list button')).toHaveCount(8);
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
  await page.locator('.moves-list button').last().click();
  await expect(
    page.getByRole('button', { name: 'Nächster Zug', exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole('button', { name: 'Nächstes Spiel', exact: true }),
  ).toBeDisabled();
  await picker.selectOption('2');
  await expect(page.locator('.board-heading')).toContainText('Spiel 2');
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  const analysis = page.locator('.match-analysis');
  await openMatchCalculation(page);
  await expect(page.getByLabel('Analyseumfang', { exact: true })).toHaveValue(
    'game',
  );
  const depth = page.getByLabel('Partie-Analysetiefe', { exact: true });
  await depth.selectOption('0');
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
    plies: 0,
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
  await openMatchCalculation(page);
  await page.getByLabel('Analyseumfang', { exact: true }).selectOption('match');
  await depth.selectOption('0');
  const full = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      new URL(r.url()).pathname === `/api/matches/${match.id}/analysis`,
  );
  await page
    .getByRole('button', {
      name: /^(Gesamte Partie analysieren|Partie erneut analysieren)$/,
    })
    .click();
  const fullResponse = await full;
  expect(fullResponse.request().postDataJSON()).toEqual({
    engine_id: 'gnubg',
    plies: 0,
  });
  const fullJob = await fullResponse.json();
  expect(fullJob.game_no).toBeNull();
  expect(fullJob.total_games).toBe(10);
  await expect(analysis).toHaveAttribute('data-job-id', fullJob.id);
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByText('Partieanalyse abgeschlossen', { exact: true }),
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
  await search.locator('summary').click();
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
  await page.getByLabel('Partiedatei').setInputFiles({
    name: 'long-game.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(source),
  });
  await expect(page.locator('.moves-list button')).toHaveCount(126);
  expect(calls).toHaveLength(2);
  expect(
    calls.every((url) => new URL(url).searchParams.get('game_no') === '1'),
  ).toBe(true);
  await page.locator('.moves-list button').last().click();
  await expect(page.locator('.board-navigation')).toContainText('126 / 126');
  await expect(
    page.getByRole('button', { name: 'Nächster Zug', exact: true }),
  ).toBeDisabled();
});

test('returning to an analysis filter starts at its first page', async ({
  page,
}) => {
  const match = await openReference(page);
  await completedFullJob(page, match.id);
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  const analysis = page.locator('.match-analysis');
  await openMatchCalculation(page);
  await page.getByLabel('Analyseumfang', { exact: true }).selectOption('match');
  await expect(
    page
      .locator('.workspace-analysis-command')
      .getByText('Partieanalyse abgeschlossen', { exact: true }),
  ).toBeVisible();
  await analysis.getByLabel('Nur Auffälligkeiten').uncheck();
  await expect(analysis.locator('tbody tr')).toHaveCount(50);
  const first = await analysis.locator('tbody tr').first().textContent();
  await analysis
    .getByRole('button', { name: 'Nächste Seite', exact: true })
    .click();
  await expect(
    analysis.getByRole('button', { name: 'Seite 2', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
  await expect(analysis.locator('tbody tr')).toHaveCount(50);
  await analysis
    .getByLabel('Analyse nach Entscheidung filtern')
    .selectOption('cube');
  await expect(analysis.locator('tbody tr')).toHaveCount(50);
  await analysis
    .getByLabel('Analyse nach Entscheidung filtern')
    .selectOption('');
  await expect(analysis.locator('tbody tr')).toHaveCount(50);
  await expect(analysis.locator('tbody tr').first()).toHaveText(first);
});

test('a delayed game catalog never displays a whole-match job as a game job', async ({
  page,
}) => {
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
  await page.goto('/');
  const imported = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      new URL(r.url()).pathname === '/api/imports',
  );
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.mat');
  const match = (await (await imported).json()).match;
  const fullJob = await completedFullJob(page, match.id);
  expect(fullJob?.status).toBe('completed');
  await started;
  await page.getByRole('tab', { name: 'Fehler', exact: true }).click();
  const analysis = page.locator('.match-analysis');
  try {
    await openMatchCalculation(page);
    await expect(
      page.getByRole('button', { name: 'Spiel analysieren', exact: true }),
    ).toBeDisabled();
    await expect(analysis).not.toHaveAttribute('data-job-id', fullJob.id);
    await expect(page.locator('.match-analysis-progress:visible')).toHaveCount(
      0,
    );
    await expect(analysis.locator('tbody tr')).toHaveCount(0);
    await page
      .getByLabel('Analyseumfang', { exact: true })
      .selectOption('match');
    await expect(analysis).toHaveAttribute('data-job-id', fullJob.id);
    await page
      .getByLabel('Analyseumfang', { exact: true })
      .selectOption('game');
    await expect(page.locator('.match-analysis-progress:visible')).toHaveCount(
      0,
    );
  } finally {
    release();
  }
  await page.unrouteAll({ behavior: 'wait' });
  await expect(page.locator('.board-heading')).toContainText('Spiel 1');
});
