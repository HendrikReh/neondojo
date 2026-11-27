import configurationOptions from '../../../shared/configuration-options.json' with { type: 'json' };
import EngineLabel from '../shared/engine/EngineLabel';
import ActionButton from '../shared/ui/ActionButton';
import OpenSageInstallation, {
  type useOpenSageInstallation,
} from './OpenSageInstallation';
import type { AppSettings, EngineInfo } from './settings-api';
import { useSettingsText } from './settings-messages';

export type CheckState =
  { path: string; version: string } | { path: string; error: unknown } | null;

function ParallelJobsField({
  id,
  value,
  onChange,
  t,
}: {
  id: string;
  value: number | null;
  onChange: (value: number | null) => void;
  t: ReturnType<typeof useSettingsText>;
}) {
  const {
    min,
    max,
    default: defaultJobs,
  } = configurationOptions.parallel_analysis_jobs;
  const retained = value != null && (value < min || value > max);
  const labelId = `${id}-parallel-jobs-label`;
  const helpId = `${id}-parallel-jobs-help`;
  return (
    <>
      <label className="settings-field settings-parallel-jobs">
        <span className="settings-thread-label">
          <span id={labelId}>{t('parallelAnalysisJobs')}</span>
          <span
            className={retained ? 'settings-help' : 'sr-only'}
            id={`${id}-parallel-range`}
          >
            {t('configurableThreadRange', { min, max })}
            {retained && <> {t('retainedThreadCount', { count: value })}</>}
          </span>
        </span>
        <select
          value={value ?? ''}
          aria-labelledby={labelId}
          aria-describedby={`${id}-parallel-range ${helpId}`}
          onChange={(event) =>
            onChange(
              event.target.value === '' ? null : Number(event.target.value),
            )
          }
        >
          <option value="">
            {t('defaultThreads', { count: defaultJobs })}
          </option>
          {retained && (
            <option value={value} disabled>
              {value}
            </option>
          )}
          {Array.from({ length: max - min + 1 }, (_, index) => min + index).map(
            (jobs) => (
              <option key={jobs} value={jobs}>
                {jobs}
              </option>
            ),
          )}
        </select>
      </label>
      <p className="settings-help" id={helpId}>
        {t('parallelAnalysisJobsHelp')}
      </p>
    </>
  );
}

/** Dedicated Engines category: local installations, thread limits and jobs. */
export default function EngineSettingsSection({
  id,
  draft,
  update,
  disabled,
  hidden,
  engines,
  engineCatalogRefresh,
  onRefreshEngines,
  check,
  checking,
  gnubgEdited,
  onCheck,
  onInstallationEdited,
  diagnostic,
  sage,
}: {
  id: string;
  draft: AppSettings;
  update: (values: Partial<AppSettings>) => void;
  disabled: boolean;
  hidden: boolean;
  engines: EngineInfo[];
  engineCatalogRefresh: { error?: unknown } | null;
  onRefreshEngines: () => void;
  check: CheckState;
  checking: boolean;
  gnubgEdited: boolean;
  onCheck: () => void;
  onInstallationEdited: () => void;
  diagnostic: (error: unknown) => string;
  sage: ReturnType<typeof useOpenSageInstallation>;
}) {
  const t = useSettingsText();
  const engineCatalogError = engineCatalogRefresh?.error;
  const engine = engines.find(({ id }) => id === 'gnubg');
  return (
    <fieldset
      className="settings-section settings-engines-section"
      disabled={disabled}
      hidden={hidden}
    >
      <legend>{t('localEngines')}</legend>
      <div className="settings-engines-intro">
        <p id={`${id}-local-help`} className="settings-help">
          {t('localEnginesHelp')}
        </p>
        <div className="settings-engine-summary">
          {(
            [
              ['gnubg', t('installation')],
              ['bgsage', t('planned')],
            ] as const
          ).map(([provider, label]) => {
            if (engineCatalogRefresh && provider === 'bgsage') return null;
            const available = engines.find(
              ({ id }) => id === provider,
            )?.available;
            return (
              <span
                key={provider}
                className={available ? 'settings-success' : undefined}
              >
                {label} · {t(available ? 'available' : 'unavailable')}
              </span>
            );
          })}
        </div>
      </div>
      {Boolean(engineCatalogError) && (
        <div className="settings-error" role="alert">
          <p>
            {t('engineStatusError')} {diagnostic(engineCatalogError)}
          </p>
          <ActionButton
            icon="refresh"
            label={t('retry')}
            type="button"
            onClick={onRefreshEngines}
          />
        </div>
      )}
      <div className="settings-engine-cards">
        <div className="settings-engine-card">
          <fieldset className="settings-section settings-engine">
            <legend>{t('installation')}</legend>
            <div className="settings-engine-path">
              <label className="settings-field" htmlFor={`${id}-path`}>
                <span>{t('path')}</span>
                <input
                  id={`${id}-path`}
                  required
                  value={draft.gnubg_path}
                  title={draft.gnubg_path}
                  spellCheck={false}
                  autoComplete="off"
                  aria-describedby={`${id}-local-help ${id}-check-status`}
                  aria-invalid={Boolean(check && 'error' in check)}
                  onChange={(event) => {
                    onInstallationEdited();
                    update({ gnubg_path: event.target.value });
                  }}
                />
              </label>
              <div className="settings-engine-actions">
                <ActionButton
                  icon="confirm"
                  label={t('check')}
                  type="button"
                  disabled={checking || !draft.gnubg_path.trim()}
                  onClick={onCheck}
                />
                <span className="settings-engine-save-badge settings-engine-save-badge--draft">
                  {t('engineSaveWithApply')}
                </span>
              </div>
            </div>
            <div className="settings-engine-status" id={`${id}-check-status`}>
              {engine?.version && !check && !gnubgEdited && (
                <p>
                  <EngineLabel
                    details={engine.version}
                    format={(version) => t('engineVersion', { version })}
                  />
                </p>
              )}
              {!engine?.available && <p>{t('unavailable')}</p>}
              {check && 'version' in check && (
                <p role="status" className="settings-success">
                  <EngineLabel
                    details={check.version}
                    format={(version) => t('checkSuccess', { version })}
                  />
                </p>
              )}
              {check && 'error' in check && (
                <p role="alert" className="settings-error">
                  {t('checkError')} {diagnostic(check.error)}
                </p>
              )}
              {!check && gnubgEdited && <p>{t('checkNeeded')}</p>}
              {checking && <p role="status">{t('checking')}</p>}
            </div>
            <p className="settings-help">{t('checkHelp')}</p>
          </fieldset>
        </div>
        <div className="settings-engine-card">
          <OpenSageInstallation
            setup={sage}
            engine={
              engineCatalogRefresh
                ? undefined
                : engines.find(({ id }) => id === 'bgsage')
            }
          />
        </div>
      </div>
      <section
        className="settings-play-group settings-engine-concurrency"
        role="group"
        aria-labelledby={`${id}-engine-threads`}
      >
        <h3 id={`${id}-engine-threads`}>{t('engineThreadsHeading')}</h3>
        <div className="settings-field-pair">
          {(
            [
              ['gnubg', 'gnubg_threads', 'gnubgAnalysisThreads', 'analysis'],
              ['gnubg', 'gnubg_play_threads', 'gnubgPlayThreads', 'play'],
              [
                'bgsage',
                'open_sage_threads',
                'sageAnalysisThreads',
                'analysis',
              ],
              ['bgsage', 'open_sage_play_threads', 'sagePlayThreads', 'play'],
            ] as const
          ).map(([provider, field, label, purpose]) => {
            const { min, default: defaults } =
              configurationOptions.engine_threads[provider];
            const defaultCount = defaults[purpose];
            const max = engines.find(
              (engine) => engine.id === provider,
            )?.max_parallel_threads;
            const count = draft[field];
            const validRange = max != null && max >= min;
            const retained =
              count != null && (!validRange || count < min || count > max);
            const labelId = `${id}-${field}-label`;
            const limitsId = `${id}-${field}-limits`;
            return (
              <label className="settings-field" key={field}>
                <span className="settings-thread-label">
                  <span id={labelId}>{t(label)}</span>
                  <span
                    className={
                      validRange && !retained ? 'sr-only' : 'settings-help'
                    }
                    id={limitsId}
                  >
                    {validRange
                      ? t('configurableThreadRange', { min, max })
                      : t('threadRangeUnavailable')}
                    {retained && <> {t('retainedThreadCount', { count })}</>}
                  </span>
                </span>
                <select
                  value={count ?? ''}
                  disabled={!validRange && count == null}
                  aria-labelledby={labelId}
                  aria-describedby={`${limitsId} ${id}-threads-help`}
                  onChange={(event) => {
                    if (provider === 'gnubg') onInstallationEdited();
                    const value = event.target.value;
                    update({
                      [field]: value === '' ? null : Number(value),
                    });
                  }}
                >
                  <option value="">
                    {t('defaultThreads', { count: defaultCount })}
                  </option>
                  {retained && (
                    <option value={count} disabled>
                      {count}
                    </option>
                  )}
                  {validRange &&
                    Array.from(
                      { length: max - min + 1 },
                      (_, index) => min + index,
                    ).map((threads) => (
                      <option key={threads} value={threads}>
                        {threads}
                      </option>
                    ))}
                </select>
              </label>
            );
          })}
        </div>
        <p className="settings-help" id={`${id}-threads-help`}>
          {t('threadsHelp')}
        </p>
        <ParallelJobsField
          id={id}
          value={draft.parallel_analysis_jobs ?? null}
          onChange={(parallel_analysis_jobs) =>
            update({ parallel_analysis_jobs })
          }
          t={t}
        />
      </section>
    </fieldset>
  );
}
