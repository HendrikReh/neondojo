import { expect, test } from 'vitest';
import { matchContext } from './boardMath';
test('a pending cube offer preserves cube ownership and exposes the offered state', () => {
  const bytes = new Uint8Array(9);
  bytes[0] = 1;
  bytes[1] = 16;
  const id = btoa(String.fromCharCode(...bytes));
  expect(matchContext(id)).toMatchObject({ cube: 2, owner: 0, doubled: true });
  bytes[1] = 0;
  expect(matchContext(btoa(String.fromCharCode(...bytes)))).toMatchObject({
    cube: 2,
    owner: 0,
    doubled: false,
  });
});
