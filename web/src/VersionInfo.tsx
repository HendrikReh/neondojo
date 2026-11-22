import { useEffect, useState } from 'react';
import { shortEngineVersion } from './shared/engine/engine-version';
import EngineLabel from './shared/engine/EngineLabel';
import { useI18n } from './shared/i18n/i18n';
import {
  loadAppVersion,
  loadEngines,
  type AppVersion,
  type EngineInfo,
} from './settings-api';
import { versionInfoMessages } from './version-info-messages';
import './version-info.css';

const engineNames = {
  gnubg: 'GNU Backgammon',
  bgsage: 'NeonGammon',
} as const;

type EngineId = keyof typeof engineNames;
const engineIds: EngineId[] = ['gnubg', 'bgsage'];

export default function VersionInfo() {
  const { locale, t } = useI18n();
  const messages = versionInfoMessages[locale];
  const [appVersion, setAppVersion] = useState<AppVersion | null>();
  const [engines, setEngines] = useState<EngineInfo[] | null>();
  const [versionAttempt, setVersionAttempt] = useState(0);
  const [enginesAttempt, setEnginesAttempt] = useState(0);
  const [copyResult, setCopyResult] = useState<
    'copying' | 'copied' | 'copyError'
  >();

  useEffect(() => {
    const abort = new AbortController();
    void loadAppVersion(abort.signal).then(
      (version) => {
        if (!abort.signal.aborted) setAppVersion(version);
      },
      () => {
        if (!abort.signal.aborted) setAppVersion(null);
      },
    );
    return () => abort.abort();
  }, [versionAttempt]);

  useEffect(() => {
    const abort = new AbortController();
    void loadEngines(abort.signal).then(
      (catalog) => {
        if (!abort.signal.aborted) setEngines(catalog);
      },
      () => {
        if (!abort.signal.aborted) setEngines(null);
      },
    );
    return () => abort.abort();
  }, [enginesAttempt]);

  const loading = appVersion === undefined || engines === undefined;
  const unavailable = t('versions.unavailable');
  const engineRows = engineIds.map((id) => {
    const engine = engines?.find((entry) => entry.id === id);
    const available = engine?.available === true;
    const version = available ? engine.version : null;
    const details = version?.trim() ? version : null;
    return {
      name: engineNames[id],
      available,
      details,
      shortVersion: shortEngineVersion(id, details),
      fork: available && id === 'bgsage' ? engine.fork : null,
    };
  });
  const fork = engineRows.find((row) => row.fork)?.fork;
  const forkDetails = fork && {
    ...fork,
    url: `https://github.com/${fork.repository}/commit/${fork.revision}`,
  };
  const appCommit = appVersion?.commit || null;
  const appLabel =
    appVersion &&
    (appCommit
      ? `${appVersion.release} (${appCommit.slice(0, 7)})`
      : appVersion.release);
  const technicalDetails = [
    { name: messages.appVersion, value: appVersion?.release },
    { name: messages.appCommit, value: appCommit },
    ...engineRows.map((row) => ({ name: row.name, value: row.details })),
  ].filter(({ value }) => Boolean(value));

  async function copyVersionInfo() {
    if (copyResult === 'copying') return;
    const lines = [
      `${messages.appVersion}: ${appVersion?.release || unavailable}`,
      `${messages.appCommit}: ${appCommit || unavailable}`,
      ...engineRows.map(
        ({ name, available, details }) =>
          `${name}: ${details ?? (available ? messages.available : unavailable)}`,
      ),
    ];
    if (forkDetails)
      lines.push(`${t('versions.openSageFork')}: ${forkDetails.url}`);
    setCopyResult('copying');
    try {
      await navigator.clipboard.writeText(lines.join('\n'));
      setCopyResult('copied');
    } catch {
      setCopyResult('copyError');
    }
  }

  return (
    <section className="version-info" aria-labelledby="version-info-title">
      <h1 id="version-info-title" className="sr-only">
        {t('versions.title')}
      </h1>
      <dl className="version-info-summary" aria-live="polite">
        <div>
          <dt>NeonDojo</dt>
          <dd>
            {appLabel ? (
              <code title={appCommit ?? undefined}>{appLabel}</code>
            ) : (
              <span className="version-info-status">
                {appVersion === undefined ? t('versions.loading') : unavailable}
              </span>
            )}
          </dd>
        </div>
        {engineRows.map(({ name, shortVersion, available, details }) => (
          <div key={name}>
            <dt>{name}</dt>
            <dd>
              {shortVersion && (
                <code>
                  <EngineLabel details={details} format={() => shortVersion} />
                </code>
              )}
              <span
                className={
                  available
                    ? 'version-info-status version-info-status--available'
                    : 'version-info-status'
                }
              >
                {engines === undefined
                  ? t('versions.loading')
                  : available
                    ? messages.available
                    : unavailable}
              </span>
            </dd>
          </div>
        ))}
      </dl>
      {(technicalDetails.length > 0 || forkDetails) && (
        <details className="version-info-details">
          <summary>{messages.details}</summary>
          <dl>
            {technicalDetails.map(({ name, value }) => (
              <div key={name}>
                <dt>{name}</dt>
                <dd>
                  <code>{value}</code>
                </dd>
              </div>
            ))}
            {forkDetails && (
              <div>
                <dt>{t('versions.openSageFork')}</dt>
                <dd>
                  <a href={forkDetails.url} target="_blank" rel="noreferrer">
                    {forkDetails.repository} <code>{forkDetails.revision}</code>
                  </a>
                </dd>
              </div>
            )}
          </dl>
        </details>
      )}
      <div className="version-info-actions">
        <button
          type="button"
          disabled={loading}
          aria-disabled={copyResult === 'copying' || undefined}
          onClick={() => void copyVersionInfo()}
        >
          {messages.copy}
        </button>
        <p
          role="status"
          className={
            copyResult === 'copyError' ? 'version-info-copy-error' : undefined
          }
        >
          {copyResult && copyResult !== 'copying' ? messages[copyResult] : ''}
        </p>
      </div>
      {(appVersion === null || engines === null) && (
        <div className="error">
          <p role="alert">{t('versions.error')}</p>
          <button
            type="button"
            onClick={() => {
              if (appVersion === null) {
                setAppVersion(undefined);
                setVersionAttempt((current) => current + 1);
              }
              if (engines === null) {
                setEngines(undefined);
                setEnginesAttempt((current) => current + 1);
              }
            }}
          >
            {t('versions.retry')}
          </button>
        </div>
      )}
    </section>
  );
}
