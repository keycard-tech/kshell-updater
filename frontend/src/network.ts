import { state } from './base/state.js';
import * as text from './strings.js';
import { el } from './utils.js';
import { renderRows } from './update/rows.js';
import { renderVerify } from './verify/screen.js';

export const renderNote = (aboutTheList: string): void => {
  el('status-note').textContent = state.offline ? '' : aboutTheList;
};

export const heard = (online: boolean): void => {
  state.offline = !online;
  if (!online) {
    for (const { kind } of text.kinds) {
      state.files[kind] = [];
    }
    state.listError = null;
    state.preloading = false;
  }
  el('status-net').hidden = online;
  el('status-net-word').textContent = text.network.offline;
  renderNote('');
  renderRows();
  renderVerify();
};
