import {
  DESKTOP_VIEWPORTS,
  gotoWorkspace,
  seedLocale,
} from '../helpers/navigation';
import { expect, test, type Page } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { defaultConfiguration } from '../../src/settings/configuration/configuration-model';
import { ALL_CARDS } from '../../src/workspaces/study/flashcards/card-search-api';
import type { MistakeGroup } from '../../src/workspaces/study/mistakes/recurring-mistakes-api';

const players = ['Alice', 'Bob'];
const cube: MistakeGroup = {
  game_plan: 'priming',
  mistake_kind: 'missed_double',
  count: 4,
  match_count: 3,
  total_loss: 0.6,
  rates: {
    decision_count: 40,
    error_rate: 0.1,
    loss_per_100: 1.5,
    small_sample: false,
    trend: null,
  },
};
const checker: MistakeGroup = {
  game_plan: 'racing',
  mistake_kind: 'checker',
  count: 3,
  match_count: 2,
  total_loss: 0.8,
  rates: {
    decision_count: 80,
    error_rate: 0.0375,
    loss_per_100: 1,
    small_sample: false,
    trend: null,
  },
};
const small: MistakeGroup = {
  game_plan: 'anchoring',
  mistake_kind: 'wrong_pass',
  count: 2,
  match_count: 2,
  total_loss: 1,
  rates: {
    decision_count: 4,
    error_rate: 0.5,
    loss_per_100: 25,
    small_sample: true,
    trend: null,
  },
};
const empty = {
  card_count: 0,
  due_count: 0,
  next_due_at: null,
  next_deck: null,
};
const list = (groups: MistakeGroup[]) => ({
  players,
  groups,
  analyzed_matches: 3,
  analyzed_decisions: 124,
});

async function openOverview(page: Page) {
  await seedLocale(page);
  await gotoWorkspace(page, 'study');
  return page.getByRole('tabpanel', { name: 'Overview', exact: true });
}

test('the recommendation starts the highest-ranked mistake group and returns focus on Escape', async ({
  page,
}) => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.general.player_name = '  aLiCe  ';
  const controller = await profileBackend(page, { configuration });
  controller.profiles[1].configuration.general.player_name = 'Bob';
  await page.clock.install();
  const nextDue = new Date(Date.now() + 60_000).toISOString();
  let dueNow = false;
  await page.route('**/api/study/learning-summary', (route) =>
    route.fulfill({
      json: {
        card_count: 1,
        due_count: Number(dueNow),
        next_due_at: dueNow ? null : nextDue,
        next_deck: dueNow ? { id: 'deck', name: 'Scheduled review' } : null,
      },
    }),
  );
  await page.route(/\/api\/study\/mistakes\?/, (route) => {
    const player = new URL(route.request().url()).searchParams.get('player');
    return route.fulfill({
      json: list(player === 'Alice' ? [checker, small, cube] : []),
    });
  });

  const reference = { job_id: 'job', game_no: 1, decision_no: 1 };
  await page.route('**/api/study/mistakes/practice', (route) => {
    expect(route.request().postDataJSON()).toEqual({
      player: 'Alice',
      game_plan: 'priming',
      mistake_kind: 'missed_double',
    });
    return route.fulfill({
      json: {
        items: [
          {
            reference,
            kind: 'cube_offer',
            players,
            engine: 'GNUbg',
            plies: 3,
            position: {
              id: 'position',
              match_uuid: 'match',
              ordinal: 1,
              game_no: 1,
              ply: 1,
              player: 0,
              dice: [0, 0],
              board: [
                0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5,
                0, 0, 0, 0, 2, 0,
              ],
              position_id: '4HPwATDgc/ABMA',
              gnubg_match_id: 'cIkAAAAAAAAA',
              notation: '',
            },
          },
        ],
      },
    });
  });
  await page.route('**/api/study/mistakes/attempt', (route) => {
    expect(route.request().postDataJSON()).toEqual({
      reference,
      answer: { kind: 'cube_offer', action: 'double' },
    });
    return route.fulfill({
      json: {
        original: {
          best: 'double',
          played: 'no double',
          equity_loss: 0.125,
          engine: 'GNUbg',
          plies: 3,
        },
        evaluation: {
          verdict: 'best',
          chosen: 'double',
          best: 'double',
          chosen_equity: 0.125,
          best_equity: 0.125,
          equity_loss: 0,
          provenance: { kind: 'recomputed', engine: 'GNUbg', plies: 3 },
        },
      },
    });
  });
  const overview = await openOverview(page);
  await expect(overview.getByRole('combobox', { name: 'Player' })).toHaveValue(
    'Alice',
  );
  const next = overview.getByRole('region', { name: 'Next step' });
  await expect(next).toContainText('Matches with this mistake: 3');
  await expect(next).toContainText(
    '1.5 equity lost per 100 decisions (40 evaluated)',
  );
  await expect(overview.getByRole('table').getByRole('row')).toHaveCount(4);
  const smallSampleRow = overview
    .getByRole('row')
    .filter({ hasText: 'Small sample' });
  await expect(
    smallSampleRow.getByRole('cell', { name: '50.0%', exact: true }),
  ).toBeVisible();
  await expect(
    smallSampleRow.getByRole('button', { name: 'Practice' }),
  ).toBeEnabled();

  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    await expect(next).toBeInViewport();
    await expect(overview.getByRole('table')).toBeInViewport();
    const due = await overview
      .getByRole('region', { name: 'Due flashcards' })
      .boundingBox();
    const mistakes = await overview
      .getByRole('region', { name: 'My recurring mistakes' })
      .boundingBox();
    expect(due!.x + due!.width).toBeCloseTo(mistakes!.x + mistakes!.width);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    await page.screenshot({
      path: test.info().outputPath(`study-overview-${width}x${height}.png`),
    });
  }

  const trigger = next.getByRole('button', { name: 'Practice mistake group' });
  await trigger.click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
  });
  await dialog.getByRole('button', { name: 'Double', exact: true }).click();
  await dialog
    .getByRole('button', { name: 'Check answer', exact: true })
    .click();
  await expect(dialog.locator('.recurring-attempt-result')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(trigger).toBeFocused();
  const rowTrigger = overview
    .getByRole('row')
    .nth(1)
    .getByRole('button', { name: 'Practice', exact: true });
  await rowTrigger.click();
  await expect(
    dialog.getByRole('button', { name: 'Double', exact: true }),
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(rowTrigger).toBeFocused();

  await trigger.click();
  await expect(dialog).toBeVisible();
  dueNow = true;
  await page.clock.fastForward(61_000);
  await expect(trigger).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(next.getByRole('heading')).toBeFocused();

  await page.getByRole('button', { name: /^Profile:/ }).click();
  await page
    .getByRole('menuitemradio', { name: 'Deep analysis', exact: true })
    .click();
  await expect(overview.getByRole('combobox', { name: 'Player' })).toHaveValue(
    'Bob',
  );
  await expect(
    overview.getByText('No mistake group spans at least two matches.'),
  ).toBeVisible();

  // The full list opens for the player the overview shows.
  await overview
    .getByRole('button', { name: 'Open mistake overview', exact: true })
    .click();
  await expect(
    page
      .getByRole('tabpanel', { name: 'My recurring mistakes', exact: true })
      .getByRole('combobox', { name: 'Player' }),
  ).toHaveValue('Bob');
});

test('refreshes when the next card becomes due while the overview stays open', async ({
  page,
}) => {
  await profileBackend(page);
  await page.clock.install();
  const dueAt = new Date(Date.now() + 30_000).toISOString();
  let due = false;
  await page.route('**/api/study/learning-summary', (route) =>
    route.fulfill({
      json: {
        card_count: 1,
        due_count: Number(due),
        next_due_at: due ? null : dueAt,
        next_deck: due ? { id: 'deck', name: 'Scheduled review' } : null,
      },
    }),
  );
  await page.route(/\/api\/study\/mistakes\?/, (route) =>
    route.fulfill({ json: { ...list([]), players: [] } }),
  );
  const overview = await openOverview(page);
  const cards = overview.getByRole('region', { name: 'Due flashcards' });
  await expect(cards.locator('.study-overview-due')).toHaveText('0');
  await expect(cards.getByText(/^Next card due/)).toBeVisible();
  due = true;
  await page.clock.fastForward(31_000);
  await expect(cards.locator('.study-overview-due')).toHaveText('1');
  await expect(
    overview.getByRole('button', { name: 'Study due cards' }),
  ).toBeVisible();
});

test('retries due counts independently and handles a removed recommended deck', async ({
  page,
}) => {
  const configuration = structuredClone(defaultConfiguration);
  const existing = '00000000-0000-4000-8000-000000000011';
  configuration.learning.deck_id = existing;
  await profileBackend(page, { configuration });
  let available = false;
  await page.route('**/api/study/learning-summary', (route) =>
    available
      ? route.fulfill({
          json: {
            card_count: 2,
            due_count: 1,
            next_due_at: null,
            next_deck: { id: 'removed', name: 'Removed' },
          },
        })
      : route.fulfill({ status: 503, json: { error: 'Counts unavailable' } }),
  );
  await page.route(/\/api\/study\/mistakes\?/, (route) =>
    route.fulfill({ json: list([cube]) }),
  );
  await page.route('**/api/decks', (route) =>
    route.fulfill({
      json: [
        {
          id: existing,
          name: 'Existing',
          card_count: 2,
          due_count: 1,
          version: 0,
        },
      ],
    }),
  );
  let releaseNext = () => {};
  const pendingNext = new Promise<void>((resolve) => {
    releaseNext = resolve;
  });
  const nextUrl = `**/api/decks/${existing}/next`;
  await page.route(nextUrl, async (route) => {
    await pendingNext;
    await route
      .fulfill({ json: { card: null, counts: { total: 2, due: 0, new: 0 } } })
      .catch(() => {});
  });
  const overview = await openOverview(page);
  const due = overview.getByRole('region', { name: 'Due flashcards' });
  await expect(due.getByRole('alert')).toContainText('Counts unavailable');
  await overview
    .getByRole('combobox', { name: 'Player' })
    .selectOption('Alice');
  await expect(
    overview.getByRole('button', { name: 'Practice mistake group' }),
  ).toBeVisible();
  available = true;
  await due.getByRole('button', { name: 'Try again' }).click();
  await expect(due.locator('.study-overview-due')).toHaveText('1');
  const cards = page.getByRole('tab', {
    name: 'Flashcards & Anki',
    exact: true,
  });
  await cards.click();
  const nextRequest = page.waitForRequest(nextUrl);
  const studyNow = page.getByRole('button', { name: 'Study now', exact: true });
  await studyNow.click();
  await nextRequest;
  await expect(studyNow).toHaveCount(0);
  await cards.press('Home');
  await overview.getByRole('button', { name: 'Study due cards' }).click();
  await expect(cards).toHaveAttribute('aria-selected', 'true');
  await expect(
    page
      .getByRole('tabpanel', { name: 'Flashcards & Anki' })
      .getByRole('alert')
      .filter({ hasText: 'This deck is no longer available.' }),
  ).toBeVisible();
  await expect(studyNow).toBeEnabled();
  releaseNext();
  await cards.press('Home');
  await expect(overview).toBeVisible();
  await expect(overview.getByRole('combobox', { name: 'Player' })).toHaveValue(
    'Alice',
  );
  const overviewTab = page.getByRole('tab', { name: 'Overview', exact: true });
  await expect(overviewTab).toBeFocused();
  await overviewTab.press('End');
  await expect(
    page.getByRole('tab', { name: 'Practice history', exact: true }),
  ).toBeFocused();
});

test('refreshes the deck list when the recommended deck disappears before the question loads', async ({
  page,
}) => {
  await profileBackend(page);
  const deck = {
    id: '00000000-0000-4000-8000-000000000012',
    name: 'Removed',
    card_count: 1,
    due_count: 1,
    version: 0,
  };
  let removed = false;
  await page.route('**/api/study/learning-summary', (route) =>
    route.fulfill({
      json: {
        ...empty,
        card_count: 1,
        due_count: 1,
        next_deck: { id: deck.id, name: deck.name },
      },
    }),
  );
  await page.route('**/api/decks', (route) =>
    route.fulfill({ json: removed ? [] : [deck] }),
  );
  await page.route(`**/api/decks/${deck.id}/next`, (route) => {
    removed = true;
    return route.fulfill({ status: 404, json: { error: 'Not found' } });
  });
  const overview = await openOverview(page);
  await overview.getByRole('button', { name: 'Study due cards' }).click();
  const cards = page.getByRole('tabpanel', { name: 'Flashcards & Anki' });
  await expect(
    cards.getByText('This deck is no longer available.'),
  ).toBeVisible();
  await expect(
    cards.locator('select option', { hasText: 'Removed' }),
  ).toHaveCount(0);
  await expect(
    cards.getByRole('combobox', { name: 'Choose deck' }),
  ).toHaveValue(ALL_CARDS);
});

test('ignores a superseded player response even after reselecting that player', async ({
  page,
}) => {
  await profileBackend(page);
  let release = () => {};
  const oldAlice = new Promise<void>((resolve) => {
    release = resolve;
  });
  let aliceRequests = 0;
  await page.route('**/api/study/learning-summary', (route) =>
    route.fulfill({ json: empty }),
  );
  await page.route(/\/api\/study\/mistakes\?/, async (route) => {
    const player = new URL(route.request().url()).searchParams.get('player');
    if (player === 'Alice' && ++aliceRequests === 1) {
      await oldAlice;
      await route.fulfill({ json: list([checker]) }).catch(() => {});
    } else
      await route.fulfill({ json: list(player === 'Alice' ? [cube] : []) });
  });
  const overview = await openOverview(page);
  const player = overview.getByRole('combobox', { name: 'Player' });
  await overview.getByRole('button', { name: 'Choose a player' }).click();
  await expect(player).toBeFocused();
  await player.selectOption('Alice');
  await expect.poll(() => aliceRequests).toBe(1);
  await player.selectOption('Bob');
  await expect(
    overview.getByText('No mistake group spans at least two matches.'),
  ).toBeVisible();
  await player.selectOption('Alice');
  await expect(
    overview.getByRole('region', { name: 'Next step' }),
  ).toContainText('Matches with this mistake: 3');
  release();
  await page.unrouteAll({ behavior: 'wait' });
  await expect(
    overview.getByRole('region', { name: 'Next step' }),
  ).toContainText('Matches with this mistake: 3');
});

test('shows today as due reviews plus allowed new cards and saves the daily limit', async ({
  page,
}) => {
  await profileBackend(page);
  let limit = 10;
  await page.route('**/api/study/learning-summary', (route) => {
    const fresh = Math.min(1200, limit);
    return route.fulfill({
      json: {
        card_count: 1222,
        due_count: 1222,
        review_due_count: 22,
        new_count: 1200,
        new_cards_per_day: limit,
        new_available_today: fresh,
        today_count: 22 + fresh,
        next_due_at: null,
        next_deck: { id: 'deck', name: 'Training' },
      },
    });
  });
  const saved: unknown[] = [];
  await page.route('**/api/study/learning-preferences', (route) => {
    const body = route.request().postDataJSON();
    saved.push(body);
    limit = body.new_cards_per_day;
    return route.fulfill({ json: body });
  });
  await page.route(/\/api\/study\/mistakes\?/, (route) =>
    route.fulfill({ json: { ...list([]), players: [] } }),
  );
  const overview = await openOverview(page);
  const cards = overview.getByRole('region', { name: 'Due flashcards' });
  // The headline is today's workload, not every card that was ever created.
  await expect(cards.locator('.study-overview-due')).toHaveText('32');
  await expect(cards.locator('[data-study-today]')).toHaveText(
    'Today: 22 due · 10 new',
  );
  await expect(cards).toContainText(
    '1,190 more new cards are waiting for later days.',
  );
  const field = cards.getByRole('spinbutton', { name: 'New cards per day' });
  await expect(field).toHaveValue('10');
  await field.fill('2000');
  await field.press('Enter');
  expect(saved).toEqual([]);
  await field.fill('25');
  await field.press('Enter');
  await expect(cards.locator('.study-overview-due')).toHaveText('47');
  expect(saved).toEqual([{ new_cards_per_day: 25 }]);
  await expect(field).toHaveValue('25');
});
