import type { APIResponse, Page } from '@playwright/test';
import type { components } from '../src/generated-api';
import { expect, test } from './test';

type Schema = components['schemas'];

test.use({ viewport: { width: 1800, height: 1169 } });

async function json<T>(response: APIResponse): Promise<T> {
  const body = await response.text();
  expect(response.ok(), body).toBe(true);
  return JSON.parse(body);
}

async function createDeck(page: Page, checkerOnlyAtScore?: number) {
  const suffix = crypto.randomUUID();
  const metadata = `MI[length:23][ws:${checkerOnlyAtScore ?? 0}][bs:0]`;
  const checker = `(;GM[6]PW[Study ${suffix}]PB[Opponent]${metadata};W[31hefe])`;
  const cube = `(;GM[6]${metadata}AE[a:y]AW[f]AB[s]PL[W]CV[2]CO[w];W[double];B[drop])`;
  const imported = await json<{ match: { id: string } }>(
    await page.request.post('/api/imports', {
      multipart: {
        file: {
          name: `active-study-${suffix}.sgf`,
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            checkerOnlyAtScore === undefined ? checker + cube : checker,
          ),
        },
      },
    }),
  );
  const job = await json<Schema['Job']>(
    await page.request.post(`/api/matches/${imported.match.id}/analysis`, {
      data: { plies: 0, engine_id: 'gnubg' },
    }),
  );
  await expect
    .poll(
      async () =>
        (
          await json<Schema['Job']>(
            await page.request.get(`/api/jobs/${job.id}`),
          )
        ).status,
      { timeout: 30_000 },
    )
    .toBe('completed');
  const deck = await json<Schema['Deck']>(
    await page.request.post('/api/decks', {
      data: { name: `Active answers ${suffix}` },
    }),
  );
  const added = await json<Schema['AddCardsResult']>(
    await page.request.post(`/api/decks/${deck.id}/cards`, {
      data: { job_id: job.id, min_loss: 0 },
    }),
  );
  expect(added.total).toBe(checkerOnlyAtScore === undefined ? 3 : 1);
  return deck;
}

async function openDeck(page: Page, deck: Schema['Deck']) {
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  const study = page.getByRole('region', { name: 'Study', exact: true });
  await study.getByLabel('Choose deck', { exact: true }).selectOption(deck.id);
  await study.getByRole('button', { name: 'Study now', exact: true }).click();
  return study;
}

test('actively answers and reviews all three card types, with an idempotent retry and reload', async ({
  page,
}) => {
  test.setTimeout(90_000);
  const deck = await createDeck(page);
  const nextUrl = `/api/decks/${deck.id}/next`;
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  const study = await openDeck(page, deck);
  const seen = new Set<string>();

  for (let i = 0; i < 3; i++) {
    const { card: question } = await json<Schema['NextCard']>(
      await page.request.get(nextUrl),
    );
    expect(question).toBeTruthy();
    if (!question) throw new Error('Expected a due card');
    const card = study.locator(`.study-card[data-card-id="${question.id}"]`);
    const result = card.locator('.study-attempt-result');
    const solution = card.locator('.study-answer');
    const submit = card.getByRole('button', {
      name: question.kind === 'checker' ? 'Check move' : 'Check answer',
      exact: true,
    });
    await expect(card).toBeVisible();
    await expect(solution).toHaveCount(0);
    await expect(result).toHaveCount(0);
    await expect(submit).toBeDisabled();
    const attemptUrl = `**/api/cards/${question.id}/attempt`;
    const checkerSteps = [
      { from: 24, to: 21 },
      { from: 24, to: 23 },
    ];
    const expectedAnswer =
      question.kind === 'checker'
        ? { kind: question.kind, steps: checkerSteps }
        : {
            kind: question.kind,
            action: question.kind === 'cube_offer' ? 'double' : 'take',
          };

    if (question.kind === 'checker') {
      const point = card.getByRole('button', { name: 'Point 24', exact: true });
      await expect(card.locator('.move-arrows')).toHaveCount(0);
      await point.focus();
      await page.keyboard.press('Enter');
      await expect(submit).toBeDisabled();
      await point.click();
      await expect(submit).toBeEnabled();
      await card.getByRole('button', { name: /^Undo(?: step)?$/ }).click();
      await expect(submit).toBeDisabled();
      await point.click();
    } else {
      const responding = question.kind === 'cube_response';
      await expect(card.locator('[data-die-owner]')).toHaveCount(0);
      await expect(
        card.locator(
          responding
            ? '[data-cube="4"][data-cube-pending="true"][data-cube-owner="centered"]'
            : '[data-cube="2"][data-cube-pending="false"][data-cube-owner="0"]',
        ),
      ).toBeVisible();
      const choice = card.getByRole('button', {
        name: responding ? 'Take double' : 'Double',
        exact: true,
      });
      await choice.click();
      await expect(choice).toHaveAttribute('aria-pressed', 'true');
      await expect(solution).toHaveCount(0);
    }
    await expect(submit).toBeEnabled();

    if (question.kind === 'checker') {
      // Lose the first response after PostgreSQL has accepted the attempt.
      await page.route(
        attemptUrl,
        async (route) => {
          const persisted = await route.fetch();
          expect(persisted.ok(), await persisted.text()).toBe(true);
          await route.fulfill({
            status: 503,
            json: { error: 'Attempt response unavailable' },
          });
        },
        { times: 1 },
      );
    }
    const first = page.waitForResponse((response) =>
      response.url().endsWith(`/cards/${question.id}/attempt`),
    );
    await submit.click();
    const response = await first;
    const input = response.request().postDataJSON();
    expect(input).toMatchObject({
      expected_version: question.version,
      answer: expectedAnswer,
      request_id: expect.any(String),
    });
    let attempt: Schema['AttemptedCard'];
    if (question.kind === 'checker') {
      expect(response.status()).toBe(503);
      await expect(card.getByRole('alert')).toContainText(
        'Attempt response unavailable',
      );
      await expect(submit).toBeEnabled();
      await expect(solution).toHaveCount(0);
      const retry = page.waitForResponse((r) =>
        r.url().endsWith(`/cards/${question.id}/attempt`),
      );
      await submit.click();
      const retried = await retry;
      expect(retried.request().postDataJSON()).toEqual(input);
      expect(retried.ok(), await retried.text()).toBe(true);
      attempt = await retried.json();
      await expect(result).toBeVisible();

      await page.route('**/api/study/learning-summary', async (route) => {
        const { counts } = await json<Schema['NextCard']>(
          await page.request.get(nextUrl),
        );
        await route.fulfill({
          json: {
            card_count: counts.total,
            // Due reviews and the new cards still allowed today.
            due_count: counts.due + (counts.new_available ?? 0),
            next_due_at: null,
            next_deck:
              counts.due + (counts.new_available ?? 0)
                ? { id: deck.id, name: deck.name }
                : null,
          },
        });
      });
      await page.reload();
      await page.getByRole('tab', { name: 'Study', exact: true }).click();
      const due = page.getByRole('region', { name: 'Due flashcards' });
      await expect(due.locator('.study-overview-due')).toHaveText(
        String(3 - i),
      );
      await page.getByRole('button', { name: 'Study due cards' }).click();
      await expect(result).toBeVisible();
      const preview = card.getByRole('button', {
        name: 'Your move',
        exact: true,
      });
      await preview.click();
      await expect
        .poll(async () =>
          card
            .locator('.move-arrows > [data-from]')
            .evaluateAll((arrows) =>
              arrows.map((arrow) => [
                Number(arrow.getAttribute('data-from')),
                Number(arrow.getAttribute('data-to')),
              ]),
            ),
        )
        .toEqual(checkerSteps.map(({ from, to }) => [from, to]));
      for (const [width, height] of [
        [1800, 1169],
        [2560, 1440],
        [3840, 2160],
      ]) {
        await page.setViewportSize({ width, height });
        await expect(solution).toBeVisible();
        expect(
          await page.evaluate(() => document.documentElement.scrollWidth),
        ).toBeLessThanOrEqual(width);
        await page.screenshot({
          path: test.info().outputPath(`active-study-${width}x${height}.png`),
        });
      }
      await page.setViewportSize({ width: 1800, height: 1169 });
    } else {
      expect(response.ok(), await response.text()).toBe(true);
      attempt = await response.json();
    }
    expect(attempt.submitted).toEqual(expectedAnswer);
    expect(attempt.evaluation.equity_loss).toBeGreaterThanOrEqual(0);
    const resumed = await json<Schema['NextCard']>(
      await page.request.get(nextUrl),
    );
    expect(resumed.active_attempt?.revealed.session_id).toBe(
      attempt.revealed.session_id,
    );
    expect(resumed.active_attempt?.evaluation).toEqual(attempt.evaluation);
    expect(resumed.card?.review_count).toBe(0);
    await expect(result).toBeVisible();
    await expect(solution).toBeVisible();

    const reviewed = page.waitForResponse((r) =>
      r.url().endsWith(`/cards/${question.id}/review`),
    );
    await card.getByRole('button', { name: /^Good/ }).click();
    const review = await reviewed;
    expect(review.ok(), await review.text()).toBe(true);
    expect(review.request().postDataJSON().session_id).toBe(
      attempt.revealed.session_id,
    );
    expect((await review.json()).review_count).toBe(1);
    await expect(card).toHaveCount(0);
    seen.add(question.kind);
  }

  expect([...seen].sort()).toEqual(['checker', 'cube_offer', 'cube_response']);
  await expect(
    study.getByText('No cards are due in this deck right now.', {
      exact: true,
    }),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Overview', exact: true }).click();
  await expect(
    page
      .getByRole('region', { name: 'Due flashcards' })
      .locator('.study-overview-due'),
  ).toHaveText('0');
});

test('switching decks while a response is pending keeps the new question hidden', async ({
  page,
}) => {
  const first = await createDeck(page, 1);
  const second = await createDeck(page, 2);
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  const study = await openDeck(page, first);
  const card = study.locator('.study-card');
  await expect(card).toBeVisible();
  const oldId = await card.getAttribute('data-card-id');
  const oldAttemptUrl = `**/api/cards/${oldId}/attempt`;
  let release = () => {};
  const hold = new Promise<void>((resolve) => (release = resolve));
  let persisted: (value: { ok: boolean; body: string }) => void = () => {};
  const attemptSaved = new Promise<{ ok: boolean; body: string }>(
    (resolve) => (persisted = resolve),
  );
  let finished = () => {};
  const attemptFinished = new Promise<void>((resolve) => (finished = resolve));
  await page.route(oldAttemptUrl, async (route) => {
    try {
      const response = await route.fetch();
      persisted({ ok: response.ok(), body: await response.text() });
      await hold;
      await route.fulfill({ response }).catch(() => {});
    } finally {
      finished();
    }
  });
  try {
    const point = card.getByRole('button', { name: 'Point 24', exact: true });
    await point.click();
    await point.click();
    await card.getByRole('button', { name: 'Check move', exact: true }).click();
    const saved = await attemptSaved;
    expect(saved.ok, saved.body).toBe(true);
    await study
      .getByLabel('Choose deck', { exact: true })
      .selectOption(second.id);
    await study.getByRole('button', { name: 'Study now', exact: true }).click();
    await expect(card).toBeVisible();
    await expect(card).not.toHaveAttribute('data-card-id', oldId ?? '');
    release();
    await attemptFinished;
    await expect(card.locator('.study-answer')).toHaveCount(0);
    await expect(card.locator('.study-attempt-result')).toHaveCount(0);
    await expect(
      card.getByRole('button', { name: 'Check move', exact: true }),
    ).toBeDisabled();
  } finally {
    release();
    await page.unroute(oldAttemptUrl);
  }
});
