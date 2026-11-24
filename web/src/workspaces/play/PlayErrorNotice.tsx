import ActionButton from '../../shared/ui/ActionButton';
import { useI18n } from '../../shared/i18n/i18n';
import { usePlayText } from './play-messages';
import { usePlayModeText } from './play-mode-messages';

/** A failed request and how to repeat it. */
export default function PlayErrorNotice({
  error,
  hint,
  busy,
  onHint,
  onRetry,
}: {
  error: unknown;
  /** The request was for a hint. */
  hint: boolean;
  busy: boolean;
  onHint: () => void;
  onRetry: () => void;
}) {
  const t = usePlayText();
  const modeT = usePlayModeText();
  const { formatError } = useI18n();
  return (
    <div role="alert" className="play-error">
      <p>{t(hint ? 'hintError' : 'unknownError')}</p>
      <p>{formatError(error)}</p>
      {hint && (
        <ActionButton
          disabled={busy}
          onClick={onHint}
          icon="reset"
          label={modeT('retry')}
        >
          {modeT('retry')}
        </ActionButton>
      )}
      <ActionButton
        disabled={busy}
        onClick={onRetry}
        icon="refresh"
        label={t('retry')}
      >
        {t('retry')}
      </ActionButton>
    </div>
  );
}
