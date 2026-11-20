import { state } from './base/state.js';
import * as text from './strings.js';
import { el } from './utils.js';
import { retryOnline } from './update/releases.js';
import { renderRows } from './update/rows.js';
import { ensureList, renderVerify } from './verify/screen.js';

const forgetOnline = (): void => {
  for (const { kind } of text.kinds) state.files[kind] = [];
  state.listError = null;
  state.preloading = false;
};

export const reachabilityChanged = (reachable: boolean): void => {
  state.offline = !reachable;
  if (reachable) {
    void retryOnline(true);
    void ensureList();
  } else {
    forgetOnline();
  }

  el('status-net').hidden = reachable;
  el('status-net-word').textContent = text.network.offline;
  el('status-note').textContent = '';
  renderRows();
  renderVerify();
};
