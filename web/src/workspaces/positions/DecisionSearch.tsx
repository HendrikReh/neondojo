import { useEffect, useRef, useState } from 'react';
import { useAnalysisErrorThresholds } from '../../settings/configuration/ConfigurationProvider';
import type { Theme } from '../../shared/board/themes';
import { useI18n } from '../../shared/i18n/i18n';
import ActionButton from '../../shared/ui/ActionButton';
import { useDatabases } from '../library/useDatabases';
import RecurringMistakePractice from '../study/mistakes/RecurringMistakePractice';
import {
  loadMistakes,
  searchDecisions,
  type DecisionFilters,
  type DecisionHit,
  type DecisionSearchPage,
  type MistakeQuestion,
} from '../study/mistakes/recurring-mistakes-api';
import { useMistakeText } from '../study/mistakes/recurring-mistakes-messages';
import { mistakeReferenceKey } from '../study/mistakes/recurring-mistakes-provenance';
import DecisionDeckAction, {
  DecisionDeckStatus,
  type DecisionDeckOutcome,
} from './DecisionDeckAction';
import DecisionFilterBar, {
  decisionFilters,
  emptyDecisionDraft,
  type DecisionSearchDraft,
} from './DecisionFilterBar';
import DecisionPreviewPane, {
  hasDecisionPosition,
  isDecisionOpenable,
} from './DecisionPreviewPane';
import DecisionResultsTable from './DecisionResultsTable';
import { useDecisionSearchText } from './decision-search-messages';
import './decision-search.css';

export { decisionFilters } from './DecisionFilterBar';

type Result =
  | { status: 'idle' }
  | { status: 'error'; error: unknown }
  | { status: 'ready'; page: DecisionSearchPage };

export default function DecisionSearch({
  visible,
  revision,
  bookmarkRevision,
  onBookmarkChanged,
  deckRevision,
  onDeckChanged,
  onStudy,
  theme,
  onOpenPosition,
  onOpenCubeDecision,
}: {
  visible: boolean;
  revision: number;
  bookmarkRevision: number;
  onBookmarkChanged: () => void;
  deckRevision: number;
  onDeckChanged: () => void;
  onStudy: (deckId: string) => void;
  theme: Theme;
  onOpenPosition: (id: string) => void;
  /** Opens a cube decision, which has no position of its own, in its game. */
  onOpenCubeDecision: (decision: {
    matchId: string;
    gameNo: number;
    decisionNo: number;
  }) => void;
}) {
  const t = useDecisionSearchText();
  const mistake = useMistakeText();
  const { formatError } = useI18n();
  const [players, setPlayers] = useState<string[]>([]);
  const profileLoss = useAnalysisErrorThresholds().error;
  const [draft, setDraft] = useState<DecisionSearchDraft>(emptyDecisionDraft);
  const [applied, setApplied] = useState<{
    filters: DecisionFilters;
    usesProfileLoss: boolean;
    page: number;
    attempt: number;
  }>();
  // Keep applied filters independent of unsubmitted draft edits. A change to
  // their inherited minimum starts a new result set at its first page.
  if (applied?.usesProfileLoss && applied.filters.min_loss !== profileLoss) {
    setApplied({
      ...applied,
      filters: { ...applied.filters, min_loss: profileLoss },
      page: 1,
    });
  }
  const [result, setResult] = useState<Result>({ status: 'idle' });
  const [loading, setLoading] = useState(false);
  const [selected, setSelected] = useState<string>();
  const [practice, setPractice] = useState<MistakeQuestion[]>();
  const [deckOutcome, setDeckOutcome] = useState<DecisionDeckOutcome>();
  const practiceButton = useRef<HTMLButtonElement>(null);
  // "My player profile" fills the player until one is chosen here.
  const playerChosen = useRef(false);
  // That player's decisions are listed once, on the first visit.
  const autoSearch = useRef(true);

  // Like Saved positions, Clear resets the filters and refreshes shown
  // results; the chosen player stays because every search needs one.
  function clearFilters() {
    const next = { ...emptyDecisionDraft, player: draft.player };
    setDraft(next);
    if (applied) submitDraft(next);
  }
  function submitDraft(next: DecisionSearchDraft) {
    const filters = decisionFilters(next, profileLoss);
    if (!filters) return false;
    setApplied((current) => ({
      filters: { ...filters, direction: current?.filters.direction },
      usesProfileLoss: next.minLoss === undefined,
      page: 1,
      attempt: (current?.attempt ?? 0) + 1,
    }));
    return true;
  }
  function changeField(key: keyof DecisionSearchDraft, value: string) {
    if (key === 'player') playerChosen.current = true;
    setDraft((current) => ({ ...current, [key]: value }));
  }

  const { databases } = useDatabases(revision, { enabled: visible });

  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    void loadMistakes(
      { database_id: draft.database || undefined },
      abort.signal,
    )
      .then(({ players, resolved_player }) => {
        if (abort.signal.aborted) return;
        setPlayers(players);
        setDraft((current) => {
          if (!current.player && !playerChosen.current && resolved_player)
            return { ...current, player: resolved_player };
          return current.player && !players.includes(current.player)
            ? { ...current, player: '' }
            : current;
        });
      })
      .catch(() => undefined);
    return () => abort.abort();
  }, [visible, revision, draft.database]);

  useEffect(() => {
    if (!visible || applied || !autoSearch.current) return;
    if (playerChosen.current) {
      autoSearch.current = false;
      return;
    }
    if (!draft.player) return;
    if (submitDraft(draft)) autoSearch.current = false;
  }, [visible, applied, draft, profileLoss]);

  useEffect(() => {
    if (!visible || !applied) return;
    const abort = new AbortController();
    // The previous results stay visible while the next ones load.
    setLoading(true);
    void searchDecisions(
      { ...applied.filters, page: applied.page },
      abort.signal,
    )
      .then((page) => {
        if (abort.signal.aborted) return;
        setLoading(false);
        setResult({ status: 'ready', page });
        const keys = page.items.map((item) =>
          mistakeReferenceKey(item.reference),
        );
        setSelected((current) =>
          current && keys.includes(current) ? current : keys[0],
        );
      })
      .catch((error: unknown) => {
        if (abort.signal.aborted) return;
        setLoading(false);
        setResult({ status: 'error', error });
      });
    return () => abort.abort();
  }, [visible, applied, revision]);

  const filters = decisionFilters(draft, profileLoss);
  const items = result.status === 'ready' ? result.page.items : [];
  const preview =
    items.find((item) => mistakeReferenceKey(item.reference) === selected) ??
    items[0];
  const keys = items.map((item) => mistakeReferenceKey(item.reference));
  const direction = applied?.filters.direction ?? 'desc';

  function openHit(hit: DecisionHit) {
    if (hasDecisionPosition(hit)) onOpenPosition(hit.position.id);
    else if (isDecisionOpenable(hit))
      onOpenCubeDecision({
        matchId: hit.position.match_uuid,
        gameNo: hit.reference.game_no,
        decisionNo: hit.reference.decision_no,
      });
  }
  function open(key: string) {
    const hit = items.find(
      (item) => mistakeReferenceKey(item.reference) === key,
    );
    if (hit) openHit(hit);
  }
  const practicable = items.filter((item) => item.practicable).slice(0, 20);
  const pageKey = `page:${keys.join(',')}`;

  // The result count and practice action lead the table toolbar, or stand
  // alone above an empty result.
  const summary = result.status === 'ready' && (
    <>
      <p role="status">{t('found', { count: result.page.total })}</p>
      <ActionButton
        ref={practiceButton}
        icon="cards"
        label={t('practice')}
        tooltip={t('practiceHint')}
        disabled={practicable.length === 0}
        onClick={() => setPractice(practicable)}
      >
        {t('practice')}
      </ActionButton>
      <DecisionDeckAction
        references={items.map((item) => item.reference)}
        outcomeKey={pageKey}
        label={t('addPage')}
        tooltip={t('addPageHint')}
        compact={false}
        revision={deckRevision}
        onAdded={(outcome) => {
          setDeckOutcome(outcome);
          onDeckChanged();
        }}
        onDeckCreated={onDeckChanged}
      />
      {deckOutcome?.key === pageKey && (
        <DecisionDeckStatus outcome={deckOutcome} onStudy={onStudy} />
      )}
    </>
  );

  return (
    <section
      className="position-browser decision-search"
      aria-labelledby="decision-search-title"
    >
      <div className="position-browser-heading">
        <h1 id="decision-search-title">{t('title')}</h1>
      </div>
      <DecisionFilterBar
        draft={draft}
        players={players}
        databases={databases}
        profileLoss={profileLoss}
        canSearch={Boolean(filters)}
        onFieldChange={changeField}
        onSubmit={() => submitDraft(draft)}
        onClear={clearFilters}
      />

      {result.status === 'idle' && !loading && !!draft.player && (
        <p className="position-empty decision-search-idle" role="status">
          {t('idle', { action: t('search'), player: draft.player })}
        </p>
      )}
      {loading && result.status !== 'ready' && (
        <p role="status">{t('loading')}</p>
      )}
      {result.status === 'error' && (
        <div role="alert">
          <p>{formatError(result.error)}</p>
          <ActionButton
            icon="refresh"
            label={mistake('retry')}
            onClick={() =>
              setApplied(
                (current) =>
                  current && { ...current, attempt: current.attempt + 1 },
              )
            }
          >
            {mistake('retry')}
          </ActionButton>
        </div>
      )}
      {result.status === 'ready' && (
        <>
          {items.length === 0 ? (
            <>
              <div className="decision-search-summary">{summary}</div>
              <p className="position-empty">{t('empty')}</p>
            </>
          ) : (
            <div className="position-browser-layout">
              <DecisionResultsTable
                visible={visible}
                items={items}
                keys={keys}
                preview={preview}
                page={result.page.page}
                total={result.page.total}
                limit={result.page.limit}
                direction={direction}
                loading={loading}
                summary={summary}
                onSelect={setSelected}
                onOpen={open}
                onToggleSort={() =>
                  setApplied(
                    (current) =>
                      current && {
                        ...current,
                        page: 1,
                        filters: {
                          ...current.filters,
                          direction: direction === 'desc' ? 'asc' : undefined,
                        },
                      },
                  )
                }
                onPageChange={(page) =>
                  setApplied((current) => current && { ...current, page })
                }
              />
              {preview && (
                <DecisionPreviewPane
                  preview={preview}
                  theme={theme}
                  bookmarkRevision={bookmarkRevision}
                  onBookmarkChanged={onBookmarkChanged}
                  deckRevision={deckRevision}
                  deckOutcome={deckOutcome}
                  onDeckAdded={(outcome) => {
                    setDeckOutcome(outcome);
                    onDeckChanged();
                  }}
                  onDeckChanged={onDeckChanged}
                  onStudy={onStudy}
                  onOpen={openHit}
                />
              )}
            </div>
          )}
        </>
      )}
      {visible && practice && (
        <RecurringMistakePractice
          items={practice}
          theme={theme}
          returnFocus={practiceButton}
          onClose={() => setPractice(undefined)}
        />
      )}
    </section>
  );
}
