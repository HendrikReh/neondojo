import { useI18n } from '../../../shared/i18n/i18n';
import { defineMessages } from '../../../shared/i18n/messages';

const messages = defineMessages('candidate');

export const candidateMessages = messages.catalogs;
export const useCandidateText = messages.useText;

export function useCandidateLossText() {
  const { formatNumber } = useI18n();
  const text = useCandidateText();
  const digits = { minimumFractionDigits: 3, maximumFractionDigits: 3 };
  return (loss: number | null, unit?: string) =>
    loss === null
      ? '—'
      : unit === 'mwc'
        ? text('percentagePoints', { value: formatNumber(loss * 100, digits) })
        : formatNumber(loss, digits);
}

export function useCandidateEquityText(digits = 3) {
  const { formatEquity, formatPercent } = useI18n();
  const text = useCandidateText();
  const unit = (kind: string) =>
    ({
      cubeful: 'Cubeful',
      cubeless: 'Cubeless',
      normalized_cubeful: text('normalizedCubeful'),
      mwc: 'MWC',
    })[kind] ?? text('unknownUnit');
  const equity = (value: number | null | undefined, kind: string) =>
    typeof value === 'number' && Number.isFinite(value)
      ? kind === 'mwc'
        ? formatPercent(value, digits)
        : formatEquity(value, digits)
      : '—';
  return { unit, equity };
}
