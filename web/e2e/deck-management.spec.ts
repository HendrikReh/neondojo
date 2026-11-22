import type { CardPage } from '../src/workspaces/study/flashcards/learning-api';
import { expect, test, type Locator, type Page } from './test';

test.use({ viewport: { width: 1800, height: 1169 }, actionTimeout: 5000 });
const initial = () => [
  {
    id: '11111111-1111-4111-8111-111111111111',
    name: 'Race practice',
    card_count: 2,
    version: 3,
    created_at: '2026-11-09T00:00:00Z',
  },
  {
    id: '22222222-2222-4222-8222-222222222222',
    name: 'Cube practice',
    card_count: 0,
    version: 0,
    created_at: '2026-11-09T00:00:00Z',
  },
];
async function setup(page: Page) {
  const state = {
    decks: initial(),
    writes: [] as { method: string; url: string; body: unknown }[],
    conflict: false,
  };
  await page.addInitScript(() => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem(
      'neondojo.deck',
      '11111111-1111-4111-8111-111111111111',
    );
  });
  await page.route('**/api/decks', (route) =>
    route.fulfill({ json: state.decks }),
  );
  await page.route('**/api/decks/*/cards?*', (route) =>
    route.fulfill({ json: { items: [], next_cursor: null } }),
  );
  await page.route(
    /\/api\/decks\/(?:11111111-1111-4111-8111-111111111111|22222222-2222-4222-8222-222222222222)(\?.*)?$/,
    async (route) => {
      const request = route.request(),
        method = request.method(),
        url = request.url();
      state.writes.push({ method, url, body: request.postDataJSON() });
      const deck = state.decks.find((item) => url.includes(item.id))!;
      if (state.conflict)
        return route.fulfill({
          status: 409,
          json: { error: 'deck changed elsewhere' },
        });
      if (method === 'PUT') {
        const body = request.postDataJSON();
        expect(body.expected_version).toBe(deck.version);
        deck.name = body.name;
        deck.version++;
        return route.fulfill({ json: deck });
      }
      expect(new URL(url).searchParams.get('expected_version')).toBe(
        String(deck.version),
      );
      state.decks = state.decks.filter((item) => item.id !== deck.id);
      return route.fulfill({ status: 204 });
    },
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('region', { name: 'Study', exact: true });
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    '11111111-1111-4111-8111-111111111111',
  );
  return { state, study };
}

async function deckAction(
  page: Page,
  study: Locator,
  deckName: string,
  action: 'Rename deck' | 'Delete deck',
) {
  const library = page.getByRole('dialog', {
    name: 'Manage decks',
    exact: true,
  });
  if (!(await library.isVisible())) {
    await study
      .getByRole('button', { name: 'Manage decks', exact: true })
      .click();
  }
  const button = library
    .getByRole('listitem')
    .filter({ has: page.getByText(deckName, { exact: true }) })
    .getByRole('button', { name: action, exact: true });
  await button.click();
  return button;
}

test('creating a deck from the library opens it in card search', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('tabpanel', { name: 'Flashcards & Anki' });
  await study.getByLabel('Choose deck').selectOption('__all__');
  await study
    .getByRole('button', { name: 'Manage decks', exact: true })
    .click();
  const library = page.getByRole('dialog', {
    name: 'Manage decks',
    exact: true,
  });
  const name = `From library ${crypto.randomUUID()}`;
  await library.getByLabel('Deck name', { exact: true }).fill(name);
  await library
    .getByRole('button', { name: 'Create deck', exact: true })
    .click();
  await expect(library).toHaveCount(0);
  await expect(
    study.getByLabel('Choose deck').locator('option:checked'),
  ).toContainText(name);
  await expect(
    study
      .locator('.learning-deck-panel')
      .getByRole('heading', { name, exact: true }),
  ).toBeVisible();
});

test('rename and delete require explicit actions; cancellation restores focus and writes nothing', async ({
  page,
}) => {
  const { state, study } = await setup(page);
  const rename = await deckAction(page, study, 'Race practice', 'Rename deck');
  let dialog = page.getByRole('dialog', { name: 'Rename deck', exact: true });
  await expect(dialog.getByLabel('Deck name', { exact: true })).toHaveValue(
    'Race practice',
  );
  await dialog.getByLabel('Deck name', { exact: true }).fill('Draft');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(rename).toBeFocused();
  await deckAction(page, study, 'Race practice', 'Delete deck');
  dialog = page.getByRole('dialog', { name: 'Delete deck', exact: true });
  await expect(dialog).toContainText('Race practice');
  await expect(dialog).toContainText('2 cards');
  await expect(dialog).toContainText(
    'Saved positions, learning cards and review history remain',
  );
  await expect(
    dialog.getByRole('button', { name: 'Delete deck', exact: true }),
  ).toBeDisabled();
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  expect(state.writes).toEqual([]);
});

test('conflicts retain the rename draft until latest deck details are reviewed and explicitly saved', async ({
  page,
}) => {
  const { state, study } = await setup(page);
  state.conflict = true;
  await deckAction(page, study, 'Race practice', 'Rename deck');
  const dialog = page.getByRole('dialog', { name: 'Rename deck', exact: true });
  await dialog.getByLabel('Deck name', { exact: true }).fill('My draft');
  await dialog.getByRole('button', { name: 'Save name', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('changed');
  await expect(dialog.getByLabel('Deck name', { exact: true })).toHaveValue(
    'My draft',
  );
  await expect(
    dialog.getByRole('button', { name: 'Save name', exact: true }),
  ).toBeDisabled();
  expect(state.writes).toHaveLength(1);
  state.decks[0] = {
    ...state.decks[0],
    name: 'Changed elsewhere',
    version: 4,
    card_count: 5,
  };
  state.conflict = false;
  await dialog
    .getByRole('button', { name: 'Refresh deck details', exact: true })
    .click();
  await expect(dialog).toContainText('Changed elsewhere');
  await expect(dialog).toContainText('5 cards');
  await expect(dialog.getByLabel('Deck name', { exact: true })).toHaveValue(
    'My draft',
  );
  await dialog.getByRole('button', { name: 'Save name', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(
    study.getByLabel('Choose deck', { exact: true }).locator('option:checked'),
  ).toContainText('My draft');
  expect(state.writes).toHaveLength(2);
});

test('deleting the selected deck clears study and late next-card replies, then persists the remaining deck', async ({
  page,
}) => {
  const { state, study } = await setup(page);
  let release!: () => void, started!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const waiting = new Promise<void>((resolve) => {
    started = resolve;
  });
  await page.route(
    '**/api/decks/11111111-1111-4111-8111-111111111111/next',
    async (route) => {
      started();
      await held;
      await route
        .fulfill({ json: { card: null, counts: { due: 999, new: 999 } } })
        .catch(() => {});
    },
  );
  await study.getByRole('button', { name: 'Study now', exact: true }).click();
  await waiting;
  await deckAction(page, study, 'Race practice', 'Delete deck');
  const dialog = page.getByRole('dialog', { name: 'Delete deck', exact: true });
  await dialog.getByRole('checkbox').check();
  await dialog
    .getByRole('button', { name: 'Delete deck', exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    '22222222-2222-4222-8222-222222222222',
  );
  release();
  await expect(study.locator('.study-card')).toHaveCount(0);
  await expect(
    study.getByRole('button', { name: 'Back to card search', exact: true }),
  ).toHaveCount(0);
  await expect(study).not.toContainText('999');
  expect(state.writes).toHaveLength(1);
  await expect
    .poll(() =>
      page.evaluate(
        async () =>
          (await (await fetch('/api/configuration')).json())
            .current_configuration.learning.deck_id,
      ),
    )
    .toBe('11111111-1111-4111-8111-111111111111');
  await deckAction(page, study, 'Cube practice', 'Delete deck');
  const last = page.getByRole('dialog', { name: 'Delete deck', exact: true });
  await last.getByRole('checkbox').check();
  await last.getByRole('button', { name: 'Delete deck', exact: true }).click();
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    '__all__',
  );
  await expect(
    study.getByRole('button', { name: 'Study now', exact: true }),
  ).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(
        async () =>
          (await (await fetch('/api/configuration')).json())
            .current_configuration.learning.deck_id,
      ),
    )
    .toBe('11111111-1111-4111-8111-111111111111');
});

const question = (id: string) => ({
  id,
  version: 1,
  kind: 'checker',
  players: ['Ada', 'Grace'],
  prompt: 'Question',
  review_count: 3,
  due_at: '2026-11-09T00:00:00Z',
  position: {
    id: 'position',
    match_uuid: 'match',
    ordinal: 1,
    game_no: 1,
    ply: 1,
    player: 0,
    dice: [6, 1],
    board: Array(26).fill(0),
    position_id: '4HPwATDgc/ABMA',
    gnubg_match_id: '',
    notation: '',
  },
  source: {
    kind: 'analysis',
    job_id: 'job',
    match_id: 'match',
    position_id: 'position',
    game_no: 1,
    decision_no: null,
  },
});
test('rename keeps a later-page preview and a pending study answer, and modal keyboard focus stays contained', async ({
  page,
}) => {
  const { study } = await setup(page);
  const firstPage = Array.from({ length: 50 }, (_, i) => question(`card-${i}`));
  await page.route(
    '**/api/decks/11111111-1111-4111-8111-111111111111/cards?*',
    (route) =>
      route.fulfill({
        json: {
          items:
            new URL(route.request().url()).searchParams.get('page') === '2'
              ? [question('card-50')]
              : firstPage,
          total: 51,
          next_cursor: null,
        },
      }),
  );
  const chooser = study.getByLabel('Choose deck', { exact: true });
  await chooser.selectOption('22222222-2222-4222-8222-222222222222');
  await chooser.selectOption('11111111-1111-4111-8111-111111111111');
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(50);
  await study.getByRole('button', { name: 'Next page', exact: true }).click();
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(1);
  await study
    .getByRole('button', { name: 'Preview card 51', exact: true })
    .click();
  await deckAction(page, study, 'Race practice', 'Rename deck');
  let dialog = page.getByRole('dialog', { name: 'Rename deck', exact: true });
  const name = dialog.getByLabel('Deck name', { exact: true });
  await expect(name).toBeFocused();
  await name.fill('Renamed race');
  await dialog.getByRole('button', { name: 'Save name', exact: true }).focus();
  await page.keyboard.press('Tab');
  await expect(name).toBeFocused();
  await dialog.getByRole('button', { name: 'Save name', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(1);
  await expect(study.locator('.learning-card-preview')).toHaveAttribute(
    'data-card-id',
    'card-50',
  );
  await page.route(
    '**/api/decks/11111111-1111-4111-8111-111111111111/next',
    (route) =>
      route.fulfill({
        json: { card: question('card-50'), counts: { due: 1, new: 0 } },
      }),
  );
  let release!: () => void, started!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const waiting = new Promise<void>((resolve) => {
    started = resolve;
  });
  await page.route('**/api/cards/card-50/reveal', async (route) => {
    started();
    await held;
    await route.fulfill({
      json: {
        card_id: 'card-50',
        session_id: 'session',
        answer: {
          solution: {
            candidates: [],
            decision: null,
            engine: 'GNUbg',
            plies: 1,
          },
          notes: '',
          tags: [],
        },
        intervals: { again: 60, hard: 60, good: 60, easy: 60 },
      },
    });
  });
  await study.getByRole('button', { name: 'Study now', exact: true }).click();
  await study.getByRole('button', { name: 'Show answer', exact: true }).click();
  await waiting;
  await deckAction(page, study, 'Renamed race', 'Rename deck');
  dialog = page.getByRole('dialog', { name: 'Rename deck', exact: true });
  await dialog.getByLabel('Deck name', { exact: true }).fill('Still studying');
  await dialog.getByRole('button', { name: 'Save name', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(study.locator('.study-card')).toHaveAttribute(
    'data-card-id',
    'card-50',
  );
  release();
  await expect(study.locator('.study-answer')).toBeVisible();
  await expect(study.locator('.study-card')).toContainText('Still studying');
});

test('a stale delete requires refreshed counts and a new explicit confirmation', async ({
  page,
}) => {
  const { state, study } = await setup(page);
  state.conflict = true;
  await deckAction(page, study, 'Race practice', 'Delete deck');
  const dialog = page.getByRole('dialog', { name: 'Delete deck', exact: true });
  await dialog.getByRole('checkbox').check();
  await dialog
    .getByRole('button', { name: 'Delete deck', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toContainText('changed');
  await expect(dialog.getByRole('checkbox')).not.toBeChecked();
  state.decks[0] = { ...state.decks[0], version: 4, card_count: 5 };
  state.conflict = false;
  await dialog
    .getByRole('button', { name: 'Refresh deck details', exact: true })
    .click();
  await expect(dialog).toContainText('5 cards');
  await expect(
    dialog.getByRole('button', { name: 'Delete deck', exact: true }),
  ).toBeDisabled();
  expect(state.writes).toHaveLength(1);
  await dialog.getByRole('checkbox').check();
  await dialog
    .getByRole('button', { name: 'Delete deck', exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    '22222222-2222-4222-8222-222222222222',
  );
});

test('real renamed decks update position membership and Anki; deleting one shared deck preserves reviewed cards', async ({
  page,
}) => {
  test.setTimeout(120000);
  const suffix = crypto.randomUUID(),
    renamed = `Renamed ${suffix}`;
  const database = await (
    await page.request.post('/api/databases', {
      data: { name: `Deck management ${suffix}` },
    })
  ).json();
  const imported = await (
    await page.request.post(`/api/imports?database_id=${database.id}`, {
      multipart: {
        file: {
          name: 'deck-management.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[Deck ${suffix}]PB[Opponent]MI[length:29][ws:5][bs:6];W[31hefe])`,
          ),
        },
      },
    })
  ).json();
  const position = (
    await (
      await page.request.get(`/api/positions?match_id=${imported.match.id}`)
    ).json()
  ).items[0];
  const job = await (
    await page.request.post('/api/jobs', {
      data: { position_id: position.id, kind: 'analysis', plies: 1 },
    })
  ).json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
    )
    .toBe('completed');
  expect(
    (await page.request.put(`/api/positions/${position.id}/bookmark`)).ok(),
  ).toBe(true);
  const decks = [];
  for (const name of [`Original ${suffix}`, `Shared ${suffix}`]) {
    const deck = await (
      await page.request.post('/api/decks', { data: { name } })
    ).json();
    expect(
      (
        await page.request.post(`/api/decks/${deck.id}/positions`, {
          data: { position_ids: [position.id] },
        })
      ).ok(),
    ).toBe(true);
    decks.push(deck);
  }
  const originalCard = (
    await (await page.request.get(`/api/decks/${decks[0].id}/cards`)).json()
  ).items[0];
  const revealed = await (
    await page.request.post(`/api/cards/${originalCard.id}/reveal`, {
      data: { expected_version: originalCard.version },
    })
  ).json();
  expect(
    (
      await page.request.post(`/api/cards/${originalCard.id}/review`, {
        data: {
          session_id: revealed.session_id,
          request_id: crypto.randomUUID(),
          rating: 'good',
        },
      })
    ).ok(),
  ).toBe(true);
  const before: CardPage['items'] = (
    await (await page.request.get(`/api/decks/${decks[1].id}/cards`)).json()
  ).items;
  expect(before.find((card) => card.id === originalCard.id)?.review_count).toBe(
    originalCard.review_count + 1,
  );
  await page.addInitScript((id) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.deck', id);
  }, decks[0].id);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('region', { name: 'Study', exact: true });
  await expect(study.locator('.learning-card-preview')).toHaveAttribute(
    'data-card-id',
    originalCard.id,
  );
  await deckAction(page, study, decks[0].name, 'Rename deck');
  const rename = page.getByRole('dialog', { name: 'Rename deck', exact: true });
  await rename.getByLabel('Deck name', { exact: true }).fill(renamed);
  await rename.getByRole('button', { name: 'Save name', exact: true }).click();
  await expect(rename).toHaveCount(0);
  await expect(
    study.getByLabel('Choose deck', { exact: true }).locator('option:checked'),
  ).toContainText(renamed);
  await expect(study.locator('.learning-card-preview')).toHaveAttribute(
    'data-card-id',
    originalCard.id,
  );
  for (const [width, height] of [
    [1800, 1169],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    expect(
      (await study
        .locator('.learning-deck-panel .learning-controls')
        .boundingBox())!.height,
    ).toBeLessThanOrEqual(65);
    expect(
      (await study
        .locator('.learning-card-list tbody tr')
        .first()
        .boundingBox())!.height,
    ).toBeLessThanOrEqual(40);
    await page.screenshot({
      path: test.info().outputPath(`deck-management-${width}.png`),
    });
  }
  const download = page.waitForEvent('download');
  await study.locator('.learning-anki-menu > summary').click();
  await study
    .getByRole('button', { name: 'Download Anki deck', exact: true })
    .click();
  expect((await download).suggestedFilename()).toBe(`${renamed}.apkg`);
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  const positions = page.getByRole('tabpanel', {
    name: 'Positions',
    exact: true,
  });
  await positions
    .getByRole('combobox', { name: 'Database', exact: true })
    .selectOption(database.id);
  await positions
    .getByRole('button', { name: 'Search positions', exact: true })
    .click();
  await expect(positions.locator('.position-table tbody tr')).toHaveCount(1);
  await expect(positions.locator('.position-preview-decks')).toContainText(
    renamed,
  );
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await study
    .getByLabel('Choose deck', { exact: true })
    .selectOption(decks[0].id);
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    decks[0].id,
  );
  await deckAction(page, study, renamed, 'Delete deck');
  const deletion = page.getByRole('dialog', {
    name: 'Delete deck',
    exact: true,
  });
  await deletion.getByRole('checkbox').check();
  await deletion
    .getByRole('button', { name: 'Delete deck', exact: true })
    .click();
  await expect(deletion).toHaveCount(0);
  await study
    .getByLabel('Choose deck', { exact: true })
    .selectOption(decks[1].id);
  await expect(study.locator('.learning-card-preview')).toHaveAttribute(
    'data-card-id',
    originalCard.id,
  );
  const after = (
    await (await page.request.get(`/api/decks/${decks[1].id}/cards`)).json()
  ).items;
  const history = (cards: CardPage['items']) =>
    cards.map((card) => [
      card.id,
      card.version,
      card.review_count,
      card.due_at,
    ]);
  expect(history(after)).toEqual(history(before));
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await expect(positions.locator('.position-preview-decks')).not.toContainText(
    renamed,
  );
  await expect(positions.locator('.position-preview-decks')).toContainText(
    decks[1].name,
  );
  await expect(positions.locator('.position-preview .board')).toBeVisible();
});

test('deck search, card moves and shared membership removal work in Study', async ({
  page,
}) => {
  const suffix = crypto.randomUUID();
  const imported = await (
    await page.request.post('/api/imports', {
      multipart: {
        file: {
          name: 'deck-move.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[Move ${suffix}]PB[Opponent]MI[length:31][ws:5][bs:7];W[31hefe])`,
          ),
        },
      },
    })
  ).json();
  const position = (
    await (
      await page.request.get(`/api/positions?match_id=${imported.match.id}`)
    ).json()
  ).items[0];
  const job = await (
    await page.request.post('/api/jobs', {
      data: { position_id: position.id, kind: 'analysis', plies: 1 },
    })
  ).json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
    )
    .toBe('completed');
  const [source, target, other] = await Promise.all(
    ['Source', 'Target', 'Other'].map(async (prefix) => {
      const response = await page.request.post('/api/decks', {
        data: { name: `${prefix} ${suffix}` },
      });
      await expect(response).toBeOK();
      return response.json();
    }),
  );
  for (const deck of [source, other]) {
    const response = await page.request.post(`/api/decks/${deck.id}/cards`, {
      data: { position_id: position.id },
    });
    await expect(response).toBeOK();
  }
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('tabpanel', { name: 'Flashcards & Anki' });
  await study.getByRole('button', { name: 'Manage decks' }).click();
  const decks = page.getByRole('dialog', { name: 'Manage decks' });
  await decks.getByRole('searchbox').fill(source.name.toUpperCase());
  await expect(decks.getByRole('listitem')).toHaveCount(1);
  await expect(
    decks.getByRole('button', { name: 'Rename deck' }),
  ).toBeVisible();
  await expect(
    decks.getByRole('button', { name: 'Delete deck' }),
  ).toBeVisible();
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(decks.getByRole('searchbox')).toBeInViewport();
    await page.screenshot({ path: `test-results/deck-library-${width}.png` });
  }
  await decks.getByRole('button', { name: 'Open deck' }).click();
  await study.getByRole('checkbox', { name: 'Select card 1' }).check();
  await study
    .getByRole('button', { name: 'Move cards (1)', exact: true })
    .click();
  const move = page.getByRole('dialog', { name: 'Move cards (1)' });
  await move.getByLabel('Target deck').selectOption(target.id);
  await move
    .getByRole('button', { name: 'Move cards (1)', exact: true })
    .click();
  await expect(
    study.getByText(`Moved 1 card to “${target.name}”.`),
  ).toBeVisible();
  await expect(
    study.getByRole('checkbox', { name: 'Select card 1' }),
  ).toHaveCount(0);

  await study.getByRole('button', { name: 'Find duplicates' }).click();
  const finder = page.getByRole('dialog', { name: 'Find duplicates' });
  const remove = `Remove from “${other.name}”`;
  await finder.getByRole('button', { name: remove, exact: true }).click();
  await finder
    .getByRole('group', { name: remove })
    .getByRole('button', { name: remove })
    .click();
  await expect(
    finder.getByRole('button', { name: remove, exact: true }),
  ).toHaveCount(0);
  for (const [deck, count] of [
    [source, 0],
    [target, 1],
    [other, 0],
  ] as const) {
    const result = await (
      await page.request.get(`/api/decks/${deck.id}/cards`)
    ).json();
    expect(result.items).toHaveLength(count);
  }
});
