import { expect, test } from 'vitest';
import { databaseBackupMessages } from './database-backup-messages';

test('the restore review action names the replacement being confirmed', () => {
  expect(databaseBackupMessages.de.review).toBe('Wiederherstellung ansehen');
  expect(databaseBackupMessages.en.review).toBe('Review database replacement');
});
