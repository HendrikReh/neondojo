import {
  configurationDifferences,
  normalizeConfiguration,
  type ConfigurationDifference,
  type ProfileConfiguration,
} from './configuration-model';

// A pair is one user-facing filter: restoring only an endpoint can reorder
// native dice or make a previously valid interval invalid.
const fieldGroups = [
  { path: 'positions.definition.dice', keys: ['die1', 'die2'] },
  {
    path: 'positions.definition.match_points_range',
    keys: ['match_points_min', 'match_points_max'],
  },
  { path: 'positions.definition.loss_range', keys: ['min_loss', 'max_loss'] },
  {
    path: 'library.filters.match_points_range',
    keys: ['match_points_min', 'match_points_max'],
  },
  { path: 'library.filters.date_range', keys: ['date_from', 'date_to'] },
].map((group) => ({
  ...group,
  fields: group.keys.map(
    (key) => `${group.path.slice(0, group.path.lastIndexOf('.'))}.${key}`,
  ),
}));

function valueAt(configuration: ProfileConfiguration, path: string): unknown {
  let value: unknown = configuration;
  for (const key of path.split('.'))
    value = (value as Record<string, unknown>)?.[key];
  return value === '' || value === undefined ? null : value;
}

/** Visibility membership is independent of the separately listed column order. */
export function profileChangeRows(
  saved: ProfileConfiguration,
  current: ProfileConfiguration,
): ConfigurationDifference[] {
  const source = normalizeConfiguration(saved);
  const target = normalizeConfiguration(current);
  const rows: ConfigurationDifference[] = [];
  const included = new Set<string>();
  for (const difference of configurationDifferences(source, target)) {
    if (
      difference.path === 'library.visible_columns' &&
      JSON.stringify([...source.library.visible_columns].sort()) ===
        JSON.stringify([...target.library.visible_columns].sort())
    )
      continue;
    const group = fieldGroups.find((entry) =>
      entry.fields.includes(difference.path),
    );
    if (!group) rows.push(difference);
    else if (!included.has(group.path)) {
      included.add(group.path);
      rows.push({
        path: group.path,
        saved: group.fields.map((field) => valueAt(source, field)),
        current: group.fields.map((field) => valueAt(target, field)),
      });
    }
  }
  return rows;
}

/** Restore an actual difference, retaining absent optional values as absent. */
export function restoreConfigurationField(
  saved: ProfileConfiguration,
  current: ProfileConfiguration,
  path: string,
): ProfileConfiguration {
  const source = normalizeConfiguration(saved);
  const restored = normalizeConfiguration(current);
  if (
    !profileChangeRows(source, restored).some(
      (difference) => difference.path === path,
    )
  )
    return restored;
  const fields = fieldGroups.find((group) => group.path === path)?.fields ?? [
    path,
  ];
  for (const field of fields) {
    const keys = field.split('.');
    let savedParent = source as unknown as Record<string, unknown>;
    let currentParent = restored as unknown as Record<string, unknown>;
    for (const key of keys.slice(0, -1)) {
      savedParent = savedParent[key] as Record<string, unknown>;
      currentParent = currentParent[key] as Record<string, unknown>;
    }
    const key = keys.at(-1)!;
    if (Object.hasOwn(savedParent, key))
      currentParent[key] = structuredClone(savedParent[key]);
    else delete currentParent[key];
  }
  return normalizeConfiguration(restored);
}
