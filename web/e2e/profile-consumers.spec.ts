import { test, expect, type Page } from '@playwright/test';
import { profileBackend, secondProfileId } from './profile-backend';
import {
  defaultConfiguration,
  type ProfileConfiguration,
} from '../src/configuration-model';

const firstId = '00000000-0000-4000-8000-000000000001';
const databaseId = '00000000-0000-0000-0000-000000000001';
const deckA = '11111111-1111-4111-8111-111111111111';
const deckB = '22222222-2222-4222-8222-222222222222';
const match = (name: string) => ({
  id: '33333333-3333-4333-8333-333333333333',
  database_id: databaseId,
  match_no: 17,
  filename: 'profile.sgf',
  display_name: name,
  format: 'sgf',
  white: `${name} white`,
  black: `${name} black`,
  match_length: 7,
  position_count: 1,
  game_count: 1,
  created_at: '2026-11-10T12:00:00Z',
  played_on: null,
  tournament: '',
  round: '',
  place: '',
  result_state: 'unknown',
  white_score: null,
  black_score: null,
  white_pr: null,
  black_pr: null,
  pr_method: null,
  pr_profile: null,
  analysis_status: null,
  tags: [],
  comment: '',
  version: 0,
});

async function fixture(
  page: Page,
  configure?: (a: ProfileConfiguration, b: ProfileConfiguration) => void,
) {
  const a = structuredClone(defaultConfiguration),
    b = structuredClone(defaultConfiguration);
  for (const configuration of [a, b]) {
    configuration.opening_book.path = '/books/default.db';
    configuration.opening_book.xg_path = '/books/XG.ob';
  }
  a.library.filters.q = 'Alpha';
  a.library.visible_columns = ['id', 'white', 'black'];
  a.library.column_order = [
    'white',
    ...a.library.column_order.filter((id) => id !== 'white'),
  ];
  a.library.column_widths = { id: 90, white: 240, black: 200 };
  a.library.sort = 'id';
  a.library.direction = 'asc';
  b.library.filters.q = 'Beta';
  b.library.visible_columns = ['black', 'id'];
  b.library.column_order = [
    'black',
    ...b.library.column_order.filter((id) => id !== 'black'),
  ];
  b.library.column_widths = { id: 80, black: 310 };
  b.library.sort = 'white';
  b.library.direction = 'desc';
  configure?.(a, b);
  const controller = await profileBackend(page, {
    configuration: a,
    profiles: [
      { id: firstId, name: 'Standard', revision: 1, configuration: a },
      {
        id: secondProfileId,
        name: 'Deep analysis',
        revision: 1,
        configuration: b,
      },
    ],
  });
  await page.route('**/api/databases', (route) =>
    route.fulfill({
      json: [
        {
          id: databaseId,
          name: 'Profile test',
          description: '',
          is_default: true,
          is_system: true,
          version: 0,
          match_count: 1,
          created_at: '2026-11-10T12:00:00Z',
          updated_at: '2026-11-10T12:00:00Z',
        },
      ],
    }),
  );
  await page.route('**/api/decks', (route) =>
    route.fulfill({
      json: [
        {
          id: deckA,
          name: 'Race practice',
          card_count: 1,
          due_count: 1,
          version: 0,
        },
        {
          id: deckB,
          name: 'Cube practice',
          card_count: 1,
          due_count: 1,
          version: 0,
        },
      ],
    }),
  );
  await page.route(/\/api\/(?:cards|decks\/[^/]+\/cards)\?/, (route) =>
    route.fulfill({ json: { items: [], total: 0, next_cursor: null } }),
  );
  return controller;
}

async function switchProfile(page: Page, name: string) {
  await page.getByRole('button', { name: /^Profile:/ }).click();
  await page.getByText(name, { exact: true }).click();
  await expect(
    page.getByRole('button', { name: new RegExp(`^Profile: ${name}`) }),
  ).toBeVisible();
}
const headers = (page: Page) =>
  page
    .locator('.database-table thead th')
    .evaluateAll((cells) =>
      cells.map((cell) => cell.getAttribute('data-column')),
    );

test('filters and local column layouts survive profile switches and reload', async ({
  page,
}) => {
  const controller = await fixture(page);
  const queries: URL[] = [];
  await page.route('**/api/library/matches?*', (route) => {
    const url = new URL(route.request().url());
    queries.push(url);
    return route.fulfill({
      json: {
        items: [match(url.searchParams.get('q') ?? '')],
        next_cursor: null,
      },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(1);
  expect(await headers(page)).toEqual([
    'select',
    'white',
    'id',
    'black',
  ]);
  await expect(
    page.getByRole('textbox', { name: 'Search matches' }),
  ).toHaveValue('Alpha');
  await page.getByRole('button', { name: /Profile test/ }).click();
  await page
    .getByRole('textbox', { name: 'Search matches' })
    .fill('Current search');
  await expect(page.locator('.database-table tbody tr')).toContainText(
    'Current search black',
  );
  const width = async () => (await page
    .locator('th[data-column="white"]')
    .boundingBox())!.width;
  expect(await width()).toBeCloseTo(240, 0);
  await page.locator('th[data-column="white"]').getByRole('separator').press('ArrowRight');
  await expect.poll(width).toBeCloseTo(250, 0);
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  await switchProfile(page, 'Deep analysis');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(
    page.getByRole('textbox', { name: 'Search matches' }),
  ).toHaveValue('Current search');
  await expect(page.locator('.database-table tbody tr')).toContainText(
    'Current search black',
  );

  expect(await headers(page)).toEqual(['select', 'white', 'id', 'black']);
  await expect.poll(width).toBeCloseTo(250, 0);
  expect(
    queries.some(
      (url) =>
        url.searchParams.get('q') === 'Current search' &&
        url.searchParams.get('sort') === 'id' &&
        url.searchParams.get('direction') === 'asc' &&
        url.searchParams.get('database_id') === databaseId,
    ),
  ).toBe(true);
  await switchProfile(page, 'Standard');
  await expect(
    page.getByRole('textbox', { name: 'Search matches' }),
  ).toHaveValue('Current search');
  expect(await headers(page)).toEqual([
    'select',
    'white',
    'id',
    'black',
  ]);
  expect(
    controller.calls
      .filter((call) => call.method !== 'GET')
      .map((call) => call.path),
  ).toEqual([
    `/profiles/${secondProfileId}/activate`,
    `/profiles/${firstId}/activate`,
  ]);
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(
    page.getByRole('button', { name: /Profile test/ }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect(
    page.getByRole('textbox', { name: 'Search matches' }),
  ).toHaveValue('Current search');
  await expect.poll(width).toBeCloseTo(250, 0);
});

test('a late search response cannot replace a newer session query', async ({
  page,
}) => {
  await fixture(page, (a) => {
    a.library.filters.q = 'slow';
  });
  let release!: () => void, started!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const waiting = new Promise<void>((resolve) => {
    started = resolve;
  });
  await page.route('**/api/library/matches?*', async (route) => {
    const q = new URL(route.request().url()).searchParams.get('q') ?? '';
    if (q === 'slow') {
      started();
      await held;
    }
    await route
      .fulfill({
        json: {
          items: [match(q === 'slow' ? 'STALE RESULT' : 'Beta')],
          next_cursor: null,
        },
      })
      .catch(() => {});
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await waiting;
  await page.getByRole('textbox', { name: 'Search matches' }).fill('Beta');
  await expect(page.locator('.database-table tbody tr')).toContainText(
    'Beta black',
  );
  release();
  await expect(page.locator('.database-table tbody tr')).not.toContainText(
    'STALE RESULT',
  );
});

test('a profile switch retains the active learning card and the selected deck', async ({
  page,
}) => {
  const controller = await fixture(page, (a, b) => {
    a.learning.deck_id = deckA;
    b.learning.deck_id = deckB;
  });
  const nextCalls: string[] = [];
  await page.route('**/api/library/matches?*', (route) =>
    route.fulfill({ json: { items: [], next_cursor: null } }),
  );
  await page.route('**/api/decks/*/next', (route) => {
    nextCalls.push(route.request().url());
    return route.fulfill({
      json: {
        counts: { due: 1, new: 0 },
        card: {
          id: '44444444-4444-4444-8444-444444444444',
          version: 1,
          kind: 'checker',
          players: ['Learner', 'Opponent'],
          prompt: '',
          review_count: 0,
          position: {
            id: '55555555-5555-4555-8555-555555555555',
            match_uuid: match('').id,
            ordinal: 0,
            game_no: 1,
            ply: 1,
            player: 0,
            dice: [3, 1],
            board: Array(26).fill(0),
            position_id: '4HPwATDgc/ABMA',
            gnubg_match_id: 'cAkAAAAAAAAA',
            notation: '',
          },
        },
      },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('region', { name: 'Study', exact: true });
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    deckA,
  );
  await study.getByRole('button', { name: 'Study now', exact: true }).click();
  await expect(page.locator('.study-card')).toHaveAttribute(
    'data-card-id',
    '44444444-4444-4444-8444-444444444444',
  );
  await switchProfile(page, 'Deep analysis');
  await expect(page.locator('.study-card')).toHaveAttribute(
    'data-card-id',
    '44444444-4444-4444-8444-444444444444',
  );
  expect(nextCalls).toHaveLength(1);
  expect(controller.state.current_configuration.learning.deck_id).toBe(deckB);
  await study
    .getByRole('button', { name: 'Back to card search', exact: true })
    .click();
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    deckA,
  );
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckB);
  await switchProfile(page, 'Standard');
  await page.reload();
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    deckB,
  );
  expect(
    controller.calls
      .filter((call) => call.method !== 'GET')
      .map((call) => call.path),
  ).toEqual([
    `/profiles/${secondProfileId}/activate`,
    `/profiles/${firstId}/activate`,
  ]);
});

test('switching profiles preserves an unsaved note without submitting it', async ({
  page,
}) => {
  await fixture(page);
  await page.route('**/api/library/matches?*', (route) =>
    route.fulfill({ json: { items: [], next_cursor: null } }),
  );
  await page.goto('/');
  const marker = crypto.randomUUID();
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: `profile-note-${marker}.sgf`,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Profile ${marker}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  const comment = page
    .locator('.annotation-panel')
    .getByLabel('Comment', { exact: true });
  await expect(comment).toBeEnabled();
  await comment.fill(`Unsaved profile note ${marker}`);
  const writes: string[] = [];
  page.on('request', (request) => {
    if (request.method() !== 'GET')
      writes.push(new URL(request.url()).pathname);
  });
  await switchProfile(page, 'Deep analysis');
  await expect(comment).toHaveValue(`Unsaved profile note ${marker}`);
  expect(writes).toEqual([`/api/profiles/${secondProfileId}/activate`]);
});

test('raw library search text retains typed spaces while the applied query is canonical', async ({
  page,
}) => {
  test.setTimeout(15000);
  const controller = await fixture(page);
  const queries: URL[] = [];
  await page.route('**/api/library/matches?*', (route) => {
    queries.push(new URL(route.request().url()));
    return route.fulfill({ json: { items: [], next_cursor: null } });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  const input = page.getByRole('textbox', { name: 'Search matches' });
  await input.fill('');
  await input.pressSequentially('Alpha Beta ');
  await expect(input).toHaveValue('Alpha Beta ');
  await expect
    .poll(() =>
      queries.some((url) => url.searchParams.get('q') === 'Alpha Beta'),
    )
    .toBe(true);
  expect(controller.state.current_configuration.library.filters.q).toBe(
    'Alpha',
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(input).toHaveValue('Alpha Beta');
  expect(controller.calls.filter((call) => call.method !== 'GET')).toEqual([]);
});
