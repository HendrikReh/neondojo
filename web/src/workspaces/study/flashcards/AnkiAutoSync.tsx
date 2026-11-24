import { useEffect, useRef, useState } from 'react';
import { useLatest } from '../../../shared/hooks/useLatest';
import { useCardPracticeText } from './card-practice-messages';
import { useI18n } from '../../../shared/i18n/i18n';
import { AppError, dateTimeFormat } from '../../../shared/i18n/i18n-core';
import {
  loadAnkiAutoSync,
  setAnkiAutoSync,
  type AnkiAutoSyncSettings,
} from './learning-api';

export default function AnkiAutoSync({
  deckId,
  empty,
  busy,
  onReceived,
}: {
  deckId: string;
  empty: boolean;
  busy: boolean;
  onReceived: () => void;
}) {
  const t = useCardPracticeText();
  const { formatError, locale } = useI18n();
  const [settings, setSettings] = useState<AnkiAutoSyncSettings>();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>();
  const [readError, setReadError] = useState<unknown>();
  const read = useRef<AbortController | null>(null);
  const current = useRef<AnkiAutoSyncSettings | undefined>(undefined);
  const onReceivedRef = useLatest(onReceived);

  useEffect(() => {
    let mounted = true;
    let timer: ReturnType<typeof setTimeout>;
    async function refresh() {
      const controller = new AbortController();
      read.current = controller;
      try {
        const next = await loadAnkiAutoSync(deckId, controller.signal);
        if (controller.signal.aborted) return;
        const previous = current.current;
        current.current = next;
        setSettings(next);
        setReadError(undefined);
        if (next.last_received_at && next.last_received_at !== previous?.last_received_at)
          onReceivedRef.current();
      } catch (failure) {
        if (!controller.signal.aborted) setReadError(failure);
      } finally {
        if (mounted && !controller.signal.aborted) timer = setTimeout(() => void refresh(), 10000);
      }
    }
    if (!saving) void refresh();
    return () => {
      mounted = false;
      read.current?.abort();
      clearTimeout(timer);
    };
  }, [deckId, saving]);

  async function change(enabled: boolean) {
    read.current?.abort();
    setSaving(true);
    setError(undefined);
    try {
      const next = await setAnkiAutoSync(deckId, { enabled });
      if (!enabled && current.current?.last_received_at) onReceivedRef.current();
      current.current = next;
      setSettings(next);
    } catch (failure) {
      setError(failure);
    } finally {
      setSaving(false);
    }
  }

  const active = !!settings?.profile;
  const failure = error || (!settings && readError);
  const lastSync = settings?.last_synced_at;
  const status = settings?.last_outcome === 'failed'
    ? t('ankiAutoFailed')
    : settings?.last_outcome === 'no_matches'
      ? t('ankiAutoNoMatches')
      : lastSync
        ? t('ankiAutoLastSync', {
            date: dateTimeFormat(locale, {
              dateStyle: 'short', timeStyle: 'short',
            }).format(new Date(lastSync)),
          })
        : t('ankiAutoWaiting');
  return (
    <div className="learning-anki-auto-sync">
      <label>
        <input
          type="checkbox"
          checked={active}
          disabled={!settings || saving || (!active && (busy || empty))}
          onChange={(event) => void change(event.target.checked)}
        />
        {t('ankiAutoSync')}
      </label>
      {active && <span>{t('ankiAutoProfile', { profile: settings?.profile ?? '' })} · {status}</span>}
      {!!failure && (
        <span role="alert">
          {failure instanceof AppError && failure.params.status === '503'
            ? t('ankiUnavailable')
            : failure instanceof AppError && failure.params.status === '409'
              ? t('ankiAutoLinkFirst')
              : formatError(failure)}
        </span>
      )}
    </div>
  );
}
