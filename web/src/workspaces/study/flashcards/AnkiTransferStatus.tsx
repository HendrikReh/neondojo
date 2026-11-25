import { useI18n } from '../../../shared/i18n/i18n';
import { useCardPracticeText } from './card-practice-messages';
import type { AnkiTransfers } from './useAnkiTransfers';

/** The running Anki transfer and the result of the last one. */
export default function AnkiTransferStatus({
  transfers: { progress, sending, syncing, notice, error },
}: {
  transfers: AnkiTransfers;
}) {
  const practiceT = useCardPracticeText();
  const { formatNumber } = useI18n();
  return (
    <>
      {progress && (
        <p role="status">
          {practiceT('ankiImporting', {
            file: progress.file,
            current: formatNumber(progress.current),
            total: formatNumber(progress.total),
          })}
        </p>
      )}
      {sending && <p role="status">{practiceT('sendingAnki')}</p>}
      {syncing && <p role="status">{practiceT('syncingAnkiReviews')}</p>}
      {notice && <p role="status">{notice}</p>}
      {error && (
        <p className="learning-anki-error" role="alert">
          {error}
        </p>
      )}
    </>
  );
}
