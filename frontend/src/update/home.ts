/* The landing card: what this Shell runs, and what it is owed. */

import { state } from '../base/state.js';
import { images } from '../config.js';
import * as text from '../strings.js';
import { el } from '../utils.js';
import { filesFor, versionOf } from './files.js';
import { owedVersions } from './record.js';

export const renderHome = (): void => {
  const owed = owedVersions();
  const latest = {
    firmware: versionOf(filesFor('firmware')[0]),
    database: versionOf(filesFor('database')[0]),
  };
  const present = Boolean(state.device);
  const readable = present && Boolean(state.config);

  const parts: string[] = [];
  if (readable && state.config) {
    if (state.config.fwVersion !== latest.firmware) {
      parts.push(text.home.partFirmware(state.config.fwVersion));
    }
    if (String(state.config.dbVersion) !== latest.database) {
      parts.push(text.home.partDatabase(String(state.config.dbVersion)));
    }
  }

  const badge = el('home-badge');
  badge.textContent = !present
    ? text.home.badgeNoDevice
    : !readable
      ? text.home.badgeNotReady
      : parts.length
        ? text.home.badgeReady
        : text.home.badgeUpToDate;
  badge.className = `ksh__badge${
    readable && !owed ? ' ksh__badge--ready' : ' ksh__badge--warn'
  }`;

  el('home-text').textContent = !present
    ? text.home.connect
    : !readable
      ? (state.configError ?? text.home.silentDevice)
      : parts.length
        ? text.home.updated(parts.join(' and '), parts.length > 1)
        : text.home.current;

  el('home-sub').textContent = !present
    ? text.home.start
    : !readable
      ? text.home.replug
      : owed
        ? text.home.owed(owed.join(' and '))
        : state.listError
          ? text.home.offline
          : state.preloading
            ? text.home.checking
            : text.home.airGapped;

  el<HTMLImageElement>('home-img').src = present ? images.shell : images.connect;

  const card = el<HTMLButtonElement>('card-update');
  card.disabled = !readable;
  card.setAttribute('aria-disabled', String(!readable));
};
