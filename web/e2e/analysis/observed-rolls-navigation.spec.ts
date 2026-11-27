import { seedLocale } from '../helpers/navigation';
import { profileBackend } from '../helpers/profile-backend';
import { backendTest, expect, test, type Page } from '../helpers/test';

function analysisWorkspace(page: Page) {
  return page.locator('#workspace-panel-analysis');
}

async function openMatch(page: Page) {
  await seedLocale(page, 'de');
  await page.goto('/');
  const imported = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      new URL(response.url()).pathname === '/api/imports',
  );
  await page
    .getByLabel('Matchdatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  const matchId: unknown = (await (await imported).json()).match?.id;
  if (typeof matchId !== 'string')
    throw new Error('Import returned no match ID');
  await expect(
    analysisWorkspace(page).locator('.moves-list .move-history-row.current'),
  ).toHaveCount(1);
  return matchId;
}

function diceRequests(page: Page, matchId: string) {
  const gameNumbers: string[] = [];
  page.on('request', (request) => {
    const url = new URL(request.url());
    if (url.pathname === `/api/matches/${matchId}/dice`)
      gameNumbers.push(url.searchParams.get('game_no') ?? '');
  });
  return gameNumbers;
}

test('game switching paints the board without waiting for observed rolls', async ({
  page,
}) => {
  const matchId = await openMatch(page);
  const requests = diceRequests(page, matchId);
  let delayMs = 0;
  await page.route(
    (url) => url.pathname === `/api/matches/${matchId}/dice`,
    async (route) => {
      if (!delayMs) return route.continue();
      const response = await route.fetch();
      await new Promise((resolve) => setTimeout(resolve, delayMs));
      await route.fulfill({ response });
    },
  );

  const workspace = analysisWorkspace(page);
  const picker = workspace.getByLabel('Spiel auswählen', { exact: true });
  async function switchGame(game: number) {
    await picker.evaluate((select, gameNo) => {
      const owner = select.closest<HTMLElement>('#workspace-panel-analysis');
      if (!owner)
        throw new Error('Game picker is outside the analysis workspace');
      delete owner.dataset.gameSwitchMs;
      select.addEventListener(
        'change',
        () => {
          const start = performance.now();
          const sample = () => {
            const heading = owner.querySelector('.board-heading');
            const currentMove = owner.querySelector(
              '.moves-list .move-history-row.current',
            );
            if (
              heading?.textContent?.includes(`Spiel ${gameNo}`) &&
              currentMove
            ) {
              owner.dataset.gameSwitchMs = String(performance.now() - start);
            } else if (performance.now() - start < 5_000) {
              requestAnimationFrame(sample);
            }
          };
          requestAnimationFrame(sample);
        },
        { capture: true, once: true },
      );
    }, game);
    await picker.selectOption(String(game));
    await expect(workspace).toHaveAttribute('data-game-switch-ms', /\d/);
    return Number(await workspace.getAttribute('data-game-switch-ms'));
  }

  await switchGame(4);
  requests.length = 0;
  for (const delay of [0, 250]) {
    delayMs = delay;
    const samples = [];
    for (let step = 0; step < 10; step++) {
      samples.push(await switchGame(step % 2 ? 4 : 3));
    }
    samples.sort((a, b) => a - b);
    const median = ((samples[4] + samples[5]) / 2).toFixed(1);
    console.log(
      `Game switch, dice delay ${delay} ms: ${requests.length} dice requests, browser median ${median} ms, range ${samples[0].toFixed(1)}–${samples[9].toFixed(1)} ms`,
    );
    expect.soft(requests).toEqual([]);
    requests.length = 0;
  }
});

test('observed rolls load on demand and stay scoped to the selected game', async ({
  page,
}) => {
  const matchId = await openMatch(page);
  const requests = diceRequests(page, matchId);
  const workspace = analysisWorkspace(page);
  const panel = workspace.locator('.analysis-panel');
  const picker = workspace.getByLabel('Spiel auswählen', { exact: true });
  const rolls = panel.getByRole('tab', { name: 'Spielwürfe', exact: true });
  const observed = panel.locator('.observed');
  const firstGame = await picker.inputValue();

  await rolls.click();
  await expect(observed).toContainText(`Spiel ${firstGame}`);
  expect(requests).toEqual([firstGame]);
  await panel.getByRole('tab', { name: 'Zugkandidaten', exact: true }).click();
  await rolls.click();
  await expect(observed).toContainText(`Spiel ${firstGame}`);
  expect(requests).toEqual([firstGame]);

  let release = () => {};
  let ready = () => {};
  let delivered = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    ready = resolve;
  });
  const settled = new Promise<void>((resolve) => {
    delivered = resolve;
  });
  let failOnce = true;
  await page.route(
    (url) => url.pathname === `/api/matches/${matchId}/dice`,
    async (route) => {
      const game = new URL(route.request().url()).searchParams.get('game_no');
      if (game === '3') {
        const response = await route.fetch();
        ready();
        await gate;
        try {
          await route.fulfill({ response });
        } finally {
          delivered();
        }
      } else if (game === '4' && failOnce) {
        failOnce = false;
        await route.fulfill({
          status: 503,
          json: { error: 'Recorded rolls temporarily unavailable' },
        });
      } else {
        await route.continue();
      }
    },
  );
  try {
    await picker.selectOption('3');
    await started;
    await expect(workspace.locator('.board-heading')).toContainText('Spiel 3');
    await expect(observed).toHaveCount(0);
    await picker.selectOption('4');
    await expect(workspace.locator('.board-heading')).toContainText('Spiel 4');
    await expect(panel.getByRole('alert')).toBeVisible();
    await panel.getByRole('button', { name: 'Erneut versuchen' }).click();
    await expect(observed).toContainText('Spiel 4');
    release();
    await settled;
    await expect(observed).toContainText('Spiel 4');
    expect(requests).toEqual([firstGame, '3', '4', '4']);
  } finally {
    release();
  }
});

backendTest('saving opening-book settings fetches once', async ({ page }) => {
  const backend = await profileBackend(page, { initialized: false });
  const matchId = await openMatch(page);
  const rolls = diceRequests(page, matchId);
  const workspace = analysisWorkspace(page);
  const observed = workspace.locator('.analysis-panel .observed');
  await workspace.getByRole('tab', { name: 'Spielwürfe', exact: true }).click();
  await expect(observed).toContainText('Spiel 1');
  expect(rolls).toEqual(['1']);
  const table = await observed.innerText();

  const nextPosition = await workspace
    .locator('.moves-list .move-history-row.current ~ .move-history-row')
    .first()
    .getAttribute('data-position-id');
  if (!nextPosition) throw new Error('Next move has no position ID');
  const bookPath = `/api/positions/${nextPosition}/opening-book`;
  const catalogPath = `/api/matches/${matchId}/analyses`;
  const waitForBook = () =>
    page.waitForResponse(
      (response) => new URL(response.url()).pathname === bookPath,
    );
  const nextBook = waitForBook();
  const nextCatalog = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return (
      url.pathname === catalogPath &&
      url.searchParams.get('position_id') === nextPosition
    );
  });
  await workspace
    .getByRole('button', { name: 'Nächster Zug', exact: true })
    .click();
  await expect(workspace.locator('.move-history-row.current')).toHaveAttribute(
    'data-position-id',
    nextPosition,
  );
  expect((await nextBook).ok()).toBe(true);
  expect((await nextCatalog).ok()).toBe(true);
  await page.waitForLoadState('networkidle');
  await expect(observed).toHaveText(table, { useInnerText: true });
  expect(rolls).toEqual(['1']);

  const settings = page.getByRole('dialog', {
    name: 'Einstellungen',
    exact: true,
  });
  const books = page.getByRole('dialog', {
    name: 'Eröffnungsbücher',
    exact: true,
  });
  const provider = books.getByRole('combobox', {
    name: 'Eröffnungsbuch-Quelle',
    exact: true,
  });
  await page
    .getByRole('button', { name: 'Einstellungen', exact: true })
    .click();
  await settings.getByRole('button', { name: 'Analyse', exact: true }).click();

  const manageBooks = settings.getByRole('button', {
    name: 'Eröffnungsbücher verwalten',
    exact: true,
  });
  // Count requests from effects scheduled by the committed settings render.
  const afterRender = () =>
    page.evaluate(
      () =>
        new Promise<void>((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
        ),
    );
  async function applyBookSettings() {
    const refreshed = waitForBook();
    await books.getByRole('button', { name: 'Anwenden', exact: true }).click();
    await expect(books).not.toBeVisible();
    expect((await refreshed).ok()).toBe(true);
    await afterRender();
  }

  let bookReads = 0;
  let catalogReads = 0;
  page.on('request', (request) => {
    const { pathname } = new URL(request.url());
    if (pathname === bookPath) bookReads++;
    if (pathname === catalogPath) catalogReads++;
  });
  rolls.length = 0;
  for (let save = 0; save < 5; save++) {
    await manageBooks.click();
    const previous = await provider.inputValue();
    await provider.selectOption(previous === 'off' ? 'automatic' : 'off');
    await applyBookSettings();
  }
  console.log(
    `Opening book, 5 changed saves: ${bookReads} book lookups, ${rolls.length} dice reads, ${catalogReads} catalog reads`,
  );
  expect.soft(bookReads).toBe(5);

  const beforeReapply = bookReads;
  await manageBooks.click();
  await expect(provider).toBeVisible();
  await applyBookSettings();
  expect(bookReads - beforeReapply).toBe(1);

  await page.keyboard.press('Escape');
  await expect(settings).not.toBeVisible();
  const beforeRemote = bookReads;
  const remote = structuredClone(backend.state.current_configuration);
  remote.opening_book.xg_path = '/books/changed-in-another-window.ob';
  backend.updateRemote(remote);
  const remoteBook = waitForBook();
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  expect((await remoteBook).ok()).toBe(true);
  await afterRender();
  expect(bookReads - beforeRemote).toBe(1);
  await expect(observed).toHaveText(table, { useInnerText: true });
  expect.soft(rolls).toEqual([]);
  expect(catalogReads).toBe(0);
});
