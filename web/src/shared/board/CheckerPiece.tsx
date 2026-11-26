import { checkerSource, type PieceSet } from './piece-artwork';

/** The board supplies the slot; the selected set supplies the face. */
export default function CheckerPiece({
  x,
  y,
  width,
  height,
  white,
  set,
  variant,
  onError,
}: {
  x: number;
  y: number;
  width: number;
  height: number;
  white: boolean;
  set: PieceSet;
  variant: number;
  onError: () => void;
}) {
  const image = checkerSource(set, white, variant);
  if (image)
    return (
      <image
        href={image}
        x={x}
        y={y}
        width={width}
        height={height}
        onError={onError}
      />
    );
  const size = Math.min(width, height);
  return (
    <g className={`theme-${set.id}`}>
      <circle
        cx={x + width / 2}
        cy={y + height / 2}
        r={size / 2 - 0.75}
        className={white ? 'white-checker' : 'black-checker'}
      />
      <circle
        cx={x + width / 2}
        cy={y + height / 2}
        r={size * 0.34}
        className="checker-ring"
      />
    </g>
  );
}
