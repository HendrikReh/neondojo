import type { components } from './generated-api';
import CubeDecisionDetails from './CubeDecisionDetails';
import MoveExplanation from './MoveExplanation';
import type { ExplainedPosition } from './move-explanation';
import EngineLabel from './EngineLabel';
import { useI18n } from './i18n';
import { useLearningText } from './learning-messages';

const verdictKeys = {
  best: 'bestAnswer',
  playable: 'verdictPlayable',
  error: 'verdictError',
  forced: 'verdictForced',
} as const;

export default function PracticeResult({
  evaluation: result,
  storedSource,
  cube,
  checker,
}: {
  /** Position of a checker question; explains a play that was not best. */
  checker?: ExplainedPosition;
  evaluation: components['schemas']['PracticeEvaluation'];
  storedSource?: string;
  /** Decision context of a cube question; adds all outcomes and the take point. */
  cube?: {
    matchId: string;
    player: number;
    response: boolean;
    players: readonly [string, string];
    /** Allows calculating chances that the saved analysis lacks. */
    position?: components['schemas']['Position'];
  };
}) {
  const t = useLearningText();
  const { formatDecision, formatEquity, formatNumber } = useI18n();
  const { provenance } = result;
  const source =
    provenance.kind === 'stored' && storedSource
      ? storedSource
      : [
          provenance.engine,
          provenance.plies == null
            ? null
            : `${formatNumber(provenance.plies)}-ply`,
        ]
          .filter(Boolean)
          .join(' · ');
  return (
    <>
      <p>
        <strong>{t(verdictKeys[result.verdict])}</strong>
      </p>
      <p>
        {t('chosenAnswer')}: {formatDecision(result.chosen)}
        {result.chosen_equity != null &&
          ` · ${formatEquity(result.chosen_equity)}`}
      </p>
      <p>
        {t('best')}: {formatDecision(result.best)}
        {result.best_equity != null && ` · ${formatEquity(result.best_equity)}`}
      </p>
      <p>
        {t('loss')}:{' '}
        {formatNumber(result.equity_loss, {
          minimumFractionDigits: 3,
          maximumFractionDigits: 6,
        })}
      </p>
      {cube && (
        <CubeDecisionDetails
          matchId={cube.matchId}
          player={cube.player}
          response={cube.response}
          evaluation={result.cube_evaluation}
          players={cube.players}
          position={cube.position}
        />
      )}
      {checker && result.verdict !== 'best' && result.verdict !== 'forced' && (
        <MoveExplanation
          position={checker}
          best={result.best}
          played={result.chosen}
        />
      )}
      {provenance.kind !== 'forced' && (
        <p>
          {t(
            provenance.kind === 'stored' ? 'sourceStored' : 'sourceRecomputed',
          )}
          {source && (
            <>
              {' · '}
              <EngineLabel
                details={provenance.engine ?? source}
                tooltip={source !== provenance.engine ? source : undefined}
              />
            </>
          )}
        </p>
      )}
    </>
  );
}
