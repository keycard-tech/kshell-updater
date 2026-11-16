import * as text from '../strings.js';
import type { Markup } from '../utils.js';
import { html, shortHash } from '../utils.js';
import type { ReadyFile } from '../base/state.js';
import type { ReleaseFile } from '../types.js';

export type Checked = 'ready' | 'failed' | 'pending' | null;

interface ShaView {
  content: Markup;
  verified: boolean;
}

export const shaFromFile = (ready: ReadyFile | null): ShaView => {
  const info = ready?.info;
  if (!info) return { content: '', verified: false };
  const mark = ready?.error
    ? html`<i class="ksh__sha-state ksh__sha-state--bad">${text.hash.unreadable}</i>`
    : info.matchesRelease
      ? html`<i class="ksh__sha-state ksh__sha-state--ok" title="${info.matchesRelease}">${text.hash.matchesRelease}</i>`
      : html`<i class="ksh__sha-state">${text.hash.fromFile}</i>`;
  return {
    content: html`${text.hash.label} <b>${shortHash(info.sha256)}</b>${mark}`,
    verified: !ready?.error,
  };
};

export const shaFromRelease = (file: ReleaseFile | null, checked: Checked): ShaView => {
  if (!file) return { content: '', verified: false };
  const mark =
    checked === 'failed'
      ? html`<i class="ksh__sha-state ksh__sha-state--bad">${text.hash.notVerified}</i>`
      : checked === 'pending'
        ? html`<i class="ksh__sha-state">${text.hash.checking}</i>`
        : '';
  const title = checked === 'ready' ? text.hash.matchesTitle : '';
  return {
    content: html`${text.hash.label}<b${title ? html` title="${title}"` : ''}> ${shortHash(file.sha256)}</b>${mark}`,
    verified: checked === 'ready',
  };
};

export const versionOptions = (files: ReleaseFile[], selected: number | null): Markup =>
  html`${files.map(
    (file) => html`<option value="${file.variant}"${file.variant === selected ? ' selected' : ''}>${text.row.variantOption(
      file.version ?? file.date,
      text.variants[file.variant ?? -1] ?? '',
    )}</option>`,
  )}`;
