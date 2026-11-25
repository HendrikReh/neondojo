import { useEffect, useRef, useState } from 'react';
import { AppError } from '../../shared/i18n/i18n-core';
import {
  actInPlaySession,
  loadPlaySession,
  type PlayAction,
  type PlaySession,
} from './play-api';

const storageKey = 'neondojo.play-session';
export function storedSession() {
  try {
    return localStorage.getItem(storageKey);
  } catch {
    return null;
  }
}
function forgetStoredSession(id: string) {
  try {
    if (localStorage.getItem(storageKey) === id)
      localStorage.removeItem(storageKey);
  } catch {
    /* Nothing is restored automatically without storage. */
  }
}

export type PlaySessionRequest = {
  id: string;
  token: number;
  resume?: boolean;
  keepTurnAtVersion?: number;
};

/** How an accepted checkpoint relates to the one it replaces. */
export interface AcceptedSession {
  value: PlaySession;
  sameSession: boolean;
  sameGame: boolean;
  preserveSteps: boolean;
}

/**
 * The live match: restoring and loading it, and running one request at a time.
 * A newer load or an unmount discards the result of an older request.
 */
export function usePlaySession({
  visible,
  requestedSession,
  onLoad,
  onAccept,
  onSaved,
}: {
  visible: boolean;
  requestedSession?: PlaySessionRequest;
  /** Before another match loads. */
  onLoad: () => void;
  /** Before a checkpoint is shown; resets what belongs to the previous one. */
  onAccept: (accepted: AcceptedSession) => void;
  onSaved?: () => void;
}) {
  const [session, setSession] = useState<PlaySession | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(),
    // A saved match is restored first; without one, the setup dialog opens.
    [setup, setSetup] = useState(() => storedSession() === null);
  const [pendingRoll, setPendingRoll] = useState<PlayAction | null>(null);
  const generation = useRef(0),
    locked = useRef(false),
    initialized = useRef(false);
  const consumedRequest = useRef<number | undefined>(undefined);
  const retrySession = useRef<string | null>(null);
  const initialLoad = useRef<AbortController | null>(null);
  useEffect(
    () => () => {
      initialLoad.current?.abort();
      initialLoad.current = null;
      ++generation.current;
      locked.current = false;
      initialized.current = false;
      consumedRequest.current = undefined;
    },
    [],
  );
  function accept(value: PlaySession, preserveSteps = false) {
    onAccept({
      value,
      sameSession: value.id === session?.id,
      sameGame: value.id === session?.id && value.game_no === session?.game_no,
      preserveSteps,
    });
    setSession(value);
    setSetup(false);
    setError(undefined);
    retrySession.current = value.id;
    onSaved?.();
    try {
      localStorage.setItem(storageKey, value.id);
    } catch {
      /* Resume through Match Database remains available. */
    }
  }
  useEffect(() => {
    if (!visible) return;
    const requested =
      requestedSession && consumedRequest.current !== requestedSession.token;
    // Explicit selection may replace a read; an in-flight native action must finish.
    if (locked.current && !(requested && initialLoad.current)) return;
    if (!requested && initialized.current) return;
    initialized.current = true;
    if (requested) consumedRequest.current = requestedSession.token;
    const id = requested ? requestedSession.id : storedSession();
    if (!id) return;
    if (
      requested &&
      requestedSession.keepTurnAtVersion !== undefined &&
      !locked.current &&
      session?.id === id &&
      session.version === requestedSession.keepTurnAtVersion &&
      !session.paused
    ) {
      setSetup(false);
      return;
    }
    load(id, requested ? requestedSession.resume : false, !requested);
  }, [visible, requestedSession, busy]);
  /** `restoring` marks the automatic restore of the last stored session. */
  function load(id: string, resume = false, restoring = false) {
    if (locked.current && !initialLoad.current) return;
    setSetup(false);
    setError(undefined);
    retrySession.current = id;
    initialLoad.current?.abort();
    const abort = new AbortController(),
      token = ++generation.current;
    initialLoad.current = abort;
    onLoad();
    setBusy(true);
    locked.current = true;
    void loadPlaySession(id, abort.signal)
      .then(async (value) => {
        if (token !== generation.current) return;
        if (resume && value.paused) {
          // Once the mutation starts, a later selection must wait for it.
          initialLoad.current = null;
          try {
            value = await actInPlaySession(value, 'resume');
          } catch (e) {
            if (token === generation.current) accept(value);
            throw e;
          }
        }
        if (token === generation.current) accept(value);
      })
      .catch((e) => {
        if (abort.signal.aborted || token !== generation.current) return;
        // A deleted session is not an error: forget it and offer a new match.
        if (
          restoring &&
          e instanceof AppError &&
          String(e.params.status) === '404'
        ) {
          forgetStoredSession(id);
          retrySession.current = null;
          setSetup(true);
        } else setError(e);
      })
      .finally(() => {
        if (token === generation.current) {
          initialLoad.current = null;
          setBusy(false);
          locked.current = false;
        }
      });
  }
  async function run(
    work: () => Promise<PlaySession>,
    onSuccess?: (value: PlaySession) => void | Promise<void>,
    preserveSteps = false,
  ) {
    if (locked.current) return;
    locked.current = true;
    setBusy(true);
    setError(undefined);
    const token = ++generation.current;
    try {
      const value = await work();
      if (token === generation.current) {
        await onSuccess?.(value);
        if (token === generation.current) accept(value, preserveSteps);
      }
    } catch (e) {
      if (token === generation.current) setError(e);
    } finally {
      if (token === generation.current) {
        locked.current = false;
        setPendingRoll(null);
        setBusy(false);
      }
    }
  }
  return {
    session,
    busy,
    error,
    setError,
    setup,
    setSetup,
    pendingRoll,
    setPendingRoll,
    load,
    run,
    /** Loads the match of the last failed request again. */
    retry: () => {
      const id = retrySession.current ?? storedSession();
      if (id) load(id);
    },
    isLocked: () => locked.current,
  };
}
