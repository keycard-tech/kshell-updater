/* The published releases, and the bytes behind them. */

import { listReleases, preloadImages } from '../base/bridge.js';
import { state } from '../base/state.js';
import * as text from '../strings.js';
import { el, reason } from '../utils.js';
import { filesFor, versionOf } from './files.js';
import { renderHome } from './home.js';
import { renderRows, renderSummary } from './rows.js';

export const loadReleases = async (): Promise<void> => {
  for (const { kind } of text.kinds) {
    try {
      state.files[kind] = (await listReleases(kind)).files;
    } catch (error) {
      state.listError = reason(error);
      state.files[kind] = [];
    }
    renderRows();
  }
  const firmware = versionOf(filesFor('firmware')[0]);
  const database = versionOf(filesFor('database')[0]);
  el('status-note').textContent = state.listError
    ? text.releases.offline
    : firmware && database
      ? text.releases.online(firmware, database)
      : text.releases.reading;
  renderHome();
};

/** Every offered file, including both firmware variants: pressing Transfer only ever writes. */
export const preload = async (): Promise<void> => {
  const files = text.kinds
    .flatMap(({ kind }) => filesFor(kind).map((file) => ({ kind, url: file.url })))
    .filter((file) => !state.cache[file.url]);
  if (!files.length) {
    state.preloading = false;
    renderSummary();
    return;
  }
  state.preloading = true;
  renderSummary();
  try {
    for (const prepared of await preloadImages(files)) {
      state.prepared[prepared.key] = prepared.info;
      state.cache[prepared.key] = { ok: true };
    }
  } catch (error) {
    state.listError = reason(error);
  }
  state.preloading = false;
  renderRows();
};
