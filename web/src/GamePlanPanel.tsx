import Tooltip from './shared/ui/Tooltip';
import ActionButton from './shared/ui/ActionButton';
import type { GamePlans } from './game-plan-api';
import { useGamePlans } from './useGamePlans';
import { useGamePlanText } from './game-plan-messages';
import './game-plan.css';

export default function GamePlanPanel({
  board,
  white,
  black,
  active = true,
}: {
  board: readonly number[];
  white: string;
  black: string;
  active?: boolean;
}) {
  const { plans, loading, error, retry } = useGamePlans(board, active);
  return (
    <GamePlanPanelView
      white={white}
      black={black}
      plans={plans}
      loading={loading}
      error={error}
      onRetry={retry}
    />
  );
}

export function GamePlanPanelView({
  white,
  black,
  plans,
  loading,
  error,
  onRetry,
}: {
  white: string;
  black: string;
  plans?: Readonly<GamePlans>;
  loading: boolean;
  error?: unknown;
  onRetry: () => void;
}) {
  const l = useGamePlanText();
  return (
    <section
      className="game-plan-panel"
      aria-label={l('title')}
      aria-busy={loading}
    >
      <div className="game-plan-heading">
        <span>{l('title')}</span>
        <Tooltip
          as="span"
          className="game-plan-help"
          tabIndex={0}
          role="note"
          aria-label={l('title')}
          text={l('explanation')}
        >
          ⓘ
        </Tooltip>
      </div>
      {plans ? (
        <dl className="game-plan-players">
          {(['white', 'black'] as const).map((color) => {
            const name = color === 'white' ? white : black;
            return (
              <div
                key={color}
                data-game-plan={color}
                data-plan={plans.game_over ? 'finished' : plans[color]}
              >
                <dt title={`${l(color)} · ${name}`}>
                  <span>{l(color)}</span> · {name}
                </dt>
                <dd>{l(plans.game_over ? 'finished' : plans[color])}</dd>
              </div>
            );
          })}
        </dl>
      ) : error ? (
        <div className="game-plan-unavailable">
          <span role="status">{l('unavailable')}</span>
          <ActionButton
            icon="refresh"
            label={l('retry')}
            compact
            type="button"
            onClick={onRetry}
          />
        </div>
      ) : (
        <span className="game-plan-loading" role="status">
          {loading ? l('loading') : '—'}
        </span>
      )}
    </section>
  );
}
