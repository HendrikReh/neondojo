import { expect, test } from './test';

test.use({ viewport: { width: 1800, height: 1169 }, actionTimeout: 10000 });

test('search results add, move, remove and delete selected cards', async ({
  page,
}, info) => {
  const player = `Card actions ${crypto.randomUUID()}`;
  const imported = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'card-actions.sgf',
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(
          `(;GM[6]PW[${player}]PB[Opponent]MI[length:17][ws:3][bs:1];W[31hefe];B[42qusu])`,
        ),
      },
    },
  });
  await expect(imported).toBeOK();
  const { match } = await imported.json();
  const { items: positions } = await (
    await page.request.get(`/api/positions?match_id=${match.id}`)
  ).json();
  const [source, target] = await Promise.all(
    ['Source', 'Target'].map(async (prefix) => {
      const response = await page.request.post('/api/decks', {
        data: { name: `${prefix} ${player}` },
      });
      await expect(response).toBeOK();
      return response.json();
    }),
  );
  expect(positions).toHaveLength(2);
  for (const position of positions) {
    const job = await page.request.post('/api/jobs', {
      data: { position_id: position.id, kind: 'analysis', plies: 0 },
    });
    await expect(job).toBeOK();
    const { id } = await job.json();
    await expect
      .poll(
        async () =>
          (await (await page.request.get(`/api/jobs/${id}`)).json()).status,
      )
      .toBe('completed');
    await expect(
      await page.request.post(`/api/decks/${source.id}/cards`, {
        data: { position_id: position.id },
      }),
    ).toBeOK();
  }
  const loadDeckCards = async (deck: typeof source) =>
    (await (await page.request.get(`/api/decks/${deck.id}/cards`)).json())
      .items;
  const original = await loadDeckCards(source);
  expect(original).toHaveLength(2);
  const ids = original.map((c: { id: string }) => c.id);
  const reviewed = original[0];
  const reveal = await page.request.post(`/api/cards/${reviewed.id}/reveal`, {
    data: { expected_version: reviewed.version },
  });
  await expect(reveal).toBeOK();
  await expect(
    await page.request.post(`/api/cards/${reviewed.id}/review`, {
      data: {
        session_id: (await reveal.json()).session_id,
        request_id: crypto.randomUUID(),
        rating: 'good',
      },
    }),
  ).toBeOK();

  await page.addInitScript(() => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.deck', '__all__');
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  const tab = page.getByRole('tab', { name: 'Flashcards & Anki', exact: true });
  await tab.click();
  const study = page.getByRole('tabpanel', { name: 'Flashcards & Anki' });
  const scope = study.getByRole('combobox', { name: 'Choose deck' });
  const actions = study.getByRole('group', { name: 'Card actions' });
  const rows = study.locator('.learning-card-list tbody tr');
  const button = (name: string) =>
    actions.getByRole('button', { name, exact: true });
  const dialog = (name: string) =>
    page.getByRole('dialog', { name, exact: true });
  async function confirm(name: string) {
    await dialog(name).getByRole('button', { name, exact: true }).click();
    await expect(dialog(name)).toHaveCount(0);
  }
  async function toDeck(name: string) {
    await button(name).click();
    await dialog(name).getByLabel('Target deck').selectOption(target.id);
    await confirm(name);
  }

  await study.getByLabel('Search text', { exact: true }).fill(player);
  await study
    .getByRole('button', { name: 'Search cards', exact: true })
    .click();
  await expect(scope).toHaveValue('__all__');
  await expect(rows).toHaveCount(2);
  await study
    .getByRole('button', { name: 'Practice results', exact: true })
    .click();
  const practice = study.getByRole('region', { name: 'Practice results' });
  await expect(practice.getByText('Card 1 of 2')).toBeVisible();
  await practice
    .getByRole('button', { name: 'Show answer', exact: true })
    .click();
  const marker = practice.getByRole('checkbox', { name: 'Mark for review' });
  await marker.click();
  await expect(marker).toBeChecked();
  await practice
    .getByRole('button', { name: 'Back to card search', exact: true })
    .click();
  await study
    .getByRole('button', { name: 'Resume practice', exact: true })
    .click();
  await expect(marker).toBeChecked();
  await practice
    .getByRole('button', { name: 'Next card', exact: true })
    .click();
  await expect(practice.getByText('Card 2 of 2')).toBeVisible();
  await practice
    .getByRole('button', { name: 'Show answer', exact: true })
    .click();
  await practice
    .getByRole('button', { name: 'Next card', exact: true })
    .click();
  await practice
    .getByRole('button', {
      name: 'Retry errors and marked cards (1)',
      exact: true,
    })
    .click();
  await expect(practice.getByText('Card 1 of 1')).toBeVisible();
  await expect(
    practice.getByRole('button', { name: 'Show answer', exact: true }),
  ).toBeVisible();
  await practice
    .getByRole('button', { name: 'Back to card search', exact: true })
    .click();
  const afterPractice = await loadDeckCards(source);
  expect(
    afterPractice.find((card: { id: string }) => card.id === reviewed.id)
      .review_count,
  ).toBe(1);
  const firstCard = study
    .getByRole('checkbox', { name: /^Select card \d$/ })
    .first();
  await firstCard.check();
  await study
    .getByRole('button', { name: 'Pip count practice', exact: true })
    .click();
  const pipDialog = dialog('Pip count practice');
  await pipDialog.getByLabel('White', { exact: true }).fill('0');
  await pipDialog.getByLabel('Black', { exact: true }).fill('0');
  await pipDialog
    .getByRole('button', { name: 'Check pip counts', exact: true })
    .click();
  await expect(
    pipDialog.getByText(/Incorrect.*pips.*from the bar/),
  ).toHaveCount(2);
  await pipDialog.getByRole('button', { name: 'Close', exact: true }).click();
  await firstCard.uncheck();
  await expect(button('Add to deck')).toBeDisabled();
  await expect(button('Remove from this deck')).toHaveCount(0);
  await study.getByRole('checkbox', { name: 'Select this page' }).check();
  await expect(actions).toContainText('2 selected');
  const checkColumn = study.getByRole('columnheader').first();
  expect((await checkColumn.boundingBox())?.width).toBeLessThanOrEqual(60);
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(actions).toBeInViewport();
    await expect(button('Delete permanently')).toBeInViewport();
    await page.screenshot({
      path: info.outputPath(`card-actions-${width}.png`),
    });
  }
  await toDeck('Add to deck');
  await expect(
    study.getByText(`Added to “${target.name}”: 2. Already in deck: 0.`),
  ).toBeVisible();
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText(target.name);
  expect(
    (await loadDeckCards(target)).map((c: { id: string }) => c.id),
  ).toEqual(ids);
  expect((await loadDeckCards(target))[0].review_count).toBe(1);
  await study.getByRole('checkbox', { name: 'Select this page' }).check();
  await toDeck('Add to deck');
  await expect(
    study.getByText(`Added to “${target.name}”: 0. Already in deck: 2.`),
  ).toBeVisible();

  await scope.selectOption(source.id);
  await expect(rows).toHaveCount(2);
  await study.getByRole('checkbox', { name: 'Select this page' }).check();
  await toDeck('Move cards (2)');
  await expect(rows).toHaveCount(0);
  expect(await loadDeckCards(source)).toEqual([]);
  await scope.selectOption(target.id);
  await expect(rows).toHaveCount(2);
  await study.getByRole('checkbox', { name: 'Select this page' }).check();
  await button('Remove from this deck').click();
  await confirm('Remove from this deck');
  await expect(rows).toHaveCount(0);

  await scope.selectOption({ label: 'Without a deck' });
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText('Not in a deck yet');
  const [first, second] = await study
    .getByRole('checkbox', { name: /^Select card \d$/ })
    .all();
  await first.check();
  await second.check();
  await button('Delete permanently').click();
  const deletion = dialog('Delete permanently');
  await expect(deletion).toContainText('Permanently delete 2 selected cards');
  await expect(deletion).toContainText(
    'Their review history will be deleted too.',
  );
  const cancel = deletion.getByRole('button', { name: 'Cancel' });
  await expect(cancel).toBeFocused();
  await cancel.click();
  await expect(rows).toHaveCount(2);
  await expect(button('Delete permanently')).toBeFocused();
  await button('Delete permanently').click();
  await confirm('Delete permanently');
  await expect(study.getByText('Cards deleted: 2.')).toBeVisible();
  await expect(rows).toHaveCount(0);
  const remaining = await page.request.get('/api/cards', {
    params: { q: player },
  });
  expect((await remaining.json()).total).toBe(0);
});
