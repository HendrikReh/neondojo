import ActionIcon from './action-icons';
import { useCallback, useEffect, useId, useRef, useState } from 'react';
import type { DragEvent } from 'react';
import { useI18n } from './i18n';

export default function MatchDropZone({
  disabled,
  onFiles,
}: {
  disabled: boolean;
  onFiles: (files: File[]) => void;
}) {
  const { t } = useI18n();
  const formatsId = useId();
  const depth = useRef(0);
  const [dragging, setDragging] = useState(false);
  const clearDrag = useCallback(() => {
    depth.current = 0;
    setDragging(false);
  }, []);
  useEffect(() => {
    if (disabled) clearDrag();
  }, [disabled, clearDrag]);
  useEffect(() => {
    const events = ['blur', 'dragend', 'drop'] as const;
    events.forEach((event) => window.addEventListener(event, clearDrag));
    return () =>
      events.forEach((event) => window.removeEventListener(event, clearDrag));
  }, [clearDrag]);
  const active = dragging && !disabled;
  const hasFiles = (event: DragEvent) =>
    Array.from(event.dataTransfer.types).includes('Files');
  return (
    <div
      className="match-drop-zone"
      role="region"
      aria-label={t('app.dropZoneTitle')}
      aria-describedby={formatsId}
      aria-busy={disabled}
      data-drag-active={active}
      onDragEnter={(event) => {
        if (!hasFiles(event)) return;
        event.preventDefault();
        if (!disabled) {
          depth.current++;
          setDragging(true);
        }
      }}
      onDragOver={(event) => {
        if (!hasFiles(event)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = disabled ? 'none' : 'copy';
        if (!disabled) {
          depth.current = Math.max(1, depth.current);
          setDragging(true);
        }
      }}
      onDragLeave={() => {
        depth.current = Math.max(0, depth.current - 1);
        if (!depth.current) setDragging(false);
      }}
      onDrop={(event) => {
        event.preventDefault();
        event.stopPropagation();
        clearDrag();
        const files = Array.from(event.dataTransfer.files);
        if (!disabled && files.length) onFiles(files);
      }}
    >
      <ActionIcon name="import" size={24} />
      <span className="match-drop-zone-title" role="status" aria-live="polite">
        {disabled
          ? t('app.importRunning')
          : active
            ? t('app.dropZoneRelease')
            : t('app.dropZoneTitle')}
      </span>
      <span className="match-drop-zone-formats" id={formatsId}>
        MAT · XG · SGF · BGF · OGXM
      </span>
    </div>
  );
}
