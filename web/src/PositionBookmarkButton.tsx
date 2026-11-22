import ActionButton from './shared/ui/ActionButton';
import { useEffect, useRef, useState } from 'react';
import {
  loadPositionBookmark,
  removePositionBookmark,
  savePositionBookmark,
} from './position-bookmark-api';
import { usePositionBookmarkText } from './position-bookmark-messages';
import { useI18n } from './shared/i18n/i18n';
import './position-bookmark.css';

type Operation = 'load' | 'save' | 'remove';
interface BookmarkState {
  identity: string;
  saved?: boolean;
  pending?: Operation;
  error?: { operation: Operation; cause: unknown };
}

export default function PositionBookmarkButton({
  positionId,
  revision,
  onChanged,
}: {
  positionId: string;
  revision: number;
  onChanged: () => void;
}) {
  const l = usePositionBookmarkText(),
    { formatError } = useI18n();
  const [retry, setRetry] = useState(0);
  const identity = `${positionId}:${revision}:${retry}`;
  const current = useRef(identity);
  current.current = identity;
  const changed = useRef(onChanged);
  changed.current = onChanged;
  const request = useRef<AbortController | undefined>(undefined);
  const [state, setState] = useState<BookmarkState>({
    identity,
    pending: 'load',
  });

  useEffect(() => {
    const abort = new AbortController();
    request.current = abort;
    setState({ identity, pending: 'load' });
    void loadPositionBookmark(positionId, abort.signal)
      .then((status) => {
        if (!abort.signal.aborted && current.current === identity)
          setState({ identity, saved: status.saved });
      })
      .catch((cause: unknown) => {
        if (!abort.signal.aborted && current.current === identity)
          setState({ identity, error: { operation: 'load', cause } });
      });
    return () => request.current?.abort();
  }, [identity, positionId]);

  // Render can select a different source before the old effect is cleaned up.
  const status: BookmarkState =
    state.identity === identity ? state : { identity, pending: 'load' };

  async function change(operation: 'save' | 'remove') {
    if (
      current.current !== identity ||
      status.pending ||
      status.saved === undefined
    )
      return;
    request.current?.abort();
    const abort = new AbortController();
    request.current = abort;
    setState({ identity, saved: status.saved, pending: operation });
    try {
      let saved: boolean;
      if (operation === 'save')
        saved = (await savePositionBookmark(positionId, abort.signal)).saved;
      else {
        await removePositionBookmark(positionId, abort.signal);
        saved = false;
      }
      if (abort.signal.aborted || current.current !== identity) return;
      setState({ identity, saved });
      changed.current();
    } catch (cause) {
      if (!abort.signal.aborted && current.current === identity)
        setState({
          identity,
          saved: status.saved,
          error: { operation, cause },
        });
    }
  }

  function retryFailure() {
    const operation = status.error?.operation;
    if (operation === 'load') setRetry((value) => value + 1);
    else if (operation) void change(operation);
  }

  const busy = !!status.pending;
  return (
    <div className="position-bookmark-control" aria-busy={busy}>
      {status.saved !== undefined ? (
        <ActionButton
          icon={status.saved ? 'unbookmark' : 'bookmark'}
          label={l(
            status.pending === 'save'
              ? 'saving'
              : status.pending === 'remove'
                ? 'removing'
                : status.saved
                  ? 'remove'
                  : 'save',
          )}
          compact
          type="button"
          className="played-preview-button"
          aria-pressed={status.saved}
          disabled={busy}
          onClick={() => void change(status.saved ? 'remove' : 'save')}
        />
      ) : busy ? (
        <span role="status">{l('loading')}</span>
      ) : null}
      {status.error && (
        <>
          <p role="alert">
            {l(
              status.error.operation === 'load'
                ? 'loadError'
                : status.error.operation === 'save'
                  ? 'saveError'
                  : 'removeError',
            )}{' '}
            {formatError(status.error.cause)}
          </p>
          <ActionButton
            icon="refresh"
            label={l('retry')}
            compact
            type="button"
            className="played-preview-button"
            onClick={retryFailure}
          />
        </>
      )}
    </div>
  );
}
