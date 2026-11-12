import { useEffect, useState } from 'react';
import { useI18n } from './i18n';
import { loadAppVersion, loadEngines, type EngineInfo } from './settings-api';
import './version-info.css';

function CommitId({ revision }: { revision: string }) {
  return <code title={revision}>{revision.slice(0, 7)}</code>;
}

export default function VersionInfo() {
  const { t } = useI18n();
  const [appVersion, setAppVersion] = useState<string | null>();
  const [engines, setEngines] = useState<EngineInfo[] | null>();
  const [attempt, setAttempt] = useState(0);

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
    void loadEngines(abort.signal).then(
      (catalog) => {
        if (!abort.signal.aborted) setEngines(catalog);
      },
      () => {
        if (!abort.signal.aborted) setEngines(null);
      },
    );
    return () => abort.abort();
  }, [attempt]);

  function engineVersion(engine: EngineInfo | undefined) {
    if (engines === undefined) return undefined;
    return engine?.available ? engine.version : null;
  }

  const gnubg = engines?.find((entry) => entry.id === 'gnubg');
  const sage = engines?.find((entry) => entry.id === 'bgsage');
  const rows = [
    ['BG Tutor', appVersion],
    ['GNU Backgammon', engineVersion(gnubg)],
    ['Open Sage', engineVersion(sage), sage?.available ? sage.fork : null],
  ] as const;

  return (
    <section className="version-info" aria-labelledby="version-info-title">
      <h1 id="version-info-title">{t('versions.title')}</h1>
      <dl aria-live="polite">
        {rows.map(([name, version, fork]) => (
          <div key={name}>
            <dt>{name}</dt>
            <dd>
              {fork && (
                <strong className="version-info-fork">
                  {t('versions.openSageFork')} ·{' '}
                  <a
                    href={`https://github.com/${fork.repository}/commit/${fork.revision}`}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {fork.repository} <CommitId revision={fork.revision} />
                  </a>
                </strong>
              )}
              {version ? (
                name === 'BG Tutor' ? (
                  <CommitId revision={version} />
                ) : (
                  <code>{version}</code>
                )
              ) : (
                t(
                  version === undefined
                    ? 'versions.loading'
                    : 'versions.unavailable',
                )
              )}
            </dd>
          </div>
        ))}
      </dl>
      {(appVersion === null || engines === null) && (
        <>
          <p role="alert">{t('versions.error')}</p>
          <button
            type="button"
            onClick={() => {
              if (appVersion === null) setAppVersion(undefined);
              if (engines === null) setEngines(undefined);
              setAttempt((current) => current + 1);
            }}
          >
            {t('versions.retry')}
          </button>
        </>
      )}
    </section>
  );
}
