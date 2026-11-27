import type { ReactNode, RefObject } from 'react';
import ActionButton from '../shared/ui/ActionButton';
import { ANALYSIS_PLIES } from '../shared/engine/analysis-depth';
import { useI18n } from '../shared/i18n/i18n';
import { errorThresholdLabels } from '../shared/review/review-statistics-messages';
import { useBenchmarkText } from '../workspaces/extras/benchmark-messages';
import {
  openingBookProviderLabel,
  useSourceText,
} from '../workspaces/analysis/candidates/analysis-source-messages';
import {
  errorThresholdFields,
  type ErrorThresholds,
  type ProfileConfiguration,
} from './configuration/configuration-model';
import type { AppSettings } from './settings-api';
import { depthOptionLabel, useSettingsText } from './settings-messages';

export default function AnalysisSettingsSection({
  id,
  disabled,
  hidden,
  draft,
  analysisPlies,
  engineField,
  hasConfiguration,
  openingBookProvider,
  thresholdFields,
  onThresholdChange,
  onUpdate,
  bookTriggerRef,
  onManageBooks,
  onBenchmark,
}: {
  id: string;
  disabled: boolean;
  hidden: boolean;
  draft: AppSettings;
  analysisPlies: number[];
  engineField: ReactNode;
  hasConfiguration: boolean;
  openingBookProvider?: ProfileConfiguration['opening_book']['provider'];
  thresholdFields: Record<keyof ErrorThresholds, string>;
  onThresholdChange: (field: keyof ErrorThresholds, value: string) => void;
  onUpdate: (values: Partial<AppSettings>) => void;
  bookTriggerRef: RefObject<HTMLButtonElement | null>;
  onManageBooks: () => void;
  onBenchmark?: () => void;
}) {
  const { locale, t: appT } = useI18n();
  const t = useSettingsText();
  const sourceT = useSourceText();
  const benchmarkT = useBenchmarkText();

  return (
    <fieldset className="settings-section" disabled={disabled} hidden={hidden}>
      <legend>{t('analysis')}</legend>
      <div className="settings-field-pair">
        {engineField}
        <label className="settings-field" htmlFor={`${id}-analysis-profile`}>
          <span>{t('analysisProfile')}</span>
          <select
            id={`${id}-analysis-profile`}
            aria-label={t('analysisProfile')}
            value={draft.analysis_plies}
            onChange={(event) =>
              onUpdate({ analysis_plies: Number(event.target.value) })
            }
          >
            {ANALYSIS_PLIES.map((ply) => (
              <option
                key={ply}
                value={ply}
                disabled={!analysisPlies.includes(ply)}
              >
                {depthOptionLabel(t, ply, draft.analysis_engine, [
                  'fast',
                  'deep',
                  'thorough',
                ])}
              </option>
            ))}
          </select>
        </label>
      </div>
      <p className="settings-help">{t('analysisHelp')}</p>
      {hasConfiguration && (
        <div
          className="settings-thresholds"
          role="group"
          aria-labelledby={`${id}-thresholds`}
        >
          <h3 id={`${id}-thresholds`}>{t('errorThresholds')}</h3>
          <div className="settings-field-pair">
            {errorThresholdFields.map((field) => (
              <label className="settings-field" key={field}>
                <span>{appT(errorThresholdLabels[field])}</span>
                <input
                  inputMode="decimal"
                  aria-describedby={`${id}-thresholds-help`}
                  value={thresholdFields[field]}
                  onChange={(event) =>
                    onThresholdChange(field, event.target.value)
                  }
                />
              </label>
            ))}
          </div>
          <p className="settings-help" id={`${id}-thresholds-help`}>
            {t('errorThresholdsHelp')}
          </p>
        </div>
      )}
      <div className="settings-analysis-row">
        <div>
          <h3>{sourceT('books')}</h3>
          {openingBookProvider && (
            <p className="settings-help">
              {openingBookProviderLabel(locale, openingBookProvider)}
            </p>
          )}
        </div>
        <ActionButton
          ref={bookTriggerRef}
          icon="open"
          label={t('manageBooks')}
          type="button"
          onClick={onManageBooks}
        />
      </div>
      {onBenchmark && (
        <div className="settings-analysis-row">
          <h3>{benchmarkT('benchmark')}</h3>
          <ActionButton
            icon="open"
            label={benchmarkT('open')}
            type="button"
            onClick={onBenchmark}
          />
        </div>
      )}
    </fieldset>
  );
}
