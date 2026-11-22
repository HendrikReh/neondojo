const versionPattern = /^v?\d+(?:\.\d+)+(?:[-+][\w.+-]+)?(?=$|[\s;(])/i;
const engineNamePattern =
  /^(GNU Backgammon|GNUbg|NeonGammon|eXtreme Gammon|BGBlitz|HedgeHog|Snowie)(?=$|[\s;(])/i;
const namedVersionPattern = new RegExp(
  `^(.+?\\s+${versionPattern.source.slice(1)})`,
  'i',
);

/** User-facing short name for a stored engine ID; `bgsage` is NeonGammon. */
export function engineDisplayName(id: string) {
  return id === 'gnubg' ? 'GNUbg' : id === 'bgsage' ? 'NeonGammon' : id;
}

export function shortEngineVersion(
  engine: 'gnubg' | 'bgsage',
  details: string | null,
) {
  const prefix =
    engine === 'gnubg' ? /^(?:GNU Backgammon|GNUbg)\s+/i : /^NeonGammon\s+/i;
  return details?.trim().replace(prefix, '').match(versionPattern)?.[0];
}

export function shortEngineLabel(details: string) {
  const identity = details.split(/;| · /, 1)[0].trim();
  const name = identity.match(engineNamePattern)?.[0];
  if (!name)
    return (
      identity.match(versionPattern)?.[0] ??
      identity.match(namedVersionPattern)?.[0] ??
      identity
    );
  const version = identity
    .slice(name.length)
    .trimStart()
    .match(versionPattern)?.[0];
  return version ? `${name} ${version}` : name;
}
