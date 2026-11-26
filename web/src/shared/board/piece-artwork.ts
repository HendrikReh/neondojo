import type { BgBlitzTheme } from './themes';

const artwork = import.meta.glob<string>('./checkers/*.webp', {
  eager: true,
  import: 'default',
  query: '?url&no-inline',
});

function checkerArtwork(theme: string, white: boolean, variant = 0) {
  return artwork[
    `./checkers/${theme}-${white ? 'white' : 'black'}-${variant % 2}.webp`
  ];
}

export interface PieceSet {
  id: string;
  archive?: BgBlitzTheme;
}

export function pieceSet(id: string, archives: BgBlitzTheme[]): PieceSet {
  const archive = archives.find((item) => item.id === id);
  return { id: !archive && id.startsWith('bgblitz:') ? 'forest' : id, archive };
}

export function checkerSource(set: PieceSet, white: boolean, variant = 0) {
  const images = set.archive?.checkers[white ? 'white' : 'black'];
  return images
    ? images[variant % images.length].src
    : checkerArtwork(set.id, white, variant);
}

export function cubeImage(
  theme: BgBlitzTheme | undefined,
  value: number,
  owner: number | null,
  pending: boolean,
) {
  const face = String(value === 1 ? 64 : value);
  return (
    (!pending && owner === null && value === 1
      ? theme?.cubes['64_r']
      : undefined) ?? theme?.cubes[face]
  );
}
