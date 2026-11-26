import { nativeCheckerSize } from './themes';

const artwork = import.meta.glob<string>('./checkers/*.webp', {
  eager: true,
  import: 'default',
  query: '?url&no-inline',
});

export function checkerArtwork(theme: string, white: boolean, variant = 0) {
  return artwork[
    `./checkers/${theme}-${white ? 'white' : 'black'}-${variant % 2}.webp`
  ];
}

/** Shared by checker stacks and the moving checker during playback. */
export default function BuiltInChecker({
  x = 0,
  y = 0,
  white,
  theme,
  variant,
}: {
  x?: number;
  y?: number;
  white: boolean;
  theme?: string;
  variant: number;
}) {
  const image = theme && checkerArtwork(theme, white, variant);
  if (image)
    return (
      <image
        href={image}
        x={x - nativeCheckerSize / 2}
        y={y - nativeCheckerSize / 2}
        width={nativeCheckerSize}
        height={nativeCheckerSize}
      />
    );
  return (
    <>
      <circle
        cx={x}
        cy={y}
        r={nativeCheckerSize / 2 - 0.75}
        className={white ? 'white-checker' : 'black-checker'}
      />
      <circle
        cx={x}
        cy={y}
        r={nativeCheckerSize * 0.34}
        className="checker-ring"
      />
    </>
  );
}
