import { useEffect, useState } from 'react';
import {
  loadMyPlayerProfile,
  loadVirtualPlayers,
  saveMyPlayerProfile,
  type VirtualPlayerProfile,
} from '../workspaces/players/virtual-player-api';
import { useVirtualPlayerText } from '../workspaces/players/virtual-player-messages';

/**
 * The Settings draft for "My player profile". The choice belongs to the
 * library, not to a configuration profile, so it is saved through the player
 * API when the dialog applies its changes.
 */
export function useMyPlayerProfileDraft(open: boolean, reload: unknown) {
  const [options, setOptions] = useState<VirtualPlayerProfile[]>();
  const [initial, setInitial] = useState<string | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<unknown>();
  useEffect(() => {
    if (!open) return;
    const abort = new AbortController();
    setOptions(undefined);
    setError(undefined);
    Promise.all([
      loadVirtualPlayers(abort.signal),
      loadMyPlayerProfile(abort.signal),
    ]).then(
      ([items, me]) => {
        if (abort.signal.aborted) return;
        setOptions(items);
        setInitial(me?.id ?? null);
        setDraft(me?.id ?? null);
      },
      (failure: unknown) => {
        if (!abort.signal.aborted) setError(failure);
      },
    );
    return () => abort.abort();
  }, [open, reload]);
  return {
    options,
    error,
    draft,
    setDraft,
    changed: options !== undefined && draft !== initial,
    discard: () => setDraft(initial),
    async save(signal: AbortSignal) {
      const saved = await saveMyPlayerProfile(draft, signal);
      setInitial(saved?.id ?? null);
      setDraft(saved?.id ?? null);
    },
  };
}

export function MyPlayerProfileField({
  id,
  state,
}: {
  id: string;
  state: ReturnType<typeof useMyPlayerProfileDraft>;
}) {
  const v = useVirtualPlayerText();
  return (
    <>
      <label className="settings-field" htmlFor={id}>
        <span>{v('myProfile')}</span>
        <select
          id={id}
          value={state.draft ?? ''}
          disabled={!state.options}
          aria-describedby={`${id}-help`}
          onChange={(event) => state.setDraft(event.target.value || null)}
        >
          <option value="">{v('myProfileNone')}</option>
          {state.options?.map((option) => (
            <option key={option.id} value={option.id}>
              {option.name}
            </option>
          ))}
        </select>
      </label>
      <p id={`${id}-help`} className="settings-help">
        {state.error === undefined
          ? v('myProfileHelp')
          : v('myProfileUnavailable')}
      </p>
    </>
  );
}
