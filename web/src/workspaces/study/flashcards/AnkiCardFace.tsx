export default function AnkiCardFace({
  id,
  title,
  side = 'front',
}: {
  id: string;
  title: string;
  side?: 'front' | 'back';
}) {
  return (
    <iframe
      className="anki-card-face"
      title={title}
      src={`/api/cards/${encodeURIComponent(id)}/anki/${side}`}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
    />
  );
}
