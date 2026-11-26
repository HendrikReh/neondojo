import type { CSSProperties } from 'react';
import { nativeField } from './themes';

const surfaces = import.meta.glob<string>('./surfaces/*.svg', {
  eager: true,
  query: '?url',
  import: 'default',
});

export function boardSurface(theme: string) {
  return surfaces[`./surfaces/${theme}.svg`];
}

// The gallery renders several boards together, including two copies of the
// selected finish. Each SVG needs its own paint servers.
export function finishPaint(id: string): CSSProperties {
  return {
    '--frame-fill': `url(#${id}-rail)`,
  } as CSSProperties;
}

export default function BoardFinish({
  id,
  surface,
}: {
  id: string;
  surface: string;
}) {
  return (
    <g aria-hidden="true" pointerEvents="none">
      <defs>
        <linearGradient id={`${id}-rail`} x2="0" y2="1">
          <stop stopColor="var(--rail-highlight)" />
          <stop offset=".1" stopColor="var(--frame)" />
          <stop offset=".9" stopColor="var(--frame)" />
          <stop offset="1" stopColor="var(--rail-shadow)" />
        </linearGradient>
      </defs>
      <rect
        x="5"
        y="5"
        width="670"
        height="460"
        rx="10"
        className="board-inlay"
      />
      <image href={surface} {...nativeField} preserveAspectRatio="none" />
    </g>
  );
}
