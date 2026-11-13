import { useI18n } from './i18n';
import {
  normalizeConfiguration,
  type ProfileConfiguration,
} from './configuration-model';
import {
  profileFieldKey,
  useProfileText,
  type ProfileMessageKey,
} from './profile-messages';
import { themes } from './themes';
import { languages } from './i18n-core';
import { ALL_CARDS, CARDS_WITHOUT_DECK } from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import { LIBRARY_COLUMNS } from './library-columns';
import { useEffect, useId, useState } from 'react';
import { request } from './api';
import ActionButton from './ActionButton';
import { profileChangeRows } from './profile-changes';

export function configurationSummaryRows(configuration: ProfileConfiguration) {
  const rows: { path: string; value: unknown }[] = [];
  function visit(value: unknown, path: string) {
    if (value && typeof value === 'object' && !Array.isArray(value))
      for (const [key, v] of Object.entries(value)) {
        if (key !== 'schema_version') visit(v, path ? `${path}.${key}` : key);
      }
    else rows.push({ path, value });
  }
  visit(normalizeConfiguration(configuration), '');
  return rows;
}
export default function ProfileSummary({
  configuration,
  compareTo,
  differencesOnly = false,
  onResetField,
  resetDisabled = false,
}: {
  configuration: ProfileConfiguration;
  compareTo?: ProfileConfiguration;
  differencesOnly?: boolean;
  onResetField?: (path: string) => void;
  resetDisabled?: boolean;
}) {
  const t = useProfileText(),
    { locale } = useI18n();
  const differencesHeading = useId();
  const searchText = useCardSearchText();
  const [resourceNames, setResourceNames] = useState<Record<string, string>>(
    {},
  );
  useEffect(() => {
    const controller = new AbortController();
    void Promise.allSettled([
      request<{ id: string; name: string }[]>('/databases', {
        signal: controller.signal,
      }),
      request<{ id: string; name: string }[]>('/decks', {
        signal: controller.signal,
      }),
    ]).then((results) => {
      if (controller.signal.aborted) return;
      const names: Record<string, string> = {};
      for (const result of results)
        if (result.status === 'fulfilled')
          for (const resource of result.value)
            names[resource.id] = resource.name;
      setResourceNames(names);
    });
    return () => controller.abort();
  }, [configuration.library.database_id, configuration.learning.deck_id]);
  const label = (path: string) => {
    const key = profileFieldKey(path);
    if (key) return t(key);
    const leaf = path.split('.').at(-1)!;
    const column = LIBRARY_COLUMNS.find((c) => c.id === leaf);
    return column ? leaf.replaceAll('_', ' ') : leaf.replaceAll('_', ' ');
  };
  const fieldLabel = (path: string) => {
    const parts = path.split('.');
    const context =
      parts.length > 2 && ['column_widths', 'generation'].includes(parts[1])
        ? t(parts[1] as ProfileMessageKey)
        : undefined;
    return [t(parts[0] as ProfileMessageKey), context, label(path)]
      .filter(Boolean)
      .join(' · ');
  };
  const format = (value: unknown, path: string): string => {
    if (value === null || value === undefined || value === '')
      return path.includes('column_widths') ? t('automatic') : t('empty');
    if (typeof value === 'boolean') return t(value ? 'yes' : 'no');
    if (path.endsWith('.human_player'))
      return t(value === 0 ? 'white' : 'black');
    if (typeof value === 'number')
      return new Intl.NumberFormat(locale).format(value);
    if (
      Array.isArray(value) &&
      (path === 'positions.definition.dice' || path.endsWith('_range'))
    ) {
      const numberOrText = (part: unknown) => {
        const numeric = Number(part);
        return Number.isFinite(numeric) && !path.endsWith('date_range')
          ? new Intl.NumberFormat(locale).format(numeric)
          : String(part);
      };
      const [from, to] = value;
      if (from == null && to == null) return t('empty');
      if (path.endsWith('.dice'))
        return value
          .filter((part) => part != null)
          .map(numberOrText)
          .join(' · ');
      return from == null
        ? `≤ ${numberOrText(to)}`
        : to == null
          ? `≥ ${numberOrText(from)}`
          : `${numberOrText(from)} – ${numberOrText(to)}`;
    }
    if (Array.isArray(value))
      return value.map((v) => label(String(v))).join(', ');
    if (typeof value === 'object')
      return Object.entries(value)
        .map(
          ([key, entry]) => `${label(key)}: ${format(entry, `${path}.${key}`)}`,
        )
        .join(' · ');
    const s = String(value);
    if (path.endsWith('.engine')) {
      if (s === 'gnubg') return 'GNUbg';
      if (s === 'bgsage') return 'Open Sage';
    }
    if (s === ALL_CARDS) return t('allCards');
    if (s === CARDS_WITHOUT_DECK) return searchText('withoutDeck');
    if (path.endsWith('.locale'))
      return languages.find((l) => l.id === s)?.name ?? s;
    if (path.endsWith('board_theme'))
      return themes.find((theme) => theme.id === s)?.name ?? s;
    if (path.endsWith('.database_id') || path.endsWith('.deck_id'))
      return resourceNames[s] ?? t('unavailableReference');
    const enumFields = [
      'engine',
      'scope',
      'search_filter',
      'default_mode',
      'mode',
      'bearoff_direction',
      'board_view',
      'provider',
      'sort',
      'direction',
      'kind',
      'status',
      'severity',
    ];
    const key = profileFieldKey(s);
    return enumFields.includes(path.split('.').at(-1)!) && key ? t(key) : s;
  };
  const rows = differencesOnly ? [] : configurationSummaryRows(configuration);
  const groups = [...new Set(rows.map((row) => row.path.split('.')[0]))];
  const differences = compareTo
    ? profileChangeRows(configuration, compareTo)
    : [];
  return (
    <div
      className={`profile-summary${differencesOnly ? ' profile-summary--differences' : ''}`}
    >
      {compareTo && (
        <section
          aria-labelledby={differencesOnly ? undefined : differencesHeading}
        >
          {!differencesOnly && (
            <h3 id={differencesHeading}>{t('differences')}</h3>
          )}
          {differences.length ? (
            <div className="profile-differences">
              <table>
                <thead>
                  <tr>
                    <th scope="col">{t('field')}</th>
                    <th scope="col">{t('saved')}</th>
                    <th scope="col">{t('current')}</th>
                  </tr>
                </thead>
                <tbody>
                  {differences.map((d) => (
                    <tr key={d.path}>
                      <th scope="row">{fieldLabel(d.path)}</th>
                      <td>{format(d.saved, d.path)}</td>
                      <td>
                        <div className="profile-current-value">
                          <span data-profile-current-value>
                            {format(d.current, d.path)}
                          </span>
                          {onResetField && (
                            <ActionButton
                              type="button"
                              icon="undo"
                              compact
                              className="profile-field-reset"
                              label={t('resetField', {
                                name: fieldLabel(d.path),
                              })}
                              disabled={resetDisabled}
                              onClick={() => onResetField(d.path)}
                            />
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p>{t('noDifferences')}</p>
          )}
        </section>
      )}
      {!differencesOnly && (
        <details className="profile-saved-configuration">
          <summary>{t('summary')}</summary>
          {groups.map((group) => (
            <details key={group}>
              <summary>{t(group as ProfileMessageKey)}</summary>
              <dl>
                {rows
                  .filter((r) => r.path.startsWith(`${group}.`))
                  .map((row) => (
                    <div key={row.path}>
                      <dt>{label(row.path)}</dt>
                      <dd>{format(row.value, row.path)}</dd>
                    </div>
                  ))}
              </dl>
            </details>
          ))}
        </details>
      )}
    </div>
  );
}
