import ActionButton from '../../../shared/ui/ActionButton';
import { useCallback, useEffect, useId, useRef, useState } from 'react';
import { request } from '../../../shared/api/api';
import { useLibraryText } from '../../../library-messages';
import {
  annotationPath,
  annotationValidation,
  isAnnotationConflict,
  normalizedAnnotationTags,
  type Annotation,
  type AnnotationKind,
  type AnnotationValidation,
} from './annotation-api';
import { useAnnotationI18n } from './annotation-messages';

interface Draft {
  tags: string[];
  comment: string;
  tagInput: string;
  version: number;
  baseline: string;
}

const signature = (tags: string[], comment: string) =>
  JSON.stringify([tags, comment]);
const dirty = (draft: Draft) =>
  !!draft.tagInput || signature(draft.tags, draft.comment) !== draft.baseline;
const emptyDraft = (): Draft => ({
  tags: [],
  comment: '',
  tagInput: '',
  version: 0,
  baseline: signature([], ''),
});
const fromAnnotation = (annotation: Annotation): Draft => ({
  tags: annotation.tags,
  comment: annotation.comment,
  tagInput: '',
  version: annotation.version,
  baseline: signature(annotation.tags, annotation.comment),
});

// Keep drafts while the user browses other positions. Saved notes live in the DB.
const drafts = new Map<string, Draft>();

interface AnnotationPanelProps {
  kind: AnnotationKind;
  id: string;
  onSaved?: () => void;
}

export default function AnnotationPanel(props: AnnotationPanelProps) {
  return <AnnotationEditor key={`${props.kind}:${props.id}`} {...props} />;
}

function AnnotationEditor({ kind, id, onSaved }: AnnotationPanelProps) {
  const { t, formatError } = useAnnotationI18n();
  const l = useLibraryText();
  const cacheKey = `${kind}:${id}`;
  const htmlId = useId();
  const [draft, setDraft] = useState(
    () => drafts.get(cacheKey) ?? emptyDraft(),
  );
  const draftRef = useRef(draft);
  const [ready, setReady] = useState(() => drafts.has(cacheKey));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [validation, setValidation] = useState<AnnotationValidation | null>(
    null,
  );
  const loadController = useRef<AbortController | null>(null);
  const saveController = useRef<AbortController | null>(null);

  const replaceDraft = useCallback(
    (next: Draft) => {
      draftRef.current = next;
      setDraft(next);
      if (dirty(next)) drafts.set(cacheKey, next);
      else drafts.delete(cacheKey);
    },
    [cacheKey],
  );

  const load = useCallback(
    async (discardDraft: boolean) => {
      loadController.current?.abort();
      const controller = new AbortController();
      loadController.current = controller;
      setLoading(true);
      setError(null);
      setValidation(null);
      setSaved(false);
      try {
        const annotation = await request<Annotation>(annotationPath(kind, id), {
          signal: controller.signal,
        });
        if (controller.signal.aborted) return;
        if (!discardDraft && dirty(draftRef.current)) {
          setConflict(annotation.version !== draftRef.current.version);
        } else {
          replaceDraft(fromAnnotation(annotation));
          setConflict(false);
        }
        setReady(true);
      } catch (failure) {
        if (!controller.signal.aborted) setError(failure);
      } finally {
        if (!controller.signal.aborted) setLoading(false);
      }
    },
    [id, kind, replaceDraft],
  );

  useEffect(() => {
    void load(false);
    return () => {
      loadController.current?.abort();
      saveController.current?.abort();
    };
  }, [load]);

  const edit = (next: Draft) => {
    replaceDraft(next);
    setValidation(null);
    setSaved(false);
  };

  const addTag = () => {
    const current = draftRef.current;
    if (!current.tagInput.trim()) return;
    const issue = annotationValidation([...current.tags, current.tagInput], '');
    if (issue) {
      setValidation(issue);
      return;
    }
    edit({
      ...current,
      tags: normalizedAnnotationTags([...current.tags, current.tagInput]),
      tagInput: '',
    });
  };

  const save = async () => {
    if (saving || loading || !ready || conflict) return;
    const current = draftRef.current;
    const allTags = [...current.tags, current.tagInput];
    const issue = annotationValidation(allTags, current.comment);
    setValidation(issue);
    if (issue) return;
    const controller = new AbortController();
    saveController.current = controller;
    setSaving(true);
    setSaved(false);
    setError(null);
    try {
      const annotation = await request<Annotation>(annotationPath(kind, id), {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        signal: controller.signal,
        body: JSON.stringify({
          tags: normalizedAnnotationTags(allTags),
          comment: current.comment,
          expected_version: current.version,
        }),
      });
      if (controller.signal.aborted) return;
      replaceDraft(fromAnnotation(annotation));
      setConflict(false);
      setSaved(true);
      onSaved?.();
    } catch (failure) {
      if (controller.signal.aborted) return;
      if (isAnnotationConflict(failure)) setConflict(true);
      else setError(failure);
    } finally {
      if (!controller.signal.aborted) setSaving(false);
    }
  };

  const disabled = !ready || loading || saving;
  const unsaved = dirty(draft);

  return (
    <section
      className="annotation-panel"
      aria-labelledby={`${htmlId}-title`}
      data-annotation-kind={kind}
      data-annotation-id={id}
    >
      <div className="section-heading annotation-heading">
        <div>
          <span className="eyebrow">{t('eyebrow')}</span>
          <h3 id={`${htmlId}-title`}>
            {kind === 'game'
              ? `${l('game')} · ${l('comment')}`
              : kind === 'move'
                ? `${l('sourceMove')} · ${l('comment')}`
                : t(kind === 'match' ? 'matchTitle' : 'positionTitle')}
          </h3>
        </div>
        {unsaved && <span className="annotation-status">{t('unsaved')}</span>}
      </div>
      {loading && <p role="status">{t('loading')}</p>}
      {!!error && (
        <div className="error" role="alert">
          <p>{formatError(error)}</p>
          {!ready && (
            <ActionButton
              icon="refresh"
              label={t('retry')}
              compact
              type="button"
              onClick={() => void load(false)}
              disabled={loading}
            />
          )}
        </div>
      )}
      {conflict && (
        <div className="annotation-conflict" role="alert">
          <p>{t('conflict')}</p>
          <ActionButton
            icon="refresh"
            label={t('loadLatest')}
            type="button"
            onClick={() => void load(true)}
            disabled={loading || saving}
          />
        </div>
      )}
      <div className="annotation-fields">
        <div className="annotation-tags">
          <label htmlFor={`${htmlId}-tag`}>{t('tags')}</label>
          <div className="annotation-tag-entry">
            <input
              id={`${htmlId}-tag`}
              value={draft.tagInput}
              placeholder={t('tagPlaceholder')}
              aria-describedby={`${htmlId}-tag-help`}
              disabled={disabled}
              onChange={(event) =>
                edit({ ...draftRef.current, tagInput: event.target.value })
              }
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  addTag();
                }
              }}
            />
            <ActionButton
              icon="add"
              label={t('addTag')}
              compact
              type="button"
              onClick={addTag}
              disabled={disabled || !draft.tagInput.trim()}
            />
          </div>
          <p className="fineprint annotation-help" id={`${htmlId}-tag-help`}>
            {t('tagHelp')}
          </p>
          {!!draft.tags.length && (
            <ul className="annotation-tag-list">
              {draft.tags.map((tag) => (
                <li className="annotation-tag" key={tag}>
                  <span>{tag}</span>
                  <ActionButton
                    icon="close"
                    label={t('removeTag', { tag })}
                    compact
                    type="button"
                    aria-label={t('removeTag', { tag })}
                    disabled={disabled}
                    onClick={() =>
                      edit({
                        ...draftRef.current,
                        tags: draftRef.current.tags.filter(
                          (item) => item !== tag,
                        ),
                      })
                    }
                  />
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="annotation-comment">
          <label htmlFor={`${htmlId}-comment`}>{t('comment')}</label>
          <textarea
            id={`${htmlId}-comment`}
            value={draft.comment}
            rows={4}
            placeholder={t('commentPlaceholder')}
            disabled={disabled}
            aria-describedby={`${htmlId}-comment-count`}
            onChange={(event) =>
              edit({ ...draftRef.current, comment: event.target.value })
            }
          />
          <small className="fineprint" id={`${htmlId}-comment-count`}>
            {t('commentCount', { count: [...draft.comment].length })}
          </small>
        </div>
      </div>
      {validation && (
        <p className="error" role="alert">
          {t(validation)}
        </p>
      )}
      <div className="annotation-actions">
        <ActionButton
          icon="save"
          label={t(saving ? 'saving' : 'save')}
          compact
          type="button"
          className="primary"
          disabled={disabled || !unsaved || conflict}
          onClick={() => void save()}
        />
        {saved && (
          <span className="annotation-status" role="status">
            {t('saved')}
          </span>
        )}
      </div>
    </section>
  );
}
