import { expect, type Page } from '@playwright/test';

export async function openMatchAnalysisOptions(page: Page) {
  const trigger = page
    .locator('.workspace-analysis-command')
    .getByRole('button', {
      name: /^(Analysis options|Analyseoptionen)$/,
    });
  await expect(trigger).toBeVisible();
  if ((await trigger.getAttribute('aria-expanded')) === 'false')
    await trigger.click();
}

export async function openPositionCalculation(page: Page, dice = false) {
  const labels = dice
    ? ['Recalculate dice equity', 'Würfel-Equity neu berechnen']
    : ['Recalculate position', 'Position neu berechnen'];
  const dialog = page.getByRole('dialog', {
    name: new RegExp(`^(${labels.join('|')})$`),
  });
  if (!(await dialog.isVisible())) {
    const trigger = page
      .locator('.analysis-commandbar')
      .getByRole('button', { name: new RegExp(`^(${labels.join('|')}) …$`) });
    const options = page
      .locator('.workspace-analysis-command')
      .getByRole('button', {
        name: /^(Analysis options|Analyseoptionen)$/,
      });
    await expect(options.or(trigger).first()).toBeVisible();
    if (!(await trigger.isVisible())) await openMatchAnalysisOptions(page);
    await trigger.click();
  }
  return dialog;
}

export async function openFullMatchAnalysis(page: Page) {
  const match = page.locator('.analysis-heading[data-match-id]');
  await expect(match).toBeVisible();
  const matchId = await match.getAttribute('data-match-id');
  await page
    .getByRole('tab', { name: /^(Match database|Matchdatenbank)$/ })
    .click();
  const library = page.locator('.library-main');
  const row = library.locator(`tr[data-match-id="${matchId}"]`).first();
  await expect(library.locator('.database-count')).toHaveText(/^\d/);
  // A repeated import can select an older match beyond the first library page.
  while ((await row.count()) === 0) {
    const next = library
      .locator('.search-pagination')
      .getByRole('button', { name: /^(Next page|Nächste Seite)$/ });
    await expect(next).toBeEnabled();
    await next.click();
    await expect(library.locator('.search-pagination')).toHaveAttribute(
      'aria-busy',
      'false',
    );
  }
  const actions = library.locator('.library-batch-bar');
  const clear = actions.getByRole('button', {
    name: /^(Clear selection|Auswahl aufheben)$/,
  });
  if ((await clear.isVisible()) && (await clear.isEnabled()))
    await clear.click();
  // A library refresh right after an import can re-render the row and drop a
  // selection made a moment earlier; select until the batch bar confirms it.
  await expect(async () => {
    await row.getByRole('checkbox').check();
    await expect(actions.getByRole('status')).toHaveText(/^1\s/, {
      timeout: 1_000,
    });
  }).toPass();
  await actions
    .getByRole('button', {
      name: /^(Analyze selected matches|Ausgewählte Matches analysieren) …$/,
    })
    .click();
  return page.getByRole('dialog', {
    name: /^(Analyze entire match|Gesamtes Match analysieren)$/,
  });
}

export async function submitFullMatchAnalysis(
  page: Page,
  dialog: ReturnType<Page['getByRole']>,
) {
  const submitted = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      /^\/api\/matches\/[^/]+\/analysis$/.test(
        new URL(response.url()).pathname,
      ),
  );
  await dialog
    .getByRole('button', {
      name: /^(Analyze entire match|Analyze match again|Gesamtes Match analysieren|Match erneut analysieren)$/,
    })
    .click();
  const response = await submitted;
  expect(response.ok(), await response.text()).toBe(true);
  await expect(dialog).toBeHidden({ timeout: 180_000 });
  await expect(
    page.getByRole('tab', { name: /^(Statistics|Statistik)$/ }),
  ).toHaveAttribute('aria-selected', 'true');
  return response;
}

export async function calculateDiceEquity(page: Page) {
  const dialog = await openPositionCalculation(page, true);
  await dialog
    .getByRole('button', { name: /^(Dice equity|Würfel-Equity)$/ })
    .click();
}
