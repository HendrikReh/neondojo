import { expect, test, type Page } from './test';

test.use({ viewport: { width: 1800, height: 1169 }, actionTimeout: 10000 });
const deckA = '11111111-1111-4111-8111-111111111111';
const deckB = '22222222-2222-4222-8222-222222222222';
async function setup(page: Page, singleDeck = false) {
  const cards = Array.from({ length: 53 }, (_, index) => ({
    id: `card-${index}`,
    version: 1,
    kind: index === 2 ? 'cube_offer' : 'checker',
    players: [index === 52 ? 'Mochy' : 'Ada', 'Hendrik'],
    prompt: 'What is the best checker play?',
    due_at: '2027-01-01T00:00:00Z',
    review_count: index === 52 ? 4 : 0,
    position: {
      id: `card-${index}`,
      match_uuid: 'match',
      ordinal: index,
      game_no: 1,
      ply: index + 1,
      player: 0,
      dice: [3, 1],
      board: Array(26).fill(0),
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cAkAAAAAAAAA',
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
    browse: {
      decks:
        index === 52
          ? [
              { id: deckA, name: 'Prime' },
              { id: deckB, name: 'Mochy studies' },
            ]
          : [{ id: deckA, name: 'Prime' }],
      tags: index === 52 ? ['prime'] : [],
      comment_excerpt: index === 52 ? 'Effective prime lesson' : '',
    },
  }));
  const queries: URL[] = [];
  const writes: string[] = [];
  let slow: (() => Promise<void>) | undefined;
  await page.addInitScript(() => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.deck', '__all__');
  });
  await page.route('**/api/decks', (route) =>
    route.fulfill({
      json: [
        {
          id: deckA,
          name: 'Prime',
          card_count: 53,
          due_count: 0,
          version: 0,
          created_at: '2026-01-01T00:00:00Z',
        },
        {
          id: deckB,
          name: 'Mochy studies',
          card_count: 1,
          due_count: 0,
          version: 0,
          created_at: '2026-01-01T00:00:00Z',
        },
      ].filter((deck) => !singleDeck || deck.id === deckB),
    }),
  );
  const respond = async (
    route: Parameters<Parameters<Page['route']>[1]>[0],
  ) => {
    const url = new URL(route.request().url());
    queries.push(url);
    const p = url.searchParams;
    if (p.get('q') === 'slow') await slow?.();
    let result = cards.filter(
      (card) =>
        (!p.get('q') ||
          `${card.players.join(' ')} ${card.browse.comment_excerpt} ${card.browse.tags.join(' ')}`
            .toLowerCase()
            .includes(p.get('q')!.toLowerCase())) &&
        (!p.get('player') ||
          card.players[0]
            .toLowerCase()
            .includes(p.get('player')!.toLowerCase())) &&
        (!p.get('tag') || card.browse.tags.includes(p.get('tag')!)) &&
        (!p.get('kind') || card.kind === p.get('kind')) &&
        (!p.get('status') ||
          (p.get('status') === 'reviewed' && card.review_count > 0)),
    );
    if (url.pathname.includes(deckB))
      result = result.filter((c) => c.browse.decks.some((d) => d.id === deckB));
    const total = result.length;
    const page = Number(p.get('page') ?? '1');
    result = result.slice((page - 1) * 50);
    const items = result.slice(0, 50);
    await route
      .fulfill({
        json: {
          items,
          total,
          next_cursor: result.length > 50 ? items.at(-1)!.id : null,
        },
      })
      .catch(() => {});
  };
  await page.route(/\/api\/(?:cards|decks\/[^/]+\/cards)(?:\?.*)?$/, respond);
  await page.route('**/api/decks/*/anki-auto-sync', (route) =>
    route.fulfill({
      json: {
        profile: null,
        last_synced_at: null,
        last_received_at: null,
        last_outcome: null,
      },
    }),
  );
  page.on('request', (req) => {
    if (
      !['GET', 'HEAD'].includes(req.method()) &&
      /\/api\/(cards|decks)/.test(req.url())
    )
      writes.push(req.url());
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  return {
    study: page.getByRole('region', { name: 'Study', exact: true }),
    queries,
    writes,
    setSlow: (fn: () => Promise<void>) => {
      slow = fn;
    },
  };
}

test('Flashcards has an external heading, server search beyond the first page and a persistent question-side preview', async ({
  page,
}) => {
  const { study, queries, writes } = await setup(page);
  await expect(study).toHaveCSS('border-top-width', '0px');
  const heading = study.locator('h1');
  await expect(heading).toHaveText('Flashcards');
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(50);
  await expect(study.locator('.learning-deck-panel')).toHaveCount(0);
  for (const name of [
    'Study now',
    'Download Anki deck',
    'Rename deck',
    'Delete deck',
    'Create deck',
  ]) {
    await expect(study.getByRole('button', { name, exact: true })).toHaveCount(
      0,
    );
  }
  await study
    .getByRole('button', { name: 'Manage decks', exact: true })
    .click();
  const library = page.getByRole('dialog', {
    name: 'Manage decks',
    exact: true,
  });
  await expect(
    library.getByRole('button', { name: 'Create deck', exact: true }),
  ).toBeVisible();
  await expect(
    library.getByRole('button', { name: 'Rename deck', exact: true }),
  ).toHaveCount(2);
  await expect(
    library.getByRole('button', { name: 'Delete deck', exact: true }),
  ).toHaveCount(2);
  await library.getByRole('button', { name: 'Cancel', exact: true }).click();
  await study.getByLabel('Search text', { exact: true }).fill('Mochy');
  await study
    .getByRole('button', { name: 'Search cards', exact: true })
    .click();
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(1);
  await expect(study.locator('.learning-card-list')).toContainText('Mochy');
  await expect(study.locator('.learning-card-list')).toContainText(
    'Mochy studies',
  );
  await expect(study.locator('.learning-card-preview')).toHaveAttribute(
    'data-card-id',
    'card-52',
  );
  await expect(study.locator('.learning-card-preview .board')).toBeVisible();
  expect(queries.at(-1)!.searchParams.get('q')).toBe('Mochy');
  await study.getByLabel('Search text', { exact: true }).fill('no-result');
  await study
    .getByRole('button', { name: 'Search cards', exact: true })
    .click();
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(0);
  await expect(study.locator('.learning-card-preview .board')).toHaveCount(0);
  await study
    .getByRole('button', { name: 'Clear filters', exact: true })
    .click();
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(50);
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckB);
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(1);
  await expect(
    study.getByRole('button', { name: 'Study now', exact: true }),
  ).toBeEnabled();
  await expect(
    study.getByRole('button', { name: 'Rename deck', exact: true }),
  ).toHaveCount(0);
  await expect(
    study.getByRole('button', { name: 'Delete deck', exact: true }),
  ).toHaveCount(0);
  await study
    .getByLabel('Choose deck', { exact: true })
    .selectOption('__without_deck__');
  await expect(study.locator('.learning-deck-panel')).toHaveCount(0);
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckB);
  for (const viewport of [
    { width: 1800, height: 1169 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ]) {
    await page.setViewportSize(viewport);
    const workspace = await study.boundingBox();
    expect(workspace!.width).toBeGreaterThanOrEqual(viewport.width - 80);
    expect(workspace!.x).toBeLessThanOrEqual(40);
    const list = await study.locator('.learning-card-list').boundingBox(),
      preview = await study.locator('.learning-card-preview').boundingBox();
    expect(preview!.x).toBeGreaterThanOrEqual(list!.x + list!.width);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: test.info().outputPath(`flashcards-${viewport.width}.png`),
    });
  }
  expect(writes).toEqual([]);
  await expect(study.locator('.study-answer')).toHaveCount(0);
});

test('Send to Anki targets the selected deck and reports the import outcome', async ({
  page,
}) => {
  const { study } = await setup(page);
  const send = study.getByRole('button', { name: 'Send to Anki', exact: true });
  await expect(send).toHaveCount(0);
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckB);
  let attempts = 0;
  await page.route(`**/api/decks/${deckB}/send-to-anki`, async (route) => {
    expect(route.request().method()).toBe('POST');
    expect(route.request().postDataJSON()).toEqual({ locale: 'en' });
    if (++attempts === 1) {
      await route.fulfill({
        status: 503,
        json: { error: 'Connection refused' },
      });
    } else {
      await route.fulfill({ status: 204 });
    }
  });
  await study.locator('.learning-anki-menu > summary').click();
  await send.click();
  await expect(study.getByRole('alert')).toHaveText(
    'Open Anki Desktop with the AnkiConnect add-on listening on port 8765.',
  );
  await study.locator('.learning-anki-menu > summary').click();
  await send.click();
  await expect(study.getByText('Anki imported “Mochy studies”.')).toBeVisible();
  await expect(study.getByRole('alert')).toHaveCount(0);
  expect(attempts).toBe(2);
});

test('Sync Anki reviews reports both directions for the selected deck', async ({
  page,
}) => {
  const { study } = await setup(page);
  const sync = study.getByRole('button', { name: 'Sync Anki reviews' });
  await expect(sync).toHaveCount(0);
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckB);
  await page.route(`**/api/decks/${deckB}/sync-anki-reviews`, async (route) => {
    expect(route.request().method()).toBe('POST');
    expect(route.request().postDataJSON()).toEqual({});
    await route.fulfill({
      json: {
        profile: 'Test profile',
        matched_cards: 1,
        unmatched_cards: 0,
        received_reviews: 2,
        sent_reviews: 1,
      },
    });
  });
  await study.locator('.learning-anki-menu > summary').click();
  await sync.click();
  await expect(study.getByText(/Anki profile Test profile/)).toContainText(
    '2 reviews received · 1 sent · 1 card matched · 0 unmatched',
  );
});

test('Anki sync overview shows linked decks, transfer counts and the pause reason', async ({
  page,
}) => {
  const { study } = await setup(page);
  let paused = false;
  let received = 2;
  let retryFailed = false;
  let retried = false;
  let lastResolved = false;
  const actions: string[] = [];
  const failedRun = {
    id: 'failed-run',
    profile: 'Test profile',
    finished_at: '2026-11-13T12:01:00Z',
    received_reviews: 2,
    sent_reviews: 3,
    outcome: 'failed',
    problem: 'timeout',
    error_detail: 'AnkiConnect did not confirm the review import in time',
  };
  await page.route(`**/api/decks/${deckB}/anki-auto-sync/history`, (route) =>
    route.fulfill({
      json: [
        ...(retried
          ? [
              {
                ...failedRun,
                id: 'retry-success',
                finished_at: '2026-11-13T12:04:00Z',
                received_reviews: 0,
                sent_reviews: 0,
                outcome: 'synced',
                problem: null,
                error_detail: null,
              },
            ]
          : []),
        ...(retryFailed
          ? [
              {
                ...failedRun,
                id: 'retry-failure',
                finished_at: '2026-11-13T12:03:00Z',
                received_reviews: 0,
                sent_reviews: 0,
                problem: 'conflict',
                error_detail: 'Open the linked Anki profile',
                resolved_at: lastResolved ? '2026-11-13T12:05:00Z' : null,
              },
            ]
          : []),
        ...(received === 2
          ? [failedRun]
          : [
              {
                ...failedRun,
                id: 'successful-run',
                finished_at: '2026-11-13T12:02:00Z',
                received_reviews: 1,
                sent_reviews: 0,
                outcome: 'synced',
                problem: null,
                error_detail: null,
              },
              {
                ...failedRun,
                resolved_at: retried ? '2026-11-13T12:04:00Z' : null,
              },
            ]),
      ],
    }),
  );
  await page.route('**/api/study/anki-auto-sync', (route) =>
    route.fulfill({
      json: [
        {
          deck_id: deckB,
          deck_name: 'Mochy studies',
          profile: 'Test profile',
          last_synced_at: '2026-11-13T12:00:00Z',
          received_reviews: received,
          sent_reviews: 3,
          open_errors: lastResolved ? 0 : retryFailed && !retried ? 2 : 1,
          paused,
          status: paused ? 'paused' : 'other_profile',
        },
        {
          deck_id: deckA,
          deck_name: 'Prime',
          profile: 'Test profile',
          last_synced_at: null,
          received_reviews: 0,
          sent_reviews: 0,
          open_errors: 0,
          paused: false,
          status: 'waiting',
        },
      ],
    }),
  );
  await page.route(
    `**/api/decks/${deckB}/anki-auto-sync/action`,
    async (route) => {
      expect(route.request().method()).toBe('POST');
      const { action, retry_of } = route.request().postDataJSON();
      actions.push(action);
      if (action === 'sync_now') {
        if (received === 2) {
          expect(retry_of).toBeUndefined();
          received++;
        } else if (!retryFailed) {
          expect(retry_of).toBe(failedRun.id);
          retryFailed = true;
          await route.fulfill({
            status: 409,
            json: { error: 'Open the linked Anki profile' },
          });
          return;
        } else if (!retried) {
          expect(retry_of).toBe(failedRun.id);
          retried = true;
        } else {
          expect(retry_of).toBe('retry-failure');
          lastResolved = true;
        }
      } else paused = action === 'pause';
      await route.fulfill({ status: 204 });
    },
  );
  await page.route(`**/api/decks/${deckA}/anki-auto-sync/history`, (route) =>
    route.fulfill({ json: [] }),
  );
  const trigger = study.getByRole('button', { name: 'Anki sync', exact: true });
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: 'Anki sync' });
  const openDecks = dialog.getByRole('checkbox', {
    name: 'Decks with open errors only',
  });
  const row = dialog.getByRole('row').filter({ hasText: 'Mochy studies' });
  await expect(row.getByRole('rowheader')).toContainText(
    'Anki profile Test profile',
  );
  await expect(row.getByRole('cell').nth(0)).toContainText(/11\/13\/26/);
  await expect(row.getByRole('cell').nth(1)).toHaveText('2');
  await expect(row.getByRole('cell').nth(2)).toHaveText('3');
  await expect(row.getByRole('cell').nth(3)).toHaveText(
    'Paused. Open the linked Anki profile.',
  );
  const toggle = row.getByRole('button', { name: 'History', exact: true });
  const count = (open: number) =>
    row.getByRole('button', {
      name: `Open errors for Mochy studies (${open})`,
    });
  const filter = dialog.getByRole('combobox', { name: 'Filter runs' });
  const zero = dialog.getByRole('button', {
    name: 'Open errors for Prime (0)',
  });
  await expect(zero).toHaveText('0');
  await zero.click();
  await expect(filter).toHaveValue('open');
  await expect(
    dialog.getByText('No open errors.', { exact: true }),
  ).toBeVisible();
  await expect(openDecks).not.toBeChecked();
  await openDecks.check();
  await expect(zero).toHaveCount(0);
  await expect(count(1)).toHaveText('1');
  await count(1).click();
  await expect(filter).toHaveValue('open');
  const history = dialog.getByRole('table', {
    name: 'History · Mochy studies',
  });
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(history.getByText(failedRun.error_detail)).toBeVisible();
  await filter.selectOption({ label: 'All runs' });
  await row.getByRole('button', { name: 'Pause', exact: true }).click();
  await expect(row.getByRole('cell').nth(3)).toHaveText('Paused manually');
  await row.getByRole('button', { name: 'Sync now' }).click();
  await expect(row.getByRole('cell').nth(1)).toHaveText('3');
  await expect(row.getByRole('cell').nth(3)).toHaveText('Paused manually');
  const newRun = history.getByRole('row').filter({ hasText: 'Successful' });
  await expect(newRun.locator('time')).toHaveAttribute(
    'datetime',
    '2026-11-13T12:02:00Z',
  );
  await expect(newRun.getByRole('cell').nth(0)).toHaveText('1');
  await expect(newRun.getByRole('cell').nth(1)).toHaveText('0');
  await count(1).click();
  await expect(filter).toHaveValue('open');
  await expect(newRun).toHaveCount(0);
  const failedRow = history
    .getByRole('row')
    .filter({ hasText: failedRun.error_detail });
  const retry = failedRow.getByRole('button', { name: 'Retry sync' });
  await retry.click();
  await expect(dialog.getByRole('alert')).toContainText(
    'Open the linked Anki profile',
  );
  await expect(count(2)).toHaveText('2');
  await expect(
    history.getByText('Open the linked Anki profile', { exact: true }),
  ).toBeVisible();
  await retry.click();
  await expect(
    dialog.getByRole('status').filter({ hasText: 'Retry completed.' }),
  ).toBeVisible();
  await expect(filter).toHaveValue('open');
  await expect(newRun).toHaveCount(0);
  await expect(failedRow).toHaveCount(0);
  await expect(count(1)).toHaveText('1');
  await expect(
    history.getByText('Open the linked Anki profile', { exact: true }),
  ).toBeVisible();
  await expect(row.getByRole('cell').nth(1)).toHaveText('3');
  await expect(row.getByRole('cell').nth(3)).toHaveText('Paused manually');
  await filter.selectOption({ label: 'Failed runs only' });
  await expect(failedRow).toContainText('Resolved');
  await expect(failedRow.locator('time').last()).toHaveAttribute(
    'datetime',
    '2026-11-13T12:04:00Z',
  );
  await expect(retry).toHaveCount(0);
  await filter.selectOption({ label: 'All runs' });
  const retryRun = history
    .getByRole('row')
    .filter({ has: page.locator('time[datetime="2026-11-13T12:04:00Z"]') });
  await expect(retryRun.getByRole('cell').nth(0)).toHaveText('0');
  await expect(retryRun.getByRole('cell').nth(1)).toHaveText('0');
  await expect(
    retryRun.getByRole('button', { name: 'Retry sync' }),
  ).toHaveCount(0);
  for (const viewport of [
    { width: 1800, height: 1169 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ]) {
    await page.setViewportSize(viewport);
    await dialog.screenshot({
      path: test.info().outputPath(`anki-overview-${viewport.width}.png`),
    });
  }
  await history
    .getByRole('row')
    .filter({ hasText: 'Open the linked Anki profile' })
    .getByRole('button', { name: 'Retry sync' })
    .click();
  await expect(row).toHaveCount(0);
  await expect(history).toHaveCount(0);
  await expect(dialog.getByText('No decks have open errors.')).toBeVisible();
  await expect(openDecks).toBeChecked();
  await openDecks.uncheck();
  await expect(zero).toBeVisible();
  await expect(count(0)).toBeVisible();
  await row.getByRole('button', { name: 'Resume', exact: true }).click();
  await expect(row.getByRole('cell').nth(3)).toHaveText(
    'Paused. Open the linked Anki profile.',
  );
  expect(actions).toEqual([
    'pause',
    'sync_now',
    'sync_now',
    'sync_now',
    'sync_now',
    'resume',
  ]);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

test('Retry all open errors runs each affected deck once and continues after a failure', async ({
  page,
}) => {
  const { study } = await setup(page);
  const attempts: string[] = [];
  const affected = [
    { deck_id: deckA, deck_name: 'Prime', open_errors: 2 },
    { deck_id: deckB, deck_name: 'Mochy studies', open_errors: 3 },
    { deck_id: 'clean', deck_name: 'Clean', open_errors: 0 },
  ];
  await page.route('**/api/study/anki-auto-sync', (route) =>
    route.fulfill({
      json: affected.map((deck) => ({
        profile: 'Test profile',
        last_synced_at: null,
        received_reviews: 0,
        sent_reviews: 0,
        paused: true,
        status: 'paused',
        ...deck,
      })),
    }),
  );
  await page.route('**/api/decks/*/anki-auto-sync/action', async (route) => {
    const id = new URL(route.request().url()).pathname.split('/')[3];
    expect(route.request().postDataJSON()).toEqual({
      action: 'retry_open_errors',
      include_result: true,
    });
    attempts.push(id);
    if (id === deckA) {
      await route.fulfill({
        json: {
          outcome: 'failed',
          sent_reviews: 0,
          received_reviews: 1,
          open_errors: 2,
          error_detail: 'Anki did not confirm the transfer',
        },
      });
      return;
    }
    expect(attempts).toEqual([deckA, deckB]);
    affected[1].open_errors = 0;
    await route.fulfill({
      json: {
        outcome: 'synced',
        sent_reviews: 3,
        received_reviews: 4,
        open_errors: 0,
      },
    });
  });
  await study.getByRole('button', { name: 'Anki sync', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Anki sync' });
  await dialog
    .getByRole('checkbox', { name: 'Decks with open errors only' })
    .check();
  const bulk = dialog.getByRole('button', { name: 'Retry all open errors' });
  await bulk.click();
  await expect(dialog.getByRole('status')).toContainText(
    '1 successful, 1 failed.',
  );
  const results = dialog.getByRole('table', { name: 'Retry results by deck' });
  const result = (deck: string) =>
    results.getByRole('row').filter({ hasText: deck });
  await expect(result('Prime').getByRole('cell')).toHaveText([
    /Failed.*Anki did not confirm the transfer/s,
    '0',
    '1',
    '2',
  ]);
  await expect(result('Mochy studies').getByRole('cell')).toHaveText([
    'Successful',
    '3',
    '4',
    '0',
  ]);
  await expect(result('Clean')).toHaveCount(0);
  const overview = dialog.getByRole('table', {
    name: 'Reviews transferred through this link',
  });
  await expect(
    overview.getByRole('rowheader', { name: /Mochy studies|Clean/ }),
  ).toHaveCount(0);
  await expect(
    overview.getByRole('rowheader', { name: /Prime/ }),
  ).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Open errors for Prime (2)' }),
  ).toBeVisible();
  expect(attempts).toEqual([deckA, deckB]);
  for (const viewport of [
    { width: 1800, height: 1169 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ]) {
    await page.setViewportSize(viewport);
    await dialog.screenshot({
      path: test.info().outputPath(`anki-retry-results-${viewport.width}.png`),
    });
  }
});

test('Automatic Anki sync is enabled and disabled for the selected deck', async ({
  page,
}) => {
  const { study } = await setup(page);
  let linked = false;
  await page.route(`**/api/decks/${deckB}/anki-auto-sync`, async (route) => {
    if (route.request().method() === 'PUT') {
      linked = route.request().postDataJSON().enabled;
    }
    await route.fulfill({
      json: {
        profile: linked ? 'Test profile' : null,
        last_synced_at: null,
        last_received_at: null,
        last_outcome: null,
      },
    });
  });
  const toggle = study.getByRole('checkbox', {
    name: 'Sync reviews automatically',
  });
  await expect(toggle).toHaveCount(0);
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckB);
  await expect(toggle).toBeEnabled();
  await expect(toggle).not.toBeChecked();
  await toggle.click();
  await expect(toggle).toBeChecked();
  await expect(study.getByText(/Anki profile Test profile/)).toBeVisible();
  await toggle.click();
  await expect(toggle).not.toBeChecked();
  await expect(study.getByText(/Anki profile Test profile/)).toHaveCount(0);
  expect(linked).toBe(false);
});

test('Move stays visible when the search has no source or the source has no destination', async ({
  page,
}) => {
  const { study } = await setup(page, true);
  const move = study.getByRole('button', { name: /^Move cards/ });
  await expect(move).toBeVisible();
  await expect(move).toBeDisabled();
  await expect(
    study.getByText('Moving cards requires two decks.'),
  ).toBeVisible();
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckB);
  await study.getByRole('checkbox', { name: 'Select card 1' }).check();
  await expect(move).toBeVisible();
  await expect(move).toBeDisabled();
});

test('flashcard search uses the position search layout and page controls', async ({
  page,
}) => {
  const { study, queries, writes } = await setup(page);
  const form = study.locator('form.position-filter-form');
  const chooser = form.getByLabel('Choose deck', { exact: true });
  const preview = study.locator('.learning-card-preview');
  const results = study.getByRole('table', {
    name: 'Search results',
    exact: true,
  });
  const pagination = study.getByRole('navigation', {
    name: 'Search results',
    exact: true,
  });
  await expect(form.getByLabel('Search text', { exact: true })).toBeVisible();
  await expect(
    form.getByLabel('Player on roll', { exact: true }),
  ).toBeVisible();
  await expect(chooser).toHaveValue('__all__');
  await expect(
    form
      .locator('.position-filter-footer')
      .getByRole('button', { name: 'Clear filters' }),
  ).toBeVisible();
  await expect(
    pagination.getByRole('button', { name: 'Previous page' }),
  ).toBeDisabled();
  await expect(results.locator('tbody tr')).toHaveCount(50);
  await pagination.getByRole('button', { name: 'Page 2' }).click();
  await expect(
    pagination.getByRole('button', { name: 'Page 2' }),
  ).toHaveAttribute('aria-current', 'page');
  await expect(results.locator('tbody tr')).toHaveCount(3);
  await expect(preview).toHaveAttribute('data-card-id', 'card-50');
  await pagination.getByRole('button', { name: 'Previous page' }).click();
  await expect(results.locator('tbody tr')).toHaveCount(50);
  await pagination.getByRole('button', { name: 'Next page' }).click();
  await expect(results.locator('tbody tr')).toHaveCount(3);
  expect(
    queries.filter((query) => query.searchParams.get('page') === '2'),
  ).toHaveLength(2);
  await chooser.selectOption(deckB);
  await expect(pagination).toHaveCount(0);
  await expect(results.locator('tbody tr')).toHaveCount(1);
  await expect(preview).toHaveAttribute('data-card-id', 'card-52');
  await page.setViewportSize({ width: 390, height: 844 });
  expect((await preview.boundingBox())!.y).toBeLessThan(
    (await results.boundingBox())!.y,
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  expect(writes).toEqual([]);
});

test('filters preserve scope and a delayed old search cannot replace newer results', async ({
  page,
}) => {
  const { study, queries, writes, setSlow } = await setup(page);
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckA);
  await study.getByText('More filters', { exact: true }).click();
  const tag = study.getByLabel('Exact tag', { exact: true });
  await tag.fill('prime');
  await study
    .getByLabel('Review status', { exact: true })
    .selectOption('reviewed');
  await tag.press('Enter');
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(1);
  expect(queries.at(-1)!.pathname).toContain(deckA);
  expect(queries.at(-1)!.searchParams.get('tag')).toBe('prime');
  expect(queries.at(-1)!.searchParams.get('status')).toBe('reviewed');
  await study
    .getByRole('button', { name: 'Clear filters', exact: true })
    .click();
  await expect(study.getByLabel('Choose deck', { exact: true })).toHaveValue(
    deckA,
  );
  let release = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  setSlow(() => held);
  await study.getByLabel('Search text', { exact: true }).fill('slow');
  await study
    .getByRole('button', { name: 'Search cards', exact: true })
    .click();
  await expect.poll(() => queries.at(-1)!.searchParams.get('q')).toBe('slow');
  await study.getByLabel('Search text', { exact: true }).fill('Mochy');
  await study
    .getByRole('button', { name: 'Search cards', exact: true })
    .click();
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(1);
  release();
  await page.waitForLoadState('networkidle');
  await expect(study.locator('.learning-card-preview')).toHaveAttribute(
    'data-card-id',
    'card-52',
  );
  expect(writes).toEqual([]);
});

test('starts active practice with the applied filters', async ({ page }) => {
  const { study } = await setup(page);
  await study.getByLabel('Search text', { exact: true }).fill('Mochy');
  await study
    .getByRole('button', { name: 'Search cards', exact: true })
    .click();
  await expect(study.locator('.learning-card-list tbody tr')).toHaveCount(1);
  await study
    .getByLabel('Search text', { exact: true })
    .fill('Unapplied draft');
  const posted = page.waitForRequest(
    (request) =>
      request.method() === 'POST' &&
      new URL(request.url()).pathname === '/api/study/card-rounds',
  );
  await study
    .getByRole('button', { name: 'Practice results', exact: true })
    .click();
  const query = new URL((await posted).url()).searchParams;
  expect(query.get('q')).toBe('Mochy');
  expect(query.get('limit')).toBe('10');
  expect(query.has('after')).toBe(false);
});

test('the card preview can be resized and keeps its width after reload', async ({
  page,
}) => {
  const { study } = await setup(page);
  const preview = study.locator('.learning-card-preview');
  const handle = study.getByRole('separator', { name: 'Resize card preview' });
  await expect(preview.locator('.board')).toBeVisible();
  await expect(handle).toBeVisible({ timeout: 4000 });
  const originalWidth = (await preview.boundingBox())!.width;
  await handle.scrollIntoViewIfNeeded();
  const { x, y, width, height } = (await handle.boundingBox())!;
  await page.mouse.move(x + width / 2, y + Math.min(height / 2, 30));
  await page.mouse.down();
  await page.mouse.move(x - 240 + width / 2, y + Math.min(height / 2, 30), {
    steps: 8,
  });
  await page.mouse.up();
  await expect
    .poll(async () => (await preview.boundingBox())!.width)
    .toBe(originalWidth + 240);
  await handle.press('ArrowRight');
  const adjustedWidth = (await preview.boundingBox())!.width;
  expect(adjustedWidth).toBeLessThan(originalWidth + 240);

  await page.reload();
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  await expect
    .poll(async () => (await preview.boundingBox())!.width)
    .toBe(adjustedWidth);

  await handle.press('End');
  await expect(handle).toHaveAttribute(
    'aria-valuenow',
    (await handle.getAttribute('aria-valuemax'))!,
  );
  await expect(study.locator('.learning-card-list')).toBeVisible();
  await page.setViewportSize({ width: 1100, height: 1000 });
  await expect(handle).toBeHidden();
  await expect(preview.locator('.board')).toBeVisible();
});
