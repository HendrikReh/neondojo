import { importSgfMatch } from '../helpers/match-fixtures';
import { DESKTOP_VIEWPORTS, seedLocale } from '../helpers/navigation';
import { expect, test } from '../helpers/test';

test('edits an imported player profile and opens its match', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.setViewportSize({ width: 1800, height: 1169 });
  const alias = `Spieler ${crypto.randomUUID()}`;
  await importSgfMatch(page, {
    filename: 'player.sgf',
    game: { white: alias, black: 'Gegner', moves: ';W[31hefe]' },
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Spielerdatenbank' }).click();
  await page.getByRole('searchbox', { name: 'Spieler suchen' }).fill(alias);
  await page
    .locator('.player-list')
    .getByRole('button', { name: new RegExp(alias) })
    .click();
  await expect(page.getByRole('textbox', { name: 'Vorname' })).toBeHidden();
  await expect(
    page.getByRole('columnheader', { name: 'Gegner' }),
  ).toBeVisible();
  await expect(page.getByText('7-Punkte-Match')).toBeVisible();
  for (const name of ['Ergebnis', 'PR', 'Event'])
    await expect(page.getByRole('columnheader', { name })).toBeVisible();
  await page
    .getByRole('button', { name: 'Ungewertete Matches analysieren …' })
    .click();
  const analysis = page.getByRole('dialog', {
    name: 'Gesamtes Match analysieren',
  });
  await expect(analysis).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(analysis).toBeHidden();
  await page.getByRole('button', { name: 'Bearbeiten', exact: true }).click();
  const editor = page.getByRole('dialog', { name: 'Profil bearbeiten' });
  await editor.getByRole('textbox', { name: 'Vorname' }).fill('Anna');
  await editor.getByRole('textbox', { name: 'Nachname' }).fill('Beispiel');
  const second = `Zweitname ${crypto.randomUUID()}`;
  await editor
    .getByRole('textbox', { name: 'Importierter Spielername' })
    .fill(second);
  await editor.getByRole('button', { name: 'Speichern' }).click();
  await expect(editor.getByText('Profil gespeichert.')).toBeVisible();
  await editor.screenshot({
    path: test.info().outputPath('player-edit-dialog.png'),
  });
  await expect(
    editor.getByRole('listitem').filter({ hasText: second }),
  ).toHaveText(second);
  await expect(
    editor.getByRole('button', { name: 'Speichern' }),
  ).toBeDisabled();
  await editor.getByRole('button', { name: 'Schließen' }).click();
  await expect(
    page.getByRole('heading', { name: 'Anna Beispiel' }),
  ).toBeVisible();
  await expect(
    page.locator('.player-list').getByText(alias, { exact: false }),
  ).toBeVisible();
  await expect(page.locator('.player-card-aliases')).toHaveText(
    `Aliasse: ${alias}, ${second}`,
  );
  await page.getByRole('button', { name: 'Zusammenführen …' }).click();
  const merge = page.getByRole('dialog', {
    name: 'Spielerprofile zusammenführen',
  });
  const candidateSearch = merge.getByRole('searchbox');
  await expect(candidateSearch).toBeFocused();
  await candidateSearch.fill(alias);
  await expect(merge.getByText('Keine Spieler gefunden.')).toBeVisible();
  await candidateSearch.fill('Gegner');
  await merge.getByRole('button', { name: 'Gegner', exact: true }).click();
  await expect(merge.getByText('Bleibt erhalten')).toBeVisible();
  await expect(merge.getByText('Wird übernommen')).toBeVisible();
  await expect(
    merge.getByRole('region', { name: 'Bleibt erhalten' }),
  ).toContainText('Anna Beispiel');
  await expect(
    merge.getByRole('region', { name: 'Wird übernommen' }),
  ).toContainText('Gegner');
  await expect(
    merge.getByRole('region', { name: 'Wird übernommen' }),
  ).toContainText('1 Match ·');
  await merge.getByRole('button', { name: 'Schließen' }).click();
  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    await expect(page.locator('.player-card')).toBeVisible();
    for (const selector of ['.player-result-table', '.player-match-table']) {
      await expect
        .poll(() =>
          page
            .locator(selector)
            .evaluate(
              (table) =>
                table.getBoundingClientRect().width <=
                (table.parentElement?.clientWidth ?? 0) + 1,
            ),
        )
        .toBe(true);
    }
    expect(
      await page
        .locator('.player-directory')
        .evaluate((element) => element.scrollWidth <= element.clientWidth),
    ).toBe(true);
    await page.locator('.player-directory').screenshot({
      path: test.info().outputPath(`player-directory-${width}.png`),
    });
  }
  const matches = page.locator('.player-match-table');
  await matches.getByRole('button', { name: 'Gegner', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Gegner' })).toBeVisible();
  await page
    .locator('.player-list')
    .getByRole('button', { name: 'Anna Beispiel' })
    .click();
  await expect(
    page.getByRole('heading', { name: 'Anna Beispiel' }),
  ).toBeVisible();
  await matches.getByText('7-Punkte-Match').click();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByRole('tab', { name: 'Spielerdatenbank' }).click();
  await page.getByRole('button', { name: 'Zusammenführen …' }).click();
  await merge.getByRole('searchbox').fill('Gegner');
  await merge.getByRole('button', { name: 'Gegner', exact: true }).click();
  await merge
    .getByRole('button', { name: 'Spielerprofile zusammenführen' })
    .click();
  await expect(merge).toBeHidden();
  await expect(matches.getByText('Beide Seiten')).toBeVisible();
  await expect(page.getByText('Profile zusammengeführt.')).toBeVisible();
});

test('shows the platform when imported profiles share a name', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  const name = `shared${crypto.randomUUID().slice(0, 8)}`;
  for (const [file, site] of [
    ['galaxy.sgf', 'BackgammonGalaxy'],
    ['opengammon.sgf', 'OpenGammon'],
  ]) {
    await importSgfMatch(page, {
      filename: file,
      game: {
        white: name,
        black: `Opponent ${site}`,
        extraHeaders: `PC[${site}]`,
        moves: ';W[31hefe]',
      },
    });
  }
  await page.goto('/');
  await page.getByRole('tab', { name: 'Player database' }).click();
  await page.getByRole('searchbox', { name: 'Search players' }).fill(name);
  const list = page.locator('.player-list');
  const rows = list.getByRole('row').filter({ hasText: name });
  await expect(rows).toHaveCount(2);
  for (const platform of ['backgammongalaxy.com', 'opengammon.com'])
    await expect(rows.filter({ hasText: `Platform ${platform}` })).toHaveCount(
      1,
    );
  // A name of its own needs no platform.
  await page
    .getByRole('searchbox', { name: 'Search players' })
    .fill('Opponent BackgammonGalaxy');
  const opponent = list
    .getByRole('row')
    .filter({ hasText: 'Opponent BackgammonGalaxy' });
  await expect(opponent).toHaveCount(1);
  await expect(opponent.locator('.player-name-platforms')).toHaveCount(0);
});

test('springs directly between numbered player result pages and resets on search', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  const requestedPages: number[] = [];
  await page.route('**/api/players?*', async (route) => {
    const params = new URL(route.request().url()).searchParams;
    const pageNumber = Number(params.get('page') ?? '1');
    requestedPages.push(pageNumber);
    const total = params.has('q') ? 1 : 351;
    await route.fulfill({
      json: {
        items: [
          {
            id: `player-${pageNumber}`,
            display_name: `Spieler auf Seite ${pageNumber}`,
            match_count: 1,
            game_count: 1,
          },
        ],
        next_cursor: null,
        total,
      },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Spielerdatenbank' }).click();
  const list = page.locator('.player-list');
  const pages = list.getByRole('navigation');
  await expect(pages.getByRole('button', { name: 'Anfang' })).toBeDisabled();
  await expect(pages.getByRole('button', { name: 'Seite 5' })).toBeVisible();
  await pages.getByRole('button', { name: 'Seite 3' }).click();
  await expect(list.getByText('Spieler auf Seite 3')).toBeVisible();
  await pages.getByRole('button', { name: 'Ende' }).click();
  await expect(list.getByText('Spieler auf Seite 8')).toBeVisible();
  await expect(pages.getByRole('button', { name: 'Seite 8' })).toHaveAttribute(
    'aria-current',
    'page',
  );
  expect(requestedPages.filter((page) => page !== 1)).toEqual([3, 8]);
  await pages.getByRole('button', { name: 'Anfang' }).click();
  await expect(list.getByText('Spieler auf Seite 1')).toBeVisible();
  await pages.getByRole('button', { name: 'Ende' }).click();
  await expect(list.getByText('Spieler auf Seite 8')).toBeVisible();
  await page.getByRole('searchbox', { name: 'Spieler suchen' }).fill('Filter');
  await expect(list.getByText('Spieler auf Seite 1')).toBeVisible();
  await expect(pages).toHaveCount(0);
});

test('offers profiles with a similar name before a merge search is typed', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  const id = '7b0f6f0e-6a55-4d0c-9a53-2f6a3c1d9e10';
  const queries: (string | null)[] = [];
  await page.route('**/api/players?*', async (route) => {
    const params = new URL(route.request().url()).searchParams;
    const candidates = params.get('exclude_id') === id;
    if (candidates) queries.push(params.get('q'));
    await route.fulfill({
      json: {
        items: [
          candidates
            ? {
                id: 'c2a1c9c4-0d2b-4c58-8a43-5b1d6f1f7a11',
                display_name: 'Akiko Yazawa',
                match_count: 4,
                game_count: 22,
              }
            : {
                id,
                display_name: 'Akiko_Yazawa',
                match_count: 2,
                game_count: 20,
              },
        ],
        next_cursor: null,
        total: 1,
      },
    });
  });
  await page.route(`**/api/players/${id}`, (route) =>
    route.fulfill({
      json: {
        id,
        display_name: 'Akiko_Yazawa',
        first_name: '',
        last_name: '',
        version: 1,
        match_count: 2,
        game_count: 20,
        aliases: [],
        ratings: [],
        rating_history: [],
      },
    }),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Spielerdatenbank' }).click();
  await page.getByRole('button', { name: 'Akiko_Yazawa' }).click();
  await page.getByRole('button', { name: 'Zusammenführen …' }).click();
  const merge = page.getByRole('dialog', {
    name: 'Spielerprofile zusammenführen',
  });
  await expect(merge.getByRole('searchbox')).toBeFocused();
  await expect(merge.getByText('Ähnliche Namen')).toBeVisible();
  await expect(
    merge.getByRole('button', { name: 'Akiko Yazawa' }),
  ).toBeVisible();
  expect(new Set(queries)).toEqual(new Set(['Akiko']));
  await merge.getByRole('searchbox').fill('Yaz');
  await expect(merge.getByText('Ähnliche Namen')).toBeHidden();
});

test('sorts the player list from its column headings', async ({ page }) => {
  await seedLocale(page, 'de');
  const orders: string[] = [];
  await page.route('**/api/players?*', async (route) => {
    const params = new URL(route.request().url()).searchParams;
    const order = `${params.get('sort')} ${params.get('direction')} ${params.get('page')}`;
    if (orders.at(-1) !== order) orders.push(order);
    await route.fulfill({
      json: {
        items: [
          {
            id: 'player-1',
            display_name: `Sortiert nach ${params.get('sort')}`,
            match_count: 3,
            game_count: 7,
          },
        ],
        next_cursor: null,
        total: 120,
      },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Spielerdatenbank' }).click();
  const list = page.locator('.player-list');
  const column = (name: string) => list.getByRole('columnheader', { name });
  await expect(column('Matches')).toHaveAttribute('aria-sort', 'descending');
  await expect(column('Spieler')).toHaveAttribute('aria-sort', 'none');
  await list.getByRole('button', { name: 'Seite 2' }).click();
  await list.getByRole('button', { name: 'Nach Matches sortieren' }).click();
  await expect(column('Matches')).toHaveAttribute('aria-sort', 'ascending');
  await list.getByRole('button', { name: 'Nach Spieler sortieren' }).click();
  await expect(list.getByText('Sortiert nach name')).toBeVisible();
  await expect(column('Spieler')).toHaveAttribute('aria-sort', 'ascending');
  await list.getByRole('button', { name: 'Nach Spieler sortieren' }).click();
  await expect(column('Spieler')).toHaveAttribute('aria-sort', 'descending');
  expect(orders).toEqual([
    'matches desc 1',
    'matches desc 2',
    'matches asc 1',
    'name asc 1',
    'name desc 1',
  ]);
});
