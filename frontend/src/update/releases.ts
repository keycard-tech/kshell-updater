import { listReleases, preloadImages } from '../base/bridge.js';
import { state } from '../base/state.js';
import * as text from '../strings.js';
import { listRetryMs } from '../config.js';
import { sentence } from '../errors.js';
import { el, reason } from '../utils.js';
import { checkedOf, filesFor } from './files.js';
import { renderHome } from './home.js';
import { renderRows, renderSummary } from './rows.js';

let lastPass = 0;

export const loadReleases = async (): Promise<void> => {
  lastPass = Date.now();
  let failure: string | null = null;
  let said = '';
  for (const { kind } of text.kinds) {
    try {
      const catalogue = await listReleases(kind);
      state.files[kind] = catalogue.files;
      if (!catalogue.newestOffered) said = text.listing.newestUnhashed;
      else if (!catalogue.verified) said = text.listing.hashList;
      console.info(`${kind}: ${catalogue.unofferable} files have no published hash`);
    } catch (error) {
      console.warn(`could not list ${kind} releases:`, reason(error));
      failure = reason(error);
      state.files[kind] = [];
    }
    renderRows();
  }

  state.listError = failure;
  el('status-note').textContent = failure ? sentence.offline : said;
  renderHome();
};

export const retryListing = async (immediate: boolean): Promise<void> => {
  if (state.busy) return;
  const waiting = state.listError || text.kinds.some(({ kind }) => checkedOf(kind) === 'failed');
  if (!waiting) return;
  if (!immediate && Date.now() - lastPass < listRetryMs) return;
  await loadReleases();
  await preload();
};

export const preload = async (): Promise<void> => {
  const files = text.kinds
    .flatMap(({ kind }) => filesFor(kind).map((file) => ({ kind, url: file.url })))
    .filter((file) => !state.cache[file.url]?.ok);
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
