import { listReleases, preloadImages } from '../base/bridge.js';
import { state } from '../base/state.js';
import * as text from '../strings.js';
import { listRetryMs } from '../config.js';
import { sentence } from '../errors.js';
import { el, reason } from '../utils.js';
import { filesFor } from './files.js';
import { renderHome } from './home.js';
import { renderRows, renderSummary } from './rows.js';

let lastPass = 0;

export const loadReleases = async (): Promise<void> => {
  lastPass = Date.now();
  let failure: string | null = null;
  for (const { kind } of text.kinds) {
    try {
      state.files[kind] = (await listReleases(kind)).files;
    } catch (error) {
      console.warn(`could not list ${kind} releases:`, reason(error));
      failure = reason(error);
      state.files[kind] = [];
    }
    renderRows();
  }
  
  state.listError = failure;
  el('status-note').textContent = failure ? sentence.offline : '';
  renderHome();
};

/** A list that could not be read stays missing until a pass succeeds, so the app asks again by
    itself: at once when the person says so or the browser announces a network, and slowly by itself
    for a system that announces nothing - where the app usually starts before the network is up. */
export const retryListing = async (immediate: boolean): Promise<void> => {
  if (!state.listError || state.busy) return;
  if (!immediate && Date.now() - lastPass < listRetryMs) return;
  await loadReleases();
  await preload();
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
