import type { BgBlitzTheme } from './themes';

const artwork = import.meta.glob<string>('./checkers/*.webp', {
  eager: true,
  import: 'default',
  query: '?url&no-inline',
});

export interface PieceSet {
  id: string;
  archive?: BgBlitzTheme;
}

export function pieceSet(id: string, archives: BgBlitzTheme[]): PieceSet {
  const archive = archives.find((item) => item.id === id);
  return { id: !archive && id.startsWith('bgblitz:') ? 'forest' : id, archive };
}

export function checkerArtwork(set: PieceSet, white: boolean, variant = 0) {
  const side = white ? 'white' : 'black';
  const images = set.archive?.checkers[side];
  if (images) return images[variant % images.length];
  const src = artwork[`./checkers/${set.id}-${side}-${variant % 2}.webp`];
  return src ? { src, width: 192, height: 192 } : undefined;
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
