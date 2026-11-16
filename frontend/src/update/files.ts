/* Which image each row points at, and whether it has been checked. */

import { state } from '../base/state.js';
import type { ImageKind, ReleaseFile } from '../types.js';

export type Checked = 'ready' | 'failed' | 'pending' | null;

export const filesFor = (kind: ImageKind): ReleaseFile[] => state.files[kind];

/** The chosen variant for firmware, the one listing a database has for the database. */
export const selectedFile = (kind: ImageKind): ReleaseFile | null => {
  const files = filesFor(kind);
  if (kind === 'database') return files[0] ?? null;
  return files.find((file) => file.variant === state.variant[kind]) ?? files[0] ?? null;
};

export const versionOf = (file: ReleaseFile | null | undefined): string =>
  file?.version ?? file?.date ?? '';

/** The `Prepared.key` of what this row would write. */
export const keyFor = (kind: ImageKind): string | null =>
  state.mode[kind] === 'file'
    ? (state.ready[kind]?.key ?? null)
    : (selectedFile(kind)?.url ?? null);

export const checkedOf = (kind: ImageKind): Checked => {
  const key = keyFor(kind);
  if (!key) return null;
  const entry = state.cache[key];
  return entry ? (entry.ok ? 'ready' : 'failed') : 'pending';
};

export const noteOf = (kind: ImageKind): string | undefined => {
  const key = keyFor(kind);
  return key ? state.cache[key]?.note : undefined;
};

export const isReadyToFlash = (kind: ImageKind): boolean => checkedOf(kind) === 'ready';
