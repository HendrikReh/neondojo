import { useId } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import { depthLabel } from '../../shared/engine/analysis-depth';
import { engineDisplayName } from '../../shared/engine/engine-version';
import { formatPly } from '../../shared/format/ply';
import { useI18n } from '../../shared/i18n/i18n';
import type { EngineInfo } from '../../settings/settings-api';
import {
  duelDepths,
  MAX_BLOCK_GAMES,
  unitLabel,
  validNumber,
  type Competitor,
  type SeriesDraft,
  type Text,
} from './engine-series';

function EngineChoice({
  side,
  value,
  engines,
  onChange,
  text,
  number,
}: {
  side: 'engineA' | 'engineB';
  value: Competitor;
  engines: EngineInfo[];
  onChange: (next: Competitor) => void;
  text: Text;
  number: (value: number) => string;
}) {
  const depths = duelDepths(value.engine_id, engines);
  return (
    <fieldset>
      <legend>{text(side)}</legend>
      <select
        aria-label={text(side)}
        value={value.engine_id}
        onChange={(event) => {
          const id = event.target.value;
          onChange({ engine_id: id, plies: 2 });
        }}
      >
        {!engines.some((engine) => engine.id === value.engine_id) && (
          <option value={value.engine_id} disabled>
            {engineDisplayName(value.engine_id)}
          </option>
        )}
        {engines.map((engine) => (
          <option
            key={engine.id}
            value={engine.id}
            disabled={!engine.available}
          >
            {engine.name}
            {!engine.available && ` · ${text('unavailable')}`}
          </option>
        ))}
      </select>
      <label>
        {text('strength')}
        <select
          value={value.plies}
          onChange={(event) =>
            onChange({
              ...value,
              plies: Number(event.target.value),
            })
          }
        >
          {!depths.includes(value.plies) && (
            <option value={value.plies} disabled>
              {formatPly(value.plies, number)} · {text('unavailable')}
            </option>
          )}
          {depths.map((depth) => (
            <option value={depth} key={depth}>
              {depthLabel(depth, value.engine_id)}
            </option>
          ))}
        </select>
      </label>
    </fieldset>
  );
}

/** The dialog that configures and starts a series. */
export default function EngineSeriesSetup({
  draft,
  setDraft,
  engines,
  engineError,
  retryEngines,
  missingRulesEngine,
  advancedOpen,
  setAdvancedOpen,
  error,
  canStart,
  busy,
  close,
  start,
  text,
}: {
  draft: SeriesDraft;
  setDraft: (draft: SeriesDraft) => void;
  engines: EngineInfo[] | undefined;
  engineError: unknown;
  retryEngines: () => void;
  missingRulesEngine: boolean;
  advancedOpen: boolean;
  setAdvancedOpen: (open: boolean) => void;
  error: unknown;
  canStart: boolean;
  busy: boolean;
  close: () => void;
  start: () => void;
  text: Text;
}) {
  const { formatNumber, formatError } = useI18n();
  const advancedBody = useId();
  const formatHint = useId();
  const { a, b, format, mode, length, blockGames, count, seed } = draft;
  const money = format === 'money_cubeless';
  const totalGames =
    validNumber(blockGames, MAX_BLOCK_GAMES) && validNumber(count, 1000)
      ? Number(blockGames) * Number(count) * (mode === 'paired' ? 2 : 1)
      : undefined;

  return (
    <LibraryDialog native title={text('newSeries')} onClose={close} busy={busy}>
      <form
        className="library-dialog-form engine-match-setup"
        onSubmit={(event) => {
          event.preventDefault();
          start();
        }}
      >
        <div className="engine-match-fields">
          <EngineChoice
            side="engineA"
            value={a}
            engines={engines ?? []}
            onChange={(value) => setDraft({ ...draft, a: value })}
            text={text}
            number={formatNumber}
          />
          <EngineChoice
            side="engineB"
            value={b}
            engines={engines ?? []}
            onChange={(value) => setDraft({ ...draft, b: value })}
            text={text}
            number={formatNumber}
          />
        </div>
        <div className="engine-match-rules">
          <label>
            {text('format')}
            <select
              value={format}
              aria-describedby={money ? formatHint : undefined}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  format:
                    event.target.value === 'money_cubeless'
                      ? 'money_cubeless'
                      : 'match',
                })
              }
            >
              <option value="match">{text('formatMatch')}</option>
              <option value="money_cubeless">{text('formatMoney')}</option>
            </select>
          </label>
          <label>
            {text('dice')}
            <select
              value={mode}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  mode: event.target.value === 'random' ? 'random' : 'paired',
                })
              }
            >
              <option value="paired">{text('paired')}</option>
              <option value="random">{text('random')}</option>
            </select>
          </label>
          {money ? (
            <label>
              {text('blockGames')}
              <input
                required
                type="number"
                min="1"
                max={MAX_BLOCK_GAMES}
                value={blockGames}
                onChange={(event) =>
                  setDraft({ ...draft, blockGames: event.target.value })
                }
              />
            </label>
          ) : (
            <label>
              {text('length')}
              <input
                required
                type="number"
                min="1"
                max="25"
                value={length}
                onChange={(event) =>
                  setDraft({ ...draft, length: event.target.value })
                }
              />
            </label>
          )}
          <label>
            {text(unitLabel(money, mode === 'paired'))}
            <input
              required
              type="number"
              min="1"
              max="1000"
              value={count}
              onChange={(event) =>
                setDraft({ ...draft, count: event.target.value })
              }
            />
          </label>
        </div>
        {money && (
          <p id={formatHint} className="engine-match-format-hint">
            {text('formatMoneyHint')}
            {totalGames !== undefined &&
              ` ${text('totalGames')}: ${formatNumber(totalGames)}`}
          </p>
        )}
        <details
          className="engine-match-advanced"
          open={advancedOpen}
          onToggle={(event) => setAdvancedOpen(event.currentTarget.open)}
        >
          <summary aria-controls={advancedBody}>{text('advanced')}</summary>
          <label id={advancedBody}>
            {text('optionalSeed')}
            <input
              value={seed}
              onChange={(event) =>
                setDraft({ ...draft, seed: event.target.value })
              }
              onInvalid={() => setAdvancedOpen(true)}
              spellCheck={false}
              pattern="[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
            />
          </label>
        </details>
        {engineError !== undefined && (
          <div className="error" role="alert">
            {formatError(engineError)}{' '}
            <ActionButton
              icon="refresh"
              label={text('retry')}
              compact
              type="button"
              onClick={retryEngines}
            />
          </div>
        )}
        {!engines && engineError === undefined && (
          <p role="status">{text('loading')}</p>
        )}
        {missingRulesEngine && <p role="status">{text('noEngines')}</p>}
        {error !== undefined && (
          <div className="error">
            <p role="alert">{text('startFailed')}</p>
            <details>
              <summary>{text('details')}</summary>
              <p>{formatError(error)}</p>
            </details>
          </div>
        )}
        <div className="library-dialog-actions">
          <ActionButton
            icon="play"
            type="submit"
            className="primary"
            label={text('start')}
            disabled={!canStart || busy}
          />
        </div>
      </form>
    </LibraryDialog>
  );
}
