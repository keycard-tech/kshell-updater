import { cancelFlash, flashImage } from '../base/bridge.js';
import { show } from '../base/screens.js';
import { state } from '../base/state.js';
import { human } from '../errors.js';
import * as text from '../strings.js';
import type { ImageKind } from '../types.js';
import { el, reason, slot } from '../utils.js';
import { refresh, reportConnected, showWaiting } from './device.js';
import { isReadyToFlash, keyFor, selectedFile, versionOf } from './files.js';
import { dropRecord, intendedVersions, onDeviceVersions, writeRecord } from './record.js';
import { wantedKinds } from './rows.js';

const kinds = text.kinds.map((entry) => entry.kind);

const itemLabel = (kind: ImageKind): string => {
  const name = text.title(kind);
  if (state.mode[kind] === 'file') {
    return `${name} ${state.ready[kind]?.info?.version ?? ''}`.trim();
  }
  const file = selectedFile(kind);
  const variant =
    file?.variant === null || file?.variant === undefined
      ? ''
      : `-${text.variants[file.variant] ?? ''}`;
  return `${name} ${versionOf(file)}${variant}`;
};

export const startTransfer = async (): Promise<void> => {
  const wanted = wantedKinds();
  if (state.busy || !wanted.length || wanted.some((kind) => !isReadyToFlash(kind))) return;

  state.queue = wanted.map((kind) => {
    const key = keyFor(kind) ?? '';
    return {
      kind,
      key,
      label: itemLabel(kind),
      bytes: state.prepared[key]?.bytes ?? 0,
      done: false,
    };
  });
  state.step = 0;
  state.transfer = null;
  state.transferError = null;
  el<HTMLButtonElement>('btn-cancel').disabled = false;
  show('screen-transfer');
  await runQueue();
};

export const cancelTransfer = (): void => {
  el<HTMLButtonElement>('btn-cancel').disabled = true;
  void cancelFlash();
};

const runQueue = async (): Promise<void> => {
  state.busy = true;
  try {
    await writeRecord();
    for (const [index, item] of state.queue.entries()) {
      if (item.done) continue; // a retry after a failure starts where the queue stopped
      state.step = index + 1;
      state.transfer = { done: 0, total: item.bytes };
      renderTransfer();
      try {
        state.config = await flashImage({
          kind: item.kind,
          key: item.key,
        });
        item.done = true;
      } catch (error) {
        console.warn('the transfer stopped:', reason(error));
        state.transferError = reason(error);
        break;
      }
    }

    const now = onDeviceVersions(state.config);
    const intended = intendedVersions();
    const behind = kinds.filter(
      (kind) => intended[kind] !== null && now?.[kind] !== intended[kind],
    );

    if (state.transferError) {
      await writeRecord();
      showFailed(state.transferError);
      return;
    }
    if (now && behind.length) {
      showFailed(
        text.failed.cameBack(
          behind
            .map((kind) => text.failed.cameBackItem(kind, now[kind] ?? '', intended[kind] ?? ''))
            .join(' and '),
        ),
      );
      await writeRecord();
      return;
    }

    reportConnected();
    if (!now) {
      await writeRecord();
      backToUpdates();
      void refresh();
      return;
    }
    await dropRecord();
    el('done-summary').textContent = text.done.versions(now.firmware, now.database);
    show('screen-done');
  } finally {
    state.busy = false;
  }
};

const showFailed = (error: string): void => {
  const says = human(error);
  const cut = says.indexOf('. ');
  el('failed-title').textContent = cut < 0 ? says || text.failed.nothing : says.slice(0, cut + 1);
  el('failed-detail').textContent = cut < 0 ? '' : says.slice(cut + 2);
  show('screen-failed');
};

export const backToUpdates = (): void => {
  state.transferError = null;
  state.queue = [];
  state.step = 0;
  state.transfer = null;
  show('screen-ready');
};

export const retryTransfer = async (): Promise<void> => {
  if (!state.device) {
    showWaiting();
    return;
  }
  state.transferError = null;
  el<HTMLButtonElement>('btn-cancel').disabled = false;
  show('screen-transfer');
  await runQueue();
};

export const renderTransfer = (): void => {
  const current = state.transfer ?? { done: 0, total: 0 };
  const percent = current.total ? Math.floor((current.done / current.total) * 100) : 0;

  el('transfer-title').textContent = text.transfer.titleBusy;
  el('transfer-percent').textContent = text.transfer.percent(percent);
  el('transfer-bar').style.width = `${percent}%`;
  el('transfer-caption').textContent = text.transfer.keepConnected;

  state.queue.forEach((item, index) => {
    const row = el('transfer-items').querySelector<HTMLElement>(
      `[data-item="${index}"]`,
    ) as HTMLElement;
    const moving = index + 1 === state.step && !item.done;
    row.hidden = false;
    row.classList.toggle('ksh__item--waiting', !item.done && !moving);
    slot(row, 'label').textContent = item.label;
    slot(row, 'spin').hidden = !moving;
    const marker = slot(row, 'state');
    marker.hidden = moving;
    marker.textContent = item.done ? text.transfer.item.done : text.transfer.item.queued;
    marker.className = item.done
      ? 'ksh__item-state ksh__item-state--latest'
      : 'ksh__item-state ksh__item-state--waiting';
  });
  for (let index = state.queue.length; index < 2; index += 1) {
    (
      el('transfer-items').querySelector<HTMLElement>(`[data-item="${index}"]`) as HTMLElement
    ).hidden = true;
  }
};
