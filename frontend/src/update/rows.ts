import { chooseFile, openExternal, pickImageFile } from '../base/bridge.js';
import { openCode } from '../airgap.js';
import { state, type ByKind, type ReadyFile } from '../base/state.js';
import { human } from '../errors.js';
import * as text from '../strings.js';
import type { ImageKind } from '../types.js';
import { asHtml, el, reason, slot } from '../utils.js';
import { shaFromFile, shaFromRelease, versionOptions } from '../views/rows.js';
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
  select: HTMLSelectElement;
  staticBox: HTMLElement;
  pick: HTMLButtonElement;
  code: HTMLButtonElement;
  hint: HTMLElement;
  sha: HTMLElement;
}

const rows = {} as ByKind<RowRefs>;

interface RowStatus {
  status: string;
  modifier: '' | 'empty' | 'latest';
  wanted: boolean;
}

const rowOf = (
  status: string,
  modifier: RowStatus['modifier'] = '',
  wanted = false,
): RowStatus => ({ status, modifier, wanted });

export const buildRows = (): void => {
  for (const entry of text.kinds) {
    const kind = entry.kind;
    const row = el('rows').querySelector<HTMLElement>(`[data-kind=${kind}]`);
    if (!row) throw new Error(`index.html has no ${kind} row`);
    const refs: RowRefs = {
      row,
      include: slot<HTMLInputElement>(row, 'include'),
      status: slot(row, 'status'),
      link: slot<HTMLAnchorElement>(row, 'link'),
      field: slot(row, 'field'),
      select: slot<HTMLSelectElement>(row, 'select'),
      staticBox: slot<HTMLElement>(row, 'static'),
      pick: slot<HTMLButtonElement>(row, 'pick'),
      code: slot<HTMLButtonElement>(row, 'code'),
      hint: slot(row, 'hint'),
      sha: slot(row, 'sha'),
    };
    rows[kind] = refs;
    refs.link.href = entry.url;
    slot(row, 'link-label').textContent = entry.link;

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
    refs.code.setAttribute('aria-label', text.airgap.button);
    refs.code.addEventListener('click', () => void openCode(text.airgap.urls[kind], text.airgap.what[kind]));
    refs.select.addEventListener('change', () => {
      state.variant[kind] = Number(refs.select.value);
      state.ready[kind] = null;
      render(kind);
    });
    refs.link.addEventListener('click', (event) => {
      event.preventDefault();
      openExternal(entry.url).catch((error: unknown) =>
        console.warn(`could not open ${entry.url}: ${reason(error)}`),
      );
    });
  }
};

const rowState = (kind: ImageKind): RowStatus => {
  if (state.mode[kind] === 'file') {
    const picked = state.ready[kind]?.info;
    if (!picked) return rowOf(text.status.noFile, 'empty');
    const onDevice = deviceVersion(kind);
    if (onDevice && olderThan(picked.version, onDevice)) return rowOf(text.status.older, 'empty');
    return rowOf(
      text.row.fromFileVersion(picked.version, text.variants[picked.fwVariant ?? -1] ?? ''),
      '',
      true,
    );
  }

  const file = selectedFile(kind);
  if (!file) {
    return state.offline || state.listError
      ? rowOf(text.status.noList, 'empty')
      : rowOf(text.status.reading, 'empty');
  }

  if (kind === 'database') {
    return sameOrNew(String(state.config?.dbVersion ?? ''), versionOf(file));
  }
  const deviceVariant = state.config?.fwVariant ?? null;
  if (deviceVariant !== null && file.variant !== deviceVariant) {
    return rowOf(text.status.variantSwitch, '', true);
  }

  if (state.config?.fwBeta) return rowOf(text.status.newVersion, '', true);
  return sameOrNew(state.config?.fwVersion ?? '', versionOf(file));
};

const sameOrNew = (onDevice: string, offered: string): RowStatus =>
  onDevice && !olderThan(onDevice, offered)
    ? rowOf(text.status.upToDate, 'latest')
    : rowOf(text.status.newVersion, '', true);

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
    button.disabled = state.busy || (button.dataset.mode === 'online' && state.offline);
  }
  refs.status.textContent = row.status;
  refs.status.className = `ksh__status${row.modifier ? ` ksh__status--${row.modifier}` : ''}`;
  refs.row.classList.toggle('ksh__row--off', !state.include[kind]);
  refs.include.checked = state.include[kind] && row.wanted;
  refs.include.disabled = !row.wanted || state.busy;

  refs.field.hidden = mode !== 'online';
  refs.pick.hidden = mode !== 'file';
  refs.link.hidden = mode !== 'online' || !selectedFile(kind);
  refs.code.hidden = !(state.listError || state.offline) || Boolean(ready) || isReadyToFlash(kind);

  if (mode === 'online') renderField(kind, refs);

  const picked = ready?.key?.split('/').pop() ?? null;
  refs.pick.classList.toggle('ksh__btn--chosen', Boolean(picked));
  refs.pick.textContent = picked ?? text.row.choose;

  const problem = rowProblem(kind, ready);
  refs.hint.classList.toggle('ksh__hint--bad', problem.bad);
  refs.hint.textContent = problem.text;
  refs.hint.title = problem.detail;
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
  refs.field.hidden ||= !files.length;
  if (!files.length) return;
  const statement = kind === 'database';
  refs.staticBox.hidden = !statement;
  refs.select.hidden = statement;
  if (statement) {
    slot(refs.staticBox, 'static-from').textContent = String(
      state.config?.dbVersion ?? text.row.unknownVersion,
    );
    slot(refs.staticBox, 'static-to').textContent = versionOf(files[0]);
    return;
  }
  const signature = files.map((file) => file.url).join('|');
  if (refs.field.dataset.signature !== signature) {
    refs.select.innerHTML = asHtml(versionOptions(files, state.variant[kind]));
    refs.field.dataset.signature = signature;
  }
};

const rowProblem = (
  kind: ImageKind,
  ready: ReadyFile | null,
): { text: string; detail: string; bad: boolean } => {
  if (ready?.error) return { text: ready.error, detail: '', bad: true };
  const onDevice = deviceVersion(kind);
  if (
    ready?.info &&
    state.mode[kind] === 'file' &&
    onDevice &&
    olderThan(ready.info.version, onDevice)
  )
    return {
      text: text.row.older(text.title(kind).toLowerCase(), onDevice),
      detail: '',
      bad: true,
    };
  if (checkedOf(kind) === 'failed')
    return { text: noteOf(kind) ?? text.row.unreadable, detail: '', bad: true };
  return { text: '', detail: '', bad: false };
};

export const renderSummary = (): void => {
  const wanted = wantedKinds();
  const badge = el('select-badge');
  badge.textContent = wanted.length ? text.summary.selected(wanted.length) : text.summary.none;
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
    console.warn(`could not prepare the ${kind} image:`, reason(error));
    state.ready[kind] = { error: human(reason(error)) };
  }
  render(kind);
};
