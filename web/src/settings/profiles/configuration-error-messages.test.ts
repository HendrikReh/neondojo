import { expect, it } from 'vitest';
import { AppError } from '../../shared/i18n/i18n-core';
import { HttpError } from '../../shared/http/api';
import { ConfigurationConflict } from '../configuration/configuration-api';
import {
  configurationErrorText,
  configurationIssueText,
  configurationPathText,
  hasNameIssue,
} from './configuration-error-messages';

const http = (code: string, issues: HttpError['issues'] = []) =>
  new HttpError(422, 'English server text', code, issues);

it.each([
  'duplicate_name',
  'active_profile',
  'profile_not_found',
  'revision_conflict',
  'unavailable_resources',
  'invalid_configuration',
  'internal_error',
])('translates the backend code %s without the server text', (code) => {
  const texts = (['en', 'de'] as const).map((locale) =>
    configurationErrorText(locale, http(code)),
  );
  expect(new Set(texts).size).toBe(texts.length);
  for (const text of texts) expect(text).not.toContain('English server text');
});

it.each([
  'uncertain_result',
  'journal_conflict',
  'stale_profile',
  'remote_conflict',
])('translates the client conflict %s', (code) => {
  const error = new ConfigurationConflict(code, 'English client text');
  expect(configurationErrorText('de', error)).not.toContain('English');
});

it('shows the details of an unknown code and of other errors', () => {
  expect(configurationErrorText('de', http('new_code'))).toBe(
    'Anfrage fehlgeschlagen (HTTP 422). English server text',
  );
  expect(configurationErrorText('de', new AppError('error.network'))).toBe(
    'Verbindung zum Backend fehlgeschlagen.',
  );
  expect(configurationErrorText('de', new Error('Broken'))).toBe(
    'Ein Fehler ist aufgetreten. Broken',
  );
});

it('says which engine is unavailable', () => {
  const issue = (engine?: 'gnubg' | 'bgsage') =>
    configurationIssueText('en', {
      path: 'analysis.engine',
      code: 'engine_unavailable',
      message: 'Server text',
      engine,
    });
  expect(issue('gnubg')).toContain('GNUbg');
  expect(issue('bgsage')).toContain('NeonGammon');
  expect(issue()).toBe('The engine is not available.');
});

it('keeps the server text where it is the only detail', () => {
  const issue = {
    path: 'play.plies',
    code: 'invalid_value',
    message: 'Too deep',
  };
  expect(configurationIssueText('de', issue)).toBe('Ungültiger Wert: Too deep');
  expect(configurationIssueText('de', { ...issue, code: 'new_code' })).toBe(
    'Too deep',
  );
});

it('names the section and the value of an issue', () => {
  expect(configurationPathText('de', 'play.engine')).not.toBe(
    configurationPathText('de', 'analysis.engine'),
  );
  expect(configurationPathText('en', 'analysis.engine')).toBe(
    'Analysis · Engine',
  );
  expect(configurationPathText('en', 'name')).not.toContain('·');
  expect(configurationPathText('en', 'unknown.value')).toBe('unknown · value');
});

it('recognizes a refused profile name', () => {
  const invalid = http('invalid_configuration', [
    { path: 'name', code: 'invalid_value', message: 'Too long' },
  ]);
  expect(hasNameIssue(invalid)).toBe(true);
  expect(hasNameIssue(http('duplicate_name'))).toBe(true);
  expect(hasNameIssue(http('internal_error'))).toBe(false);
  expect(configurationErrorText('en', invalid)).toBe(
    'Enter a name with 1–80 characters.',
  );
});
