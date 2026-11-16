import { listReleases, preloadImages } from '../base/bridge.js';
import { state } from '../base/state.js';
import * as text from '../strings.js';
import { el, reason } from '../utils.js';
import { filesFor } from './files.js';
import { renderHome } from './home.js';
import { renderRows, renderSummary } from './rows.js';

export const loadReleases = async (): Promise<void> => {
  for (const { kind } of text.kinds) {
    try {
      state.files[kind] = (await listReleases(kind)).files;
    } catch (error) {
      // The reason is a support detail, and it has nowhere else to go now the row says only `No list`.
      console.warn(`could not list ${kind} releases:`, reason(error));
      state.listError = reason(error);
      state.files[kind] = [];
    }
    renderRows();
  }
  el('status-note').textContent = state.listError ? text.releases.offline : '';
  renderHome();
};

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
