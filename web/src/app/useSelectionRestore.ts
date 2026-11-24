import { useEffect, useRef, useState } from 'react';
import { request } from '../shared/http/api';
import { AppError } from '../shared/i18n/i18n-core';
import type { LibraryMatch } from '../workspaces/library/library-api';
import { isUnfinishedCompetition } from '../workspaces/library/library-match-summary';
import type { useGameNavigation } from './useGameNavigation';
import {
  clearSelection,
  readSelection,
  saveSelection,
} from './workspace-state';

/** Opens the last match and position at the start and remembers the current one. */
export function useSelectionRestore(
  navigation: ReturnType<typeof useGameNavigation>,
  /** The last match is an unfinished bot match, which only Play may show. */
  onPlaySession: (id: string) => void,
) {
  const [restoring, setRestoring] = useState(false);
  const restorationRequest = useRef<AbortController | null>(null);
  useEffect(() => {
    const previous = readSelection();
    if (!previous) return;
    const abort = new AbortController();
    restorationRequest.current = abort;
    const intent = navigation.intent();
    const current = () =>
      !abort.signal.aborted && navigation.intent() === intent;
    setRestoring(true);
    void (async () => {
      // Metadata is available even while a Competition source is restricted.
      // Do not restore that source through the analysis endpoints.
      let metadata: LibraryMatch | undefined;
      let missing = false;
      try {
        metadata = await request<LibraryMatch>(
          `/matches/${previous.matchId}/metadata`,
          { signal: abort.signal },
        );
      } catch (e) {
        // Old/missing metadata still uses normal navigation and its real error.
        missing = e instanceof AppError && String(e.params.status) === '404';
      }
      if (!current()) return;
      if (
        metadata &&
        (metadata.play_paused || isUnfinishedCompetition(metadata)) &&
        metadata.play_session_id
      ) {
        navigation.clear();
        onPlaySession(metadata.play_session_id);
      } else {
        const opening = previous.positionId
          ? navigation.openPosition(previous.positionId)
          : navigation.openMatch(previous.matchId, previous.gameNo);
        // Opening starts a new navigation; a later one must not be cleared.
        const openIntent = navigation.intent();
        const opened = await opening;
        if (
          !opened &&
          missing &&
          !abort.signal.aborted &&
          navigation.intent() === openIntent
        ) {
          // The last match or position no longer exists: start empty.
          navigation.clear();
          clearSelection();
        }
      }
    })().finally(() => {
      if (!abort.signal.aborted) setRestoring(false);
      if (restorationRequest.current === abort)
        restorationRequest.current = null;
    });
    return () => {
      abort.abort();
      if (restorationRequest.current === abort)
        restorationRequest.current = null;
    };
    // Navigation intent and aborts fence this single startup restore.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    if (
      navigation.match &&
      navigation.game &&
      !navigation.loading &&
      !navigation.error &&
      navigation.gameLoaded
    ) {
      saveSelection({
        matchId: navigation.match.id,
        gameNo: navigation.game.game_no,
        positionId: navigation.position?.id,
      });
    }
  }, [
    navigation.match,
    navigation.game,
    navigation.loading,
    navigation.error,
    navigation.gameLoaded,
    navigation.position,
  ]);
  function cancelRestoration() {
    if (!restorationRequest.current) return;
    restorationRequest.current.abort();
    restorationRequest.current = null;
    navigation.cancel();
    setRestoring(false);
  }
  return { restoring, cancelRestoration };
}
