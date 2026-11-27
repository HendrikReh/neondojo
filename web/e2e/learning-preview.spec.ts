import type { Position } from '../src/shared/http/api';
import type { CardPage } from '../src/workspaces/study/flashcards/learning-api';
import { expect, test } from './test';

test.use({ actionTimeout: 10000, viewport: { width: 1800, height: 1169 } });
test('flashcard browsing previews the selected question without changing learning progress', async ({
  page,
}) => {
  const suffix = crypto.randomUUID();
  const source = `(;GM[6]PW[Preview ${suffix}]PB[Opponent]MI[length:13][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]MI[length:13][ws:1][bs:0];W[31hefe])`;
  const imported = await (
    await page.request.post('/api/imports', {
      multipart: {
        file: {
          name: 'card-preview.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(source),
        },
      },
    })
  ).json();
  const positions: Position[] = (
    await (
      await page.request.get(
        `/api/positions?match_id=${imported.match.id}&limit=100`,
      )
    ).json()
  ).items;
  // Cards come from the match's displayed analysis and from saved positions.
  const job = await (
    await page.request.post(`/api/matches/${imported.match.id}/analysis`, {
      data: { plies: 1 },
    })
  ).json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
      { timeout: 60000 },
    )
    .toBe('completed');
  for (const position of positions)
    expect(
      (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
    ).toBe(true);
  const decks = [];
  for (const [index, ids] of [
    [0, positions.slice(0, 2).map((p) => p.id)],
    [1, [positions[2].id]],
  ] as const) {
    const deck = await (
      await page.request.post('/api/decks', {
        data: { name: `Preview deck ${index} ${suffix}` },
      })
    ).json();
    const result = await page.request.post(`/api/decks/${deck.id}/positions`, {
      data: { position_ids: ids },
    });
    expect(result.ok()).toBe(true);
    decks.push(deck);
  }
  const questions: CardPage['items'] = (
    await (await page.request.get(`/api/decks/${decks[0].id}/cards`)).json()
  ).items;
  const other = (
    await (await page.request.get(`/api/decks/${decks[1].id}/cards`)).json()
  ).items[0];
  const writes: string[] = [];
  page.on('request', (request) => {
    if (
      request.method() !== 'GET' &&
      /\/api\/(cards|decks)\//.test(request.url())
    )
      writes.push(request.url());
  });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('region', { name: 'Study', exact: true });
  const moveHint = study.getByText(
    'Choose the source deck under “Choose deck”.',
    {
      exact: true,
    },
  );
  const interfaceFont = await page
    .locator('.app-shell')
    .evaluate((shell) => getComputedStyle(shell).fontFamily);
  await expect(moveHint).toHaveCSS('font-family', interfaceFont);
  await study
    .getByLabel('Choose deck', { exact: true })
    .selectOption(decks[0].id);
  const rows = study.locator('.learning-card-list tbody tr'),
    preview = study.locator('.learning-card-preview');
  await expect(rows).toHaveCount(2);
  await expect(preview.locator('.board')).toBeVisible();
  await expect(preview).toHaveAttribute('data-card-id', questions[0].id);
  await rows
    .nth(1)
    .getByRole('button', { name: 'Preview card 2', exact: true })
    .click();
  await expect(preview).toHaveAttribute('data-card-id', questions[1].id);
  await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('ArrowUp');
  await expect(preview).toHaveAttribute('data-card-id', questions[0].id);
  await page.keyboard.press('End');
  await expect(preview).toHaveAttribute('data-card-id', questions[1].id);
  expect((await rows.first().boundingBox())!.height).toBeLessThanOrEqual(40);
  expect((await preview.boundingBox())!.x).toBeGreaterThanOrEqual(
    (await study.locator('.learning-card-list').boundingBox())!.x +
      (await study.locator('.learning-card-list').boundingBox())!.width,
  );
  expect(writes).toEqual([]);
  await expect(study.locator('.study-answer')).toHaveCount(0);
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await expect(preview.locator('.board')).toHaveCount(0);
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await expect(preview).toHaveAttribute('data-card-id', questions[1].id);
  await expect(preview.locator('.board')).toBeVisible();
  await page.screenshot({ path: test.info().outputPath('card-preview.png') });
  await expect(
    study.getByRole('button', { name: 'Show answer', exact: true }),
  ).toHaveCount(0);
  // A delayed new deck cannot keep a previous deck's board visible.
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route(`**/api/decks/${decks[1].id}/cards?*`, async (route) => {
    const response = await route.fetch();
    await held;
    await route.fulfill({ response }).catch(() => {});
  });
  await study
    .getByLabel('Choose deck', { exact: true })
    .selectOption(decks[1].id);
  await expect(preview.locator('.board')).toHaveCount(0);
  await expect(preview).not.toHaveAttribute('data-card-id', questions[1].id);
  release();
  await expect(preview).toHaveAttribute('data-card-id', other.id);
  await expect(preview.locator('.board')).toBeVisible();
  await page.unroute(`**/api/decks/${decks[1].id}/cards?*`);
  expect(writes).toEqual([]);
  const after: CardPage['items'] = (
    await (await page.request.get(`/api/decks/${decks[0].id}/cards`)).json()
  ).items;
  expect(
    after.map((card) => [
      card.id,
      card.version,
      card.review_count,
      card.due_at,
    ]),
  ).toEqual(
    questions.map((card) => [
      card.id,
      card.version,
      card.review_count,
      card.due_at,
    ]),
  );
  await study.getByRole('button', { name: 'Study now', exact: true }).click();
  await expect(study.locator('.study-card')).toBeVisible();
  await expect(study.locator('.learning-card-preview')).toHaveCount(0);
  await expect(study.locator('.study-answer')).toHaveCount(0);
  await study.getByRole('button', { name: 'Show answer', exact: true }).click();
  await expect(study.locator('.study-answer')).toBeVisible();
});

test('more cards preserve the preview and a late page cannot survive a deck round trip', async ({
  page,
}) => {
  test.setTimeout(120000);
  const suffix = crypto.randomUUID();
  const source = Array.from(
    { length: 52 },
    (_, score) =>
      `(;GM[6]PW[Paging ${suffix}]PB[Opponent]MI[length:63][ws:${score}][bs:0];W[31hefe])`,
  ).join('');
  const imported = await (
    await page.request.post('/api/imports', {
      multipart: {
        file: {
          name: 'paging-cards.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(source),
        },
      },
    })
  ).json();
  const positions: Position[] = (
    await (
      await page.request.get(
        `/api/positions?match_id=${imported.match.id}&limit=100`,
      )
    ).json()
  ).items;
  expect(positions.length).toBe(52);
  const job = await (
    await page.request.post(`/api/matches/${imported.match.id}/analysis`, {
      data: { plies: 1 },
    })
  ).json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
      { timeout: 90000 },
    )
    .toBe('completed');
  // Only saved positions can be added to a deck.
  for (const position of positions)
    expect(
      (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
    ).toBe(true);
  const deck = await (
    await page.request.post('/api/decks', {
      data: { name: `Paged preview ${suffix}` },
    })
  ).json();
  const transfer = await (
    await page.request.post(`/api/decks/${deck.id}/positions`, {
      data: { position_ids: positions.map((position) => position.id) },
    })
  ).json();
  expect(transfer.total).toBe(52);
  const other = await (
    await page.request.post('/api/decks', {
      data: { name: `Other preview ${suffix}` },
    })
  ).json();
  expect(
    (
      await page.request.post(`/api/decks/${other.id}/positions`, {
        data: { position_ids: [positions[0].id] },
      })
    ).ok(),
  ).toBe(true);
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('region', { name: 'Study', exact: true });
  const chooser = study.getByLabel('Choose deck', { exact: true });
  const rows = study.locator('.learning-card-list tbody tr'),
    preview = study.locator('.learning-card-preview');
  await chooser.selectOption(deck.id);
  await expect(rows).toHaveCount(50);
  await rows.nth(10).click();
  const selected = await rows.nth(10).getAttribute('data-card-id');
  await study.getByRole('button', { name: 'Page 2', exact: true }).click();
  await expect(rows).toHaveCount(2);
  await expect(preview).toHaveAttribute('data-card-id', selected!);
  await rows.last().focus();
  await page.keyboard.press('End');
  await expect(preview).toHaveAttribute(
    'data-card-id',
    (await rows.last().getAttribute('data-card-id'))!,
  );
  await chooser.selectOption(other.id);
  await expect(rows).toHaveCount(1);
  await chooser.selectOption(deck.id);
  await expect(rows).toHaveCount(50);
  let release = () => {},
    started = () => {},
    finished = () => {};
  const held = new Promise<void>((resolve) => {
      release = resolve;
    }),
    waiting = new Promise<void>((resolve) => {
      started = resolve;
    }),
    done = new Promise<void>((resolve) => {
      finished = resolve;
    });
  await page.route(`**/api/decks/${deck.id}/cards?*`, async (route) => {
    const params = new URL(route.request().url()).searchParams;
    // Hold only the request for the second page.
    if (!params.has('after') && params.get('page') !== '2')
      return route.continue();
    const response = await route.fetch();
    started();
    await held;
    await route.fulfill({ response }).catch(() => {});
    finished();
  });
  await study.getByRole('button', { name: 'Page 2', exact: true }).click();
  await waiting;
  await chooser.selectOption(other.id);
  await expect(rows).toHaveCount(1);
  await chooser.selectOption(deck.id);
  await expect(rows).toHaveCount(50);
  const fresh = await preview.getAttribute('data-card-id');
  release();
  await done;
  await expect(rows).toHaveCount(50);
  await expect(preview).toHaveAttribute('data-card-id', fresh!);
  await page.unroute(`**/api/decks/${deck.id}/cards?*`);
  await study.getByRole('button', { name: 'Page 2', exact: true }).click();
  await expect(rows).toHaveCount(2);
});
