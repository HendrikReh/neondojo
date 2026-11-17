import { useMistakeText } from './recurring-mistakes-messages';
import Tooltip from './Tooltip';

/**
 * Limits mistake statistics to matches played without learning aids, so the
 * error rates and their trend describe unaided play.
 */
export default function IndependentOnlyToggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  const t = useMistakeText();
  return (
    <Tooltip as="span" text={t('independentOnlyHelp')}>
      <label className="recurring-toggle">
        <input
          type="checkbox"
          checked={checked}
          onChange={(event) => onChange(event.target.checked)}
        />
        {t('independentOnly')}
      </label>
    </Tooltip>
  );
}
