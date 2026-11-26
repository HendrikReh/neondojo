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
  // The artwork's transparent margin contains its shadow. The visible face
  // fits the same 37.4-unit hit area as the plain SVG pieces.
  const image = theme && checkerArtwork(theme, white, variant);
  if (image)
    return <image href={image} x={x - 20} y={y - 20} width="40" height="40" />;
  return (
    <>
      <circle
        cx={x}
        cy={y}
        r="18.7"
        className={white ? 'white-checker' : 'black-checker'}
      />
      <circle cx={x} cy={y} r="13.5" className="checker-ring" />
    </>
  );
}
