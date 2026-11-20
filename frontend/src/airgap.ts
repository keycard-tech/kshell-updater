import { linkCode, openExternal } from './base/bridge.js';
import { human } from './errors.js';
import * as text from './strings.js';
import { el, reason } from './utils.js';

const drawn: Record<string, string> = {};

export const openCode = async (url: string, what: string): Promise<void> => {
  const tile = el('ag-code');
  const hint = el('ag-hint');
  tile.innerHTML = '';
  hint.classList.remove('ksh__hint--bad');
  hint.textContent = text.airgap.hint;
  el('ag-title').textContent = text.airgap.download(what);
  el('ag-modal').hidden = false;
  try {
    if (!drawn[url]) drawn[url] = await linkCode(url);
    tile.innerHTML = drawn[url];
  } catch (error) {
    hint.classList.add('ksh__hint--bad');
    hint.textContent = human(reason(error));
  }
};

const closeCode = (): void => {
  el('ag-modal').hidden = true;
};

export const buildAirgap = (): void => {
  el('home-airgap-prompt').textContent = text.airgap.prompt;
  const guide = el<HTMLAnchorElement>('ag-guide');
  guide.href = text.links.airGapped;
  guide.textContent = text.airgap.guide;
  el('ag-close').textContent = text.airgap.close;

  guide.addEventListener('click', (event) => {
    event.preventDefault();
    event.stopPropagation();
    openExternal(text.links.airGapped).catch((error: unknown) =>
      console.warn(`could not open ${text.links.airGapped}: ${reason(error)}`),
    );
  });
  el('ag-modal').addEventListener('click', closeCode);
  addEventListener('keydown', (event) => {
    if (event.key === 'Escape') closeCode();
  });
};
