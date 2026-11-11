import { test, expect, type Page } from './test';
async function fixture(
  page: Page,
  locale = 'en',
  failedBookmark = false,
  databaseId?: string,
) {
  const response = await page.request.post(
    `/api/imports${databaseId ? `?database_id=${databaseId}` : ''}`,
    {
      multipart: {
        file: {
          name: 'position-actions.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[Toolbar ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
          ),
        },
      },
    },
  );
  expect(response.ok()).toBe(true);
  const imported = await response.json();
  const positions = await (
    await page.request.get(
      `/api/positions?match_id=${imported.match.id}&limit=100`,
    )
  ).json();
  const position = positions.items[0];
  await page.addInitScript(
    ({ id, locale, databaseId }) => {
      localStorage.setItem('bgtutor.locale', locale);
      if (databaseId) localStorage.setItem('bgtutor.database', databaseId);
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      );
    },
    { id: imported.match.id, locale, databaseId },
  );
  if (failedBookmark)
    await page.route(`**/api/positions/${position.id}/bookmark`, (r) =>
      r.fulfill({ status: 503, json: { error: 'Unavailable' } }),
    );
  await page.goto('/');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return position;
}
for (const [width, locale] of [
  [1800, 'en'],
  [390, 'de'],
] as const)
  test(`position actions are adjacent above the board at ${width}px in ${locale}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1100 });
    await fixture(page, locale);
    const toolbar = page.locator('.board-review-toolbar');
    const bookmark = toolbar.getByRole('button', {
      name: locale === 'de' ? 'Position merken' : 'Save position',
      exact: true,
    });
    const search = toolbar.getByRole('button', {
      name: locale === 'de' ? 'Diese Stellung suchen' : 'Search this position',
      exact: true,
    });
    await expect(bookmark).toBeVisible();
    await expect(search).toBeVisible();
    await expect(page.locator('.analysis-heading')).toHaveAttribute(
      'aria-busy',
      'false',
    );
    // The estimate above the toolbar wraps onto multiple lines on mobile.
    await expect(
      page.locator('.analysis-time-estimate small').first(),
    ).toBeVisible();
    await expect(page.locator('.position-code button')).toHaveCount(0);
    // The analysis header may grow as metadata arrives. Read all rectangles
    // in one layout snapshot so a change between reads cannot skew the test.
    const { b, s, board } = await toolbar.evaluate((element) => ({
      b: element
        .querySelector('.position-bookmark-control button')!
        .getBoundingClientRect()
        .toJSON(),
      s: element
        .querySelector('button[data-action-icon="search"]')!
        .getBoundingClientRect()
        .toJSON(),
      board: element
        .closest('.analysis-workspace')!
        .querySelector('.board')!
        .getBoundingClientRect()
        .toJSON(),
    }));
    expect(s.x).toBeGreaterThanOrEqual(b.x + b.width + 4);
    expect(s.x - b.x - b.width).toBeLessThanOrEqual(12);
    expect(s.y).toBeCloseTo(b.y, 0);
    expect(s.width).toBeGreaterThanOrEqual(44);
    expect(s.height).toBeGreaterThanOrEqual(44);
    expect(s.y + s.height).toBeLessThanOrEqual(board.y);
    expect(s.x + s.width).toBeLessThanOrEqual(width);
    await search.focus();
    await expect(page.getByRole('tooltip')).toHaveText(
      locale === 'de' ? 'Diese Stellung suchen' : 'Search this position',
    );
    await page.keyboard.press('Escape');
    await expect(page.getByRole('tooltip')).toHaveCount(0);
    await search.blur();
    await page.mouse.move(1, 1);
    await search.hover();
    await expect(page.getByRole('tooltip')).toHaveText(
      locale === 'de' ? 'Diese Stellung suchen' : 'Search this position',
    );
    await page.screenshot({
      path: test.info().outputPath(`position-actions-${width}.png`),
    });
  });
test('one click opens the database and runs the current source-position search', async ({
  page,
}) => {
  const position = await fixture(page);
  // Search the underlying analysis position even while the board shows Final.
  await page.getByRole('button', { name: 'Final', exact: true }).click();
  const response = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('position_id') === position.position_id,
  );
  await page
    .locator('.board-review-toolbar')
    .getByRole('button', { name: 'Search this position', exact: true })
    .click();
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(
    page.locator('.position-search').getByRole('textbox'),
  ).toBeVisible();
  await expect(
    page.locator('.position-search').getByRole('textbox'),
  ).toHaveValue(position.position_id);
  expect((await response).ok()).toBe(true);
});

test('similar search opens from the analysis board with the current match excluded', async ({
  page,
}) => {
  const position = await fixture(page);
  const { items } = await (
    await page.request.get(
      `/api/positions?match_id=${position.match_uuid}&limit=100`,
    )
  ).json();
  const neighbor = items[1];
  const requests: URLSearchParams[] = [];
  await page.route(`**/api/positions/${position.id}/similar*`, (route) => {
    const params = new URL(route.request().url()).searchParams;
    requests.push(params);
    return route.fulfill({
      json: {
        items:
          params.get('include_current_match') === 'true'
            ? [
                {
                  position: neighbor,
                  match_name: 'Same match',
                  white: 'White',
                  black: 'Black',
                  distance: 4,
                },
              ]
            : [],
      },
    });
  });
  await page
    .getByRole('button', { name: 'Find similar positions', exact: true })
    .click();
  const results = page.getByRole('region', {
    name: 'Similar positions',
    exact: true,
  });
  await expect(results.getByText('No similar positions found.')).toBeVisible();
  const include = results.getByRole('checkbox', {
    name: 'Include current match',
  });
  await expect(include).not.toBeChecked();
  expect(requests.every((params) => params.size === 0)).toBe(true);
  const labels = ['Same dice', 'Same match score', 'Same cube value and owner'];
  for (const name of labels) {
    const filter = results.getByRole('checkbox', { name, exact: true });
    await expect(filter).not.toBeChecked();
    await filter.check();
  }
  await expect
    .poll(() => Object.fromEntries(requests.at(-1) ?? []))
    .toEqual({
      same_dice: 'true',
      same_score: 'true',
      same_cube: 'true',
    });
  await expect(include).not.toBeChecked();
  await include.check();
  await expect(
    results.getByRole('button', { name: 'Same match' }),
  ).toBeVisible();
  expect(Object.fromEntries(requests.at(-1) ?? [])).toEqual({
    same_dice: 'true',
    same_score: 'true',
    same_cube: 'true',
    include_current_match: 'true',
  });
  await expect(results.getByRole('img')).toBeVisible();
  await results
    .getByRole('button', { name: 'Open analysis', exact: true })
    .click();
  await expect(
    page.getByRole('tab', { name: 'Analysis', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(page.locator('.position-code code')).toHaveText(
    neighbor.position_id,
  );
  await page
    .getByRole('button', { name: 'Find similar positions', exact: true })
    .click();
  await expect(include).not.toBeChecked();
  for (const name of labels)
    await expect(
      results.getByRole('checkbox', { name, exact: true }),
    ).not.toBeChecked();
  await results
    .getByRole('button', { name: 'Back to saved positions', exact: true })
    .click();
  await expect(results).toBeHidden();
});

test('practice uses the latest filtered similar positions in result order', async ({
  page,
}) => {
  const position = await fixture(page);
  const hit = (matchName: string, distance: number) => ({
    position: { ...position, id: crypto.randomUUID(), notation: '8/5 6/5' },
    match_name: matchName,
    white: 'White',
    black: 'Black',
    distance,
  });
  const unfiltered = hit('Unfiltered position', 0);
  const filtered = [
    hit('Closest filtered position', 1),
    hit('Second filtered position', 3),
  ];
  let releaseResults: () => void = () => {};
  const waitForFilter = new Promise<void>((resolve) => {
    releaseResults = resolve;
  });
  await page.route(
    `**/api/positions/${position.id}/similar*`,
    async (route) => {
      const query = new URL(route.request().url()).searchParams;
      if (!query.has('same_dice'))
        return route.fulfill({ json: { items: [unfiltered] } });
      expect(query.has('include_current_match')).toBe(false);
      await waitForFilter;
      return route.fulfill({ json: { items: filtered } });
    },
  );
  const reveals: string[] = [];
  await page.route('**/api/positions/*/practice-answer', (route) => {
    const id = new URL(route.request().url()).pathname.split('/').at(-2)!;
    reveals.push(id);
    return route.fulfill({
      json: { best: '8/5 6/5', engine: 'GNU Backgammon', depth: '2-ply' },
    });
  });
  await page.getByRole('button', { name: 'Find similar positions' }).click();
  await expect(
    page.getByRole('button', { name: unfiltered.match_name }),
  ).toBeVisible();
  const practice = page.getByRole('button', {
    name: 'Practice results',
    exact: true,
  });
  await expect(practice).toBeEnabled();
  await page.getByRole('checkbox', { name: 'Same dice', exact: true }).check();
  await expect(practice).toBeDisabled();
  releaseResults();
  await practice.click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice similar positions',
  });
  await expect(dialog.getByText('1 / 2', { exact: true })).toBeVisible();
  await expect(dialog.getByText(filtered[0].match_name)).toBeVisible();
  await expect(dialog.getByText('8/5 6/5', { exact: true })).toHaveCount(0);
  expect(reveals).toEqual([]);
  await dialog.getByRole('button', { name: 'Reveal answer' }).click();
  await expect(dialog.getByText('8/5 6/5', { exact: true })).toBeVisible();
  await expect(dialog.getByRole('button', { name: 'Your move' })).toHaveCount(
    0,
  );
  expect(reveals).toEqual([filtered[0].position.id]);
  await dialog.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(dialog.getByText('2 / 2', { exact: true })).toBeVisible();
  await expect(dialog.getByText(filtered[1].match_name)).toBeVisible();
  await expect(dialog.getByText('8/5 6/5', { exact: true })).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Finish', exact: true }).click();
  await expect(dialog.getByText('Practice complete')).toBeVisible();
  await expect(dialog.getByText('No moves were scored.')).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Retry missed moves' }),
  ).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(practice).toBeFocused();
  await expect(
    page.getByRole('checkbox', { name: 'Same dice', exact: true }),
  ).toBeChecked();
});

test('practice scores the move played on the board', async ({ page }) => {
  const position = await fixture(page);
  await page.route(`**/api/positions/${position.id}/similar*`, (route) =>
    route.fulfill({
      json: {
        items: [
          {
            position,
            match_name: 'Practice example',
            white: 'White',
            black: 'Black',
            distance: 2,
          },
        ],
      },
    }),
  );
  await page.route(
    `**/api/positions/${position.id}/practice-score`,
    (route) => {
      expect(route.request().postDataJSON()).toEqual({
        steps: [
          { from: 24, to: 21 },
          { from: 24, to: 23 },
        ],
      });
      return route.fulfill({
        json: {
          best: '8/5 6/5',
          played: '24/23 24/21',
          best_equity: 0.22,
          played_equity: -0.041,
          equity_loss: 0.261,
          engine: 'GNU Backgammon',
          depth: '2-ply',
        },
      });
    },
  );
  await page.getByRole('button', { name: 'Find similar positions' }).click();
  await page
    .getByRole('button', { name: 'Practice results', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice similar positions',
  });
  const submit = dialog.getByRole('button', {
    name: 'Check my move',
    exact: true,
  });
  const point = dialog.getByRole('button', { name: 'Point 24', exact: true });
  const yourMove = dialog.getByRole('button', {
    name: 'Your move',
    exact: true,
  });
  const bestPlay = dialog.getByRole('button', {
    name: 'Best play',
    exact: true,
  });
  await expect(yourMove).toHaveCount(0);
  await expect(submit).toBeDisabled();
  await point.click();
  await expect(submit).toBeDisabled();
  await point.click();
  await expect(submit).toBeEnabled();
  await dialog.getByRole('button', { name: 'Undo step', exact: true }).click();
  await expect(submit).toBeDisabled();
  await point.click();
  await submit.click();
  await expect(dialog.getByText('0.261', { exact: true })).toBeVisible();
  await expect(dialog.getByText('24/23 24/21', { exact: true })).toBeVisible();
  await expect(dialog.getByText('8/5 6/5', { exact: true })).toBeVisible();
  await expect(point).toHaveCount(0);
  await expect(bestPlay).toHaveAttribute('aria-pressed', 'true');
  const arrows = dialog.locator('.move-arrows > [data-from]');
  const paths = () =>
    arrows.evaluateAll((nodes) =>
      nodes.map(
        (node) =>
          `${node.getAttribute('data-from')}/${node.getAttribute('data-to')}`,
      ),
    );
  await expect.poll(paths).toEqual(['8/5', '6/5']);
  await yourMove.click();
  await expect(yourMove).toHaveAttribute('aria-pressed', 'true');
  await expect.poll(paths).toEqual(['24/21', '24/23']);
  await expect(dialog.getByRole('img', { name: /^Your move/ })).toBeVisible();
  await expect(dialog.getByText('0.261', { exact: true })).toBeVisible();
  await bestPlay.click();
  await expect(yourMove).toHaveAttribute('aria-pressed', 'false');
  await expect.poll(paths).toEqual(['8/5', '6/5']);
});

test('practice summarizes scored moves and retries only missed positions', async ({
  page,
}) => {
  const source = await fixture(page);
  const saveRequests: unknown[] = [];
  await page.route('**/api/study/similar-practice', (route) => {
    if (route.request().method() !== 'POST') return route.continue();
    saveRequests.push(route.request().postDataJSON());
    return [1, 3].includes(saveRequests.length)
      ? route.fulfill({
          status: 503,
          json: { error: 'Temporarily unavailable' },
        })
      : route.continue();
  });
  const hits = ['First miss', 'Correct play', 'Reveal only', 'Second miss'].map(
    (match_name, distance) => ({
      match_name,
      distance,
      position: { ...source, id: crypto.randomUUID() },
      white: 'White',
      black: 'Black',
    }),
  );
  await page.route(`**/api/positions/${source.id}/similar*`, (route) =>
    route.fulfill({ json: { items: hits } }),
  );
  const answer = { best: '8/5 6/5', engine: 'GNU Backgammon', depth: '2-ply' };
  await page.route('**/api/positions/*/practice-answer', (route) =>
    route.fulfill({ json: answer }),
  );
  const scored: string[] = [];
  await page.route('**/api/positions/*/practice-score', (route) => {
    const id = new URL(route.request().url()).pathname.split('/').at(-2)!;
    const index = hits.findIndex((hit) => hit.position.id === id);
    const loss = scored.includes(id) ? 0 : [0.3, 0, 0, 0.15][index];
    scored.push(id);
    return route.fulfill({
      json: {
        ...answer,
        played: loss ? '24/23 24/21' : answer.best,
        best_equity: 0.25,
        played_equity: 0.25 - loss,
        equity_loss: loss,
      },
    });
  });
  await page.getByRole('button', { name: 'Find similar positions' }).click();
  await page
    .getByRole('button', { name: 'Practice results', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice similar positions',
  });
  const advance = (name: 'Next' | 'Finish') =>
    dialog.getByRole('button', { name, exact: true }).click();
  async function playMove(points: number[]) {
    for (const point of points)
      await dialog
        .getByRole('button', { name: `Point ${point}`, exact: true })
        .click();
    await dialog
      .getByRole('button', { name: 'Check my move', exact: true })
      .click();
    await expect(dialog.getByText('Normalized equity loss')).toBeVisible();
  }
  await playMove([24, 24]);
  await advance('Next');
  await playMove([8, 6]);
  await advance('Next');
  await dialog.getByRole('button', { name: 'Reveal answer' }).click();
  await expect(dialog.getByText('8/5 6/5', { exact: true })).toBeVisible();
  await advance('Next');
  await playMove([24, 24]);
  await advance('Finish');
  await expect(
    dialog.getByText('Average equity loss', { exact: true }),
  ).toBeVisible();
  await expect(dialog.getByText('0.150', { exact: true })).toBeVisible();
  await expect(dialog.getByText('3 / 4', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: 'Retry save', exact: true }).click();
  await expect(dialog.getByText('Saved to practice history.')).toBeVisible();
  expect(saveRequests).toEqual([saveRequests[0], saveRequests[0]]);
  const retry = dialog.getByRole('button', {
    name: 'Retry missed moves',
    exact: true,
  });
  await retry.click();
  await expect(dialog.getByText('1 / 2', { exact: true })).toBeVisible();
  await expect(dialog.getByText('First miss')).toBeVisible();
  await expect(dialog.getByText('Normalized equity loss')).toHaveCount(0);
  await playMove([8, 6]);
  await advance('Next');
  await expect(dialog.getByText('Second miss')).toBeVisible();
  await playMove([8, 6]);
  await advance('Finish');
  await expect(dialog.getByText('0.000', { exact: true })).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Retry save', exact: true }),
  ).toBeVisible();
  await expect(retry).toHaveCount(0);
  expect(scored).toEqual(
    [0, 1, 3, 0, 3].map((index) => hits[index].position.id),
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Study & Anki', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Practice history', exact: true })
    .click();
  const history = page.getByRole('table', { name: 'Practice history' });
  await expect(
    history.getByRole('row').filter({ hasText: '0.150' }),
  ).toContainText('3 / 4');
  await expect(
    history.getByRole('row').filter({ hasText: '0.000' }),
  ).toContainText('Retry');
  expect(saveRequests).toHaveLength(4);
  expect(saveRequests[3]).toEqual(saveRequests[2]);
});

test('practice history charts initial rounds and lets the keyboard reach retries and earlier rounds', async ({
  page,
}) => {
  const round = (day: number, loss: number, is_retry = false) => ({
    id: crypto.randomUUID(),
    completed_at: new Date(Date.UTC(2026, 0, day, 12)).toISOString(),
    total_positions: 1,
    scored_moves: 1,
    missed_moves: 1,
    total_equity_loss: loss,
    average_equity_loss: loss,
    is_retry,
  });
  const original = [1, 2, 3, 4, 5, 6].map((day) => round(day, day / 10));
  const retry = round(7, 0.9, true);
  const cursor = original[1].id;
  await page.route('**/api/study/similar-practice*', (route) => {
    const before = new URL(route.request().url()).searchParams.get('before');
    return route.fulfill({
      json:
        before === cursor
          ? { items: [original[0]], next_cursor: null }
          : {
              items: [retry, ...original.slice(1).reverse()],
              next_cursor: cursor,
            },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study & Anki', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Practice history', exact: true })
    .click();
  const history = page.getByRole('region', { name: 'Practice history' });
  await history.getByRole('button', { name: 'Load older rounds' }).click();
  const chart = history.getByRole('figure', {
    name: 'Average equity loss over time',
  });
  await expect(chart.getByRole('img')).toHaveCount(7);
  const last = chart.getByRole('img', { name: /^Retry .*0\.900$/ });
  await last.focus();
  await last.press('ArrowLeft');
  const latest = chart.getByRole('img', { name: /0\.600.*0\.400$/ });
  await expect(latest).toBeFocused();
  await expect(page.getByRole('tooltip')).toContainText('0.400');
  await chart.getByLabel('Rolling average window').selectOption('10');
  await expect(
    chart.getByRole('img', { name: /0\.600.*0\.350$/ }),
  ).toBeVisible();
  await expect(last).toHaveAccessibleName(/^Retry .*0\.900$/);
});

test('search remains available when bookmark status cannot be loaded', async ({
  page,
}) => {
  await fixture(page, 'en', true);
  const toolbar = page.locator('.board-review-toolbar');
  await expect(toolbar.getByRole('alert')).toBeVisible();
  const search = toolbar.getByRole('button', {
    name: 'Search this position',
    exact: true,
  });
  await expect(search).toBeEnabled();
  await search.click();
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
});

test('explicit searches clear prior scope and filters, rerun the position and do not restore cleared intent', async ({
  page,
}) => {
  const position = await fixture(page);
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  const filtered = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('q') === 'Toolbar',
  );
  await page.getByLabel('Search matches', { exact: true }).fill('Toolbar');
  expect(new URL((await filtered).url()).searchParams.get('q')).toBe('Toolbar');
  const search = async () => {
    await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
    const response = page.waitForResponse(
      (r) =>
        new URL(r.url()).pathname === '/api/library/matches' &&
        new URL(r.url()).searchParams.get('position_id') ===
          position.position_id,
    );
    await page
      .getByRole('button', { name: 'Search this position', exact: true })
      .click();
    const next = new URL((await response).url()).searchParams;
    expect(next.has('database_id')).toBe(false);
    expect(next.has('q')).toBe(false);
  };
  await search();
  await expect(page.getByLabel('Search matches', { exact: true })).toHaveValue(
    '',
  );
  await page
    .locator('.database-table tbody tr')
    .first()
    .getByRole('checkbox')
    .check();
  await expect(page.locator('.library-batch-bar')).toBeVisible();
  await search();
  await expect(page.locator('.library-batch-bar')).toHaveCount(0);
  const cleared = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      !new URL(r.url()).searchParams.has('position_id'),
  );
  await page
    .getByRole('button', { name: 'Clear position filter', exact: true })
    .click();
  await cleared;
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(page.locator('.library-position-filter')).toHaveCount(0);
  await expect(page.locator('.position-search input')).toHaveValue('');
});

test('a later source-position search cannot be replaced by a delayed earlier result', async ({
  page,
}) => {
  const position = await fixture(page);
  const metadata = await (
    await page.request.get(`/api/matches/${position.match_uuid}/metadata`)
  ).json();
  let release = () => {},
    finished = () => {};
  const delayed = new Promise<void>((resolve) => {
      release = resolve;
    }),
    done = new Promise<void>((resolve) => {
      finished = resolve;
    });
  await page.route('**/api/library/matches?*', async (route) => {
    const query = new URL(route.request().url()).searchParams;
    const source = query.get('position_id');
    if (!source) return route.continue();
    if (source === position.position_id) {
      await delayed;
      await route
        .fulfill({
          json: {
            items: [
              {
                ...metadata,
                white: 'Stale source result',
                display_name: 'Stale source result',
              },
            ],
            next_cursor: null,
          },
        })
        .catch(() => {});
      finished();
    } else
      await route.fulfill({
        json: {
          items: [
            {
              ...metadata,
              white: 'Current source result',
              display_name: 'Current source result',
            },
          ],
          next_cursor: null,
        },
      });
  });
  const oldRequest = page.waitForRequest(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('position_id') === position.position_id,
  );
  await page
    .getByRole('button', { name: 'Search this position', exact: true })
    .click();
  await oldRequest;
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.position-code code')).not.toHaveText(
    position.position_id,
  );
  const nextId = await page.locator('.position-code code').textContent();
  await page
    .getByRole('button', { name: 'Search this position', exact: true })
    .click();
  await expect(page.locator('.position-search input')).toHaveValue(nextId!);
  await expect(page.locator('.database-table')).toContainText(
    'Current source result',
  );
  release();
  await done;
  await expect(page.locator('.database-table')).not.toContainText(
    'Stale source result',
  );
  await expect(page.locator('.position-search input')).toHaveValue(nextId!);
});

test('analysis search finds occurrences in different databases and clears every prior filter', async ({
  page,
}) => {
  const suffix = crypto.randomUUID();
  const db1 = await (
    await page.request.post('/api/databases', {
      data: { name: `Global source ${suffix}` },
    })
  ).json();
  const db2 = await (
    await page.request.post('/api/databases', {
      data: { name: `Global other ${suffix}` },
    })
  ).json();
  const position = await fixture(page, 'en', false, db1.id);
  const otherName = `Other occurrence ${suffix}`;
  const imported = await page.request.post(
    `/api/imports?database_id=${db2.id}`,
    {
      multipart: {
        file: {
          name: 'global-occurrence.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[${otherName}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe])`,
          ),
        },
      },
    },
  );
  expect(imported.ok()).toBe(true);
  const other = await imported.json();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page.locator('.database-table button[data-sort="id"]').click();
  await page
    .getByLabel('Search matches', { exact: true })
    .fill('exclude-all-occurrences');
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  const filters = page.locator('.library-filters');
  for (const label of [
    'Player',
    'Match name',
    'Tournament',
    'Round',
    'Tag',
    'Comment',
  ])
    await filters.getByLabel(label, { exact: true }).fill('old-filter');
  const fields = await filters.locator('input[type="date"]').all();
  await fields[0].fill('2030-01-01');
  await fields[1].fill('2040-01-01');
  await filters
    .getByLabel('Date type', { exact: true })
    .selectOption('imported');
  await filters.getByLabel('Match type', { exact: true }).selectOption('money');
  await filters.getByLabel('Result', { exact: true }).selectOption('draw');
  const numeric = await filters.locator('input[type="number"]').all();
  await numeric[0].fill('25');
  await numeric[1].fill('30');
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  const response = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('position_id') === position.position_id,
  );
  await page
    .getByRole('button', { name: 'Search this position', exact: true })
    .click();
  const params = new URL((await response).url()).searchParams;
  expect(params.get('sort')).toBe('id');
  expect(params.get('direction')).toBe('asc');
  expect(
    [...params.keys()].filter(
      (key) =>
        !['position_id', 'sort', 'direction', 'limit', 'date_field'].includes(
          key,
        ),
    ),
  ).toEqual([]);
  await expect(
    page
      .locator('.database-sidebar')
      .getByRole('button', { name: /All databases/ }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByLabel('Search matches', { exact: true })).toHaveValue(
    '',
  );
  expect(
    await filters
      .locator('input')
      .evaluateAll((inputs) =>
        inputs
          .map((input) => (input as HTMLInputElement).value)
          .filter(Boolean),
      ),
  ).toEqual([position.position_id]);
  await expect(
    page.locator(`tr[data-match-id="${position.match_uuid}"]`),
  ).toBeVisible();
  await expect(
    page.locator(`tr[data-match-id="${other.match.id}"]`),
  ).toBeVisible();
  await expect(page.locator('.database-table')).toContainText(otherName);
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const state = await (await fetch('/api/configuration')).json();
        return state.current_configuration.library.database_id;
      }),
    )
    .toBe('');
});
