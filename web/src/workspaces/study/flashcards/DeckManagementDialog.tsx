import ActionButton from '../../../shared/ui/ActionButton';
import ModalDialog from '../../../shared/ui/ModalDialog';
import { useIsMounted } from '../../../shared/hooks/useIsMounted';
import { useId, useRef, useState } from 'react';
import { useI18n } from '../../../shared/i18n/i18n';
import { useDeckManagementText } from './deck-management-messages';
import {
  deleteDeck,
  isDeckConflict,
  renameDeck,
  type VersionedDeck,
} from './deck-management-api';
import './deck-management.css';

export default function DeckManagementDialog({
  mode,
  deck,
  onClose,
  onRenamed,
  onDeleted,
  onRefresh,
}: {
  mode: 'rename' | 'delete';
  deck: VersionedDeck;
  onClose: () => void;
  onRenamed: (deck: VersionedDeck) => void;
  onDeleted: (id: string) => void;
  onRefresh: () => Promise<VersionedDeck[]>;
}) {
  const t = useDeckManagementText(),
    { formatError } = useI18n(),
    id = useId();
  const mounted = useIsMounted();
  const pending = useRef(false);
  const [observed, setObserved] = useState(deck);
  const [name, setName] = useState(deck.name);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [conflict, setConflict] = useState(false);
  const [missing, setMissing] = useState(false);
  const [refreshed, setRefreshed] = useState(false);
  async function refresh() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      const decks = await onRefresh();
      if (!mounted.current) return;
      const latest = decks.find((item) => item.id === deck.id);
      setConfirmed(false);
      setError(null);
      setMissing(!latest);
      setConflict(false);
      if (latest) {
        setObserved(latest);
        setRefreshed(true);
      }
    } catch (error) {
      if (mounted.current) setError(error);
    } finally {
      pending.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  async function save() {
    if (
      pending.current ||
      conflict ||
      missing ||
      (mode === 'delete' ? !confirmed : !name.trim())
    )
      return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try {
      if (mode === 'rename') {
        const updated = await renameDeck(observed, name);
        if (mounted.current) onRenamed(updated);
      } else {
        await deleteDeck(observed);
        if (mounted.current) onDeleted(observed.id);
      }
    } catch (error) {
      if (mounted.current) {
        setError(error);
        setConflict(isDeckConflict(error));
        setRefreshed(false);
        setConfirmed(false);
      }
    } finally {
      pending.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <ModalDialog
      className="deck-management-dialog"
      ariaLabelledBy={`${id}-title`}
      ariaDescribedBy={
        mode === 'delete' ? `${id}-details ${id}-keep` : `${id}-details`
      }
      busy={busy}
      onCancel={onClose}
      restoreFocusFallback={() =>
        document.querySelector<HTMLSelectElement>('.learning-workspace select')
      }
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <h2 id={`${id}-title`}>{t(mode)}</h2>
        <p id={`${id}-details`} className="deck-management-details">
          {t('details', { name: observed.name, count: observed.card_count })}
        </p>
        {mode === 'rename' ? (
          <label className="deck-management-name">
            {t('name')}
            <input
              autoFocus
              maxLength={120}
              value={name}
              disabled={busy || missing}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
        ) : (
          <>
            <p id={`${id}-keep`}>{t('keep')}</p>
            <label className="deck-management-confirm">
              <input
                type="checkbox"
                checked={confirmed}
                disabled={busy || conflict || missing}
                onChange={(event) => setConfirmed(event.target.checked)}
              />
              {t('confirm', {
                name: observed.name,
                count: observed.card_count,
              })}
            </label>
          </>
        )}
        {Boolean(error) && (
          <p role="alert">{conflict ? t('changed') : formatError(error)}</p>
        )}
        {missing && <p role="alert">{t('missing')}</p>}
        {refreshed && !error && <p role="status">{t('refreshed')}</p>}
        {conflict && (
          <ActionButton
            icon="refresh"
            label={t('refresh')}
            type="button"
            disabled={busy}
            onClick={() => void refresh()}
          >
            {t('refresh')}
          </ActionButton>
        )}
        <footer>
          <ActionButton
            icon="cancel"
            label={t('cancel')}
            type="button"
            disabled={busy}
            onClick={onClose}
          >
            {t('cancel')}
          </ActionButton>
          <ActionButton
            icon={mode === 'delete' ? 'delete' : 'save'}
            label={t(busy ? 'working' : mode === 'delete' ? 'delete' : 'save')}
            type="submit"
            className={mode === 'delete' ? 'deck-management-delete' : 'primary'}
            disabled={
              busy ||
              conflict ||
              missing ||
              (mode === 'delete'
                ? !confirmed
                : !name.trim() || name.trim() === observed.name)
            }
          >
            {t(busy ? 'working' : mode === 'delete' ? 'delete' : 'save')}
          </ActionButton>
        </footer>
      </form>
    </ModalDialog>
  );
}
