/* Markup of one release row and the pieces inside it. */

import * as text from '../strings.js';
import type { Markup } from '../utils.js';
import { html, shortHash } from '../utils.js';
import type { ReadyFile } from '../base/state.js';
import type { ImageKind, ReleaseFile } from '../types.js';

export type Checked = 'ready' | 'failed' | 'pending' | null;

export interface Entry {
  kind: ImageKind;
  title: string;
  link: string;
  url: string;
}

export const rowShell = ({ title, link, url }: Entry): Markup => html`
  <div class="ksh__row-top">
    <div class="ksh__row-title">
      <input type="checkbox" data-slot="include" checked aria-label="${text.row.includeLabel(title)}" />
      <h2 class="ksh__h2">${title}</h2>
    </div>
    <span class="ksh__status" data-slot="status">${text.status.reading}</span>
  </div>
  <div class="ksh__row-mid">
    <div class="ksh__segmented" data-slot="mode">
      <button type="button" class="ksh__seg" data-mode="online">${text.row.online}</button>
      <button type="button" class="ksh__seg" data-mode="file">${text.row.fromFile}</button>
    </div>
    <div class="ksh__field" data-slot="field" hidden></div>
    <button type="button" class="ksh__btn ksh__btn--file ksh__btn--pick" data-slot="pick" hidden>
      ${text.row.choose}
    </button>
    <a class="ksh__link" data-slot="link" href="${url}" hidden>
      <img src="img/external.svg" alt="" />${link}
    </a>
  </div>
  <div class="ksh__row-foot">
    <span class="ksh__hint" data-slot="hint"></span>
    <span class="ksh__sha" data-slot="sha"></span>
  </div>
`;

/** The release decided the version, so there is nothing to pick: the same box, as a statement. */
export const versionStatic = (onDevice: string, latest: string): Markup => html`
  <span class="ksh__field-static">
    <span class="ksh__from">${onDevice}</span><span class="ksh__arrow">&rarr;</span><span class="ksh__to">${latest}</span>
  </span>
`;

/** omni or btc, never an older build: the version is the same in both options. */
export const versionSelect = (files: ReleaseFile[], selected: number | null): Markup => html`<select class="ksh__select" data-slot="select">${files.map((file) => html`<option value="${file.variant}"${file.variant === selected ? ' selected' : ''}>${text.row.variantOption(file.version ?? file.date, text.variants[file.variant ?? -1] ?? '')}</option>`)}</select>`;

export interface ShaView {
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
