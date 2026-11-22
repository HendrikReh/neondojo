import { request } from '../../shared/api/api';
import type { components } from '../../generated-api';

type Schema = components['schemas'];
export type VirtualPlayerProfile = Schema['VirtualPlayerProfile'];
export type VirtualPlayerDetails = Schema['VirtualPlayerDetails'];
export type VirtualPlayerRef = Schema['VirtualPlayerRef'];

const path = '/virtual-players';
const json = (method: string, body: unknown, signal?: AbortSignal) => ({
  method,
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
  signal,
});

export const loadVirtualPlayers = (signal?: AbortSignal) =>
  request<Schema['VirtualPlayerList']>(path, { signal }).then(
    ({ items }) => items,
  );

export const loadVirtualPlayer = (id: string, signal?: AbortSignal) =>
  request<VirtualPlayerDetails>(`${path}/${id}`, { signal });

export const createVirtualPlayer = (name: string) =>
  request<VirtualPlayerDetails>(path, json('POST', { name }));

export const renameVirtualPlayer = (
  profile: Pick<VirtualPlayerDetails, 'id' | 'version'>,
  name: string,
) =>
  request<VirtualPlayerDetails>(
    `${path}/${profile.id}`,
    json('PUT', { name, version: profile.version }),
  );

export const deleteVirtualPlayer = (
  profile: Pick<VirtualPlayerDetails, 'id' | 'version'>,
) =>
  request<void>(`${path}/${profile.id}?version=${profile.version}`, {
    method: 'DELETE',
  });

export const addVirtualPlayerMember = (
  profile: Pick<VirtualPlayerDetails, 'id' | 'version'>,
  memberId: string,
) =>
  request<VirtualPlayerDetails>(
    `${path}/${profile.id}/members`,
    json('POST', { profile_id: memberId, version: profile.version }),
  );

export const removeVirtualPlayerMember = (
  profile: Pick<VirtualPlayerDetails, 'id' | 'version'>,
  memberId: string,
) =>
  request<VirtualPlayerDetails>(
    `${path}/${profile.id}/members/${memberId}?version=${profile.version}`,
    { method: 'DELETE' },
  );

export const loadMyPlayerProfile = (signal?: AbortSignal) =>
  request<Schema['MyPlayerProfile']>(`${path}/me`, { signal }).then(
    ({ profile }) => profile ?? null,
  );

export const saveMyPlayerProfile = (id: string | null, signal?: AbortSignal) =>
  request<Schema['MyPlayerProfile']>(
    `${path}/me`,
    json('PUT', { id }, signal),
  ).then(({ profile }) => profile ?? null);
