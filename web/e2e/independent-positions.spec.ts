import { openPositionCalculation } from './analysis-controls';
import { expect, test, type Page } from './test';

test.use({ actionTimeout: 10000 });
async function setup(page: Page, learn = false) {
  const suffix = crypto.randomUUID(),
    tag = `kept-${suffix}`;
  const database = await (
    await page.request.post('/api/databases', {
      data: { name: `Saved origin ${suffix}` },
    })
  ).json();
  const source = `(;GM[6]PW[Keeper ${suffix}]PB[Opponent]DT[2026-10-25]EV[Study Open]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]MI[length:7][ws:2][bs:3];W[31xufe];B[42qusu])`;
  const imported = await page.request.post(
    `/api/imports?database_id=${database.id}`,
    {
      multipart: {
        file: {
          name: `independent-${suffix}.sgf`,
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(source),
        },
      },
    },
  );
  expect(imported.status()).toBe(201);
  const match = (await imported.json()).match;
  const positions = (
    await (
      await page.request.get(`/api/positions?match_id=${match.id}&limit=100`)
    ).json()
  ).items;
  const position = positions.find(
    (p: { game_no: number; ply: number }) => p.game_no === 2 && p.ply === 1,
  );
  const metadata = await (
    await page.request.get(`/api/matches/${match.id}/metadata`)
  ).json();
  expect(
    (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
  ).toBe(true);
  expect(
    (
      await page.request.put(`/api/positions/${position.id}/annotation`, {
        data: {
          expected_version: 0,
          tags: [tag],
          comment: 'Retain this position explanation',
        },
      })
    ).ok(),
  ).toBe(true);
  expect(
    (
      await page.request.put(`/api/matches/${match.id}/annotation`, {
        data: {
          expected_version: 0,
          tags: ['source-context'],
          comment: 'Retain this match explanation',
        },
      })
    ).ok(),
  ).toBe(true);
  let card: Record<string, unknown> | undefined,
    deck: { id: string } | undefined,
    native: { id: string } | undefined;
  if (learn) {
    native = await (
      await page.request.post(`/api/matches/${match.id}/analysis`, {
        data: { game_no: 2, plies: 1 },
      })
    ).json();
    await expect
      .poll(
        async () =>
          (await (await page.request.get(`/api/jobs/${native!.id}`)).json())
            .status,
      )
      .toBe('completed');
    const hint = await (
      await page.request.post('/api/jobs', {
        data: { position_id: position.id, kind: 'analysis', plies: 1 },
      })
    ).json();
    await expect
      .poll(
        async () =>
          (await (await page.request.get(`/api/jobs/${hint.id}`)).json())
            .status,
      )
      .toBe('completed');
    deck = await (
      await page.request.post('/api/decks', {
        data: { name: `Retained deck ${suffix}` },
      })
    ).json();
    const transfer = await (
      await page.request.post(`/api/decks/${deck!.id}/positions`, {
        data: { position_ids: [position.id] },
      })
    ).json();
    expect(transfer.added).toBe(1);
    card = (
      await (await page.request.get(`/api/decks/${deck!.id}/cards`)).json()
    ).items[0];
    const previousReviews = Number(card!.review_count);
    const revealed = await (
      await page.request.post(`/api/cards/${card!.id}/reveal`, {
        data: { expected_version: card!.version },
      })
    ).json();
    expect(
      (
        await page.request.post(`/api/cards/${card!.id}/review`, {
          data: {
            session_id: revealed.session_id,
            request_id: crypto.randomUUID(),
            rating: 'good',
          },
        })
      ).ok(),
    ).toBe(true);
    card = (
      await (await page.request.get(`/api/decks/${deck!.id}/cards`)).json()
    ).items[0];
    expect(card!.review_count).toBe(previousReviews + 1);
  }
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.database', id);
  }, database.id);
  await page.goto('/');
  return {
    database,
    match,
    metadata,
    positions,
    position,
    tag,
    card,
    deck,
    native,
  };
}
async function collection(page: Page, tag: string) {
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  const pane = page.getByRole('tabpanel', { name: 'Positions', exact: true });
  await pane.locator('.position-filter-details summary').click();
  await pane.getByLabel('Exact tag', { exact: true }).fill(tag);
  await pane
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  return pane;
}

test('deleting a source match retains an independent study position, notes, analysis and reviewed card', async ({
  page,
}) => {
  const f = await setup(page, true);
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  const originalRow = page.locator('.database-table tbody tr');
  await expect(originalRow).toHaveCount(1);
  await originalRow.getByRole('checkbox').check();
  await page
    .locator('.library-batch-bar')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(originalRow).toHaveCount(0);
  const retained = await page.request.get(`/api/positions/${f.position.id}`);
  expect(retained.status()).toBe(200);
  const saved = await retained.json();
  expect(saved.match_uuid).not.toBe(f.match.id);
  expect(saved.position_id).toBe(f.position.position_id);
  expect(saved.gnubg_match_id).toBe(f.position.gnubg_match_id);
  expect((await page.request.get(`/api/matches/${f.match.id}`)).status()).toBe(
    404,
  );
  expect(
    (await page.request.get(`/api/positions/${f.positions[0].id}`)).status(),
  ).toBe(404);
  const metadata = await (
    await page.request.get(`/api/matches/${saved.match_uuid}/metadata`)
  ).json();
  expect(metadata.source_available).toBe(false);
  expect(metadata.match_no).toBe(f.metadata.match_no);
  expect(metadata.white_pr).toBeNull();
  const pane = await collection(page, f.tag),
    rows = pane.locator('.position-table tbody tr');
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toHaveAttribute('data-position-id', f.position.id);
  await pane.locator('.saved-position-preview-details summary').click();
  await expect(pane.locator('.saved-position-preview-details')).toContainText(
    f.database.name,
  );
  await expect(rows.locator('.position-player')).toHaveText(f.match.white);
  await expect(pane.locator('.position-preview .board')).toBeVisible();
  await expect(pane.locator('.position-source-removed')).toBeVisible();
  await pane
    .locator('.position-preview')
    .getByRole('button', { name: 'Open analysis', exact: true })
    .click();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await expect(page.locator('.position-code code')).toHaveText(
    f.position.position_id,
  );
  await expect(page.getByLabel('Result scope', { exact: true })).toHaveCount(
    0,
  );
  await expect(
    page.getByRole('button', { name: 'Analyze game', exact: true }),
  ).toHaveCount(0);
  await expect(
    page.locator('.analysis-heading [data-match-field="id"]'),
  ).toContainText(String(f.metadata.match_no));
  const positionDepth = (await openPositionCalculation(page))
    .getByLabel('Search depth', { exact: true });
  await positionDepth.selectOption('2');
  const accepted = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/jobs' &&
      r.request().method() === 'POST',
  );
  await (await openPositionCalculation(page))
    .getByRole('button', { name: 'Analyze position', exact: true })
    .click();
  const job = await (await accepted).json();
  expect(job.plies).toBe(2);
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
    )
    .toBe('completed');
  const native = await page.request.get(`/api/jobs/${f.native!.id}`);
  expect(native.status()).toBe(200);
  const card = (
    await (await page.request.get(`/api/decks/${f.deck!.id}/cards`)).json()
  ).items[0];
  expect(card.id).toBe(f.card!.id);
  expect(card.review_count).toBe(f.card!.review_count);
  expect(card.due_at).toBe(f.card!.due_at);
  const answer = await (
    await page.request.post(`/api/cards/${card.id}/reveal`, {
      data: { expected_version: card.version },
    })
  ).json();
  expect(answer.answer.notes).toContain('Retain this match explanation');
  expect(answer.answer.notes).toContain('Retain this position explanation');
  expect(
    (await page.request.get(`/api/decks/${f.deck!.id}/export.apkg`)).ok(),
  ).toBe(true);
  await page.reload();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await expect(page.locator('.position-code code')).toHaveText(
    f.position.position_id,
  );
  await expect(page.getByLabel('Result scope', { exact: true })).toHaveCount(
    0,
  );
});

test('deleting the containing database keeps saved positions with source context and no internal library entries', async ({
  page,
}) => {
  const f = await setup(page);
  const deleted = await page.request.delete(
    `/api/databases/${f.database.id}?version=${f.database.version}&delete_matches=true`,
  );
  expect(deleted.status()).toBe(204);
  const retained = await page.request.get(`/api/positions/${f.position.id}`);
  expect(retained.status()).toBe(200);
  const saved = await retained.json();
  expect(saved.match_uuid).not.toBe(f.match.id);
  const databases = await (await page.request.get('/api/databases')).json();
  expect(databases.some((d: { id: string }) => d.id === f.database.id)).toBe(
    false,
  );
  const listed = await (
    await page.request.get(
      `/api/library/matches?player=${encodeURIComponent(f.match.white)}`,
    )
  ).json();
  expect(listed.items).toHaveLength(0);
  const pane = await collection(page, f.tag);
  await expect(pane.locator('.position-table tbody tr')).toHaveCount(1);
  await expect(pane.locator('.position-preview')).toContainText(
    f.database.name,
  );
  await expect(pane.locator('.position-preview')).toContainText('Study Open');
  await expect(pane.locator('.position-preview .board')).toBeVisible();
  await pane
    .locator('.position-preview')
    .getByRole('button', { name: 'Open analysis', exact: true })
    .click();
  await expect(page.locator('.analysis-heading h1')).toContainText(
    f.match.white,
  );
  await expect(page.locator('.position-code code')).toHaveText(
    f.position.position_id,
  );
  await expect(
    page.getByRole('button', { name: 'Recalculate position …', exact: true }),
  ).toBeEnabled();
});
