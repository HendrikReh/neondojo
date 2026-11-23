import {
  Component,
  Suspense,
  useEffect,
  useState,
  type ReactNode,
} from 'react';
import { useWorkspaceText } from '../shared/i18n/workspace-messages';
import ActionButton from '../shared/ui/ActionButton';

function LoadFailed() {
  const w = useWorkspaceText();
  return (
    <div className="error" role="alert">
      {w('chunkFailed')}
      <ActionButton
        onClick={() => window.location.reload()}
        icon="refresh"
        label={w('reloadPage')}
        compact
      />
    </div>
  );
}

function Loading() {
  const w = useWorkspaceText();
  return <p role="status">{w('loading')}</p>;
}

/**
 * Catches a failed chunk load, for example after NeonDojo was redeployed
 * while this page still refers to the files of the previous build.
 */
class ChunkBoundary extends Component<
  { children: ReactNode },
  { error?: unknown }
> {
  state: { error?: unknown } = {};

  static getDerivedStateFromError(error: unknown) {
    return { error };
  }

  render() {
    const { error } = this.state;
    if (error === undefined) return this.props.children;
    // Only a missing chunk is handled here; other errors keep propagating.
    if (!isChunkLoadError(error)) throw error;
    return <LoadFailed />;
  }
}

export function isChunkLoadError(error: unknown) {
  return (
    error instanceof Error &&
    /dynamically imported module|Importing a module script failed|Unable to preload CSS/i.test(
      error.message,
    )
  );
}

/**
 * A part of the app that is loaded on demand: shows a loading line while its
 * code arrives and a reload hint if it cannot be loaded. `quiet` renders
 * nothing while loading, for content that is not visible yet (a closed dialog).
 */
export default function LazyPanel({
  children,
  quiet = false,
}: {
  children: ReactNode;
  quiet?: boolean;
}) {
  return (
    <ChunkBoundary>
      <Suspense fallback={quiet ? null : <Loading />}>{children}</Suspense>
    </ChunkBoundary>
  );
}

/**
 * Passes `visible` on only after its content has mounted once, hidden. The
 * workspaces that used to mount at start-up rely on that order: their mount
 * effects run while hidden, and loading starts when they become visible.
 * Without it, a workspace that mounts visible starts loading in its mount
 * effects, which StrictMode runs twice in development.
 */
export function MountHidden({
  visible,
  children,
}: {
  visible: boolean;
  children: (visible: boolean) => ReactNode;
}) {
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);
  return children(visible && mounted);
}
