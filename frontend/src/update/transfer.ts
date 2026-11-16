/* The queue: what gets written, in what order, and what came back afterwards. */

import { cancelFlash, flashImage, listShells, shellConfiguration } from '../base/bridge.js';
import { show } from '../base/screens.js';
import { state } from '../base/state.js';
import { deviceReturn } from '../config.js';
import * as text from '../strings.js';
import type { AppConfiguration, ImageKind } from '../types.js';
import { asHtml, el, reason, sleep, thousands } from '../utils.js';
import { itemRow, spinner, stateWord } from '../views/transfer.js';
import { reportConnected } from './device.js';
import { isReadyToFlash, keyFor, selectedFile, versionOf } from './files.js';
import {
  dropRecord,
  intendedVersions,
  onDeviceVersions,
  writeRecord,
} from './record.js';
import { rowState, wantedKinds } from './rows.js';

const kinds = text.kinds.map((entry) => entry.kind);

/** 'Firmware 1.5.3-omni' / 'Database 20261111', as the transfer screen names them. */
export const itemLabel = (kind: ImageKind): string => {
  const name = text.title(kind);
  if (state.mode[kind] === 'file') {
    return `${name} ${state.ready[kind]?.info?.version ?? ''}`.trim();
  }
  const file = selectedFile(kind);
  const variant =
    file?.variant === null || file?.variant === undefined ? '' : `-${text.variants[file.variant] ?? ''}`;
  return `${name} ${versionOf(file)}${variant}`;
};

export const startTransfer = async (): Promise<void> => {
  const wanted = wantedKinds();
  if (state.busy || !wanted.length || wanted.some((kind) => !isReadyToFlash(kind))) return;

  // Firmware first: a token list the firmware cannot read is worse than the other way round.
  state.queue = wanted.map((kind) => {
    const status = rowState(kind).status;
    const key = keyFor(kind) ?? '';
    return {
      kind,
      key,
      label: itemLabel(kind),
      bytes: state.prepared[key]?.bytes ?? 0,
      done: false,
      // No dialog: the row's own status word is the warning that pressing the button agrees to.
      allowVariantSwitch: status === text.status.variantSwitch,
      allowDowngrade: status === text.status.downgrade,
    };
  });
  state.step = 0;
  state.transfer = null;
  state.transferError = null;
  show('screen-transfer');
  await runQueue();
};

export const cancelTransfer = (): void => {
  el('transfer-caption').textContent = text.transfer.cancelling;
  void cancelFlash();
};

/** The device does not keep the person waiting for a reboot it has not finished. */
const reportBack = async (): Promise<AppConfiguration | null> => {
  for (let attempt = 0; attempt < deviceReturn.attempts; attempt += 1) {
    try {
      const found = await listShells();
      if (found.length) {
        state.device = found[0];
        return await shellConfiguration(found[0].path);
      }
    } catch {
      // Still restarting; the next attempt asks again.
    }
    await sleep(deviceReturn.delayMs);
  }
  return null;
};

const runQueue = async (): Promise<void> => {
  state.busy = true;
  // Written before the first byte: if this window dies mid-write, the Shell is still owed these versions.
  await writeRecord();
  for (const [index, item] of state.queue.entries()) {
    state.step = index + 1;
    state.transfer = { done: 0, total: item.bytes };
    renderTransfer();
    try {
      state.config = await flashImage({
        kind: item.kind,
        key: item.key,
        allowVariantSwitch: item.allowVariantSwitch,
        allowDowngrade: item.allowDowngrade,
      });
      item.done = true;
    } catch (error) {
      state.transferError = reason(error);
      break;
    }
  }
  state.busy = false;

  // Neither outcome is taken on trust: what the Shell answers builds both the screen and the record.
  const after = await reportBack();
  if (after) state.config = after;
  const now = onDeviceVersions(after);
  const intended = intendedVersions();
  const behind = kinds.filter(
    (kind) => intended[kind] !== null && now?.[kind] !== intended[kind],
  );

  if (state.transferError) {
    await writeRecord();
    renderTransfer();
    return;
  }
  if (now && behind.length) {
    state.transferError = text.transfer.cameBack(
      behind
        .map((kind) =>
          text.transfer.cameBackItem(kind, now[kind] ?? '', intended[kind] ?? ''),
        )
        .join(' and '),
    );
    await writeRecord();
    renderTransfer();
    return;
  }

  reportConnected();
  if (!now) {
    // Kept so the next start finishes the check this one could not.
    el('done-summary').textContent = text.done.noVersion;
    el('done-verify').textContent = text.done.replug;
    show('screen-done');
    return;
  }
  await dropRecord();
  el('done-summary').textContent = text.done.versions(now.firmware, now.database);
  el('done-verify').textContent = text.done.checked;
  show('screen-done');
};

export const renderTransfer = (): void => {
  const current = state.transfer ?? { done: 0, total: 0 };
  const failed = Boolean(state.transferError);
  const cancelled = state.transferError === text.status.cancelled;
  const percent = current.total ? Math.floor((current.done / current.total) * 100) : 0;

  // One sentence and no status words: `explain` on the Rust side chose the wording.
  el('transfer-title').textContent = failed
    ? cancelled
      ? text.transfer.titleCancelled
      : text.transfer.titleFailed
    : text.transfer.titleBusy;
  el('transfer-step').textContent = text.transfer.step(
    state.step,
    Math.max(state.queue.length, 1),
  );
  el('transfer-percent').textContent = text.transfer.percent(percent);
  el('transfer-bytes').textContent = text.transfer.bytes(
    thousands(current.done),
    thousands(current.total),
  );
  el('transfer-bar').style.width = `${percent}%`;
  el('transfer-caption').textContent = failed
    ? (state.transferError ?? '')
    : text.transfer.keepConnected;
  el('transfer-checkline').hidden = failed;
  el('btn-cancel').hidden = failed;
  el('btn-transfer-back').hidden = !failed;

  el('transfer-items').innerHTML = asHtml(
    state.queue.map((item, index) => {
      const moving = index + 1 === state.step;
      const word = item.done
        ? text.transfer.item.done
        : failed && moving
          ? text.transfer.item.failed
          : failed
            ? text.transfer.item.notStarted
            : text.transfer.item.queued;
      const dim = word === text.transfer.item.queued || word === text.transfer.item.notStarted;
      const marker =
        moving && !failed
          ? spinner()
          : stateWord(word, item.done ? 'latest' : dim ? 'waiting' : 'failed');
      return itemRow(item.label, marker, dim);
    }),
  );
};
