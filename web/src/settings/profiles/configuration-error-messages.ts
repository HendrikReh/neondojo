import { useCallback } from 'react';
import {
  ConflictError,
  HttpError,
  type ErrorIssue,
} from '../../shared/http/api';
import { formatLocalizedError, type Locale } from '../../shared/i18n/i18n-core';
import { useI18n } from '../../shared/i18n/i18n';
import { defineMessages } from '../../shared/i18n/messages';
import { profileFieldKey, profileText } from './profile-messages';

const errorMessages = defineMessages('configuration-error');
const issueMessages = defineMessages('configuration-issue');

type ErrorCode = keyof typeof errorMessages.catalogs.en;
type IssueCode = keyof typeof issueMessages.catalogs.en;

export const hasNameIssue = (error: unknown) =>
  error instanceof HttpError &&
  (error.code === 'duplicate_name' ||
    error.issues.some((issue) => issue.path === 'name'));

/** The message of an error in the interface language, without its issues. */
export function configurationErrorText(locale: Locale, error: unknown) {
  if (error instanceof HttpError || error instanceof ConflictError) {
    if (error.code !== 'duplicate_name' && hasNameIssue(error))
      return profileText(locale, 'invalidName');
    if (Object.hasOwn(errorMessages.catalogs[locale], error.code))
      return errorMessages.text(locale, error.code as ErrorCode);
  }
  return formatLocalizedError(locale, error);
}

/** What is wrong with a configuration value, in the interface language. */
export function configurationIssueText(locale: Locale, issue: ErrorIssue) {
  const code =
    issue.code === 'engine_unavailable' && issue.engine
      ? `${issue.engine}_unavailable`
      : issue.code;
  return Object.hasOwn(issueMessages.catalogs[locale], code)
    ? issueMessages.text(locale, code as IssueCode, { detail: issue.message })
    : issue.message;
}

/** The section and name of a configuration value, for example "Play · Engine". */
export function configurationPathText(locale: Locale, path: string) {
  const parts = path.split('.');
  return [...new Set([parts[0], parts.at(-1)!])]
    .map((part) => {
      const key = profileFieldKey(locale, part);
      return key ? profileText(locale, key) : part;
    })
    .join(' · ');
}

export function useConfigurationErrors() {
  const { locale } = useI18n();
  return {
    message: useCallback(
      (error: unknown) => configurationErrorText(locale, error),
      [locale],
    ),
    /** A configuration value and what is wrong with it. */
    issue: useCallback(
      (issue: ErrorIssue) =>
        `${configurationPathText(locale, issue.path)}: ${configurationIssueText(locale, issue)}`,
      [locale],
    ),
    issues: (error: unknown): readonly ErrorIssue[] =>
      // The name dialog and message already say what is wrong with a name.
      error instanceof HttpError && !hasNameIssue(error) ? error.issues : [],
  };
}
