import type { ReactNode, RefObject } from 'react';
import { usePlayText } from './play-messages';
import { useI18n } from './i18n';
import PlayBoardDialog from './PlayBoardDialog';

export default function PlayCubeDecision({
  anchor,
  cube,
  busy,
  children,
}: {
  anchor: RefObject<HTMLDivElement | null>;
  cube: number;
  busy: boolean;
  children: ReactNode;
}) {
  const t = usePlayText(),
    { formatNumber } = useI18n();
  return (
    <PlayBoardDialog
      anchor={anchor}
      heading={t('cubeDecision')}
      busy={busy}
      className="play-cube-dialog"
    >
      <p>{t('double_response')}</p>
      <div className="play-cube-offer">
        <span>
          {t('cubeBefore')} {formatNumber(cube)}
        </span>
        <strong>{formatNumber(cube * 2)}</strong>
        <span>{t('cubeOffered')}</span>
      </div>
      <p role={busy ? 'status' : undefined}>
        {busy ? t('thinking') : t('cubeDecisionRequired')}
      </p>
      {children}
    </PlayBoardDialog>
  );
}
