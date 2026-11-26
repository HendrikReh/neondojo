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

export default function ForestDie({
  light,
  value,
  width,
  height,
}: {
  light: boolean;
  value: number | null;
  width: number;
  height: number;
}) {
  const id = useId();
  const ink = light ? '#19221e' : '#fff9e5';
  return (
    <svg viewBox="0 0 64 64" width={width} height={height} aria-hidden="true">
      <defs>
        <linearGradient id={id + '-edge'} x2=".72" y2="1">
          <stop stopColor={light ? '#fffef2' : '#83ae99'} />
          <stop offset=".38" stopColor={light ? '#d8d2b9' : '#316d58'} />
          <stop offset=".8" stopColor={light ? '#b9b397' : '#123c33'} />
          <stop offset="1" stopColor={light ? '#837d69' : '#092b23'} />
        </linearGradient>
        <linearGradient id={id + '-face'} x1=".25" x2=".65" y2="1">
          <stop stopColor={light ? '#fffef7' : '#194a3c'} />
          <stop offset=".6" stopColor={light ? '#f7f4e7' : '#205b48'} />
          <stop offset="1" stopColor={light ? '#ede7cd' : '#39775e'} />
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
        stroke={light ? '#fff' : '#99c8b3'}
        strokeWidth="1"
        opacity=".55"
      />
      <g fill={ink} stroke={light ? '#fffef6' : '#0c3529'} strokeWidth="1.1">
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
