import Tooltip from '../../shared/ui/Tooltip';
import type { PlaySession } from './play-api';
import {
  playModes,
  usePlayModeText,
  type PlayMode,
} from './play-mode-messages';
import type { usePlayThreshold } from './usePlayThreshold';

/** The feedback mode of the live match and the tutor threshold. */
export default function PlayModeControl({
  session,
  mode,
  threshold,
  disabled,
  onMode,
}: {
  session: PlaySession;
  mode: PlayMode;
  threshold: ReturnType<typeof usePlayThreshold>;
  disabled: boolean;
  onMode: (mode: PlayMode) => void;
}) {
  const modeT = usePlayModeText();
  return (
    <div className="play-mode-control">
      <label>
        {modeT('mode')}
        <Tooltip
          as="select"
          text={modeT(`${mode}Help`)}
          aria-label={modeT('changeMode')}
          value={mode}
          disabled={disabled}
          onChange={(event) => onMode(event.target.value as PlayMode)}
        >
          {playModes.map((value) => (
            <option key={value} value={value}>
              {modeT(value)}
            </option>
          ))}
        </Tooltip>
      </label>
      {mode === 'tutor' && (
        <div className="play-threshold-control">
          <label>
            {modeT('threshold')}
            <Tooltip
              as="input"
              aria-label={modeT('threshold')}
              text={modeT('liveThresholdHelp')}
              {...threshold.input.inputProps}
              required
              disabled={disabled}
              onChange={(event) => threshold.input.change(event.target.value)}
              onBlur={threshold.save}
            />
          </label>
          <span
            className="play-threshold-status"
            role="status"
            aria-atomic="true"
            data-state={threshold.status}
          >
            {threshold.status === 'saving'
              ? modeT('thresholdSaving')
              : threshold.status === 'saved'
                ? modeT('thresholdSaved')
                : ''}
          </span>
        </div>
      )}
      {session.learning_used && mode === 'competition' && (
        <small>{modeT('learningUsed')}</small>
      )}
    </div>
  );
}
