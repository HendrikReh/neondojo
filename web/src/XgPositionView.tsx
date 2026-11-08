import type { XgPositionResult } from './source-analysis-api';
import { useI18n } from './i18n';
import { useSourceText } from './analysis-source-messages';
export default function XgPositionView({
  position,
}: {
  position?: XgPositionResult | null;
}) {
  const l = useSourceText(),
    { formatEquity, formatPercent, formatNumber, t } = useI18n();
  return (
    <div className="xg-position-reference">
      <p>{l('xgPositionHelp')}</p>
      {position ? (
        <>
          <p className="source-provenance">
            <strong>XG Opening Book · {position.book_version}</strong>
            <span>{l('onRollPerspective')}</span>
          </p>
          {position.evaluations.map((e) => (
            <article className="xg-book-evaluation" key={e.record_index}>
              <div className="source-provenance">
                <strong>
                  {e.engine} {e.engine_version}
                </strong>
                <span>
                  {e.depth} · {e.method}
                </span>
                <b>{formatEquity(e.equity)}</b>
                <small>{l('normalizedEquity')}</small>
              </div>
              <table className="xg-probabilities">
                <thead>
                  <tr>
                    <th>{t('analysis.win')}</th>
                    <th>G</th>
                    <th>BG</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    {e.probabilities.slice(0, 3).map((p, i) => (
                      <td key={i}>{formatPercent(p, 2)}</td>
                    ))}
                  </tr>
                </tbody>
              </table>
              {e.rollout && (
                <p className="fineprint">
                  {l('rolloutGames', { count: formatNumber(e.rollout.games) })}{' '}
                  · σ{' '}
                  {formatNumber(e.rollout.standard_deviation, {
                    maximumFractionDigits: 6,
                  })}
                </p>
              )}
              <p className="xg-book-author">
                <strong>{l('sourceAuthor')}:</strong>{' '}
                {e.source_author || l('authorUnknown')}
              </p>
            </article>
          ))}
          <p className="fineprint">
            <a
              href="https://www.extremegammon.com/GetOb.aspx"
              target="_blank"
              rel="noreferrer"
            >
              eXtreme Gammon Opening Book
            </a>{' '}
            · {position.license}
          </p>
        </>
      ) : (
        <p>{l('noXgPosition')}</p>
      )}
    </div>
  );
}
