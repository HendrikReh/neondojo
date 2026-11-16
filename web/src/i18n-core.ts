import { catalogs, type Catalog, type TranslationKey } from './i18n-catalogs';
import { compactCheckerNotation, type CheckerPosition } from './move-preview';

export type Locale = keyof typeof catalogs;
export type TranslationParams = Record<string, string | number>;
export const defaultLocale: Locale = 'en';
export const localeStorageKey = 'bgtutor.locale';
export const matchEventLabels = {
  en: 'Event',
  de: 'Event',
  es: 'Evento',
  fr: 'Événement',
  pl: 'Wydarzenie',
  it: 'Evento',
} satisfies Record<Locale, string>;
export const languages: ReadonlyArray<{ id: Locale; name: string }> = [
  { id: 'de', name: 'Deutsch' },
  { id: 'en', name: 'English' },
  { id: 'es', name: 'Español' },
  { id: 'fr', name: 'Français' },
  { id: 'it', name: 'Italiano' },
  { id: 'pl', name: 'Polski' },
];

export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && Object.hasOwn(catalogs, value);
}

export function readLocale(storage?: Pick<Storage, 'getItem'>): Locale {
  try {
    const value = (storage ?? localStorage).getItem(localeStorageKey);
    return isLocale(value) ? value : defaultLocale;
  } catch {
    return defaultLocale;
  }
}

export function saveLocale(locale: Locale, storage?: Pick<Storage, 'setItem'>) {
  try {
    (storage ?? localStorage).setItem(localeStorageKey, locale);
  } catch {
    // Language switching remains available when browser storage is disabled.
  }
}

const placeholderPattern = /\{([A-Za-z][A-Za-z0-9]*)\}/g;
const pluralPattern = /\{([A-Za-z][A-Za-z0-9]*), plural,/g;
/** Parameter names a message needs, including the count of a plural block. */
export const placeholders = (text: string) =>
  [
    ...new Set(
      [...text.matchAll(placeholderPattern), ...text.matchAll(pluralPattern)]
        .map((match) => match[1])
        .filter((name): name is string => name !== undefined),
    ),
  ].sort();

/**
 * Plural forms a counted message must supply. Polish distinguishes
 * 1 karta / 2 karty / 5 kart; the other languages only one and other.
 * A category without its own form uses `other`, then `many`.
 */
export const requiredPluralCategories: Record<
  Locale,
  readonly Intl.LDMLPluralRule[]
> = {
  en: ['one', 'other'],
  de: ['one', 'other'],
  es: ['one', 'other'],
  fr: ['one', 'other'],
  it: ['one', 'other'],
  pl: ['one', 'few', 'many'],
};

type PluralBlock = {
  start: number;
  end: number;
  name: string;
  forms: Partial<Record<Intl.LDMLPluralRule, string>>;
};

const pluralCategoryNames: ReadonlySet<string> = new Set([
  'zero',
  'one',
  'two',
  'few',
  'many',
  'other',
]);
const isPluralCategory = (value: string): value is Intl.LDMLPluralRule =>
  pluralCategoryNames.has(value);

/** Index just past the brace that closes the one opened at `open`. */
function closingBrace(template: string, open: number) {
  let depth = 0;
  for (let index = open; index < template.length; index += 1) {
    if (template[index] === '{') depth += 1;
    else if (template[index] === '}' && --depth === 0) return index + 1;
  }
  throw new Error(`Unclosed plural message: ${template}`);
}

/**
 * Finds `{count, plural, one {# card} other {# cards}}` blocks. Inside a form,
 * `#` is the count in the language's number format and other placeholders
 * stay available.
 */
function pluralBlocks(template: string): PluralBlock[] {
  const blocks: PluralBlock[] = [];
  let from = 0;
  for (const match of template.matchAll(pluralPattern)) {
    const name = match[1];
    if (name === undefined || match.index < from) continue;
    const end = closingBrace(template, match.index);
    const forms: PluralBlock['forms'] = {};
    let cursor = match.index + match[0].length;
    while (cursor < end - 1) {
      const open = template.indexOf('{', cursor);
      const category = template.slice(cursor, open < 0 ? end - 1 : open).trim();
      if (open < 0 || open >= end - 1) {
        if (category) throw new Error(`Invalid plural message: ${template}`);
        break;
      }
      if (!isPluralCategory(category))
        throw new Error(`Unknown plural category "${category}": ${template}`);
      const close = closingBrace(template, open);
      forms[category] = template.slice(open + 1, close - 1);
      cursor = close;
    }
    blocks.push({ start: match.index, end, name, forms });
    from = end;
  }
  return blocks;
}

const pluralRules = new Map<Locale, Intl.PluralRules>();
/** The CLDR plural category of a count: the one place that decides it. */
export function pluralCategory(locale: Locale, count: number) {
  let rules = pluralRules.get(locale);
  if (!rules) {
    rules = new Intl.PluralRules(locale);
    pluralRules.set(locale, rules);
  }
  return rules.select(count);
}

/** Replaces each plural block with the form for its count in this language. */
export function selectPluralForms(
  locale: Locale,
  template: string,
  params: TranslationParams = {},
) {
  const blocks = pluralBlocks(template);
  if (!blocks.length) return template;
  let result = '';
  let from = 0;
  for (const { start, end, name, forms } of blocks) {
    const count = params[name];
    if (typeof count !== 'number')
      throw new Error(`Plural parameter must be a number: ${name}`);
    const form =
      forms[pluralCategory(locale, count)] ?? forms.other ?? forms.many;
    if (form === undefined)
      throw new Error(`Missing plural form: ${locale} ${template}`);
    result +=
      template.slice(from, start) +
      form.replaceAll('#', new Intl.NumberFormat(locale).format(count));
    from = end;
  }
  return result + template.slice(from);
}

/** Throws if a plural block lacks a form this language needs. */
export function validatePluralForms(locale: Locale, template: string) {
  for (const { forms } of pluralBlocks(template)) {
    for (const category of requiredPluralCategories[locale]) {
      if (forms[category] === undefined)
        throw new Error(
          `Missing plural form "${category}": ${locale} ${template}`,
        );
    }
  }
}

/**
 * Fills placeholders. Messages with plural blocks need the locale, and the
 * counted parameter must be passed as a number.
 */
export function interpolate(
  template: string,
  params: TranslationParams = {},
  locale?: Locale,
) {
  let text = template;
  if (locale) text = selectPluralForms(locale, template, params);
  else if (template.includes(', plural,'))
    throw new Error(`Plural message needs a locale: ${template}`);
  return text.replace(placeholderPattern, (_, key: string) => {
    if (!Object.hasOwn(params, key))
      throw new Error(`Missing translation parameter: ${key}`);
    return String(params[key]);
  });
}

/** Like `interpolate`, with numbers written the way the language writes them. */
export function formatMessage(
  locale: Locale,
  template: string,
  params: TranslationParams = {},
) {
  const number = new Intl.NumberFormat(locale);
  return interpolate(
    selectPluralForms(locale, template, params),
    Object.fromEntries(
      Object.entries(params).map(([name, value]) => [
        name,
        typeof value === 'number' ? number.format(value) : value,
      ]),
    ),
  );
}

/** Fail early if a translated sentence loses an argument or a catalog key. */
export function validateCatalogs(all: Record<Locale, Catalog>) {
  const keys = Object.keys(all.de) as TranslationKey[];
  for (const [locale, catalog] of Object.entries(all)) {
    if (Object.keys(catalog).length !== keys.length)
      throw new Error(`Incomplete catalog: ${locale}`);
    for (const key of keys) {
      if (
        !catalog[key] ||
        placeholders(catalog[key]).join(',') !==
          placeholders(all.de[key]).join(',')
      ) {
        throw new Error(`Invalid translation placeholders: ${locale}.${key}`);
      }
      validatePluralForms(locale as Locale, catalog[key]);
    }
  }
}
validateCatalogs(catalogs);

export function translate(
  locale: Locale,
  key: TranslationKey,
  params: TranslationParams = {},
) {
  return formatMessage(locale, catalogs[locale][key], params);
}

/** Keep errors as structured data so a visible error also changes language. */
export class AppError extends Error {
  constructor(
    readonly key: TranslationKey,
    readonly params: TranslationParams = {},
    readonly diagnostic = '',
  ) {
    super(diagnostic || key);
    this.name = 'AppError';
  }
}

export function formatLocalizedError(locale: Locale, error: unknown): string {
  if (!error) return '';
  if (error instanceof AppError) {
    const message = translate(locale, error.key, error.params);
    return error.diagnostic ? `${message} ${error.diagnostic}` : message;
  }
  const diagnostic = error instanceof Error ? error.message : String(error);
  return `${translate(locale, 'error.generic')} ${diagnostic}`;
}

const decisionKeys: Record<string, TranslationKey> = {
  'no double': 'decision.noDouble',
  double: 'decision.double',
  take: 'decision.take',
  pass: 'decision.drop',
  drop: 'decision.drop',
  'cannot move': 'decision.cannotMove',
  dance: 'decision.cannotMove',
};

export function localizeDecision(
  locale: Locale,
  notation: string,
  position?: CheckerPosition,
) {
  const key = decisionKeys[notation.trim().toLowerCase()];
  return key
    ? translate(locale, key)
    : compactCheckerNotation(notation, position);
}
