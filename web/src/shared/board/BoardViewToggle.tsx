import type { ReactNode } from 'react';
import ActionButton from '../ui/ActionButton';
import type { BoardView } from './move-preview';
import { useBoardReviewText } from './board-review-messages';

export interface BoardViewOption<T extends string> {
  value: T;
  label: ReactNode;
  disabled?: boolean;
}

export function BoardViewToggle<T extends string>({
  label,
  options,
  value,
  onChange,
  trailing,
}: {
  label: string;
  options: readonly BoardViewOption<T>[];
  value: T;
  onChange: (next: T) => void;
  trailing?: ReactNode;
}) {
  return (
    <div
      className="board-view-buttons text-navigation text-navigation--compact"
      role="group"
      aria-label={label}
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={value === option.value}
          disabled={option.disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
      {trailing}
    </div>
  );
}

const boardViews: readonly BoardView[] = ['before', 'arrows', 'after'];

export function MovePreviewViewToggle({
  value,
  onChange,
  canPreview = true,
  onReset,
  resetDisabled = false,
}: {
  value: BoardView;
  onChange: (next: BoardView) => void;
  canPreview?: boolean;
  onReset?: () => void;
  resetDisabled?: boolean;
}) {
  const review = useBoardReviewText();
  return (
    <BoardViewToggle
      label={review('views')}
      value={canPreview ? value : 'before'}
      onChange={onChange}
      options={boardViews.map((mode) => ({
        value: mode,
        label: review(mode),
        disabled: mode !== 'before' && !canPreview,
      }))}
      trailing={
        onReset && (
          <ActionButton
            type="button"
            disabled={resetDisabled}
            onClick={onReset}
            icon="reset"
            label={review('reset')}
            compact
          />
        )
      }
    />
  );
}
