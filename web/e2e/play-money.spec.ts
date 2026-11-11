import { test, expect } from './test';
test.use({ actionTimeout: 10000 });
import { historyFixture } from './play-history-review-helpers';
test('New match offers a money game, persists it and sends a zero point target', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  let payload: unknown;
  const money = {
    ...saved,
    match_length: 0,
    phase: 'opening_roll',
    history: [],
    scores: [0, 0],
    opening: null,
  };
  await page.route('**/api/play/sessions', (r) => {
    payload = r.request().postDataJSON();
    return r.fulfill({ status: 201, json: money });
  });
  await page.getByRole('button', { name: 'New match', exact: true }).click();
  await page
    .getByRole('combobox', { name: 'Match length', exact: true })
    .selectOption({ label: 'Money game' });
  await expect
    .poll(
      async () =>
        (
          await page.evaluate(() =>
            fetch('/api/configuration').then((r) => r.json()),
          )
        ).current_configuration.play.match_length,
    )
    .toBe(0);
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page.getByRole('button', { name: 'New match', exact: true }).click();
  await expect(
    page.getByRole('combobox', { name: 'Match length', exact: true }),
  ).toHaveValue('0');
  await page
    .locator('.play-setup')
    .getByRole('button', { name: 'Start match', exact: true })
    .click();
  await expect.poll(() => payload).toMatchObject({ match_length: 0 });
  await expect(page.locator('.play-match-heading')).toContainText('Money game');
  await expect(page.locator('.play-match-heading')).not.toContainText(
    '0 points',
  );
});

test('money games continue with the score and keep each saved game report', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  let reviews = 0;
  Object.assign(saved, {
    match_length: 0,
    phase: 'game_over',
    scores: [0, 3],
    game_no: 1,
    winner: 1,
    review: null,
  });
  const reports: Record<string, unknown>[] = [];
  await page.route(`**/api/play/sessions/${saved.id}/review`, async (route) => {
    reviews++;
    const { detail } = await import('./study-report-fixture');
    const report = detail().review;
    report.games[0].game_no = saved.game_no;
    Object.assign(saved, {
      version: saved.version + 1,
      review: report,
      study_report_id: `66bb9950-416c-4bde-9241-bc2fd47dc05${saved.game_no}`,
      study_report_version: saved.version + 1,
      review_game_no: saved.game_no,
    });
    reports.push(structuredClone(saved));
    await route.fulfill({ json: saved });
  });
  await page.route(
    `**/api/play/sessions/${saved.id}/actions`,
    async (route) => {
      expect(route.request().postDataJSON().action).toBe('next_game');
      Object.assign(saved, {
        version: saved.version + 1,
        phase: 'opening_roll',
        game_no: saved.game_no + 1,
        review: null,
        study_report_id: null,
        study_report_version: null,
        winner: null,
      });
      await route.fulfill({ json: saved });
    },
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const next = page.getByRole('button', { name: 'Next game', exact: true });
  await expect(next).toBeEnabled();
  expect(reviews).toBe(1);
  const first = structuredClone(reports[0]);
  await page
    .getByRole('alertdialog', { name: 'Game finished', exact: true })
    .getByRole('button', { name: 'Learning review', exact: true })
    .click();
  const reportDialog = page.getByRole('dialog', {
    name: 'Learning review',
    exact: true,
  });
  await expect(reportDialog).toBeVisible();
  await page.keyboard.press('Escape');
  await next.click();
  await expect(page.locator('.play-match-heading')).toContainText('0 : 3');
  await expect(page.locator('.play-match-heading')).toContainText('Game 2');
  Object.assign(saved, {
    version: saved.version + 1,
    phase: 'game_over',
    scores: [0, 6],
    winner: 1,
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(next).toBeEnabled();
  expect(reviews).toBe(2);
  expect(reports[0]).toEqual(first);
  expect(reports[1].scores).toEqual([0, 6]);
  expect(reports[1].study_report_id).not.toBe(reports[0].study_report_id);
});

for (const [width, locale] of [
  [1440, 'en'],
  [390, 'en'],
  [390, 'de'],
] as const) {
  test(`money sessions can end beside Next game and stay finished after reload at ${width}px (${locale})`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1000 });
    const { saved } = await historyFixture(page);
    const german = locale === 'de';
    const text = {
      play: german ? 'Spielen' : 'Playing',
      result: german ? 'Spiel beendet' : 'Game finished',
      next: german ? 'Nächstes Spiel' : 'Next game',
      end: german ? 'Sitzung beenden' : 'End session',
      ended: german ? 'Sitzung beendet' : 'Session ended',
      back: german ? 'Zurück zum Brett' : 'Back to board',
      newMatch: german ? 'Neues Match' : 'New match',
      review: german ? 'Lernauswertung' : 'Learning review',
      reviewButton: german ? 'Spielauswertung' : 'Learning review',
    };
    if (german)
      await page.evaluate(async () => {
        const state = await (await fetch('/api/configuration')).json();
        const response = await fetch('/api/configuration', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            expected_revision: state.revision,
            configuration: {
              ...state.current_configuration,
              general: { ...state.current_configuration.general, locale: 'de' },
            },
          }),
        });
        if (!response.ok) throw new Error(await response.text());
      });
    const { detail } = await import('./study-report-fixture');
    Object.assign(saved, {
      match_length: 0,
      phase: 'game_over',
      scores: [3, 6],
      game_no: 3,
      winner: 1,
      review: detail().review,
    });
    const report = structuredClone(saved.review);
    const requests: unknown[] = [];
    let fail = true;
    await page.route(
      `**/api/play/sessions/${saved.id}/actions`,
      async (route) => {
        const body = route.request().postDataJSON();
        requests.push(body);
        expect(body).toEqual({ version: 7, action: 'end_session' });
        if (fail) {
          fail = false;
          return route.fulfill({
            status: 503,
            json: { error: 'Saved session is unchanged' },
          });
        }
        Object.assign(saved, {
          phase: 'match_over',
          money_session_ended: true,
          version: 8,
        });
        return route.fulfill({ json: saved });
      },
    );
    await page.reload();
    await page.getByRole('tab', { name: text.play, exact: true }).click();
    const result = page.getByRole('alertdialog', {
      name: text.result,
      exact: true,
    });
    const next = result.getByRole('button', { name: text.next, exact: true });
    const end = result.getByRole('button', {
      name: text.end,
      exact: true,
    });
    await expect(end).toBeEnabled();
    const nextBox = await next.boundingBox(),
      endBox = await end.boundingBox();
    expect(nextBox).toBeTruthy();
    expect(endBox).toBeTruthy();
    expect(endBox!.x).toBeGreaterThan(nextBox!.x);
    expect(Math.abs(endBox!.y - nextBox!.y)).toBeLessThan(2);
    expect(Math.abs(endBox!.height - nextBox!.height)).toBeLessThan(2);
    await end.click();
    await expect(result).toContainText('Saved session is unchanged');
    await expect(next).toBeEnabled();
    await end.click();
    await expect(
      page.getByRole('alertdialog', { name: text.ended, exact: true }),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: text.next, exact: true }),
    ).toHaveCount(0);
    await expect(page.locator('.play-match-heading')).toContainText('3 : 6');
    expect(saved.review).toEqual(report);
    await page.reload();
    await page.getByRole('tab', { name: text.play, exact: true }).click();
    const finished = page.getByRole('alertdialog', {
      name: text.ended,
      exact: true,
    });
    await expect(finished).toBeVisible();
    await expect(
      finished.getByRole('button', { name: text.newMatch, exact: true }),
    ).toHaveClass(/\bprimary\b/);
    const directReview = finished.getByRole('button', {
      name: text.reviewButton,
      exact: true,
    });
    const backToBoard = finished.getByRole('button', {
      name: text.back,
      exact: true,
    });
    if (german) {
      await page.clock.install();
      for (const button of [backToBoard, directReview]) {
        await button.focus();
        await expect(button.getByRole('tooltip')).toHaveCount(0);
        await button.blur();
        await button.hover();
        await page.clock.runFor(400);
        await expect(button.getByRole('tooltip')).toHaveCount(0);
      }
    }
    await directReview.click();
    const reportDialog = page.getByRole('dialog', {
      name: text.review,
      exact: true,
    });
    await expect(reportDialog).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(reportDialog).toHaveCount(0);
    await expect(directReview).toBeFocused();
    await backToBoard.click();
    await expect(finished).toHaveCount(0);
    await expect(
      page.getByRole('button', { name: text.end, exact: true }),
    ).toHaveCount(0);
    await page
      .locator('.play-history')
      .getByRole('button', { name: text.review, exact: true })
      .click();
    await expect(
      page.getByRole('dialog', { name: text.review, exact: true }),
    ).toBeVisible();
    await expect(page.locator('.play-match-heading')).toContainText('3 : 6');
    expect(requests).toHaveLength(2);
  });
}
