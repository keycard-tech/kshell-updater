import { state } from '../base/state.js';
import * as text from '../strings.js';
import { el } from '../utils.js';
import { owedVersions } from './record.js';
import { wantedKinds } from './rows.js';

interface Words {
  badge: string;
  settled: boolean;
  prompt: string;
  hint: string;
}

const words = (): Words => {
  if (!state.device)
    return {
      badge: text.home.badgeNoDevice,
      settled: false,
      prompt: text.home.connect,
      hint: text.home.connectHint,
    };
  if (!state.config)
    return {
      badge: text.home.badgeNotReady,
      settled: false,
      prompt: text.home.silentDevice,
      hint: text.home.replug,
    };

  const owed = owedVersions();
  const variant = state.config.fwVariant;
  const runs = text.home.runs(
    state.config.fwVersion,
    variant === null || variant === undefined ? '' : `-${text.variants[variant] ?? variant}`,
    String(state.config.dbVersion),
  );
  if (owed)
    return {
      badge: text.home.badgeOwed,
      settled: false,
      prompt: text.home.prompt,
      hint: text.home.owed(owed.join(' and ')),
    };

  const updates = wantedKinds().length;
  return {
    badge: updates ? text.home.badgeUpdates(updates) : text.home.badgeUpToDate,
    settled: true,
    prompt: text.home.prompt,
    hint: runs,
  };
};

export const renderHome = (): void => {
  const said = words();
  const badge = el('home-badge');
  badge.textContent = said.badge;
  badge.className = `ksh__badge${said.settled ? ' ksh__badge--ready' : ' ksh__badge--warn'}`;
  el('home-text').textContent = said.prompt;
  el('home-sub').textContent = said.hint;

  el('card-update').setAttribute('aria-disabled', String(!state.config));
};
