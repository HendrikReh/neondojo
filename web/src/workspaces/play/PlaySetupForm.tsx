import type { FormEvent, ReactNode } from 'react';
import { useId } from 'react';
import { engineDisplayName } from '../../shared/engine/engine-version';
import { TOP_TWO_PLAY_STRENGTH } from '../../shared/engine/analysis-depth';
import ActionButton from '../../shared/ui/ActionButton';
import { formatPly } from '../../shared/format/ply';
import { useI18n } from '../../shared/i18n/i18n';
import {
  playStrengthLabel,
  useSettingsText,
} from '../../settings/settings-messages';
import type { AppSettings, EngineInfo } from '../../settings/settings-api';
import { defaultConfiguration } from '../../settings/configuration/configuration-model';
import { useLibraryText } from '../library/library-messages';
import type { PlaySession, SageDifficulty } from './play-api';
import { usePlayText } from './play-messages';
import { playModes, usePlayModeText } from './play-mode-messages';
import type { PlaySetupChoices } from './usePlaySetupOptions';

// Five-point PRs rounded from the shared 6,912-decision rollout benchmark.
const sageDifficulties = [
  { id: 'beginner', pr: 20.3, plies: 1 },
  { id: 'casual', pr: 15.8, plies: 1 },
  { id: 'intermediate', pr: 9.3, plies: 1 },
  { id: 'full_strength', pr: 1, plies: 3 },
] as const satisfies ReadonlyArray<{
  id: SageDifficulty;
  pr: number;
  plies: number;
}>;

/** The new-match form inside the setup dialog. */
export default function PlaySetupForm({
  choices,
  engines,
  errorNotice,
  busy,
  disabled,
  uiBusy,
  settings,
  session,
  onSubmit,
  onResume,
  onOpenPausedMatches,
  onOpenPlaySettings,
}: {
  choices: PlaySetupChoices;
  engines: EngineInfo[];
  errorNotice: ReactNode;
  busy: boolean;
  disabled: boolean;
  uiBusy: boolean;
  settings?: AppSettings;
  session: PlaySession | null;
  onSubmit: (event: FormEvent) => void;
  onResume: () => void;
  onOpenPausedMatches: () => void;
  onOpenPlaySettings: () => void;
}) {
  const t = usePlayText();
  const modeT = usePlayModeText();
  const settingsT = useSettingsText();
  const libraryT = useLibraryText();
  const { formatNumber, formatPoints, t: appT } = useI18n();
  const setupId = useId();
  const {
    options: setupOptions,
    setOptions: onChange,
    confirmed,
    human,
    length,
    mode,
    threshold,
    automatic,
    playerName,
    botEngine,
    botDepths,
    botDifficulty,
    botPlies,
    botTopTwo,
  } = choices;
  const sageInfo = engines.find((engine) => engine.id === 'bgsage');
  return (
    <div className="play-setup">
      {errorNotice}
      {busy && <p role="status">{t('thinking')}</p>}
      <form onSubmit={onSubmit}>
        <label>
          {t('playerName')}
          <input
            type="text"
            value={playerName}
            maxLength={120}
            autoComplete="off"
            spellCheck={false}
            disabled={disabled}
            onChange={(event) =>
              onChange({
                ...setupOptions,
                name: event.target.value,
              })
            }
          />
        </label>
        <div className="play-setup-bot">
          <label>
            {t('botEngine')}
            <select
              value={botEngine}
              disabled={disabled}
              onChange={(event) => {
                const value = event.target.value;
                if (value === 'gnubg' || value === 'bgsage')
                  onChange({
                    ...setupOptions,
                    engine: value,
                    plies: undefined,
                    topTwo: false,
                    difficulty: undefined,
                  });
              }}
            >
              {(['gnubg', 'bgsage'] as const).map((id) => {
                const available =
                  engines.find((item) => item.id === id)?.available ?? true;
                return (
                  <option
                    key={id}
                    value={id}
                    disabled={!available && id !== botEngine}
                  >
                    {engineDisplayName(id)}
                    {available ? '' : ` — ${settingsT('unavailable')}`}
                  </option>
                );
              })}
            </select>
          </label>
          <label>
            {t('strength')}
            <select
              value={
                botDifficulty ?? (botTopTwo ? TOP_TWO_PLAY_STRENGTH : botPlies)
              }
              disabled={disabled}
              onChange={(event) => {
                const value = event.target.value;
                const difficulty = sageDifficulties.find(
                  ({ id }) => id === value,
                );
                const topTwo = value === TOP_TWO_PLAY_STRENGTH;
                onChange({
                  ...setupOptions,
                  engine: botEngine,
                  plies: difficulty ? undefined : topTwo ? 2 : Number(value),
                  topTwo,
                  difficulty: difficulty?.id,
                });
              }}
            >
              {botEngine === 'bgsage' && (
                <optgroup
                  label={t(
                    length === 5 ? 'difficultyFivePoint' : 'difficultyPresets',
                  )}
                >
                  {sageDifficulties.map(({ id, pr, plies }) => (
                    <option
                      key={id}
                      value={id}
                      disabled={
                        !!sageInfo &&
                        (!sageInfo.available ||
                          sageInfo.difficulty_presets === false ||
                          !sageInfo.plies.includes(plies))
                      }
                    >
                      {t(id)}
                      {id === 'full_strength' && ` · ${t('fullStrengthDepth')}`}
                      {length === 5 &&
                        ` · ≈ ${formatNumber(pr, { minimumFractionDigits: 1 })} PR`}
                    </option>
                  ))}
                </optgroup>
              )}
              {botDepths.map((ply) => (
                <option key={ply} value={ply}>
                  {playStrengthLabel(settingsT, ply, botEngine)}
                </option>
              ))}
              {botEngine === 'bgsage' && botDepths.includes(2) && (
                <option value={TOP_TWO_PLAY_STRENGTH}>
                  {playStrengthLabel(settingsT, 2, botEngine, true)}
                </option>
              )}
            </select>
          </label>
        </div>
        <p className="play-setup-help">{t('matchChoicesHelp')}</p>
        <fieldset className="play-choice">
          <legend>{t('humanColor')}</legend>
          {([0, 1] as const).map((value) => (
            <label key={value}>
              <input
                type="radio"
                name={`${setupId}-color`}
                checked={human === value}
                disabled={disabled}
                onChange={() => onChange({ ...setupOptions, human: value })}
              />
              {t(value === 0 ? 'white' : 'black')}
            </label>
          ))}
        </fieldset>
        <label>
          {t('length')}
          <select
            value={length}
            onChange={(event) =>
              onChange({
                ...setupOptions,
                length: Number(event.target.value),
              })
            }
            disabled={disabled}
          >
            <option value="0">{appT('common.moneyGame')}</option>
            {[1, 3, 5, 7, 9, 11, 15, 21, 25].map((n) => (
              <option key={n} value={n}>
                {formatPoints(n)}
              </option>
            ))}
          </select>
        </label>
        <fieldset className="play-choice play-mode-choice">
          <legend>{modeT('mode')}</legend>
          {playModes.map((value) => (
            <label key={value}>
              <input
                type="radio"
                name={`${setupId}-mode`}
                aria-label={modeT(value)}
                aria-describedby={`${setupId}-${value}`}
                checked={mode === value}
                disabled={disabled}
                onChange={() => onChange({ ...setupOptions, mode: value })}
              />
              <span>
                <strong>{modeT(value)}</strong>
                <small id={`${setupId}-${value}`}>
                  {modeT(`${value}Help`)}
                </small>
              </span>
            </label>
          ))}
        </fieldset>
        {mode === 'tutor' && (
          <p>
            {modeT('threshold')} ·{' '}
            {formatNumber(threshold, {
              minimumFractionDigits: 3,
              maximumFractionDigits: 6,
            })}
          </p>
        )}
        {mode === 'training' && (
          <label className="play-mode-checkbox">
            <input
              type="checkbox"
              checked={automatic}
              onChange={(event) =>
                onChange({
                  ...setupOptions,
                  automatic: event.target.checked,
                })
              }
              disabled={disabled}
            />
            {modeT('automatic')}
          </label>
        )}
        <p className="play-engine play-setup-engine">
          <span>
            {settingsT('analysisEngine')} ·{' '}
            {(confirmed?.analysis.engine ?? settings?.analysis_engine) ===
            'bgsage'
              ? 'NeonGammon'
              : 'GNUbg'}{' '}
            ·{' '}
            {formatPly(
              confirmed?.analysis.plies ??
                settings?.analysis_plies ??
                defaultConfiguration.analysis.plies,
            )}
            {' / '}
            {t('rulesEngine')} · GNUbg
          </span>
          <ActionButton
            type="button"
            compact
            icon="settings"
            label={t('changeEngine')}
            disabled={disabled}
            onClick={onOpenPlaySettings}
          />
        </p>
        <div className="play-actions">
          <ActionButton
            type="submit"
            className="primary"
            disabled={disabled}
            icon="play"
            label={t('start')}
          >
            {t('start')}
          </ActionButton>
          {session && (
            <ActionButton
              type="button"
              onClick={onResume}
              disabled={uiBusy}
              icon={session.paused ? 'resume' : 'back'}
              label={t(session.paused ? 'resumeMatch' : 'backToMatch')}
            >
              {t(session.paused ? 'resumeMatch' : 'backToMatch')}
            </ActionButton>
          )}
          <ActionButton
            type="button"
            onClick={onOpenPausedMatches}
            disabled={uiBusy}
            icon="open"
            label={libraryT('pausedMatches')}
          >
            {libraryT('pausedMatches')}
          </ActionButton>
        </div>
      </form>
    </div>
  );
}
