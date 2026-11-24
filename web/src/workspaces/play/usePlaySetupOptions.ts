import { useEffect, useState } from 'react';
import configurationOptions from '../../../../shared/configuration-options.json' with { type: 'json' };
import {
  preferredDepth,
  type DepthEngine,
} from '../../shared/engine/analysis-depth';
import { useConfirmedConfiguration } from '../../settings/configuration/ConfigurationProvider';
import { defaultConfiguration } from '../../settings/configuration/configuration-model';
import {
  loadEngines,
  type AppSettings,
  type EngineInfo,
} from '../../settings/settings-api';
import type { SageDifficulty } from './play-api';
import type { PlayMode } from './play-mode-messages';

/** Choices made in the new-match dialog; unset fields use the saved defaults. */
export interface PlaySetupOptions {
  human?: number;
  length?: number;
  mode?: PlayMode;
  automatic?: boolean;
  name?: string;
  engine?: DepthEngine;
  plies?: number;
  topTwo?: boolean;
  difficulty?: SageDifficulty;
}

/**
 * The new-match choices resolved against the confirmed configuration, the
 * legacy settings and the built-in defaults.
 */
export function usePlaySetupOptions(settings: AppSettings | undefined) {
  const [options, setOptions] = useState<PlaySetupOptions>({});
  const confirmed = useConfirmedConfiguration();
  const defaults = defaultConfiguration.play;
  const human =
    options.human ?? confirmed?.play.human_player ?? defaults.human_player;
  const length =
    options.length ?? confirmed?.play.match_length ?? defaults.match_length;
  const mode =
    options.mode ??
    confirmed?.play.default_mode ??
    settings?.default_play_mode ??
    defaults.default_mode;
  const threshold =
    confirmed?.analysis.error_thresholds.error ??
    defaultConfiguration.analysis.error_thresholds.error;
  const automatic =
    options.automatic ??
    confirmed?.play.training_auto_analysis ??
    settings?.training_auto_analysis ??
    defaults.training_auto_analysis;
  // Name, bot engine and strength apply to the new match only.
  const playerName =
    options.name ??
    confirmed?.general.player_name ??
    settings?.player_name ??
    '';
  const savedBotEngine =
    confirmed?.play.engine ?? settings?.play_engine ?? defaults.engine;
  const botEngine = options.engine ?? savedBotEngine;
  const botDepths = configurationOptions.engine_profiles[botEngine].play;
  const botDifficulty = botEngine === 'bgsage' ? options.difficulty : undefined;
  const requestedPlies =
    options.plies ??
    (botEngine === savedBotEngine
      ? (confirmed?.play.plies ?? settings?.play_plies ?? defaults.plies)
      : preferredDepth(botEngine));
  const botPlies = botDepths.includes(requestedPlies)
    ? requestedPlies
    : botDepths.includes(preferredDepth(botEngine))
      ? preferredDepth(botEngine)
      : botDepths[0];
  const botTopTwo =
    botEngine === 'bgsage' &&
    botPlies === 2 &&
    (options.topTwo ??
      confirmed?.play.top_two ??
      settings?.play_top_two ??
      false);
  return {
    options,
    setOptions,
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
  };
}

export type PlaySetupChoices = ReturnType<typeof usePlaySetupOptions>;

/** The engines the new-match dialog offers, loaded each time it opens. */
export function usePlayEngines(setupOpen: boolean) {
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  useEffect(() => {
    if (!setupOpen) return;
    const abort = new AbortController();
    void loadEngines(abort.signal)
      .then((value) => {
        if (!abort.signal.aborted) setEngines(value);
      })
      .catch(() => {
        /* Engine availability is advisory; creating the match still verifies it. */
      });
    return () => abort.abort();
  }, [setupOpen]);
  return engines;
}
