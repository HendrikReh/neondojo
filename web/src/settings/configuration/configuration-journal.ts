import {
  isConfigurationDirty,
  normalizeConfiguration,
  type ProfileConfiguration,
} from './configuration-model';
import { ConfigurationConflict, isDecided } from './configuration-api';

export const configurationJournalKey = 'neondojo.configuration-pending.v1';

export type StorageAccess = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

export interface ConfigurationJournalEntry {
  baseRevision: number;
  configuration: ProfileConfiguration;
}

export function resolveConfigurationStorage(
  storage?: StorageAccess,
): StorageAccess | undefined {
  if (storage) return storage;
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

export function readConfigurationJournalRaw(
  storage?: StorageAccess,
): { ok: true; raw: string | null | undefined } | { ok: false } {
  try {
    return {
      ok: true,
      raw: resolveConfigurationStorage(storage)?.getItem(
        configurationJournalKey,
      ),
    };
  } catch {
    return { ok: false };
  }
}

export function parseConfigurationJournal(
  raw: string,
): ConfigurationJournalEntry {
  const saved = JSON.parse(raw) as {
    base_revision: number;
    configuration: unknown;
  };
  return {
    baseRevision: saved.base_revision,
    configuration: normalizeConfiguration(saved.configuration),
  };
}

export function writeConfigurationJournal(
  storage: StorageAccess | undefined,
  entry: ConfigurationJournalEntry | null,
): boolean {
  try {
    const target = resolveConfigurationStorage(storage);
    if (!target) return true;
    if (entry === null) {
      target.removeItem(configurationJournalKey);
    } else {
      target.setItem(
        configurationJournalKey,
        JSON.stringify({
          base_revision: entry.baseRevision,
          configuration: entry.configuration,
        }),
      );
    }
    return true;
  } catch {
    return false;
  }
}

export async function recoverConfigurationJournal({
  raw,
  serverRevision,
  serverConfiguration,
  onRecovered,
  onConflict,
  onFlush,
  onClean,
  onStorageError,
}: {
  raw: string | null | undefined;
  serverRevision: number;
  serverConfiguration: ProfileConfiguration;
  onRecovered: (configuration: ProfileConfiguration) => void;
  onConflict: (baseRevision: number, error: ConfigurationConflict) => void;
  onFlush: () => Promise<void>;
  onClean: () => void;
  onStorageError: () => void;
}) {
  if (!raw) return;
  try {
    const saved = parseConfigurationJournal(raw);
    if (isConfigurationDirty(serverConfiguration, saved.configuration)) {
      onRecovered(saved.configuration);
      if (saved.baseRevision !== serverRevision) {
        onConflict(
          saved.baseRevision,
          new ConfigurationConflict(
            'journal_conflict',
            'Saved local changes conflict with newer settings',
          ),
        );
      } else {
        await onFlush();
      }
    } else {
      onClean();
    }
  } catch (error) {
    if (isDecided(error)) throw error;
    onStorageError();
  }
}
