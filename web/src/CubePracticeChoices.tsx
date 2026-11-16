import { useState, type ReactNode } from 'react';
import ActionButton from './ActionButton';
import type { PracticeAnswer } from './learning-api';
import { useI18n } from './i18n';
import { useLearningText } from './learning-messages';
import { usePlayText } from './play-messages';

const cubeOfferChoices = [
  {
    answer: { kind: 'cube_offer', action: 'no_double' },
    icon: 'roll',
    key: 'N',
  },
  {
    answer: { kind: 'cube_offer', action: 'double' },
    icon: 'double',
    key: 'D',
  },
] as const;
const cubeResponseChoices = [
  {
    answer: { kind: 'cube_response', action: 'take' },
    icon: 'take',
    key: 'T',
  },
  {
    answer: { kind: 'cube_response', action: 'drop' },
    icon: 'drop',
    key: 'P',
  },
] as const;
type CubeAnswer = Exclude<PracticeAnswer, { kind: 'checker' }>;

export default function CubePracticeChoices({
  kind,
  label,
  busy,
  onCheck,
  children,
}: {
  kind: CubeAnswer['kind'];
  label: string;
  busy: boolean;
  onCheck: (answer: CubeAnswer) => void;
  children?: ReactNode;
}) {
  const [draft, setDraft] = useState<CubeAnswer>();
  const t = useLearningText();
  const play = usePlayText();
  const { formatDecision } = useI18n();
  const choices =
    kind === 'cube_offer' ? cubeOfferChoices : cubeResponseChoices;
  return (
    <>
      <div
        className="play-actions cube-practice-choices"
        role="group"
        aria-label={label}
      >
        {choices.map(({ answer, icon, key }) => {
          const action = answer.action;
          const title =
            action === 'no_double' ? formatDecision('no double') : play(action);
          return (
            <ActionButton
              type="button"
              key={action}
              icon={icon}
              label={title}
              shortcut={key}
              aria-pressed={draft?.action === action}
              disabled={busy}
              onClick={() => setDraft(answer)}
            >
              {title}
            </ActionButton>
          );
        })}
      </div>
      <div className="play-actions">
        <ActionButton
          type="button"
          icon="confirm"
          label={t('checkAnswer')}
          shortcut="Enter"
          disabled={busy || !draft}
          onClick={() => draft && onCheck(draft)}
        >
          {t('checkAnswer')}
        </ActionButton>
        {children}
      </div>
    </>
  );
}
