import { matchContext } from '../board/boardMath';
import { useSourceText } from '../../workspaces/analysis/candidates/analysis-source-messages';
import { useEffect, useRef, useState } from 'react';
import ActionButton from '../ui/ActionButton';
import {
  calculateCubeChances,
  properCubeOutcome,
  tooGoodToDouble,
  type CubeChances,
  type CubeEvaluation,
  type CubeOutcome,
  type CubePosition,
} from './cube-context-api';
import { useCubeContextText } from './cube-context-messages';
import EngineLabel from '../engine/EngineLabel';
import { useI18n } from '../i18n/i18n';
import Tooltip from '../ui/Tooltip';
import { useCubeContext } from './useCubeContext';
import './cube-decision.css';

type Players = readonly [string, string];

/** Score as points still needed, cube value and owner, in words. */
export function CubeContextLine({
  matchId,
  players,
  offeredBy,
  withFormat = true,
}: {
  matchId: string;
  players: Players;
  /** False where the match length or money rules are already shown nearby. */
  withFormat?: boolean;
  /** Absolute color of the player whose double is pending. */
  offeredBy?: number;
}) {
  const text = useCubeContextText();
  const { t } = useI18n();
  const context = matchContext(matchId);
  const parts: string[] = [];
  if (context.length) {
    if (withFormat) parts.push(t('app.matchLength', { count: context.length }));
    const away = context.scores.map((score) => context.length - score);
    if (context.crawford) parts.push(text('crawford'));
    else if (away.includes(1)) parts.push(text('postCrawford'));
  } else if (withFormat) {
    parts.push(
      `${t('common.moneyGame')} · ${text(context.jacoby ? 'jacobyOn' : 'jacobyOff')}`,
    );
  }
  const cube =
    offeredBy != null
      ? text('cubeOffered', {
          player: players[offeredBy],
          value: context.cube * 2,
        })
      : context.owner == null
        ? text('cubeCentered', { value: context.cube })
        : text('cubeOwned', {
            value: context.cube,
            player: players[context.owner],
          });
  return (
    <p className="cube-context-line" data-cube-context>
      {parts.join(' · ')}
      {context.length > 0 && (
        <>
          {parts.length > 0 && ' · '}
          <Tooltip as="span" text={text('awayHint')} tabIndex={0}>
            {context.scores
              .map((score, player) =>
                text('away', {
                  player: players[player],
                  count: context.length - score,
                }),
              )
              .join(', ')}
          </Tooltip>
        </>
      )}
      {(parts.length > 0 || context.length > 0) && ' · '}
      {cube}
    </p>
  );
}

const outcomes: readonly CubeOutcome[] = [
  'no_double',
  'double_take',
  'double_pass',
];

/**
 * All cube outcomes with their unit, the cubeless chances behind them and the
 * take point at this score. Values belong to `player`, the decision maker.
 */
export default function CubeDecisionDetails({
  matchId,
  player,
  response,
  evaluation,
  players,
  position,
}: {
  /** The decision's position; allows calculating chances that were not saved. */
  position?: CubePosition | null;
  matchId?: string;
  /** Absolute color of the player who decides. */
  player: number;
  /** True for take/pass, where the opponent doubled. */
  response: boolean;
  evaluation?: CubeEvaluation | null;
  players: Players;
}) {
  const text = useCubeContextText();
  const source = useSourceText();
  const { formatDecision, formatEquity, formatPercent, formatError } =
    useI18n();
  // Chances calculated on request belong to one position only.
  const positionKey = position
    ? `${position.position_id}:${position.gnubg_match_id}:${response}`
    : '';
  const [calculated, setCalculated] = useState<{
    key: string;
    state: 'loading' | CubeChances | { error: unknown };
  }>();
  const pending = useRef<AbortController | null>(null);
  useEffect(() => () => pending.current?.abort(), [positionKey]);
  const fresh = calculated?.key === positionKey ? calculated.state : undefined;
  const freshChances =
    fresh && fresh !== 'loading' && 'probabilities' in fresh
      ? fresh
      : undefined;
  function calculate() {
    if (!position) return;
    pending.current?.abort();
    const abort = new AbortController();
    pending.current = abort;
    setCalculated({ key: positionKey, state: 'loading' });
    void calculateCubeChances(position, response, abort.signal)
      .then((result) => {
        if (!abort.signal.aborted)
          setCalculated({ key: positionKey, state: result });
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted)
          setCalculated({ key: positionKey, state: { error } });
      });
  }
  const doubler = response ? 1 - player : player;
  const { context } = useCubeContext(matchId, doubler);
  const mwc = evaluation?.unit === 'mwc';
  const value = (number: number) =>
    mwc ? formatPercent(number, 2) : formatEquity(number);
  // A responder chooses between taking and passing; the roll-on value
  // belongs to the doubler's earlier decision.
  const labels: Record<CubeOutcome, string> = response
    ? {
        no_double: source('noDouble'),
        double_take: formatDecision('take'),
        double_pass: formatDecision('drop'),
      }
    : {
        no_double: source('noDouble'),
        double_take: source('doubleTake'),
        double_pass: source('doublePass'),
      };
  const rows = response ? outcomes.slice(1) : outcomes;
  const best = !evaluation
    ? undefined
    : !response
      ? properCubeOutcome(evaluation, false)
      : evaluation.double_take >= evaluation.double_pass
        ? 'double_take'
        : 'double_pass';
  const chances = evaluation?.probabilities ?? freshChances?.probabilities;
  // The taker is the opponent of the doubler; saved chances belong to `player`.
  const takerWins = chances ? (response ? chances[0] : chances[3]) : undefined;
  const takePoint = context?.take_point;
  return (
    <div className="cube-decision-details">
      {evaluation && (
        <table className="candidates cube-outcomes">
          <caption>
            {text('outcomes')} · {text(mwc ? 'unitMwc' : 'unitCubeful')} ·{' '}
            {text('perspective', { player: players[player] })}
          </caption>
          <tbody>
            {rows.map((outcome) => (
              <tr key={outcome} data-proper={outcome === best || undefined}>
                <th scope="row">
                  {labels[outcome]}
                  {outcome === best && (
                    <span className="cube-proper"> · {text('best')}</span>
                  )}
                </th>
                <td>{value(evaluation[outcome])}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {evaluation && !response && tooGoodToDouble(evaluation, false) && (
        <p className="cube-too-good">{text('tooGood')}</p>
      )}
      {evaluation &&
        (chances ? (
          <table className="candidates cube-chances">
            <caption>{text('chances')}</caption>
            <thead>
              <tr>
                <td />
                <th scope="col">{text('win')}</th>
                <th scope="col">{text('gammon')}</th>
                <th scope="col">{text('backgammon')}</th>
              </tr>
            </thead>
            <tbody>
              {[player, 1 - player].map((side, index) => (
                <tr key={side}>
                  <th scope="row">{players[side]}</th>
                  {[0, 1, 2].map((kind) => (
                    <td key={kind}>
                      {formatPercent(chances[index * 3 + kind], 1)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        ) : position ? (
          <div className="cube-chances-request">
            {fresh === 'loading' ? (
              <p role="status">{text('calculatingChances')}</p>
            ) : (
              <ActionButton
                type="button"
                icon="analysis"
                label={text('calculateChances')}
                onClick={calculate}
              >
                {text('calculateChances')}
              </ActionButton>
            )}
            {fresh && fresh !== 'loading' && 'error' in fresh && (
              <p role="alert">{formatError(fresh.error)}</p>
            )}
          </div>
        ) : (
          <p className="fineprint">{text('chancesUnavailable')}</p>
        ))}
      {evaluation && !evaluation.probabilities && freshChances && (
        <p className="fineprint" data-cube-chances-source>
          <EngineLabel
            details={freshChances.engine}
            format={(engine) =>
              text('calculatedChances', {
                engine,
                plies: freshChances.plies,
              })
            }
          />
        </p>
      )}
      {context && (
        <p className="cube-take-point" data-take-point>
          {takePoint
            ? text(
                takePoint.basis === 'money'
                  ? 'takePointMoney'
                  : 'takePointMatch',
                {
                  value: formatPercent(takePoint.value, 1),
                  table: takePoint.table ?? '',
                },
              )
            : text('takePointUnavailable')}
          {takePoint && takerWins != null && (
            <>
              {' '}
              {text('takerChance', {
                player: players[1 - doubler],
                value: formatPercent(takerWins, 1),
              })}
            </>
          )}
        </p>
      )}
    </div>
  );
}
