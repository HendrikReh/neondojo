import { expect, test } from './test';

test('numeric IDs and every data column sort through the API and reset paged selections', async ({
  page,
}) => {
  test.setTimeout(120000);
  const database = await (
    await page.request.post('/api/databases', {
      data: {
        name: `Sort ${crypto.randomUUID()}`,
        description: '',
      },
    })
  ).json();
  const ids: string[] = [];
  for (let i = 0; i < 55; i++) {
    const response = await page.request.post(
      `/api/imports?database_id=${database.id}`,
      {
        multipart: {
          file: {
            name: `sort-${i}.sgf`,
            mimeType: 'application/octet-stream',
            buffer: Buffer.from(
              `(;GM[6]PW[Sort ${54 - i}]PB[Opponent ${i}]MI[length:11];W[31hefe])`,
            ),
          },
        },
      },
    );
    expect(response.status()).toBe(201);
    ids.push((await response.json()).match.id);
  }
  for (let i = 0; i < 3; i++) {
    const metadata = await page.request.put(`/api/matches/${ids[i]}/metadata`, {
      data: {
        version: 0,
        display_name: `Training ${i}`,
        played_on: `2026-10-0${3 - i}`,
        tournament: ['Zeta', 'alpha', 'Alpha'][i],
        round: 'Final',
        place: 'Berlin',
        result_state: 'completed',
        white_score: i === 0 ? 9 : 11,
        black_score: i === 0 ? 11 : 9 - i,
      },
    });
    expect(metadata.ok()).toBe(true);
    const initial = await (
      await page.request.get(`/api/matches/${ids[i]}/annotation`)
    ).json();
    const saved = await page.request.put(`/api/matches/${ids[i]}/annotation`, {
      data: {
        expected_version: initial.version,
        tags: [['Zeta'], ['Alpha'], ['Beta']][i],
        comment: ['Zebra', 'apple', 'Apple'][i],
      },
    });
    expect(saved.ok(), await saved.text()).toBe(true);
  }
  const analysis = await page.request.post(`/api/matches/${ids[0]}/analysis`, {
    data: { plies: 1 },
  });
  expect(analysis.status()).toBe(202);
  const job = await analysis.json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
      { timeout: 20000 },
    )
    .toBe('completed');
  const original = await (
    await page.request.get(
      `/api/library/matches?database_id=${database.id}&sort=id&direction=asc&limit=200`,
    )
  ).json();
  const expectedIds: number[] = original.items.map(
    (entry: { match_no: number }) => entry.match_no,
  );
  expect(expectedIds).toHaveLength(55);
  expect(expectedIds).toEqual([...expectedIds].sort((a, b) => a - b));
  expect(expectedIds.every((id) => Number.isSafeInteger(id) && id > 0)).toBe(
    true,
  );
  expect(new Set(expectedIds).size).toBe(55);

  await page.addInitScript((id) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.database', id);
  }, database.id);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  const rows = page.locator('.database-table tbody tr');
  const readIds = async () =>
    (await rows.locator('.library-id-cell').allTextContents()).map((value) =>
      Number(value.replaceAll(',', '')),
    );
  await expect(rows).toHaveCount(50);

  async function sort(key: string, direction: 'asc' | 'desc') {
    const result = page.waitForResponse((response) => {
      const url = new URL(response.url());
      return (
        url.pathname === '/api/library/matches' &&
        url.searchParams.get('sort') === key &&
        url.searchParams.get('direction') === direction
      );
    });
    await page.locator(`.database-table button[data-sort="${key}"]`).click();
    const response = await result;
    expect(response.status()).toBe(200);
    expect(new URL(response.url()).searchParams.has('after')).toBe(false);
    const data = await response.json();
    await expect(rows).toHaveCount(50);
    await expect
      .poll(readIds)
      .toEqual(data.items.map((entry: { match_no: number }) => entry.match_no));
    await expect(
      page.locator(`.database-table th:has(button[data-sort="${key}"])`),
    ).toHaveAttribute(
      'aria-sort',
      direction === 'asc' ? 'ascending' : 'descending',
    );
    return data;
  }
  await sort('id', 'asc');
  await expect.poll(readIds).toEqual(expectedIds.slice(0, 50));
  await page.getByRole('button', { name: 'Next page', exact: true }).click();
  await expect(rows).toHaveCount(5);
  expect(await readIds()).toEqual(expectedIds.slice(50));
  await rows.first().getByRole('checkbox').check();
  await expect(page.locator('.library-batch-bar')).toBeVisible();
  await sort('id', 'desc');
  await expect(
    page.locator('.library-batch-bar').getByRole('button', {
      name: 'Clear selection',
      exact: true,
    }),
  ).toBeDisabled();
  await page.getByRole('button', { name: 'Next page', exact: true }).click();
  await expect(rows).toHaveCount(5);
  expect(await readIds()).toEqual([...expectedIds].reverse().slice(50));
  await page.locator('.library-column-picker summary').click();
  await page
    .getByRole('button', { name: 'Show all columns', exact: true })
    .click();
  await page.keyboard.press('Escape');
  for (const key of [
    'white',
    'white_pr',
    'black',
    'black_pr',
    'match_length',
    'score',
    'played_at',
    'created_at',
    'tournament',
    'position_count',
    'analysis_status',
    'tags',
    'comment',
  ]) {
    await sort(key, 'asc');
    await sort(key, 'desc');
  }
  const final = await (
    await page.request.get(
      `/api/library/matches?database_id=${database.id}&sort=id&direction=asc&limit=200`,
    )
  ).json();
  expect(
    final.items.map((entry: { match_no: number }) => entry.match_no),
  ).toEqual(expectedIds);
});
