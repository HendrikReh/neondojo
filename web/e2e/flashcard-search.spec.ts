import { expect, test, type Page } from './test';

test.use({ viewport: { width: 1800, height: 1169 }, actionTimeout: 10000 });
const deckA = '11111111-1111-4111-8111-111111111111';
const deckB = '22222222-2222-4222-8222-222222222222';
async function setup(page: Page) {
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
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.deck', '__all__');
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
      ],
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
    const after = p.get('after');
    if (after)
      result = result.slice(result.findIndex((c) => c.id === after) + 1);
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
  page.on('request', (req) => {
    if (
      !['GET', 'HEAD'].includes(req.method()) &&
      /\/api\/(cards|decks)/.test(req.url())
    )
      writes.push(req.url());
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study & Anki', exact: true }).click();
  return {
    study: page.getByRole('region', { name: 'Study & Anki', exact: true }),
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
  expect(
    (await heading.boundingBox())!.y + (await heading.boundingBox())!.height,
  ).toBeLessThan(
    (await study.locator('.learning-deck-panel').boundingBox())!.y,
  );
  await expect(
    study.getByRole('button', { name: 'Study now', exact: true }),
  ).toBeDisabled();
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

test('filters preserve scope and a delayed old search cannot replace newer results', async ({
  page,
}) => {
  const { study, queries, writes, setSlow } = await setup(page);
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deckA);
  await study.getByText('More filters', { exact: true }).click();
  await study.getByLabel('Exact tag', { exact: true }).fill('prime');
  await study
    .getByLabel('Review status', { exact: true })
    .selectOption('reviewed');
  await study
    .getByRole('button', { name: 'Search cards', exact: true })
    .click();
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
