import { useEffect, useState } from 'react';
import { useDecimalInput } from '../../shared/format/decimal-input';
import { changePlayMode, type PlaySession } from './play-api';

/** The tutor threshold of the live match, saved when its field loses focus. */
export function usePlayThreshold({
  session,
  setup,
  isBlocked,
  run,
}: {
  session: PlaySession | null;
  setup: boolean;
  isBlocked: () => boolean;
  run: (
    work: () => Promise<PlaySession>,
    onSuccess: () => void,
    preserveSteps: boolean,
  ) => Promise<void>;
}) {
  const [value, setValue] = useState('0.04');
  const input = useDecimalInput({
    value,
    onChange: (next) => {
      setStatus('idle');
      setValue(next);
    },
    step: 0.01,
    min: 0,
    max: 1,
    onKeyDown: (event) => {
      if (event.key === 'Enter') {
        event.preventDefault();
        event.currentTarget.blur();
      }
    },
  });
  const [status, setStatus] = useState<'idle' | 'saving' | 'saved'>('idle');
  useEffect(() => {
    setStatus('idle');
  }, [session?.id, setup]);
  useEffect(() => {
    if (status !== 'saved') return;
    const timer = window.setTimeout(() => setStatus('idle'), 3000);
    return () => window.clearTimeout(timer);
  }, [status]);
  const valid =
    value.trim() !== '' &&
    Number.isFinite(Number(value)) &&
    Number(value) > 0 &&
    Number(value) <= 1;
  function save() {
    if (!session || isBlocked() || session.phase === 'match_over') return;
    if (!valid) {
      setValue(String(session.tutor_threshold ?? 0.04));
      return;
    }
    if (Number(value) === (session.tutor_threshold ?? 0.04)) return;
    setStatus('saving');
    void run(
      () =>
        changePlayMode(
          session,
          session.play_mode ?? 'competition',
          Number(value),
        ),
      () => setStatus('saved'),
      true,
    ).then(() =>
      setStatus((current) => (current === 'saving' ? 'idle' : current)),
    );
  }
  return {
    input,
    status,
    save,
    /** Shows the threshold of an accepted checkpoint. */
    show: (accepted: PlaySession) =>
      setValue(String(accepted.tutor_threshold ?? 0.04)),
  };
}
