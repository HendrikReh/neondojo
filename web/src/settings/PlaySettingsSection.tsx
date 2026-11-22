import type { ReactNode } from 'react';
import configurationOptions from '../../../shared/configuration-options.json' with { type: 'json' };
import {
  PLAY_PLIES,
  TOP_TWO_PLAY_STRENGTH,
} from '../shared/engine/analysis-depth';
import { usePlayText } from '../workspaces/play/play-messages';
import {
  playModes,
  usePlayModeText,
  type PlayMode,
} from '../workspaces/play/play-mode-messages';
import type { AppSettings } from './settings-api';
import { playStrengthLabel, useSettingsText } from './settings-messages';

/** The Play category: engine and strength, feedback and preferences. */
export default function PlaySettingsSection({
  id,
  draft,
  update,
  disabled,
  hidden,
  engineField,
  sounds,
  onSounds,
}: {
  id: string;
  draft: AppSettings;
  update: (values: Partial<AppSettings>) => void;
  disabled: boolean;
  hidden: boolean;
  engineField: ReactNode;
  sounds: boolean;
  onSounds: (sounds: boolean) => void;
}) {
  const t = useSettingsText();
  const playT = usePlayText();
  const modeT = usePlayModeText();
  const playPlies =
    configurationOptions.engine_profiles[draft.play_engine].play;
  return (
    <fieldset className="settings-section" disabled={disabled} hidden={hidden}>
      <legend>{t('play')}</legend>
      <div className="settings-field-pair">
        {engineField}
        <label className="settings-field" htmlFor={`${id}-play-strength`}>
          <span>{t('playStrength')}</span>
          <select
            id={`${id}-play-strength`}
            aria-label={t('playStrength')}
            value={
              draft.play_top_two ? TOP_TWO_PLAY_STRENGTH : draft.play_plies
            }
            onChange={(event) => {
              const topTwo = event.target.value === TOP_TWO_PLAY_STRENGTH;
              update({
                play_plies: topTwo ? 2 : Number(event.target.value),
                play_top_two: topTwo,
              });
            }}
          >
            {PLAY_PLIES.map((ply) => (
              <option key={ply} value={ply} disabled={!playPlies.includes(ply)}>
                {playStrengthLabel(t, ply, draft.play_engine)}
              </option>
            ))}
            {draft.play_engine === 'bgsage' && playPlies.includes(2) && (
              <option value={TOP_TWO_PLAY_STRENGTH}>
                {playStrengthLabel(t, 2, draft.play_engine, true)}
              </option>
            )}
          </select>
        </label>
      </div>
      <p className="settings-help">{playT('playHelp')}</p>
      <section
        className="settings-play-group"
        role="group"
        aria-labelledby={`${id}-play-feedback`}
      >
        <h3 id={`${id}-play-feedback`}>{t('playFeedbackGroup')}</h3>
        <div className="settings-field-pair">
          <div>
            <label className="settings-field">
              <span>{modeT('defaultMode')}</span>
              <select
                aria-label={modeT('defaultMode')}
                aria-describedby={`${id}-play-mode-help`}
                value={draft.default_play_mode ?? 'coach'}
                onChange={(event) =>
                  update({
                    default_play_mode: event.target.value as PlayMode,
                  })
                }
              >
                {playModes.map((mode) => (
                  <option key={mode} value={mode}>
                    {modeT(mode)}
                  </option>
                ))}
              </select>
            </label>
            <p className="settings-help" id={`${id}-play-mode-help`}>
              {modeT(`${draft.default_play_mode ?? 'coach'}Help`)}
            </p>
          </div>
          <div className="settings-play-wide">
            <label className="settings-play-toggle">
              <input
                type="checkbox"
                aria-describedby={`${id}-play-automatic-help`}
                checked={draft.training_auto_analysis ?? true}
                onChange={(event) =>
                  update({
                    training_auto_analysis: event.target.checked,
                  })
                }
              />
              <span>{modeT('automatic')}</span>
            </label>
            <p className="settings-help" id={`${id}-play-automatic-help`}>
              {modeT('automaticHelp')}
            </p>
          </div>
        </div>
      </section>
      <section
        className="settings-play-group"
        role="group"
        aria-labelledby={`${id}-play-preferences`}
      >
        <h3 id={`${id}-play-preferences`}>{t('playPreferencesGroup')}</h3>
        <div className="settings-field-pair">
          <div>
            <label className="settings-play-toggle">
              <input
                type="checkbox"
                aria-describedby={`${id}-play-sounds-help`}
                checked={sounds}
                onChange={(event) => onSounds(event.target.checked)}
              />
              <span>{playT('sounds')}</span>
            </label>
            <p className="settings-help" id={`${id}-play-sounds-help`}>
              {playT('soundsHelp')}
            </p>
          </div>
          <div>
            <label className="settings-field" htmlFor={`${id}-bearoff`}>
              <span>{playT('direction')}</span>
              <select
                id={`${id}-bearoff`}
                aria-label={playT('direction')}
                aria-describedby={`${id}-play-direction-help`}
                value={draft.bearoff_direction ?? 'right'}
                onChange={(event) =>
                  update({
                    bearoff_direction: event.target.value as 'left' | 'right',
                  })
                }
              >
                <option value="right">{playT('right')}</option>
                <option value="left">{playT('left')}</option>
              </select>
            </label>
            <p className="settings-help" id={`${id}-play-direction-help`}>
              {playT('directionHelp')}
            </p>
          </div>
        </div>
      </section>
    </fieldset>
  );
}
