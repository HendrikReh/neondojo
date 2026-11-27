import {
  fulfillPlaySession,
  fulfillStudyReportPage,
} from '../helpers/api-fixtures';
import { DESKTOP_VIEWPORTS, openWorkspace } from '../helpers/navigation';
import { expect, test } from '../helpers/test';
import { detail, summary } from '../helpers/study-report-fixture';
import {
  historyFixture,
  historySessionId as id,
} from '../helpers/play-history-review-helpers';

test('starts from match end, reopens from Study, and inspects the pre-roll board', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  const { saved, items } = await historyFixture(page);
  const review = {
    ...detail().review!,
    games: [
      {
        game_no: 1,
        engine: 'GNUbg',
        decisions: [
          {
            kind: 'checker',
            player: 0,
            checker_no: 1,
            decision_no: 1,
            dice: [3, 1],
            played: '8/5 6/5',
            best: '8/5 6/5',
            equity_loss: 0,
            severity: 'none',
            evaluation_status: 'evaluated',
          },
          {
            kind: 'cube',
            player: 1,
            checker_no: 2,
            decision_no: 2,
            played: 'no double',
            best: 'double',
            equity_loss: 0.12,
            severity: 'bad',
            evaluation_status: 'evaluated',
          },
          {
            kind: 'checker',
            player: 1,
            checker_no: 2,
            decision_no: 3,
            dice: [4, 2],
            played: '8/4 6/4',
            best: '8/4 6/4',
            equity_loss: 0,
            severity: 'none',
            evaluation_status: 'evaluated',
          },
        ],
      },
    ],
  };
  Object.assign(saved, {
    phase: 'match_over',
    human_player: 1,
    match_forfeited: true,
    scores: [7, 0],
    winner: 0,
  });
  const report = {
    ...saved,
    version: 8,
    study_report_id: id,
    study_report_version: 8,
    review,
  };
  await page.route(`**/api/play/sessions/${id}/review`, (route) =>
    fulfillPlaySession(route, report),
  );
  await page.route('**/api/study/reports?group_by=session', (route) =>
    fulfillStudyReportPage(route, { items: [summary(id)], next_cursor: null }),
  );
  await page.route(`**/api/study/reports/${id}`, (route) =>
    fulfillPlaySession(route, report),
  );
  await page.route(`**/api/study/reports/${id}/history?version=8`, (route) =>
    route.fulfill({ json: { version: 8, session_id: id, items } }),
  );

  let started = 0;
  let complete = false;
  const result = {
    summary: 'There is one cube decision worth reviewing.',
    next_match_focus: 'Compare the score and cube ownership before rolling.',
    topics: [
      {
        topic: 'cube',
        observation: 'Review the opportunity to double.',
        evidence_refs: ['1:2'],
      },
    ],
    key_decisions: [
      {
        evidence_ref: '1:2',
        explanation: 'The saved evaluation preferred doubling.',
        next_question: 'What changes if your opponent passes?',
      },
    ],
    training: [],
    limitations: [],
  };
  await page.route(
    `**/api/study/reports/${id}/assessment?language=en`,
    (route) => {
      if (route.request().method() === 'POST') {
        expect(route.request().postDataJSON()).toEqual({ report_version: 8 });
        started++;
      }
      const job = started
        ? {
            model: 'gpt-6.1-terra',
            status: complete ? 'completed' : 'running',
            result: complete ? result : null,
            error: null,
          }
        : null;
      return route.fulfill({
        json: { availability: 'ready', abbreviated_match: true, job },
      });
    },
  );
  await page.reload();
  await openWorkspace(page, 'play');
  await page
    .getByRole('alertdialog', { name: 'Match finished' })
    .getByRole('button', { name: 'Create AI assessment', exact: true })
    .click();
  await expect(
    page.getByRole('tab', { name: 'AI assessment', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByText('Creating assessment…')).toBeVisible();
  expect(started).toBe(1);
  await page
    .getByRole('button', { name: 'Back to reports', exact: true })
    .click();

  complete = true;
  await page.getByRole('button', { name: 'Open report', exact: true }).click();
  await page.getByRole('tab', { name: 'AI assessment', exact: true }).click();
  await expect(page.getByText(result.summary)).toBeVisible();
  await expect(page.getByText(/Conceded match ·/)).toBeVisible();
  await page
    .getByRole('button', { name: 'View position', exact: true })
    .click();
  await expect(page.locator('[data-preview-decision="1:2"]')).toBeVisible();
  expect(started).toBe(1);

  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    await expect(
      page.getByRole('region', { name: 'Decision preview' }),
    ).toBeInViewport();
    await page.screenshot({
      path: test.info().outputPath(`assessment-${width}.png`),
    });
  }
});
