import { useId } from 'react';

// Indices on the standard three-by-three pip grid, read from the upper left.
const faces = [
  [],
  [4],
  [2, 6],
  [2, 4, 6],
  [0, 2, 6, 8],
  [0, 2, 4, 6, 8],
  [0, 2, 3, 5, 6, 8],
];

const colors: Record<string, readonly [string, string]> = {
  walnut: ['#fff2d9', '#603323'],
  slate: ['#edf9fc', '#273747'],
  moonstone: ['#faf8ff', '#354e91'],
  viridian: ['#f5f8dc', '#008345'],
  'terra-rossa': ['#fff1cf', '#ba101c'],
  ion: ['#f5e6fa', '#504197'],
  pelagic: ['#e3f6f4', '#075b69'],
  carbon: ['#f3f0e9', '#222229'],
};

function finish(theme: string, light: boolean) {
  const base = colors[theme]?.[light ? 0 : 1];
  if (!base) {
    // The original Forest finish.
    return light
      ? {
          edge: ['#fffef2', '#d8d2b9', '#b9b397', '#837d69'],
          face: ['#fffef7', '#f7f4e7', '#ede7cd'],
        }
      : {
          edge: ['#83ae99', '#316d58', '#123c33', '#092b23'],
          face: ['#194a3c', '#205b48', '#39775e'],
        };
  }
  const mix = (color: string, percent: number) =>
    `color-mix(in srgb, ${base}, ${color} ${percent}%)`;
  return {
    edge: [
      mix('white', 52),
      mix('white', 14),
      mix('black', 18),
      mix('black', 44),
    ],
    face: [
      mix(light ? 'white' : 'black', 16),
      base,
      mix(light ? 'black' : 'white', 12),
    ],
  };
}

export default function ThemedDie({
  theme,
  light,
  value,
  width,
  height,
}: {
  theme: string;
  light: boolean;
  value: number | null;
  width: number;
  height: number;
}) {
  const id = useId();
  const { edge, face } = finish(theme, light);
  const ink = light ? '#19221e' : '#fff9e5';
  return (
    <svg
      data-dice-theme={theme}
      viewBox="0 0 64 64"
      width={width}
      height={height}
      aria-hidden="true"
    >
      <defs>
        <linearGradient id={id + '-edge'} x2=".72" y2="1">
          <stop stopColor={edge[0]} />
          <stop offset=".38" stopColor={edge[1]} />
          <stop offset=".8" stopColor={edge[2]} />
          <stop offset="1" stopColor={edge[3]} />
        </linearGradient>
        <linearGradient id={id + '-face'} x1=".25" x2=".65" y2="1">
          <stop stopColor={face[0]} />
          <stop offset=".6" stopColor={face[1]} />
          <stop offset="1" stopColor={face[2]} />
        </linearGradient>
      </defs>
      <rect
        x="1"
        y="1.5"
        width="62"
        height="61.5"
        rx="10"
        fill={`url(#${id}-edge)`}
      />
      <path
        d="M32 2.5C51 2.5 61.5 13 61.5 32S51 61 32 61 2.5 51 2.5 32 13 2.5 32 2.5Z"
        fill={`url(#${id}-face)`}
      />
      <path
        d="M4 27Q5 6 26 3"
        fill="none"
        stroke={light ? '#fff' : edge[0]}
        strokeWidth="1"
        opacity=".55"
      />
      <g fill={ink} stroke={light ? '#fffef6' : edge[3]} strokeWidth="1.1">
        {faces[value ?? 0].map((cell) => (
          <circle
            key={cell}
            cx={16 + (cell % 3) * 16}
            cy={16 + Math.floor(cell / 3) * 16}
            r="7.1"
          />
        ))}
      </g>
      {value === null && (
        <path
          d="M25 32h14"
          stroke={ink}
          strokeWidth="3"
          strokeLinecap="round"
        />
      )}
    </svg>
  );
}
