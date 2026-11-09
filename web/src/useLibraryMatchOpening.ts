import { useEffect, useRef, useState } from 'react';
import { request, type Match, type Page, type Position } from './api';
import type { LibraryMatch } from './library-api';
import { AppError } from './i18n-core';

/** Filtered rows open their first exact occurrence; newer user intent wins. */
export function useLibraryMatchOpening({
  scope,
  positionId,
  visible,
  onOpen,
  onPosition,
}: {
  scope: string;
  positionId: string;
  visible: boolean;
  onOpen: (match: Match) => void;
  onPosition: (position: Position) => void;
}) {
  const key = JSON.stringify([scope, visible]);
  const context = useRef({ key, epoch: 0 });
  if (context.current.key !== key)
    context.current = { key, epoch: context.current.epoch + 1 };
  const epoch = context.current.epoch;
  const callbacks = useRef({ onOpen, onPosition });
  callbacks.current = { onOpen, onPosition };
  const operation = useRef<
    { epoch: number; controller: AbortController } | undefined
  >(undefined);
  const [pending, setPending] = useState<{ epoch: number; id: string }>();
  const [failure, setFailure] = useState<{ epoch: number; error: unknown }>();
  useEffect(
    () => () => {
      if (operation.current?.epoch === epoch)
        operation.current.controller.abort();
    },
    [epoch],
  );

  async function open(match: LibraryMatch) {
    if (!visible || context.current.epoch !== epoch) return;
    operation.current?.controller.abort();
    operation.current = undefined;
    setPending(undefined);
    setFailure(undefined);
    if (!positionId) {
      callbacks.current.onOpen(match);
      return;
    }
    const controller = new AbortController();
    const current = { epoch, controller };
    operation.current = current;
    setPending({ epoch, id: match.id });
    const live = () =>
      !controller.signal.aborted &&
      context.current.epoch === epoch &&
      operation.current === current;
    try {
      const params = new URLSearchParams({
        match_id: match.id,
        position_id: positionId,
        limit: '1',
      });
      const page = await request<Page<Position>>(`/positions?${params}`, {
        signal: controller.signal,
      });
      if (!live()) return;
      const position = page.items[0];
      if (!position) throw new AppError('error.positionMissing');
      if (
        position.match_uuid !== match.id ||
        position.position_id !== positionId
      )
        throw new AppError('error.invalidResponse');
      callbacks.current.onPosition(position);
    } catch (error) {
      if (live()) setFailure({ epoch, error });
    } finally {
      if (live()) setPending(undefined);
    }
  }
  return {
    open,
    openingId: pending?.epoch === epoch ? pending.id : undefined,
    error: failure?.epoch === epoch ? failure.error : undefined,
  };
}
