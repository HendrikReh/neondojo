import EngineLabel from '../../shared/engine/EngineLabel';
import { useI18n } from '../../shared/i18n/i18n';
import Tooltip from '../../shared/ui/Tooltip';
import { useSettingsText } from '../../settings/settings-messages';
import { defaultConfiguration } from '../../settings/configuration/configuration-model';
import type { PlaySession } from './play-api';
import { usePlayText } from './play-messages';

/** Score, match format, engines and turn status above the live match. */
export default function PlayMatchSummary({
  session,
  name,
  strength,
  status,
}: {
  session: PlaySession;
  name: (player: number) => string;
  strength: string | null;
  status: string;
}) {
  const t = usePlayText();
  const settingsT = useSettingsText();
  const { formatNumber, formatPoints, t: appT } = useI18n();
  const analysisEngine =
    session.engine_snapshots?.analysis.engine_version ??
    (session.engine_snapshots?.analysis.engine_id === 'bgsage'
      ? 'NeonGammon'
      : 'GNUbg');
  const rulesEngine =
    session.engine_snapshots?.rules.engine_version ??
    (session.engine.id === 'gnubg' ? session.engine.version : 'GNUbg');
  const analysisPlies = formatNumber(
    session.analysis_plies ?? defaultConfiguration.analysis.plies,
  );
  return (
    <div className="play-match-summary">
      <h2>
        {name(0)} {formatNumber(session.scores[0])} :{' '}
        {formatNumber(session.scores[1])} {name(1)}
      </h2>
      <p className="play-match-format">
        {session.match_length
          ? formatPoints(session.match_length)
          : appT('common.moneyGame')}{' '}
        · {t('game')} {formatNumber(session.game_no)}
      </p>
      <p className="play-engine play-match-engine">
        <span>
          {t('botEngine')} ·{' '}
          <EngineLabel
            details={session.engine.version}
            tooltip={[
              session.engine.version,
              strength,
              t(session.bearoff_direction === 'left' ? 'left' : 'right'),
            ].join(' · ')}
          />{' '}
          <span>{strength}</span>
        </span>
        <Tooltip
          as="span"
          className="engine-label-details"
          tabIndex={0}
          text={[
            `${settingsT('analysisEngine')} · ${analysisEngine} · ${analysisPlies}-ply`,
            `${t('rulesEngine')} · ${rulesEngine}`,
          ].join('\n')}
        >
          {t('engineDetails')}
        </Tooltip>
      </p>
      <p className="play-match-status" role="status">
        {status}
      </p>
    </div>
  );
}
