/* The two release rows: what they say, and what pressing them means. */

import { chooseFile, openExternal, pickImageFile } from '../base/bridge.js';
import { state, type ByKind, type ReadyFile } from '../base/state.js';
import * as text from '../strings.js';
import type { ImageKind } from '../types.js';
import { asHtml, el, reason } from '../utils.js';
import { rowShell, shaFromFile, shaFromRelease, versionSelect, versionStatic } from '../views/rows.js';
import {
  checkedOf,
  deviceVersion,
  filesFor,
  isReadyToFlash,
  noteOf,
  olderThan,
  selectedFile,
  versionOf,
} from './files.js';

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
    const onDevice = deviceVersion(kind);
    // Older is not a thing to be talked into: it is said, and the row stops asking. The Shell keeps
    // its own refusal for anything else we cannot judge from a version number.
    if (onDevice && olderThan(picked.version, onDevice))
      return { status: text.status.older, modifier: 'empty', wanted: false };
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

  if (kind === 'database') {
    return sameOrNew(String(state.config?.dbVersion ?? ''), versionOf(file));
  }
  const deviceVariant = state.config?.fwVariant ?? null;
  if (deviceVariant !== null && file.variant !== deviceVariant) {
    return { status: text.status.variantSwitch, modifier: '', wanted: true };
  }
  // A beta of the current version is still an update to install: the device would otherwise sit on a
  // pre-release build the release notes call the same number.
  if (state.config?.fwBeta) return { status: text.status.newVersion, modifier: '', wanted: true };
  return sameOrNew(state.config?.fwVersion ?? '', versionOf(file));
};

const sameOrNew = (onDevice: string, offered: string): RowStatus =>
  onDevice && onDevice === offered
    ? { status: text.status.upToDate, modifier: 'latest', wanted: false }
    : { status: text.status.newVersion, modifier: '', wanted: true };

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

  // Once a file is picked its name replaces the request for one, in the same compact box.
  const picked = ready?.key?.split('/').pop() ?? null;
  refs.pick.classList.toggle('ksh__btn--chosen', Boolean(picked));
  refs.pick.textContent = picked ?? text.row.choose;

  const problem = rowProblem(kind, ready);
  refs.hint.classList.toggle('ksh__hint--bad', problem.bad);
  refs.hint.textContent = problem.text;
  refs.hint.title = problem.detail;
  // With nothing to say the hint takes no room and the hash stands alone on the right.
  refs.hint.hidden = !problem.text;

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

/** One line of help per row, and only for a file the person can act on. The longer reason for a
    listing failure lives in the tooltip: in the row it would wrap and grow the screen. */
const rowProblem = (
  kind: ImageKind,
  ready: ReadyFile | null,
): { text: string; detail: string; bad: boolean } => {
  if (ready?.error) return { text: ready.error, detail: '', bad: true };
  const onDevice = deviceVersion(kind);
  if (ready?.info && state.mode[kind] === 'file' && onDevice && olderThan(ready.info.version, onDevice))
    return { text: text.row.older(text.title(kind).toLowerCase(), onDevice), detail: '', bad: true };
  // Offline needs no line here: the status word says `No list` and the status bar says why.
  if (checkedOf(kind) === 'failed')
    return { text: noteOf(kind) ?? text.row.unreadable, detail: '', bad: true };
  return { text: '', detail: '', bad: false };
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
