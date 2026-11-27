import {
  fulfillDecks,
  fulfillError,
  fulfillPlaySession,
  fulfillStudyReportPage,
} from '../helpers/api-fixtures';
import { seedLocale } from '../helpers/navigation';
import { expect, test, type Page } from '../helpers/test';
import { id, other, summary, detail } from '../helpers/study-report-fixture';
async function open(page: Page, locale = 'en') {
  await page.addInitScript(
    (locale) => localStorage.setItem('neondojo.locale', locale),
    locale,
  );
  await page.route('**/api/decks', (route) => fulfillDecks(route, []));
  await page.goto('/');
  await page
    .getByRole('tab', {
      name: locale === 'de' ? 'Lernen' : 'Study',
      exact: true,
    })
    .click({ timeout: 5000 });
  await page
    .getByRole('tab', {
      name: locale === 'de' ? 'Auswertungen' : 'Reports',
      exact: true,
    })
    .click();
}
test('Study opens persisted reports with original attempts and keeps flashcards accessible after reload', async ({
  page,
}) => {
  const writes: string[] = [];
  page.on('request', (r) => {
    const path = new URL(r.url()).pathname;
    // Personal configuration initialization/persistence is independent of
    // read-only report browsing. All domain mutations remain forbidden here.
    if (
      r.method() !== 'GET' &&
      path.startsWith('/api/') &&
      !/^\/api\/(?:configuration|profiles)(?:\/|$)/.test(path)
    )
      writes.push(r.url());
  });
  await page.route('**/api/study/reports?group_by=session', (r) =>
    fulfillStudyReportPage(r, { items: [summary()], next_cursor: null }),
  );
  await page.route(`**/api/study/reports/${id}`, (r) =>
    fulfillPlaySession(r, detail()),
  );
  await open(page);
  const archive = page.getByRole('region', { name: 'Saved reports' });
  await expect(archive).toContainText('Hendrik · GNUbg');
  await expect(archive).toContainText('4.5');
  await archive
    .getByRole('button', { name: 'Open report', exact: true })
    .click();
  const report = page.getByRole('region', { name: 'Learning review' });
  await expect(report).toContainText('13/10 6/5');
  await expect(report).toContainText('0.120');
  await report.locator('summary').click();
  await expect(report.locator('li')).toHaveCount(2);
  await page.keyboard.press('Escape');
  await page
    .getByRole('tab', { name: 'Flashcards & Anki', exact: true })
    .click();
  await expect(page.locator('.learning-workspace')).toBeVisible();
  await expect(archive).toBeHidden();
  await page.getByRole('tab', { name: 'Reports', exact: true }).click();
  await page.getByRole('button', { name: 'Open report', exact: true }).click();
  await expect(report).toBeVisible();
  await page.reload();
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page.getByRole('tab', { name: 'Reports', exact: true }).click();
  await page.getByRole('button', { name: 'Open report', exact: true }).click();
  await expect(report).toContainText('4.5');
  expect(writes).toEqual([]);
  await page.screenshot({
    path: test.info().outputPath('study-report.png'),
    fullPage: true,
  });
});
test('archive retries failed requests, paginates and refreshes on re-entry', async ({
  page,
}) => {
  let fail = true,
    fresh = false;
  await page.route('**/api/study/reports*', (r) => {
    const after = new URL(r.request().url()).searchParams.get('after');
    return fail
      ? fulfillError(r, 'Unavailable', 503)
      : fulfillStudyReportPage(
          r,
          fresh
            ? { items: [], next_cursor: null }
            : after
              ? { items: [summary(other, 'Older report')], next_cursor: null }
              : { items: [summary()], next_cursor: 'page2' },
        );
  });
  await open(page);
  const archive = page.getByRole('region', { name: 'Saved reports' });
  await expect(archive.getByRole('alert')).toBeVisible();
  fail = false;
  await archive.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(archive).toContainText('Hendrik');
  fail = true;
  await archive
    .getByRole('button', { name: 'More reports', exact: true })
    .click();
  await expect(archive.getByRole('alert')).toBeVisible();
  await expect(archive).toContainText('Hendrik');
  fail = false;
  await archive.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(archive).toContainText('Older report');
  fresh = true;
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await expect(archive).toContainText('No saved reports yet.');
  await expect(archive).not.toContainText('Older report');
});
test('late report response cannot replace newer selection and details support retry', async ({
  page,
}) => {
  let release = () => {};
  const delayed = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/study/reports?group_by=session', (r) =>
    fulfillStudyReportPage(r, {
      items: [summary(), summary(other, 'Other report')],
      next_cursor: null,
    }),
  );
  await page.route(`**/api/study/reports/${id}`, async (r) => {
    await delayed;
    await fulfillPlaySession(r, detail()).catch(() => {});
  });
  let fail = true;
  await page.route(`**/api/study/reports/${other}`, (r) =>
    fail
      ? fulfillError(r, 'Unavailable', 503)
      : fulfillPlaySession(r, { ...detail(other), play_mode: 'coach' }),
  );
  await open(page);
  const buttons = page.getByRole('button', {
    name: 'Open report',
    exact: true,
  });
  await buttons.nth(0).click();
  await page.keyboard.press('Escape');
  await buttons.nth(1).focus();
  await buttons.nth(1).press('Enter');
  const panel = page.getByRole('region', { name: 'Other report', exact: true });
  await expect(panel.getByRole('alert')).toBeVisible();
  fail = false;
  await panel.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(panel).toContainText('Coach');
  release();
  await expect(panel).not.toContainText('Tutor');
});

test('opening a report below a long archive moves keyboard focus to its visible details', async ({
  page,
}) => {
  const items = Array.from({ length: 50 }, (_, index) =>
    summary(index ? crypto.randomUUID() : id, `Match ${index + 1}`),
  );
  await page.route('**/api/study/reports?group_by=session', (r) =>
    fulfillStudyReportPage(r, { items, next_cursor: null }),
  );
  await page.route(`**/api/study/reports/${id}`, (r) =>
    fulfillPlaySession(r, detail()),
  );
  await open(page);
  await page
    .getByRole('button', { name: 'Open report', exact: true })
    .first()
    .click();
  const heading = page.getByRole('heading', { name: 'Match 1', exact: true });
  await expect(heading).toBeFocused();
  await expect(heading).toBeInViewport();
});

test('Study areas support German labels, unknown PR and arrow/Home/End navigation', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.route('**/api/decks', (r) => fulfillDecks(r, []));
  await page.route('**/api/study/similar-practice', (r) =>
    r.fulfill({ json: { items: [], next_cursor: null } }),
  );
  await page.route('**/api/study/reports?group_by=session', (r) =>
    fulfillStudyReportPage(r, {
      items: [{ ...summary(), first_attempt_rating: null }],
      next_cursor: null,
    }),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Lernen', exact: true }).click();
  const overview = page.getByRole('tab', { name: 'Übersicht', exact: true });
  const cards = page.getByRole('tab', {
    name: 'Lernkarten & Anki',
    exact: true,
  });
  const reports = page.getByRole('tab', { name: 'Auswertungen', exact: true });
  await cards.focus();
  await page.keyboard.press('ArrowRight');
  await expect(reports).toBeFocused();
  await expect(reports).toHaveAttribute('aria-selected', 'true');
  const archive = page.getByRole('region', {
    name: 'Gespeicherte Auswertungen',
  });
  await expect(archive.locator('tbody tr td').nth(6)).toHaveText('—');
  await page.keyboard.press('Home');
  await expect(overview).toBeFocused();
  await expect(
    page.getByRole('tabpanel', { name: 'Übersicht', exact: true }),
  ).toBeVisible();
  await page.keyboard.press('End');
  const practiceHistory = page.getByRole('tab', {
    name: 'Übungsverlauf',
    exact: true,
  });
  await expect(practiceHistory).toBeFocused();
  await expect(practiceHistory).toHaveAttribute('aria-selected', 'true');
  await expect(
    page.getByText('Noch keine bewerteten Übungsrunden.'),
  ).toBeVisible();
  await expect(page.getByRole('figure')).toHaveCount(0);
  const mistakes = page.getByRole('tab', {
    name: 'Meine wiederkehrenden Fehler',
    exact: true,
  });
  await page.keyboard.press('ArrowRight');
  await expect(overview).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(practiceHistory).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(mistakes).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(reports).toBeFocused();
  await expect(archive).toBeVisible();
});

test('Practice history switches rolling windows while keeping retry rounds visible', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 900 });
  await seedLocale(page);
  const round = (
    id: string,
    date: string,
    average_equity_loss: number,
    is_retry = false,
  ) => ({
    id,
    completed_at: `${date}T12:00:00Z`,
    average_equity_loss,
    is_retry,
    scored_moves: 2,
    total_positions: 3,
    missed_moves: 1,
    total_equity_loss: average_equity_loss * 2,
  });
  const latest = round('c', '2026-11-11', 0.125);
  const retry = round('r', '2026-11-10', 0.06, true);
  const prior = round('b', '2026-11-08', 0.25);
  const archive = Array.from({ length: 15 }, (_, index) =>
    round(
      `archive-${index}`,
      `2026-10-${31 - index}`,
      index === 14 ? 0.2 : 0.8,
    ),
  );
  await page.route('**/api/study/similar-practice*', (route) => {
    const before = new URL(route.request().url()).searchParams.get('before');
    return route.fulfill({
      json:
        before === prior.id
          ? {
              items: [
                round('a', '2026-11-04', 0.5),
                round('y', '2026-11-03', 0.6),
                round('x', '2026-11-02', 0.4),
                round('z', '2026-11-01', 0.8),
                ...archive,
              ],
              next_cursor: null,
            }
          : { items: [latest, retry, prior], next_cursor: prior.id },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Study', exact: true }).click();
  await page
    .getByRole('tab', { name: 'Practice history', exact: true })
    .click();
  const chart = page.getByRole('figure', {
    name: 'Average equity loss over time',
  });
  await expect(chart).toBeVisible();
  const originalPoints = chart.getByRole('img', { name: /^Practice ·/ });
  const retryPoint = chart.getByRole('img', { name: /^Retry ·.*0\.060/ });
  const line = chart.locator('polyline.practice-chart-original');
  const trend = chart.locator('polyline.practice-chart-rolling');
  const windowSelector = chart.getByRole('combobox', {
    name: 'Rolling average window',
  });
  await expect(windowSelector).toHaveValue('5');
  await expect(windowSelector.locator('option')).toHaveText([
    '5 rounds',
    '10 rounds',
    '20 rounds',
  ]);
  await expect(originalPoints).toHaveCount(2);
  await expect(retryPoint).toBeVisible();
  await expect(
    chart.getByText('Rolling average (up to 5 practice rounds)'),
  ).toBeVisible();
  await expect(line).toHaveAttribute('points', /^\S+ \S+$/);
  await expect(trend).toHaveAttribute('points', /^\S+ \S+$/);
  await originalPoints.last().focus();
  await expect(page.getByRole('tooltip')).toContainText('0.1875');
  await page.keyboard.press('ArrowLeft');
  await expect(retryPoint).toBeFocused();
  await expect(page.getByRole('tooltip')).toContainText('0.060');
  await expect(page.getByRole('tooltip')).not.toContainText('Rolling average');
  await windowSelector.selectOption('20');
  await page.getByRole('button', { name: 'Load older rounds' }).click();
  await expect(originalPoints).toHaveCount(21);
  await expect(windowSelector).toHaveValue('20');
  await expect(retryPoint).toBeVisible();
  await expect(originalPoints.first()).toHaveAttribute('aria-label', /0\.200/);
  await originalPoints.last().focus();
  await expect(page.getByRole('tooltip')).toContainText('0.69375');
  const rawLine = await line.getAttribute('points');
  const twentyRoundTrend = await trend.getAttribute('points');
  await windowSelector.selectOption('10');
  await expect(trend).not.toHaveAttribute('points', twentyRoundTrend!);
  await originalPoints.last().focus();
  await expect(page.getByRole('tooltip')).toContainText('0.5875');
  await windowSelector.selectOption('5');
  await expect(trend).toHaveAttribute('points', /100,53\.125$/);
  await expect(line).toHaveAttribute('points', rawLine!);
  await expect(retryPoint).toBeVisible();
  await originalPoints.last().focus();
  await expect(page.getByRole('tooltip')).toContainText('0.375');
  const rows = page
    .getByRole('table', { name: 'Practice history' })
    .locator('tbody tr');
  await expect(rows).toHaveCount(22);
  await expect(rows.first()).toContainText('0.125');
  await expect(rows.last()).toContainText('0.200');
});

for (const [width, locale] of [
  [1440, 'en'],
  [390, 'de'],
] as const) {
  test(`money reports are grouped by session with totals and chronological game reports at ${width}px (${locale})`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1000 });
    const a = 'aaaaaaaa-0000-4000-8000-000000000001';
    const b = 'bbbbbbbb-0000-4000-8000-000000000002';
    const bReport = 'cccccccc-0000-4000-8000-000000000003';
    const pointReport = 'dddddddd-0000-4000-8000-000000000004';
    const game = (
      reportId: string,
      sessionId: string,
      n: number,
      scores: number[],
      total: number[],
      ended: boolean,
    ) => ({
      ...summary(reportId, 'Same players'),
      match_length: 0,
      game_no: n,
      session_id: sessionId,
      scores,
      session_scores: total,
      money_session_ended: ended,
    });
    const a1 = game(id, a, 1, [3, 0], [3, 8], true);
    const a2 = game(other, a, 2, [3, 8], [3, 8], true);
    const b1 = {
      ...game(bReport, b, 1, [0, 1], [0, 1], false),
      engine_id: 'bgsage',
      bot_name: 'NeonGammon',
    };
    await page.route('**/api/study/reports*', (route) => {
      const after = new URL(route.request().url()).searchParams.get('after');
      return fulfillStudyReportPage(
        route,
        after
          ? {
              items: [summary(pointReport, 'Point match'), b1],
              next_cursor: null,
            }
          : { items: [a2, a1], next_cursor: 'page2' },
      );
    });
    const report = detail(other);
    Object.assign(report, {
      id: a,
      study_report_id: other,
      match_length: 0,
      review_game_no: 2,
    });
    report.review!.games[0].game_no = 2;
    await page.route(`**/api/study/reports/${other}`, (route) =>
      fulfillPlaySession(route, report),
    );
    await open(page, locale);
    const first = page.locator(`.study-money-session[data-session-id="${a}"]`);
    const heading = first.locator('.study-money-session-header');
    await expect(first).toBeVisible();
    await expect(heading).toContainText(
      locale === 'de' ? 'Gesamtpunktestand' : 'Total score',
    );
    await expect(heading).toContainText(/Hendrik 3\s*:\s*8 GNUbg/);
    await expect(heading).toContainText(locale === 'de' ? 'Beendet' : 'Ended');
    const rows = first.locator('tbody tr');
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0)).toContainText(
      locale === 'de' ? 'Spiel 1' : 'Game 1',
    );
    await expect(rows.nth(1)).toContainText(
      locale === 'de' ? 'Spiel 2' : 'Game 2',
    );
    await page
      .getByRole('button', {
        name: locale === 'de' ? 'Weitere Auswertungen' : 'More reports',
        exact: true,
      })
      .click();
    await expect(page.locator('.study-money-session')).toHaveCount(2);
    const second = page.locator(`.study-money-session[data-session-id="${b}"]`);
    await expect(second.locator('.study-money-session-header')).toContainText(
      /Hendrik 0\s*:\s*1 NeonGammon/,
    );
    await expect(second.locator('.study-money-session-header')).toContainText(
      locale === 'de' ? 'Laufend' : 'Ongoing',
    );
    await expect(
      page.getByRole('region', {
        name: locale === 'de' ? 'Gespeicherte Auswertungen' : 'Saved reports',
      }),
    ).toContainText('Point match');
    const box = await heading.boundingBox();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    const trigger = rows.nth(1).getByRole('button', {
      name: locale === 'de' ? 'Bericht öffnen' : 'Open report',
      exact: true,
    });
    await trigger.click();
    const dialog = page.getByRole('dialog', {
      name: locale === 'de' ? 'Lernauswertung' : 'Learning review',
      exact: true,
    });
    await expect(dialog).toContainText(locale === 'de' ? 'Spiel 2' : 'Game 2');
    await expect(dialog).toContainText('13/10 6/5');
    await page.keyboard.press('Escape');
    await expect(trigger).toBeFocused();
    await page.screenshot({
      path: test.info().outputPath('money-session-groups.png'),
      fullPage: true,
    });
  });
}
