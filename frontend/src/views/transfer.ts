/* Markup of the transfer queue. */

import type { Markup } from '../utils.js';
import { html } from '../utils.js';

export type Tone = 'latest' | 'waiting' | 'failed';

/** The turning squares, not a word: the row already says which image it is. */
export const spinner = (): Markup =>
  html`<i class="ksh__loader"><i></i><i></i></i>`;

/** Label and marker are asserted as one string, so nothing separates them in the markup. */
export const itemRow = (label: string, marker: Markup, dim: boolean): Markup =>
  html`<div class="ksh__item${dim ? ' ksh__item--waiting' : ''}">${label}${marker}</div>`;

export const stateWord = (word: string, tone: Tone): Markup =>
  html`<i class="ksh__item-state ksh__item-state--${tone}">${word}</i>`;
