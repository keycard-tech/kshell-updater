const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const KINDS = [
  {
    kind: 'firmware',
    title: 'Firmware',
    hintSwitch: 'Same version, other build: switching replaces the firmware and its data.',
    link: 'Release notes',
  },
  {
    kind: 'database',
    title: 'Database',
    hintSwitch: null,
    link: 'Version history',
  },
];

const VARIANT = { 0: 'omni', 1: 'btc' };

const state = {
  device: null,
  config: null,
  // Only the newest release reaches the view, so nothing here can offer an older build.
  files: { firmware: [], database: [] },
  mode: { firmware: 'online', database: 'online' },
  include: { firmware: true, database: true },
  variant: { firmware: 0, database: null },
  // What has been read from disk or downloaded: { info, path } or { error }.
  ready: { firmware: null, database: null },
  listError: null,
  busy: false,
  scanning: false,
  // Every offered file is downloaded and hash-checked at start-up, so the transfer only writes.
  // `cache` and `prepared` are keyed by `Prepared.key` (the release URL, or file://<path>).
  cache: {},
  prepared: {},
  preloading: true,
  // The run in hand: what is queued, which one is moving, and what came back afterwards.
  queue: [],
  step: 0,
  transfer: null,
  transferError: null,
  result: null,
  configError: null,
  screen: 'screen-home',
};

const el = (id) => document.getElementById(id);
const label = (kind) => KINDS.find((entry) => entry.kind === kind);

const rows = {};

function buildRows() {
  const host = el('rows');
  host.textContent = '';
  for (const { kind, title, link } of KINDS) {
    const row = document.createElement('div');
    row.className = 'ksh__row';
    row.dataset.kind = kind;
    row.innerHTML = `
      <div class="ksh__row-top">
        <div class="ksh__row-title">
          <input type="checkbox" data-slot="include" checked aria-label="Update ${title}" />
          <h2 class="ksh__h2">${title}</h2>
        </div>
        <div class="ksh__row-side">
          <span class="ksh__status" data-slot="status">Reading…</span>
          <a class="ksh__link" data-slot="link" hidden><img src="img/external.svg" alt="" />${link}</a>
        </div>
      </div>
      <div class="ksh__row-mid">
        <div class="ksh__segmented" data-slot="mode">
          <button type="button" class="ksh__seg" data-mode="online">Online</button>
          <button type="button" class="ksh__seg" data-mode="file">From file</button>
        </div>
        <div class="ksh__field" data-slot="field" hidden></div>
        <button type="button" class="ksh__btn ksh__btn--file ksh__btn--pick" data-slot="pick" hidden>
          Choose a .bin file
        </button>
      </div>
      <div class="ksh__row-foot">
        <span class="ksh__hint" data-slot="hint"></span>
        <span class="ksh__sha" data-slot="sha"></span>
      </div>`;
    host.append(row);

    const refs = {
      row,
      include: row.querySelector('[data-slot=include]'),
      status: row.querySelector('[data-slot=status]'),
      link: row.querySelector('[data-slot=link]'),
      field: row.querySelector('[data-slot=field]'),
      pick: row.querySelector('[data-slot=pick]'),
      hint: row.querySelector('[data-slot=hint]'),
      sha: row.querySelector('[data-slot=sha]'),
    };
    rows[kind] = refs;

    row.querySelector('[data-slot=mode]').addEventListener('click', (event) => {
      const mode = event.target?.dataset?.mode;
      if (!mode || state.mode[kind] === mode) return;
      state.mode[kind] = mode;
      render(kind);
    });
    refs.include.addEventListener('change', () => {
      state.include[kind] = refs.include.checked;
      render(kind);
    });
    refs.pick.addEventListener('click', () => chooseFile(kind));
  }
}

function filesFor(kind) {
  return state.files[kind] ?? [];
}

function selectedFile(kind) {
  const files = filesFor(kind);
  if (kind === 'database') return files[0] ?? null;
  return files.find((file) => file.variant === state.variant[kind]) ?? files[0] ?? null;
}

function versionOf(file) {
  return file?.version ?? file?.date ?? '';
}

function shortHash(hash) {
  return hash ? `${hash.slice(0, 8)}…${hash.slice(-4)}` : '';
}

/// Status text, and whether the row asks for work. 'Up to date' is the only state that does not: the
/// checkbox, the badge and the action button all follow it, so nothing can be "selected" that has
/// nowhere to go.
function rowState(kind) {
  if (state.mode[kind] === 'file') {
    const picked = state.ready[kind]?.info;
    if (!picked) return { status: 'No file', modifier: 'empty', wanted: false };
    const variant = VARIANT[picked.fwVariant] ?? '';
    return {
      status: `${picked.version}${variant ? `-${variant}` : ''} from file`,
      modifier: '',
      wanted: true,
    };
  }

  const file = selectedFile(kind);
  if (!file) {
    return state.listError
      ? { status: 'No list', modifier: 'empty', wanted: false }
      : { status: 'Reading…', modifier: 'empty', wanted: false };
  }

  const deviceVariant = state.config?.fwVariant ?? null;
  if (kind === 'firmware' && deviceVariant !== null && file.variant !== deviceVariant) {
    return { status: 'Variant switch', modifier: '', wanted: true };
  }
  const onDevice =
    kind === 'firmware' ? state.config?.fwVersion : String(state.config?.dbVersion ?? '');
  if (onDevice && onDevice === versionOf(file)) {
    return { status: 'Up to date', modifier: 'latest', wanted: false };
  }
  return { status: 'New version', modifier: '', wanted: true };
}

function wantedKinds() {
  return KINDS.map(({ kind }) => kind).filter(
    (kind) => state.include[kind] && rowState(kind).wanted,
  );
}

function render(kind) {
  const refs = rows[kind];
  const mode = state.mode[kind];
  const ready = state.ready[kind];
  const row = rowState(kind);

  for (const button of refs.row.querySelectorAll('[data-mode]')) {
    button.classList.toggle('ksh__seg--on', button.dataset.mode === mode);
  }
  refs.status.textContent = row.status;
  refs.status.className = `ksh__status${row.modifier ? ` ksh__status--${row.modifier}` : ''}`;
  refs.row.classList.toggle('ksh__row--off', !state.include[kind]);
  refs.include.checked = state.include[kind] && row.wanted;
  // Nothing may be re-aimed while bytes are going to the device.
  refs.include.disabled = !row.wanted || state.busy;
  for (const button of refs.row.querySelectorAll('[data-mode]')) button.disabled = state.busy;

  refs.field.hidden = mode !== 'online';
  refs.pick.hidden = mode !== 'file';
  refs.link.hidden = mode !== 'online' || !selectedFile(kind);

  if (mode === 'online') renderField(kind);

  // Nothing to say most of the time: the slot carries problems, not descriptions of the two kinds.
  refs.hint.textContent = ready?.error
    ? ready.error
    : row.status === 'Variant switch'
      ? label(kind).hintSwitch
      : state.listError
        ? `${state.listError} Use From file to install something you already have.`
        : preloadedState(kind) === 'failed'
          ? `${preloadedNote(kind) ?? 'The file could not be verified.'} Use From file instead.`
          : '';

  refs.sha.innerHTML = shaMarkup(kind, mode, ready, row);
  renderSummary();
}

function renderField(kind) {
  const refs = rows[kind];
  const files = filesFor(kind);
  if (!files.length) {
    refs.field.textContent = '';
    return;
  }
  if (kind === 'database') {
    const onDevice = state.config?.dbVersion ?? '—';
    refs.field.innerHTML = `<span class="ksh__field-static"><span class="ksh__from">${onDevice}</span><span class="ksh__arrow">&rarr;</span><span class="ksh__to">${versionOf(files[0])}</span></span>`;
    return;
  }
  // The choice is omni or btc, never an older build: the version is the same in both options.
  const options = files
    .map(
      (file) =>
        `<option value="${file.variant}"${file.variant === state.variant[kind] ? ' selected' : ''}>${versionOf(file)} - ${VARIANT[file.variant]}</option>`,
    )
    .join('');
  const signature = files.map((file) => file.url).join('|');
  if (refs.field.dataset.signature !== signature) {
    refs.field.innerHTML = `<select class="ksh__select" data-slot="select">${options}</select>`;
    refs.field.dataset.signature = signature;
    refs.field
      .querySelector('select')
      .addEventListener('change', (event) => {
        state.variant[kind] = Number(event.target.value);
        state.ready[kind] = null;
        render(kind);
      });
  }
}

function shaMarkup(kind, mode, ready, row) {
  if (mode === 'file') {
    const info = ready?.info;
    if (!info) return '';
    const mark = ready.error
      ? `<i class="ksh__sha-state ksh__sha-state--bad">unreadable</i>`
      : info.matchesRelease
        ? `<i class="ksh__sha-state ksh__sha-state--ok" title="${info.matchesRelease}">matches release</i>`
        : `<i class="ksh__sha-state">from file</i>`;
    return `sha256 <b>${shortHash(info.sha256)}</b>${mark}`;
  }
  const file = selectedFile(kind);
  if (!file) return '';
  // A verified online file needs no badge: every file offered here is hash-checked before the
  // screen is ready, so the plain hash is the normal case. Only a problem is worth a word.
  const state_ = preloadedState(kind);
  const mark =
    state_ === 'failed'
      ? `<i class="ksh__sha-state ksh__sha-state--bad">not verified</i>`
      : state_ === 'pending'
        ? `<i class="ksh__sha-state">checking…</i>`
        : '';
  return `sha256 <b>${shortHash(file.sha256)}</b>${mark}`;
}

/** Which of the prepared files this row currently points at: the chosen variant, or the picked file. */
function keyFor(kind) {
  return state.mode[kind] === 'file' ? state.ready[kind]?.key : selectedFile(kind)?.url;
}

const preloadedState = (kind) => {
  const key = keyFor(kind);
  if (!key) return null;
  const entry = state.cache[key];
  return entry ? (entry.ok ? 'ready' : 'failed') : 'pending';
};

const preloadedNote = (kind) => state.cache[keyFor(kind)]?.note;
const isReadyToFlash = (kind) => preloadedState(kind) === 'ready';

const SCREENS = ['screen-home', 'screen-ready', 'screen-transfer', 'screen-done'];

/** One screen at a time. The status bar belongs to the screens that have nowhere else to say it. */
function show(screen) {
  state.screen = screen;
  for (const id of SCREENS) el(id).hidden = id !== screen;
  el('statusbar').hidden = screen === 'screen-ready' || screen === 'screen-transfer';
}

/**
 * The landing screen's update card, in the words of mockups 01 and 02: what is on this Shell and
 * what can be done about it. Saying it here is what makes pressing the card an informed press.
 */
function renderHome() {
  const latest = { fw: versionOf(state.files.firmware[0]), db: versionOf(state.files.database[0]) };
  const device = Boolean(state.device);
  const readable = device && Boolean(state.config);
  const parts = [];
  if (readable) {
    if (state.config.fwVersion !== latest.fw) parts.push(`Firmware ${state.config.fwVersion}`);
    if (String(state.config.dbVersion) !== latest.db) parts.push(`database ${state.config.dbVersion}`);
  }

  el('home-badge').textContent = !device
    ? 'USB-C required'
    : !readable
      ? 'Device not ready'
      : parts.length
        ? 'Device ready'
        : 'Up to date';
  el('home-badge').className = `ksh__badge${readable ? ' ksh__badge--ready' : ' ksh__badge--warn'}`;
  el('home-text').textContent = !device
    ? 'Unlock your Shell and connect it with a USB-C cable, then choose what to transfer: firmware, database or both.'
    : !readable
      ? state.configError ?? 'The Shell did not answer. Lock it and try again.'
      : parts.length
      ? `${parts.join(' and ')} on this Shell can ${parts.length > 1 ? 'both' : ''} be updated. You approve the flash on the device screen.`
          .replace('can  be', 'can be')
      : 'This Shell runs the newest firmware and database.';
  el('home-sub').textContent = !device
    ? 'Connect your Shell to start'
    : !readable
      ? 'Unplug the Shell and plug it back in if it stays like this.'
      : state.listError
      ? 'Offline · updates from file only'
      : state.preloading
        ? 'Checking the files…'
        : 'Works with no internet connection, from files you already downloaded.';
  el('home-img').src = device ? 'img/shell.png' : 'img/connect_shell.png';
  el('card-update').disabled = !readable;
  el('card-update').setAttribute('aria-disabled', String(!readable));
}

function renderSummary() {
  const wanted = wantedKinds();
  el('select-badge').textContent = wanted.length
    ? `${wanted.length} update${wanted.length > 1 ? 's' : ''} selected`
    : 'No updates';
  el('select-badge').className = `ksh__badge${wanted.length ? ' ksh__badge--ready' : ''}`;
  el('chip-second').classList.toggle('ksh__chip--dim', wanted.length < 2);

  const waiting = wanted.filter((kind) => !isReadyToFlash(kind));
  const button = el('btn-transfer');
  button.disabled = state.busy || !wanted.length || waiting.length > 0;
  button.textContent = state.busy
    ? 'Transferring…'
    : !wanted.length
      ? 'Nothing to update'
      : state.preloading
        ? 'Preparing files…'
        : waiting.length
          ? 'File not verified'
          : 'Transfer to device';
}

/**
 * Fetch and hash-check every offered file at start-up, so pressing the button only ever writes.
 *
 * Both firmware variants are taken, not just the one the device reported: switching variant is a
 * choice made on this screen, and waiting for a download after it is made is what this step exists
 * to remove. Files already verified stay verified, so re-listing does not re-download.
 */
async function preload() {
  const files = KINDS.flatMap(({ kind }) =>
    filesFor(kind).map((file) => ({ kind, url: file.url })),
  ).filter((file) => !state.cache[file.url]);
  if (!files.length) {
    state.preloading = false;
    renderSummary();
    return;
  }
  state.preloading = true;
  renderSummary();
  try {
    for (const prepared of await invoke('preload_images', { files })) {
      state.prepared[prepared.key] = prepared.info;
      state.cache[prepared.key] = { ok: true };
    }
  } catch (error) {
    state.listError = String(error);
  }
  state.preloading = false;
  render('firmware');
  render('database');
}

const group = (value) => (value ?? 0).toLocaleString('en-US').replace(/,/g, ' ');

/** 'Firmware 1.5.3-omni' / 'Database 20261111', as the transfer screen names them. */
function itemLabel(kind) {
  if (state.mode[kind] === 'file') {
    const info = state.ready[kind]?.info;
    return `${kind === 'firmware' ? 'Firmware' : 'Database'} ${info?.version ?? ''}`.trim();
  }
  const file = selectedFile(kind);
  const variant = file?.variant === 1 ? '-btc' : file?.variant === 0 ? '-omni' : '';
  return `${kind === 'firmware' ? 'Firmware' : 'Database'} ${versionOf(file) ?? ''}${variant}`;
}

async function startTransfer() {
  const wanted = wantedKinds();
  if (state.busy || !wanted.length || wanted.some((kind) => !isReadyToFlash(kind))) return;

  // Firmware first and the database second, whatever order the rows are in: a new token list on a
  // firmware that cannot read it is worse than the other way round.
  state.queue = wanted.map((kind) => {
    const status = rowState(kind).status;
    return {
      kind,
      key: keyFor(kind),
      label: itemLabel(kind),
      bytes: state.prepared[keyFor(kind)]?.bytes ?? 0,
      done: false,
      // Both of these destroy something, and the row says so in the same words the person clicked
      // on. There is deliberately no dialog: the warning is on the screen, in the row.
      allowVariantSwitch: status === 'Variant switch',
      allowDowngrade: status === 'Downgrade',
    };
  });
  state.step = 0;
  state.transfer = null;
  state.transferError = null;
  state.result = null;
  show('screen-transfer');
  await runQueue();
}

async function runQueue() {
  state.busy = true;
  for (const [index, item] of state.queue.entries()) {
    state.step = index + 1;
    state.transfer = { kind: item.kind, label: item.label, done: 0, total: item.bytes };
    renderTransfer();
    try {
      // The device answers with the configuration it runs afterwards, which is the only proof that
      // the write landed - and it costs nothing, since the firmware restarts the device anyway.
      state.config = await invoke('flash_image', {
        request: {
          kind: item.kind,
          key: item.key,
          allowVariantSwitch: item.allowVariantSwitch,
          allowDowngrade: item.allowDowngrade,
        },
      });
      item.done = true;
    } catch (error) {
      state.transferError = String(error);
      break;
    }
  }
  state.busy = false;
  if (state.transferError) {
    renderTransfer();
    return;
  }
  state.result = {
    fw: state.config?.fwVersion ?? null,
    db: state.config?.dbVersion ?? null,
  };
  el('done-summary').textContent =
    `Your Shell now runs firmware ${state.result.fw} and database ${state.result.db}.`;
  el('status-device').textContent = `Keycard Shell connected · SN ${state.device?.serialNumber ?? ''}`;
  show('screen-done');
}

function renderTransfer() {
  const current = state.transfer ?? { done: 0, total: 0 };
  const failed = Boolean(state.transferError);
  const cancelled = state.transferError === 'cancelled';
  const percent = current.total ? Math.floor((current.done / current.total) * 100) : 0;

  el('transfer-title').textContent = failed
    ? cancelled
      ? 'Transfer cancelled'
      : 'Update failed'
    : `Transferring ${current.label ?? ''}`;
  el('transfer-sub').textContent = failed
    ? 'The Shell has not been asked to restart.'
    : 'Please do not disconnect the device.';
  el('transfer-step').textContent = `${state.step} / ${Math.max(state.queue.length, 1)}`;
  el('transfer-percent').textContent = `${percent}%`;
  el('transfer-bytes').textContent = `${group(current.done)} / ${group(current.total)} bytes`;
  el('transfer-bar').style.width = `${percent}%`;
  el('transfer-caption').textContent = failed ? state.transferError : 'Transferring update';
  el('transfer-note').textContent = failed
    ? 'Nothing else was written to the Shell.'
    : 'The device asks you to confirm the flash before it writes anything.';
  el('btn-cancel').hidden = failed;
  el('btn-transfer-back').hidden = !failed;

  el('transfer-items').innerHTML = state.queue
    .map((item, index) => {
      const label = item.done
        ? 'Done'
        : failed && index + 1 === state.step
          ? 'Failed'
          : failed
            ? 'Not started'
            : index + 1 === state.step
              ? 'Transferring'
              : 'Queued';
      const dim = label === 'Queued' || label === 'Not started';
      const stateClass =
        label === 'Done'
          ? 'ksh__item-state ksh__item-state--latest'
          : dim
            ? 'ksh__item-state ksh__item-state--waiting'
            : 'ksh__item-state';
      return `<div class="ksh__item${dim ? ' ksh__item--waiting' : ''}">${item.label}<i class="${stateClass}">${label}</i></div>`;
    })
    .join('');
}

async function loadReleases() {
  for (const { kind } of KINDS) {
    try {
      const catalogue = await invoke('list_releases', { kind });
      state.files[kind] = catalogue.files;
    } catch (error) {
      state.listError = String(error);
      state.files[kind] = [];
    }
    render(kind);
  }
  const fw = state.files.firmware[0];
  const db = state.files.database[0];
  el('status-note').textContent = state.listError
    ? 'Offline · updates from file only'
    : fw && db
      ? `Online · latest firmware ${versionOf(fw)} · latest database ${versionOf(db)}`
      : 'reading the release list…';
  renderHome();
}

async function readConfiguration(device) {
  state.config = null;
  state.configError = null;
  try {
    state.config = await invoke('shell_app_configuration', { path: device.path });
    const variant = state.config.fwVariant ?? null;
    el('device-line').textContent =
      `SN ${device.serialNumber ?? state.config.serialNumber} · on device: firmware ${state.config.fwVersion}` +
      `${variant === null ? '' : `-${VARIANT[variant] ?? variant}`}, database ${state.config.dbVersion}`;
    el('device-line').title = `attestation key ${state.config.publicKey}`;
    // Default to the variant already on the device, so the plain action is not a variant switch.
    if (variant !== null) state.variant.firmware = variant;
  } catch (error) {
    state.configError = String(error);
    el('device-line').textContent = state.configError;
  }
  render('firmware');
  render('database');
  renderHome();
}

async function refresh() {
  try {
    const devices = await invoke('list_shells');
    if (!devices.length) {
      showWaiting('No device connected');
      return;
    }
    const device = devices[0];
    state.device = device;
    el('device-name').textContent = `${device.productName || 'Keycard Shell'} connected`;
    el('status-dot').className = 'ksh__dot';
    el('status-device').textContent = `Keycard Shell connected · SN ${device.serialNumber ?? ''}`;
    // Plugging in does not hijack the landing screen; it makes the card pressable, and pressing the
    // card is what opens the update flow.
    show(state.screen);
    await readConfiguration(device);
  } catch (error) {
    el('device-line').textContent = String(error);
  }
}

function showWaiting(message) {
  state.device = null;
  state.config = null;
  show('screen-home');
  renderHome();
  el('status-device').textContent = message;
  el('status-dot').className = 'ksh__dot ksh__dot--off';
}

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
    const prepared = await invoke('pick_image_file', { file: { kind, path } });
    state.ready[kind] = { info: prepared.info, key: prepared.key };
    state.prepared[prepared.key] = prepared.info;
    state.cache[prepared.key] = { ok: true };
    state.include[kind] = true;
  } catch (error) {
    state.ready[kind] = { error: String(error) };
  }
  render(kind);
}

buildRows();
el('btn-transfer').addEventListener('click', startTransfer);
el('btn-cancel').addEventListener('click', () => {
  el('transfer-caption').textContent = 'Cancelling after the chunk in flight…';
  invoke('cancel_flash');
});
el('btn-transfer-back').addEventListener('click', () => {
  state.transferError = null;
  show('screen-ready');
  refresh();
});
el('card-update').addEventListener('click', () => {
  if (!state.device || !state.config) return;
  show('screen-ready');
  refresh();
});
el('btn-done').addEventListener('click', () => {
  show('screen-home');
  refresh();
});
listen('shell://connected', () => refresh());
listen('shell://disconnected', () => {
  // Unplugged mid-transfer: the write cannot continue, and the transfer screen would sit there
  // showing a percentage that will never move.
  if (!el('screen-transfer').hidden && !state.transferError) state.transferError = 'The device was disconnected.';
  renderTransfer();
  showWaiting('No device connected');
});
listen('shell://flash-progress', ({ payload }) => {
  state.transfer = { ...state.transfer, ...payload };
  renderTransfer();
});
listen('shell://preloaded', ({ payload }) => {
  state.cache[payload.key] = { ok: payload.ok, note: payload.note };
  if (keyFor('firmware') === payload.key) render('firmware');
  if (keyFor('database') === payload.key) render('database');
});
showWaiting('No device connected');
refresh();
loadReleases().then(preload);

// Presence is polled as well as pushed: the Rust watcher's first `shell://connected` can fire before
// this document has a listener, and a Shell that was already plugged in then stays invisible until
// the cable is moved. The same tick retries a configuration read that failed on a device still
// settling, which `readConfiguration` reports by leaving `state.config` null.
setInterval(async () => {
  if (state.scanning || state.busy) return;
  state.scanning = true;
  try {
    const found = await invoke('list_shells');
    const path = found.length ? found[0].path : null;
    if (path !== state.device?.path || (path && !state.config)) await refresh();
  } catch {
    // Unreadable for a moment while USB settles; the next tick sees it.
  } finally {
    state.scanning = false;
  }
}, 1500);
