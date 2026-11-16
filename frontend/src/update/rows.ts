/* The two release rows: what they say, and what pressing them means. */

import { chooseFile, openExternal, pickImageFile } from '../base/bridge.js';
import { state, type ByKind, type ReadyFile } from '../base/state.js';
import * as text from '../strings.js';
import type { ImageKind } from '../types.js';
import { asHtml, el, reason } from '../utils.js';
import { rowShell, shaFromFile, shaFromRelease, versionSelect, versionStatic } from '../views/rows.js';
import { checkedOf, filesFor, isReadyToFlash, noteOf, selectedFile, versionOf } from './files.js';

interface RowRefs {
  row: HTMLElement;
  include: HTMLInputElement;
  status: HTMLElement;
  link: HTMLAnchorElement;
  field: HTMLElement;
  pick: HTMLButtonElement;
  hint: HTMLElement;
  sha: HTMLElement;
}

const rows = {} as ByKind<RowRefs>;

export interface RowStatus {
  status: string;
  modifier: '' | 'empty' | 'latest';
  wanted: boolean;
}

const slot = <T extends HTMLElement>(row: HTMLElement, name: string): T =>
  row.querySelector<T>(`[data-slot=${name}]`) as T;

export const buildRows = (): void => {
  const host = el('rows');
  host.replaceChildren();
  for (const entry of text.kinds) {
    const kind = entry.kind;
    const row = document.createElement('div');
    row.className = 'ksh__row';
    row.dataset.kind = kind;
    row.innerHTML = asHtml(rowShell(entry));
    host.append(row);

    const refs: RowRefs = {
      row,
      include: slot<HTMLInputElement>(row, 'include'),
      status: slot(row, 'status'),
      link: slot<HTMLAnchorElement>(row, 'link'),
      field: slot(row, 'field'),
      pick: slot<HTMLButtonElement>(row, 'pick'),
      hint: slot(row, 'hint'),
      sha: slot(row, 'sha'),
    };
    rows[kind] = refs;

    row.querySelector('[data-slot=mode]')?.addEventListener('click', (event) => {
      const mode = (event.target as HTMLElement | null)?.dataset?.mode;
      if (mode !== 'online' && mode !== 'file') return;
      if (state.mode[kind] === mode) return;
      state.mode[kind] = mode;
      render(kind);
    });
    refs.include.addEventListener('change', () => {
      state.include[kind] = refs.include.checked;
      render(kind);
    });
    refs.pick.addEventListener('click', () => chooseImage(kind));
    refs.link.addEventListener('click', (event) => {
      event.preventDefault();
      openExternal(entry.url).catch((error: unknown) =>
        console.warn(`could not open ${entry.url}: ${reason(error)}`),
      );
    });
  }
};

/** Status text and whether the row asks for work: the checkbox, badge and action follow it. */
export const rowState = (kind: ImageKind): RowStatus => {
  if (state.mode[kind] === 'file') {
    const picked = state.ready[kind]?.info;
    if (!picked) return { status: text.status.noFile, modifier: 'empty', wanted: false };
    return {
      status: text.row.fromFileVersion(
        picked.version,
        text.variants[picked.fwVariant ?? -1] ?? '',
      ),
      modifier: '',
      wanted: true,
    };
  }

  const file = selectedFile(kind);
  if (!file) {
    return state.listError
      ? { status: text.status.noList, modifier: 'empty', wanted: false }
      : { status: text.status.reading, modifier: 'empty', wanted: false };
  }

  const deviceVariant = state.config?.fwVariant ?? null;
  if (kind === 'firmware' && deviceVariant !== null && file.variant !== deviceVariant) {
    return { status: text.status.variantSwitch, modifier: '', wanted: true };
  }
  const onDevice =
    kind === 'firmware' ? state.config?.fwVersion : String(state.config?.dbVersion ?? '');
  if (onDevice && onDevice === versionOf(file)) {
    return { status: text.status.upToDate, modifier: 'latest', wanted: false };
  }
  return { status: text.status.newVersion, modifier: '', wanted: true };
};

export const wantedKinds = (): ImageKind[] =>
  text.kinds
    .map((entry) => entry.kind)
    .filter((kind) => state.include[kind] && rowState(kind).wanted);

export const render = (kind: ImageKind): void => {
  const refs = rows[kind];
  const mode = state.mode[kind];
  const ready = state.ready[kind];
  const row = rowState(kind);

  for (const button of refs.row.querySelectorAll<HTMLButtonElement>('[data-mode]')) {
    button.classList.toggle('ksh__seg--on', button.dataset.mode === mode);
    button.disabled = state.busy;
  }
  refs.status.textContent = row.status;
  refs.status.className = `ksh__status${row.modifier ? ` ksh__status--${row.modifier}` : ''}`;
  refs.row.classList.toggle('ksh__row--off', !state.include[kind]);
  refs.include.checked = state.include[kind] && row.wanted;
  refs.include.disabled = !row.wanted || state.busy;

  refs.field.hidden = mode !== 'online';
  refs.pick.hidden = mode !== 'file';
  refs.link.hidden = mode !== 'online' || !selectedFile(kind);

  if (mode === 'online') renderField(kind, refs);

  const offline = !ready?.error && state.listError;
  const failed = checkedOf(kind) === 'failed';
  // The failure detail lives in the tooltip: in the row it would wrap and grow every screen.
  refs.hint.textContent = ready?.error
    ? ready.error
    : offline
      ? text.row.offlineHint
      : failed
        ? (noteOf(kind) ?? text.row.unreadable)
        : '';
  refs.hint.title = offline ? (state.listError ?? '') : '';

  const sha =
    mode === 'file'
      ? shaFromFile(ready ?? null)
      : shaFromRelease(selectedFile(kind), checkedOf(kind));
  refs.sha.innerHTML = asHtml(sha.content);
  refs.sha.className = `ksh__sha${sha.verified ? ' ksh__sha--ok' : ''}`;

  renderSummary();
};

export const renderRows = (): void => {
  for (const { kind } of text.kinds) render(kind);
};

const renderField = (kind: ImageKind, refs: RowRefs): void => {
  const files = filesFor(kind);
  if (!files.length) {
    refs.field.replaceChildren();
    return;
  }
  if (kind === 'database') {
    refs.field.innerHTML = asHtml(
      versionStatic(
        String(state.config?.dbVersion ?? text.row.unknownVersion),
        versionOf(files[0]),
      ),
    );
    return;
  }
  const signature = files.map((file) => file.url).join('|');
  if (refs.field.dataset.signature === signature) return;
  refs.field.innerHTML = asHtml(versionSelect(files, state.variant[kind]));
  refs.field.dataset.signature = signature;
  refs.field.querySelector('select')?.addEventListener('change', (event) => {
    state.variant[kind] = Number((event.target as HTMLSelectElement).value);
    state.ready[kind] = null;
    render(kind);
  });
};

export const renderSummary = (): void => {
  const wanted = wantedKinds();
  const badge = el('select-badge');
  badge.textContent = wanted.length
    ? text.summary.selected(wanted.length)
    : text.summary.none;
  badge.className = `ksh__badge${wanted.length ? ' ksh__badge--ready' : ''}`;

  const waiting = wanted.filter((kind) => !isReadyToFlash(kind));
  const button = el<HTMLButtonElement>('btn-transfer');
  button.disabled = state.busy || !wanted.length || waiting.length > 0;
  button.textContent = state.busy
    ? text.summary.transferring
    : !wanted.length
      ? text.summary.transfer
      : state.preloading
        ? text.summary.preparing
        : waiting.length
          ? text.summary.unverified
          : text.summary.transferTo;
};

const chooseImage = async (kind: ImageKind): Promise<void> => {
  try {
    const path = await chooseFile({
      title: kind === 'firmware' ? text.dialogs.firmware : text.dialogs.database,
      filterName: text.dialogs.imageFilter,
      extensions: [text.dialogs.extension],
    });
    if (!path) return;
    const prepared = await pickImageFile(kind, path);
    const ready: ReadyFile = { info: prepared.info, key: prepared.key };
    state.ready[kind] = ready;
    state.prepared[prepared.key] = prepared.info;
    state.cache[prepared.key] = { ok: true };
    state.include[kind] = true;
  } catch (error) {
    state.ready[kind] = { error: reason(error) };
  }
  render(kind);
};
