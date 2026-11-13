const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const el = (id) => document.getElementById(id);

const KINDS = [
  {
    kind: 'firmware',
    title: 'Firmware',
    hint: 'omni signs Bitcoin and Ethereum, btc-only signs Bitcoin only.',
  },
  {
    kind: 'database',
    title: 'Database',
    hint: 'Ethereum contract database, identified by its date.',
  },
];

const VARIANT = { 0: 'omni', 1: 'btc-only' };

const state = {
  device: null,
  config: null,
  releases: { firmware: null, database: null },
  mode: { firmware: 'online', database: 'online' },
  // Image read from disk or downloaded, per kind: { info, fromFile }
  ready: { firmware: null, database: null },
  // Set by whatever happened last (a failed list, a finished verification) and cleared when the
  // selection moves again, so the note is never both a status and a stale result.
  note: null,
  tone: 'info',
  busy: false,
};

const TONE = { info: 'var(--white-40)', ok: 'var(--teal)', bad: 'var(--error)', warn: 'var(--orange)' };

function setBadge(text, modifier) {
  el('badge').textContent = text;
  el('badge').className = `ksh__badge${modifier ? ` ksh__badge--${modifier}` : ''}`;
}

function showWaiting(message) {
  el('card-empty').hidden = false;
  el('card-device').hidden = true;
  el('subtitle').textContent = message;
  el('status-device').textContent = 'No device connected';
  setBadge('No device', 'warn');
}

function showDevice(device) {
  state.device = device;
  el('card-empty').hidden = true;
  el('card-device').hidden = false;
  el('device-name').textContent = device.productName || 'Keycard Shell';
  el('status-device').textContent = `${device.productName || 'Keycard Shell'} connected`;
  setBadge('Device ready', 'ready');
}

function onDeviceText(config) {
  if (!config) return null;
  const variant = config.fwVariant ?? null;
  return `on device: firmware ${config.fwVersion}${variant === null ? '' : `-${VARIANT[variant] ?? variant}`}, database ${config.dbVersion}`;
}

async function readConfiguration(device) {
  state.config = null;
  el('device-line').textContent = 'reading the app configuration…';
  try {
    state.config = await invoke('shell_app_configuration', { path: device.path });
    el('device-line').textContent = `serial ${device.serialNumber ?? state.config.serialNumber}`;
    el('device-line').title = `attestation key ${state.config.publicKey}`;
    el('subtitle').textContent = onDeviceText(state.config);
    renderRows();
    updateNote();
  } catch (error) {
    el('subtitle').textContent = String(error);
    setBadge('Read failed', 'error');
  }
}

const rows = {};

function buildRows() {
  const host = el('rows');
  host.textContent = '';
  for (const { kind, title } of KINDS) {
    const row = document.createElement('div');
    row.className = 'ksh__row';
    row.dataset.kind = kind;
    row.innerHTML = `
      <div class="ksh__row-top">
        <div class="ksh__row-title"><h2 class="ksh__h2">${title}</h2></div>
        <span class="ksh__status ksh__status--empty" data-slot="status">reading…</span>
      </div>
      <div class="ksh__row-mid">
        <div class="ksh__segmented" data-slot="mode">
          <button type="button" class="ksh__seg" data-mode="online">Release</button>
          <button type="button" class="ksh__seg" data-mode="file">From file</button>
        </div>
        <select class="ksh__select" data-slot="select" disabled></select>
        <button type="button" class="ksh__btn ksh__btn--file" data-slot="pick" hidden>
          Choose file…
        </button>
      </div>
      <div class="ksh__row-foot">
        <span class="ksh__hint" data-slot="hint"></span>
        <span class="ksh__sha" data-slot="sha"></span>
      </div>`;
    host.append(row);

    const refs = {
      row,
      status: row.querySelector('[data-slot=status]'),
      select: row.querySelector('[data-slot=select]'),
      pick: row.querySelector('[data-slot=pick]'),
      hint: row.querySelector('[data-slot=hint]'),
      sha: row.querySelector('[data-slot=sha]'),
    };
    rows[kind] = refs;

    row.querySelector('[data-slot=mode]').addEventListener('click', (event) => {
      const mode = event.target?.dataset?.mode;
      if (!mode || state.mode[kind] === mode) return;
      state.mode[kind] = mode;
      state.note = null;
      renderRow(kind);
      updateAction();
    });
    refs.select.addEventListener('change', () => {
      state.ready[kind] = null;
      state.note = null;
      renderRow(kind);
      updateAction();
    });
    refs.pick.addEventListener('click', () => chooseFile(kind));
  }
}

function optionsFor(kind) {
  return (state.releases[kind]?.files ?? []).filter((file) =>
    kind === 'database' ? true : file.variant !== null && file.variant !== undefined
  );
}

function selectedFile(kind) {
  const files = optionsFor(kind);
  const url = rows[kind].select.value;
  return files.find((file) => file.url === url) ?? null;
}

function defaultUrl(kind) {
  const files = optionsFor(kind);
  if (kind === 'firmware' && state.config?.fwVariant !== undefined) {
    // Offer the variant already on the device first, so the default action is not a variant switch.
    const matching = files.filter((file) => file.variant === state.config.fwVariant);
    if (matching.length) return matching[0].url;
  }
  return files[0]?.url ?? '';
}

function shortHash(hash) {
  return hash ? `${hash.slice(0, 8)}…${hash.slice(-4)}` : '—';
}

function versionOf(file) {
  return file?.version ?? file?.date ?? '';
}

function renderRow(kind) {
  const refs = rows[kind];
  const catalogue = state.releases[kind];
  const mode = state.mode[kind];
  const ready = state.ready[kind];

  for (const button of refs.row.querySelectorAll('[data-mode]')) {
    button.classList.toggle('ksh__seg--on', button.dataset.mode === mode);
  }
  refs.select.hidden = mode !== 'online';
  refs.pick.hidden = mode !== 'file';
  refs.hint.textContent = ready?.error ?? KINDS.find((k) => k.kind === kind).hint;

  if (mode === 'file') {
    refs.select.disabled = true;
    const picked = ready?.info;
    refs.pick.textContent = picked ? `${shortName(ready.path)}` : 'Choose file…';
    setStatus(refs.status, picked ? `${versionOf(picked)} from file` : 'nothing picked', picked ? '' : 'empty');
    refs.sha.innerHTML = picked
      ? `sha256 <b>${shortHash(picked.sha256)}</b><i class="ksh__sha-state">from file</i>`
      : '';
    return;
  }

  if (!catalogue) {
    refs.select.disabled = true;
    refs.select.innerHTML = '';
    setStatus(refs.status, 'reading…', 'empty');
    refs.sha.innerHTML = '';
    return;
  }

  const files = optionsFor(kind);
  // Rebuilt only when the list changes, so a re-render does not reset the person's selection.
  if (refs.select.dataset.signature !== catalogueSignature(kind)) {
    refs.select.innerHTML = files
      .map((file) => {
        const variant = VARIANT[file.variant] ?? '';
        const date = file.version ? ` · ${file.date}` : '';
        return `<option value="${file.url}">${versionOf(file)}${variant ? ` · ${variant}` : ''}${date}</option>`;
      })
      .join('');
    refs.select.value = defaultUrl(kind);
    refs.select.dataset.signature = catalogueSignature(kind);
  }
  refs.select.disabled = !files.length;

  const file = selectedFile(kind);
  if (!file) {
    setStatus(refs.status, 'nothing published', 'empty');
    refs.sha.innerHTML = '';
    return;
  }

  const onDevice = kind === 'firmware' ? state.config?.fwVersion : String(state.config?.dbVersion ?? '');
  // Older firmware reports no variant at all, which is unknown rather than a difference.
  const deviceVariant = state.config?.fwVariant ?? null;
  const switching = kind === 'firmware' && deviceVariant !== null && file.variant !== deviceVariant;
  if (switching) {
    setStatus(refs.status, 'variant switch', '');
  } else if (onDevice && onDevice === versionOf(file)) {
    setStatus(refs.status, 'up to date', 'latest');
  } else {
    setStatus(refs.status, 'new version', '');
  }

  const mark = ready?.error
    ? `<i class="ksh__sha-state ksh__sha-state--bad">${badState(ready.error)}</i>`
    : ready
      ? `<i class="ksh__sha-state ksh__sha-state--ok">verified</i>`
      : `<i class="ksh__sha-state">expected</i>`;
  refs.sha.innerHTML = `sha256 <b>${shortHash(file.sha256)}</b>${mark}`;
  if (ready?.error) refs.hint.textContent = ready.error;
}

// The download error says which check failed, in the words the Rust side uses.
function badState(error) {
  return error.includes('checksum mismatch') ? 'mismatch' : 'failed';
}

function catalogueSignature(kind) {
  return (state.releases[kind]?.files ?? []).map((file) => file.url).join('|');
}

function setStatus(node, text, modifier) {
  node.textContent = text;
  node.className = `ksh__status${modifier ? ` ksh__status--${modifier}` : ''}`;
}

function shortName(path) {
  return path.split('/').pop() || path;
}

// The dialog plugin's own command. Its api-iife.js would have to be vendored into the frontend for
// window.__TAURI__.dialog to exist, which is not worth a copy step to keep in sync.
async function chooseFile(kind) {
  try {
    const path = await invoke('plugin:dialog|open', {
      options: {
        multiple: false,
        title: kind === 'firmware' ? 'Choose a firmware image' : 'Choose a database image',
        filters: [{ name: 'Keycard image', extensions: ['bin'] }],
      },
    });
    if (!path) return;
    const image = await invoke('pick_image_file', { file: { kind, path } });
    state.ready[kind] = { info: image, path, fromFile: true };
    state.note = `${kind} ${image.version} read from ${shortName(path)}; the file's own sha256 is ${shortHash(image.sha256)}.`;
    state.tone = 'ok';
    renderRow(kind);
  } catch (error) {
    state.ready[kind] = { error: String(error), path: '' };
    state.note = String(error);
    state.tone = 'bad';
    renderRow(kind);
  }
  updateAction();
}

function target(kind) {
  if (state.mode[kind] === 'file') return state.ready[kind]?.path ? 'file' : null;
  return selectedFile(kind) ? 'release' : null;
}

function selectionSummary() {
  const parts = [];
  for (const { kind } of KINDS) {
    if (state.mode[kind] === 'file') {
      const picked = state.ready[kind]?.info;
      if (picked) parts.push(`${kind} ${versionOf(picked)} from file`);
      continue;
    }
    const file = selectedFile(kind);
    if (file) parts.push(`${kind} ${versionOf(file)}`);
  }
  if (!parts.length) return 'Nothing selected to download.';
  return `${parts.join(' and ')} will be downloaded and checked against the sha256 published in the release.`;
}

function updateNote() {
  const note = el('release-note');
  note.textContent = state.note ?? selectionSummary();
  note.style.color = TONE[state.note ? state.tone : 'info'];
}

function updateAction() {
  updateNote();
  const button = el('btn-download');
  const any = KINDS.some(({ kind }) => target(kind));
  button.disabled = state.busy || !any;
  button.textContent = state.busy ? 'Working…' : 'Download and verify';
}

async function downloadAndVerify() {
  state.busy = true;
  updateAction();
  const done = [];
  const failures = [];
  for (const { kind } of KINDS) {
    const wanted = target(kind);
    if (!wanted) continue;
    if (wanted === 'file') {
      done.push(`${kind} ${versionOf(state.ready[kind].info)} from file`);
      continue;
    }
    const file = selectedFile(kind);
    try {
      const info = await invoke('download_release', { request: { kind, url: file.url } });
      state.ready[kind] = { info, fromFile: false };
      done.push(`${kind} ${info.version}`);
    } catch (error) {
      state.ready[kind] = { error: String(error) };
      failures.push(String(error));
    }
    renderRow(kind);
  }
  state.busy = false;
  if (failures.length) {
    state.note = failures[0];
    state.tone = 'bad';
  } else if (done.length) {
    state.note = `${done.join(' and ')} — sha256 matches the published release. Writing to the device is not in this build.`;
    state.tone = 'ok';
  }
  updateAction();
}

async function loadReleases() {
  for (const { kind } of KINDS) {
    try {
      const catalogue = await invoke('list_releases', { kind });
      state.releases[kind] = catalogue;
      if (catalogue.note) {
        state.note = `some hash lists could not be read: ${catalogue.note}`;
        state.tone = 'warn';
      }
    } catch (error) {
      state.releases[kind] = { files: [], unofferable: 0, verified: false, note: String(error) };
      state.note = `release list unavailable: ${error}`;
      state.tone = 'bad';
    }
    renderRow(kind);
  }
  const fw = state.releases.firmware;
  if (fw?.files.length) {
    const hidden = fw.unofferable + state.releases.database?.unofferable;
    el('status-note').textContent = hidden
      ? `${fw.files.length} releases offered · ${hidden} without a published hash`
      : `${fw.files.length} releases offered`;
  }
  updateAction();
}

async function refresh() {
  try {
    const devices = await invoke('list_shells');
    if (!devices.length) {
      showWaiting('No Keycard Shell on USB');
      return;
    }
    showDevice(devices[0]);
    renderRows();
    await readConfiguration(devices[0]);
  } catch (error) {
    el('subtitle').textContent = String(error);
    setBadge('USB unavailable', 'error');
  }
}

function renderRows() {
  for (const { kind } of KINDS) renderRow(kind);
}

buildRows();
el('btn-download').addEventListener('click', downloadAndVerify);
listen('shell://connected', () => refresh());
listen('shell://disconnected', () => showWaiting('The Shell was unplugged'));
showWaiting('Checking for a connected Shell…');
loadReleases();
refresh();
