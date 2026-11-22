import { useId } from 'react';
import type { Match } from '../../shared/api/api';
import { useSourceText } from './candidates/analysis-source-messages';
import { useBoardReviewText } from '../../shared/board/board-review-messages';
import CubeDecisionDetails, {
  CubeContextLine,
} from '../../shared/review/CubeDecisionDetails';
import EngineLabel from '../../shared/engine/EngineLabel';
import { useI18n } from '../../shared/i18n/i18n';
import type { ReviewDecision } from './review-decisions-api';

export default function CubeReviewEvaluation({
  decision,
  players,
}: {
  decision: ReviewDecision;
  players: Pick<Match, 'white' | 'black'>;
}) {
  const heading = useId();
  const description = useId();
  const source = useSourceText();
  const review = useBoardReviewText();
  const { t, formatDecision, formatEquity, formatPercent } = useI18n();
  const available = decision.evaluation_status === 'evaluated';
  const evaluation = decision.cube_evaluation;
  const player = decision.player === 0 ? players.white : players.black;
  const response = /^(take|drop|pass)$/i.test(decision.played);
  const mwc = evaluation?.unit === 'mwc';
  const options: ReadonlyArray<readonly [string, number | null | undefined]> =
    evaluation
      ? response
        ? ([
            [formatDecision('take'), evaluation.double_take],
            [formatDecision('drop'), evaluation.double_pass],
          ] as const)
        : ([
            [source('noDouble'), evaluation.no_double],
            [source('doubleTake'), evaluation.double_take],
            [source('doublePass'), evaluation.double_pass],
          ] as const)
      : ([
          [formatDecision(decision.played), decision.played_equity],
          ...(decision.best !== decision.played
            ? [[formatDecision(decision.best), decision.best_equity] as const]
            : []),
        ] as const);
  const values = (available ? options : []).filter(
    (option): option is readonly [string, number] =>
      typeof option[1] === 'number' && Number.isFinite(option[1]),
  );
  const equityLabel = mwc ? 'MWC' : t('common.equity');
  const names = [players.white, players.black] as const;
  const matchId = decision.review_position?.gnubg_match_id;

  return (
    <section className="cube-review-evaluation" aria-labelledby={heading}>
      <h3 id={heading}>{review('cubeEvaluation')}</h3>
      <p id={description}>{source('namedCubePerspective', { player })}</p>
      <p className="fineprint">
        {source(decision.source === 'imported' ? 'imported' : 'engine')}
        {' · '}
        <EngineLabel details={decision.engine} />
        {' · '}
        {decision.depth ?? source('unknownDepth')}
      </p>
      {matchId && (
        <CubeContextLine
          matchId={matchId}
          players={names}
          offeredBy={response ? 1 - decision.player : undefined}
        />
      )}
      {available && evaluation && (
        <CubeDecisionDetails
          matchId={matchId}
          player={decision.player}
          response={response}
          evaluation={evaluation}
          players={names}
          position={decision.review_position}
        />
      )}
      {!evaluation && values.length > 0 && (
        <table
          className="candidates"
          aria-labelledby={heading}
          aria-describedby={description}
        >
          <thead>
            <tr>
              <th scope="col">{review('cubeOption')}</th>
              <th scope="col">{equityLabel}</th>
            </tr>
          </thead>
          <tbody>
            {values.map(([label, value]) => (
              <tr key={label}>
                <th scope="row">{label}</th>
                <td>{mwc ? formatPercent(value, 3) : formatEquity(value)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {(!evaluation || values.length === 0) && (
        <p className="fineprint">{review('cubeOptionsUnavailable')}</p>
      )}
    </section>
  );
}
