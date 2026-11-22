export function pipCounts(board: number[]): [number, number] {
  return board.reduce<[number, number]>(
    (pips, count, point) => {
      if (count > 0) pips[0] += count * point;
      if (count < 0) pips[1] -= count * (25 - point);
      return pips;
    },
    [0, 0],
  );
}

export function checkerCounts(board: number[]): [number, number] {
  return board.reduce<[number, number]>(
    (sum, count) => {
      if (count > 0) sum[0] += count;
      if (count < 0) sum[1] -= count;
      return sum;
    },
    [0, 0],
  );
}

export function matchContext(id: string): {
  scores: [number, number];
  length: number;
  jacoby: boolean;
  cube: number;
  owner: number | null;
  crawford: boolean;
  doubled: boolean;
} {
  const bytes = Uint8Array.from(atob(id), (char) => char.charCodeAt(0));
  const bits = (start: number, length: number) => {
    let result = 0;
    for (let bit = 0; bit < length; bit++)
      result |=
        ((bytes[Math.floor((start + bit) / 8)] >> ((start + bit) % 8)) & 1) <<
        bit;
    return result;
  };
  const owner = bits(4, 2);
  return {
    scores: [bits(36, 15), bits(51, 15)],
    length: bits(21, 15),
    jacoby: !bits(66, 1),
    cube: 2 ** bits(0, 4),
    owner: owner === 3 ? null : owner,
    crawford: !!bits(7, 1),
    doubled: !!bits(12, 1),
  };
}
