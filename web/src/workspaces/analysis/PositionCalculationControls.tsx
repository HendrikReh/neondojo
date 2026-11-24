import ActionButton from '../../shared/ui/ActionButton';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import { useI18n } from '../../shared/i18n/i18n';
import {
  ANALYSIS_PLIES,
  DEFAULT_ANALYSIS_PLIES,
  depthLabel,
} from '../../shared/engine/analysis-depth';
import { useLibraryText } from '../library/library-messages';
import type { PositionCalculation } from './usePositionCalculation';

/** Recalculates the displayed position after asking for the depth. */
export default function PositionCalculationControls({
  calculation,
  onCalculationStarted,
}: {
  calculation: PositionCalculation;
  onCalculationStarted?: () => void;
}) {
  const { t } = useI18n();
  const library = useLibraryText();
  const { engine, settings, blocked, closeSettings } = calculation;
  return (
    <>
      {(['analysis', 'dice'] as const).map((kind) => (
        <ActionButton
          key={kind}
          type="button"
          icon="analysis"
          label={`${t(kind === 'dice' ? 'analysis.recalculateDice' : 'analysis.recalculate')} …`}
          aria-haspopup="dialog"
          disabled={!calculation.canOpen}
          onClick={(event) => {
            if (!calculation.canOpen) return;
            // The dialog returns the focus to this button.
            event.currentTarget.focus();
            calculation.openSettings(kind);
          }}
        />
      ))}
      {settings && (
        <LibraryDialog
          title={t(
            settings.kind === 'dice'
              ? 'analysis.recalculateDice'
              : 'analysis.recalculate',
          )}
          native
          initialFocus="dialog"
          onClose={closeSettings}
        >
          <p className="fineprint">
            {engine === 'bgsage' ? 'NeonGammon' : 'GNUbg'}
          </p>
          <div className="analysis-controls">
            <PositionAnalysisDepth
              plies={settings.plies}
              engine={engine}
              supportedPlies={calculation.supportedPlies}
              onChange={calculation.setPlies}
            />
            <ActionButton
              type="button"
              icon="analysis"
              label={t(
                settings.kind === 'dice'
                  ? 'analysis.diceEquity'
                  : 'analysis.analyze',
              )}
              className="primary"
              disabled={blocked}
              onClick={() => {
                calculation.start();
                onCalculationStarted?.();
              }}
            />
            <button type="button" onClick={closeSettings}>
              {library('cancel')}
            </button>
          </div>
        </LibraryDialog>
      )}
    </>
  );
}

export function PositionAnalysisDepth({
  plies = DEFAULT_ANALYSIS_PLIES,
  engine,
  supportedPlies,
  onChange,
}: {
  plies?: number;
  engine?: string;
  supportedPlies: readonly number[];
  onChange: (plies: number) => void;
}) {
  const { t } = useI18n();
  return (
    <label className="depth-label">
      {t('analysis.depth')}{' '}
      <select
        aria-label={t('analysis.depth')}
        value={plies}
        onChange={(event) => onChange(Number(event.target.value))}
      >
        {ANALYSIS_PLIES.map((ply) => {
          const depth = depthLabel(ply, engine);
          return (
            <option
              key={ply}
              value={ply}
              disabled={!supportedPlies.includes(ply)}
            >
              {ply === 1
                ? t('analysis.fast', { depth })
                : ply === DEFAULT_ANALYSIS_PLIES
                  ? t('analysis.accurate', { depth })
                  : depth}
            </option>
          );
        })}
      </select>
    </label>
  );
}
