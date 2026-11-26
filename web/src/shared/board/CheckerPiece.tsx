import { checkerArtwork, type PieceSet } from './piece-artwork';

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
  const image = checkerArtwork(set, white, variant);
  if (image) {
    const faceWidth = height * (image.width / image.height);
    return (
      <image
        href={image.src}
        x={x + (width - faceWidth) / 2}
        y={y}
        width={faceWidth}
        height={height}
        onError={onError}
      />
    );
  }
  return (
    <g className={`theme-${set.id}`}>
      <circle
        cx={x + width / 2}
        cy={y + height / 2}
        r={height / 2 - 0.75}
        className={white ? 'white-checker' : 'black-checker'}
      />
      <circle
        cx={x + width / 2}
        cy={y + height / 2}
        r={height * 0.34}
        className="checker-ring"
      />
    </g>
  );
}
