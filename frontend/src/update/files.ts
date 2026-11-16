import { state } from '../base/state.js';
import type { ImageKind, ReleaseFile } from '../types.js';

export type Checked = 'ready' | 'failed' | 'pending' | null;

export const filesFor = (kind: ImageKind): ReleaseFile[] => state.files[kind];

export const selectedFile = (kind: ImageKind): ReleaseFile | null => {
  const files = filesFor(kind);
  if (kind === 'database') return files[0] ?? null;
  return files.find((file) => file.variant === state.variant[kind]) ?? files[0] ?? null;
};

export const versionOf = (file: ReleaseFile | null | undefined): string =>
  file?.version ?? file?.date ?? '';

export const olderThan = (candidate: string, onDevice: string): boolean => {
  const parts = (value: string) => value.split(/[.\s_-]+/).filter(Boolean).map(Number);
  const [a, b] = [parts(candidate), parts(onDevice)];
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const step = (a[index] ?? 0) - (b[index] ?? 0);
    if (step !== 0) return step < 0;
  }
  return false;
};

export const deviceVersion = (kind: ImageKind): string =>
  kind === 'firmware'
    ? (state.config?.fwVersion ?? '')
    : String(state.config?.dbVersion ?? '');

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
