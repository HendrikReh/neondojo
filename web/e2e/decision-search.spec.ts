import { test, expect } from './test';

const position = {
  id: '11111111-1111-4111-8111-111111111111',
  match_uuid: '22222222-2222-4222-8222-222222222222',
  ordinal: 3,
  game_no: 1,
  ply: 3,
  player: 0,
  dice: [],
  board: [
    0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    2, 0,
  ],
  position_id: '4HPwATDgc/ABMA',
  gnubg_match_id: 'cIngAAAAAAAE',
  notation: '',
};
const hit = (decision_no: number, loss: number) => ({
  reference: { job_id: 'job', game_no: 1, decision_no },
  kind: 'cube_response',
  position: { ...position, id: '00000000-0000-0000-0000-000000000000' },
  players: ['Alice', 'Bob'],
  engine: 'GNU Backgammon 1.08.003',
  plies: 2,
  depth: '2-ply',
  source: 'native',
  game_plan: 'attacking',
  mistake_kind: 'wrong_take',
  played: 'take',
  best: 'drop',
  equity_loss: loss,
  match_length: 7,
  away: [2, 4],
  crawford: false,
  cube_value: 1,
  cube_owner: null,
  practicable: true,
});

test('all analyzed decisions are filtered on the server, previewed and practiced', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.route('**/api/study/mistakes?*', (route) =>
    route.fulfill({
      json: {
        players: ['Alice', 'Bob'],
        groups: [],
        analyzed_decisions: 0,
        analyzed_matches: 0,
      },
    }),
  );
  const searches: URLSearchParams[] = [];
  await page.route('**/api/study/decisions?*', (route) => {
    const params = new URL(route.request().url()).searchParams;
    searches.push(params);
    const page_no = Number(params.get('page') ?? 1);
    return route.fulfill({
      json: {
        items: page_no === 1 ? [hit(3, 0.125), hit(9, 0.06)] : [hit(12, 0.05)],
        total: 51,
        page: page_no,
        limit: 50,
      },
    });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Positions', exact: true }).click();
  const scope = page.getByRole('group', { name: 'Position source' });
  await expect(
    scope.getByRole('button', { name: 'Saved positions' }),
  ).toHaveAttribute('aria-pressed', 'true');
  await scope.getByRole('button', { name: 'All analyzed decisions' }).click();
  const search = page.getByRole('region', { name: 'Analyzed decisions' });
  await expect(search).toContainText(
    'Choose a player to search their analyzed decisions.',
  );
  const submit = search.getByRole('button', { name: 'Search decisions' });
  await expect(submit).toBeDisabled();
  await search
    .getByRole('combobox', { name: 'Player', exact: true })
    .selectOption('Alice');
  await search
    .getByRole('combobox', { name: 'Decision type' })
    .selectOption('cube_response');
  await search
    .getByRole('spinbutton', { name: 'Player needs (away)' })
    .fill('2');
  await submit.click();
  await expect(search).toContainText('51 decisions found');
  expect(Object.fromEntries(searches[0])).toEqual({
    player: 'Alice',
    kind: 'cube_response',
    min_loss: '0.05',
    away: '2',
    limit: '50',
    page: '1',
  });
  const table = search.getByRole('table', { name: 'Analyzed decisions' });
  const first = table.getByRole('row').nth(1);
  await expect(first).toContainText('Alice – Bob');
  await expect(first).toContainText('1.3');
  await expect(first).toContainText('Wrong take');
  await expect(first).toContainText('2-away / 4-away');
  await expect(first).toContainText('0.125');
  const preview = search.getByRole('complementary', {
    name: 'Decision preview',
  });
  await expect(preview.locator('[data-cube-context]')).toContainText(
    'Bob doubles to 2',
  );
  await expect(preview).toContainText(
    'Played: Take · Best: Pass · Loss: 0.125',
  );
  // A cube decision has no saved checker position to open.
  await expect(
    preview.getByRole('button', { name: 'Open in Analysis' }),
  ).toHaveCount(0);

  await search
    .getByRole('navigation', { name: 'Analyzed decisions' })
    .getByRole('button', { name: 'Page 2', exact: true })
    .click();
  await expect(table.getByRole('row').nth(1)).toContainText('1.12');
  expect(searches.at(-1)?.get('page')).toBe('2');
  expect(searches.at(-1)?.get('away')).toBe('2');

  await search.getByRole('button', { name: 'Practice results' }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Practice recurring mistakes',
    exact: true,
  });
  await expect(dialog).toContainText('1 / 1');
  await expect(dialog.getByRole('button', { name: 'Take' })).toHaveAttribute(
    'aria-keyshortcuts',
    'T',
  );
  await page.keyboard.press('Escape');
  await expect(
    search.getByRole('button', { name: 'Practice results' }),
  ).toBeFocused();
});
