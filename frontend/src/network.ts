import { state } from './base/state.js';
import * as text from './strings.js';
import { el } from './utils.js';
import { renderRows } from './update/rows.js';
import { renderVerify } from './verify/screen.js';

export const renderNote = (aboutTheList: string): void => {
  el('status-note').textContent = state.offline ? '' : aboutTheList;
};

export const forgetOnline = (): void => {
  for (const { kind } of text.kinds) {
    state.files[kind] = [];
  }
  state.listError = null;
  state.preloading = false;
};

export const renderNetwork = (): void => {
  el('status-net').hidden = !state.offline;
  el('status-net-word').textContent = text.network.offline;
  renderNote('');
  renderRows();
  renderVerify();
};
