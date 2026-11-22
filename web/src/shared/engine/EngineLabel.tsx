import Tooltip from '../ui/Tooltip';
import { shortEngineLabel } from './engine-version';

/** Displays the engine identity while keeping the full provenance accessible. */
export default function EngineLabel({
  details,
  tooltip,
  format,
}: {
  details?: string | null;
  tooltip?: string;
  format?: (engine: string) => string;
}) {
  if (!details) return null;
  const engine = shortEngineLabel(details);
  const label = format?.(engine) ?? engine;
  const description = tooltip ?? (engine !== details ? details : undefined);
  return description ? (
    <Tooltip
      as="span"
      className="engine-label-details"
      text={description}
      tabIndex={0}
    >
      {label}
    </Tooltip>
  ) : (
    label
  );
}
