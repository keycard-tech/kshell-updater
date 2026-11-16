const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const KINDS = [
  {
    kind: 'firmware',
    title: 'Firmware',
    link: 'Release notes',
    linkUrl: 'https://shell.keycard.tech/firmware/release-notes/',
  },
  {
    kind: 'database',
    title: 'Database',
    link: 'Version history',
    linkUrl: 'https://shell.keycard.tech/update/db-version-history/',
  },
];

const VARIANT = { 0: 'omni', 1: 'btc' };

/// The same literal as `CANCELLED` in src-tauri/src/lib.rs: matched on, never shown.
const CANCELLED = 'cancelled';

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
  // Both keyed by `Prepared.key`: the release URL, or file://<path> for a file from disk.
  cache: {},
  prepared: {},
  preloading: true,
  // The run in hand: what is queued, which one is moving, and what came back afterwards.
  queue: [],
  step: 0,
  transfer: null,
  transferError: null,
  configError: null,
  record: null,
  screen: 'screen-home',
};

const el = (id) => document.getElementById(id);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const rows = {};

function buildRows() {
  const host = el('rows');
  host.textContent = '';
  for (const { kind, title, link, linkUrl } of KINDS) {
    const row = document.createElement('div');
    row.className = 'ksh__row';
    row.dataset.kind = kind;
    row.innerHTML = `
      <div class="ksh__row-top">
        <div class="ksh__row-title">
          <input type="checkbox" data-slot="include" checked aria-label="Update ${title}" />
          <h2 class="ksh__h2">${title}</h2>
        </div>
        <span class="ksh__status" data-slot="status">Reading…</span>
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
        <a class="ksh__link" data-slot="link" href="${linkUrl}" hidden
          ><img src="img/external.svg" alt="" />${link}</a>
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
    refs.link.addEventListener('click', (event) => {
      event.preventDefault();
      // Out to the desktop browser: following the link here would navigate away mid-transfer.
      invoke('plugin:opener|open_url', { url: linkUrl }).catch((error) =>
        console.warn(`could not open ${linkUrl}: ${error}`),
      );
    });
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

/// Status text and whether the row asks for work - the checkbox, badge and action follow that.
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
  refs.include.disabled = !row.wanted || state.busy;
  for (const button of refs.row.querySelectorAll('[data-mode]')) button.disabled = state.busy;

  refs.field.hidden = mode !== 'online';
  refs.pick.hidden = mode !== 'file';
  refs.link.hidden = mode !== 'online' || !selectedFile(kind);

  if (mode === 'online') renderField(kind);

  const offline = !ready?.error && state.listError;
  // The failure detail lives in the tooltip: in the row it would wrap and grow every screen.
  refs.hint.textContent = ready?.error
    ? ready.error
    : offline
      ? 'Offline - only air-gapped update is available.'
      : preloadedState(kind) === 'failed'
        ? (preloadedNote(kind) ?? 'The file could not be read.')
        : '';
  refs.hint.title = offline ? state.listError : '';

  const sha = shaMarkup(kind, mode, ready);
  refs.sha.innerHTML = sha.html;
  refs.sha.className = `ksh__sha${sha.verified ? ' ksh__sha--ok' : ''}`;
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
    refs.field.querySelector('select').addEventListener('change', (event) => {
      state.variant[kind] = Number(event.target.value);
      state.ready[kind] = null;
      render(kind);
    });
  }
}

/** Green means the published hash already matched; only a problem is worth a word. */
function shaMarkup(kind, mode, ready) {
  if (mode === 'file') {
    const info = ready?.info;
    if (!info) return { html: '', verified: false };
    const mark = ready.error
      ? `<i class="ksh__sha-state ksh__sha-state--bad">unreadable</i>`
      : info.matchesRelease
        ? `<i class="ksh__sha-state ksh__sha-state--ok" title="${info.matchesRelease}">matches release</i>`
        : `<i class="ksh__sha-state">from file</i>`;
    return {
      html: `sha256 <b>${shortHash(info.sha256)}</b>${mark}`,
      verified: !ready.error,
    };
  }
  const file = selectedFile(kind);
  if (!file) return { html: '', verified: false };
  const checked = preloadedState(kind);
  const mark =
    checked === 'failed'
      ? `<i class="ksh__sha-state ksh__sha-state--bad">not verified</i>`
      : checked === 'pending'
        ? `<i class="ksh__sha-state">checking…</i>`
        : '';
  const title = checked === 'ready' ? ' title="sha256 matches the published hash"' : '';
  return {
    html: `sha256<b${title}> ${shortHash(file.sha256)}</b>${mark}`,
    verified: checked === 'ready',
  };
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

const SCREENS = [
  'screen-home',
  'screen-ready',
  'screen-transfer',
  'screen-done',
  'screen-verify',
  'screen-scan',
  'screen-verdict',
];

/** One screen at a time. The status bar belongs to the screens that have nowhere else to say it. */
function show(screen) {
  state.screen = screen;
  for (const id of SCREENS) el(id).hidden = id !== screen;
  el('statusbar').hidden = screen === 'screen-ready' || screen === 'screen-transfer';
  if (screen !== 'screen-scan') stopCamera();
}

/** The landing card, in the words of mockups 01 and 02. */
function renderHome() {
  const outstanding = outstandingUpdate();
  const latest = {
    fw: versionOf(state.files.firmware[0]),
    db: versionOf(state.files.database[0]),
  };
  const device = Boolean(state.device);
  const readable = device && Boolean(state.config);
  const parts = [];
  if (readable) {
    if (state.config.fwVersion !== latest.fw) parts.push(`Firmware ${state.config.fwVersion}`);
    if (String(state.config.dbVersion) !== latest.db)
      parts.push(`database ${state.config.dbVersion}`);
  }

  el('home-badge').textContent = !device
    ? 'USB-C required'
    : !readable
      ? 'Device not ready'
      : parts.length
        ? 'Device ready'
        : 'Up to date';
  el('home-badge').className = `ksh__badge${
    readable && !outstanding ? ' ksh__badge--ready' : ' ksh__badge--warn'
  }`;
  el('home-text').textContent = !device
    ? 'Unlock your Shell and connect it with a USB-C cable, then choose what to transfer: firmware, database or both.'
    : !readable
      ? (state.configError ?? 'The Shell did not answer. Lock it and try again.')
      : parts.length
        ? `${parts.join(' and ')} on this Shell ${
            parts.length > 1 ? 'can both' : 'can'
          } be updated. You approve the flash on the device screen.`
        : 'This Shell runs the newest firmware and database.';
  el('home-sub').textContent = !device
    ? 'Connect your Shell to start'
    : !readable
      ? 'Unplug the Shell and plug it back in if it stays like this.'
      : outstanding
        ? `An update did not finish: this Shell still runs ${outstanding.join(' and ')}.`
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

  const waiting = wanted.filter((kind) => !isReadyToFlash(kind));
  const button = el('btn-transfer');
  button.disabled = state.busy || !wanted.length || waiting.length > 0;
  button.textContent = state.busy
    ? 'Transferring…'
    : !wanted.length
      ? 'Transfer'
      : state.preloading
        ? 'Preparing files…'
        : waiting.length
          ? 'File not verified'
          : 'Transfer to device';
}

/** Both firmware variants: switching is decided on this screen, not after the press. */
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

  // Firmware first: a token list the firmware cannot read is worse than the other way round.
  state.queue = wanted.map((kind) => {
    const status = rowState(kind).status;
    return {
      kind,
      key: keyFor(kind),
      label: itemLabel(kind),
      bytes: state.prepared[keyFor(kind)]?.bytes ?? 0,
      done: false,
      // No dialog: the row's own status word is the warning that pressing the button agrees to.
      allowVariantSwitch: status === 'Variant switch',
      allowDowngrade: status === 'Downgrade',
    };
  });
  state.step = 0;
  state.transfer = null;
  state.transferError = null;
  show('screen-transfer');
  await runQueue();
}

/** What the queue was going to put on the Shell, in the words its configuration uses. */
function intendedVersions() {
  const want = { firmware: null, database: null };
  for (const item of state.queue) {
    const version = state.prepared[item.key]?.version;
    if (version !== undefined && version !== null) {
      want[item.kind] = String(version);
    }
  }
  return want;
}

const onDeviceVersions = (config) =>
  config
    ? {
        firmware: config.fwVersion ?? null,
        database: config.dbVersion === undefined ? null : String(config.dbVersion),
      }
    : null;

/** Versions the device still owes; outlives the app, so a half-written Shell is known later. */
function outstandingUpdate() {
  const record = state.record;
  if (!record || !state.config) return null;
  const serial = state.config.serialNumber ?? state.device?.serialNumber ?? '';
  if (record.serial && serial && record.serial !== serial) return null;
  const now = onDeviceVersions(state.config);
  const behind = [];
  if (record.firmware && record.firmware !== now.firmware) behind.push(`firmware ${now.firmware}`);
  if (record.database && record.database !== now.database) behind.push(`database ${now.database}`);
  return behind.length ? behind : null;
}

async function saveRecord() {
  const intended = intendedVersions();
  state.record = {
    serial: state.config?.serialNumber ?? state.device?.serialNumber ?? '',
    firmware: intended.firmware,
    database: intended.database,
  };
  await invoke('save_update_record', { record: state.record }).catch(() => {
    // A read-only config directory costs the memory, not the update.
  });
}

/** Waits for the device to come back: a flashed firmware restarts it. */
async function verifyOnDevice() {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    try {
      const found = await invoke('list_shells');
      if (found.length) {
        state.device = found[0];
        return await invoke('shell_app_configuration', { path: found[0].path });
      }
    } catch {
      // Still restarting; the next attempt asks again.
    }
    await sleep(1000);
  }
  return null;
}

async function runQueue() {
  state.busy = true;
  // Written before the first byte: if this window dies mid-write, the Shell is still owed these
  // versions when it is next plugged in.
  await saveRecord();
  for (const [index, item] of state.queue.entries()) {
    state.step = index + 1;
    state.transfer = { done: 0, total: item.bytes };
    renderTransfer();
    try {
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

  // Neither outcome is taken on trust: what the Shell answers builds both the screen and the record.
  const after = await verifyOnDevice();
  if (after) state.config = after;
  const now = onDeviceVersions(after);
  const intended = intendedVersions();
  const behind = Object.entries(intended).filter(
    ([kind, version]) => version && now !== null && now[kind] !== version,
  );

  if (state.transferError) {
    await saveRecord();
    renderTransfer();
    return;
  }
  if (now && behind.length) {
    state.transferError = `The Shell came back with ${behind
      .map(([kind, version]) => `${kind} ${now[kind]} instead of ${version}`)
      .join(' and ')}.`;
    await saveRecord();
    renderTransfer();
    return;
  }

  el('status-device').textContent =
    `Keycard Shell connected · SN ${state.device?.serialNumber ?? ''}`;
  if (!now) {
    // Kept so the next start finishes the check this one could not.
    el('done-summary').textContent =
      'The transfer finished, but the Shell did not report its version.';
    el('done-verify').textContent = 'Unplug the Shell and plug it back in to see what it runs now.';
    show('screen-done');
    return;
  }
  state.record = null;
  await invoke('clear_update_record').catch(() => {});
  el('done-summary').textContent =
    `Your Shell now runs firmware ${now.firmware} and database ${now.database}.`;
  el('done-verify').textContent = 'Checked against the Shell after it restarted.';
  show('screen-done');
}

function renderTransfer() {
  const current = state.transfer ?? { done: 0, total: 0 };
  const failed = Boolean(state.transferError);
  const cancelled = state.transferError === CANCELLED;
  const percent = current.total ? Math.floor((current.done / current.total) * 100) : 0;

  // One sentence and no status words: `explain` on the Rust side chose the wording.
  el('transfer-title').textContent = failed
    ? cancelled
      ? 'Transfer cancelled'
      : 'Update failed'
    : 'Transferring';
  el('transfer-step').textContent = `${state.step} / ${Math.max(state.queue.length, 1)}`;
  el('transfer-percent').textContent = `${percent}%`;
  el('transfer-bytes').textContent = `${group(current.done)} / ${group(current.total)} bytes`;
  el('transfer-bar').style.width = `${percent}%`;
  el('transfer-caption').textContent = failed
    ? state.transferError
    : 'Please do not disconnect the device.';
  el('transfer-checkline').hidden = failed;
  el('btn-cancel').hidden = failed;
  el('btn-transfer-back').hidden = !failed;

  el('transfer-items').innerHTML = state.queue
    .map((item, index) => {
      const active = !failed && index + 1 === state.step;
      const text = item.done
        ? 'Done'
        : failed && index + 1 === state.step
          ? 'Failed'
          : failed
            ? 'Not started'
            : 'Queued';
      const dim = text === 'Queued' || text === 'Not started';
      const stateClass = item.done
        ? 'ksh__item-state ksh__item-state--latest'
        : dim
          ? 'ksh__item-state ksh__item-state--waiting'
          : 'ksh__item-state ksh__item-state--failed';
      // The turning squares, not the word 'Transferring': the row already says which image it is.
      const marker = active
        ? '<i class="ksh__loader"><i></i><i></i></i>'
        : `<i class="${stateClass}">${text}</i>`;
      return `<div class="ksh__item${dim ? ' ksh__item--waiting' : ''}">${item.label}${marker}</div>`;
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
    state.config = await invoke('shell_app_configuration', {
      path: device.path,
    });
    const variant = state.config.fwVariant ?? null;
    el('device-line').textContent =
      `SN ${device.serialNumber ?? state.config.serialNumber} · on device: firmware ${state.config.fwVersion}` +
      `${variant === null ? '' : `-${VARIANT[variant] ?? variant}`}, database ${state.config.dbVersion}`;
    el('device-line').title = `attestation key ${state.config.publicKey}`;
    // Default to the variant already on the device, so the plain action is not a variant switch.
    if (variant !== null) state.variant.firmware = variant;
    // A remembered update the device now satisfies is finished, whoever wrote the bytes.
    if (state.record && !outstandingUpdate()) {
      state.record = null;
      invoke('clear_update_record').catch(() => {});
    }
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
    // Plugging in only makes the card pressable; pressing it is what opens the flow.
    show(state.screen);
    await readConfiguration(device);
  } catch (error) {
    el('device-line').textContent = String(error);
  }
}

function showWaiting(message) {
  state.device = null;
  state.config = null;
  // Only the screens that ask something of the device are left behind; verify needs no cable, so a
  // poll that finds nothing must not close it.
  if (state.screen === 'screen-ready' || state.screen === 'screen-transfer') show('screen-home');
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

/* Verify: a challenge this computer wrote, and the Shell's signed answer to it. */

const HELP_VERIFY_URL = 'https://keycard.tech/help/verify-keycard-shell-authenticity';

const verify = {
  // The list the screen describes, straight from `units_status`.
  status: null,
  mode: 'online',
  net: 'unknown',
  // Why the camera or the list is not available yet, said in the note box rather than a dialog.
  problem: null,
  stream: null,
  timer: null,
  verdict: null,
  drawn: false,
  fetching: false,
  noPicture: 0,
  noPictureSent: false,
  frames: 0,
  slowSaid: false,
  window: { at: 0, frames: 0 },
  openedAt: 0,
  stopWatch: null,
  generation: 0,
  problemDetail: null,
};

/** 20261112 as the site writes it: the date the list was built is how a list is named. */
const listDate = (build) => `${build}`.replace(/^(\d{4})(\d{2})(\d{2})$/, '$1-$2-$3');

const dayDate = (days) => new Date(days * 86_400_000).toISOString().slice(0, 10);

function stopCamera() {
  clearTimeout(verify.noPicture);
  verify.stopWatch?.();
  verify.stopWatch = null;
  verify.frames = 0;
  verify.slowSaid = false;
  if (verify.timer) clearInterval(verify.timer);
  verify.timer = null;
  for (const track of verify.stream?.getTracks() ?? []) track.stop();
  verify.stream = null;
  el('scan-video').srcObject = null;
}

async function openVerify() {
  show('screen-verify');
  verify.problem = null;
  verify.drawn = false;
  // The challenge is drawn before anything asks the network for a list: it does not depend on which
  // list the answer is checked against, and a white tile sitting empty while a download is decided
  // looks exactly like the failure it is not.
  verify.status = await invoke('units_status').catch(() => null);
  renderVerify();
  await ensureChallenge();
  await fetchList();
  renderVerify();
}

/**
 * One challenge per visit: drawing a second would invalidate what the Shell is already signing. It
 * is drawn even with no list, as in mockup 14 - the code is then real but means nothing, and the
 * note box says so while the camera stays locked.
 */
async function ensureChallenge() {
  if (verify.drawn) return;
  await drawChallenge();
  verify.drawn = true;
}

/**
 * One challenge per visit: drawing a second would invalidate what the Shell is already signing.
 */
async function ensureChallenge() {
  if (verify.drawn) return;
  try {
    await drawChallenge();
    verify.drawn = true;
  } catch (error) {
    verify.problem = String(error);
  }
}

/** The fetched list, asked for after the screen is up; a cached one stays useful when this fails. */
async function fetchList() {
  if (verify.mode !== 'online') return;
  verify.fetching = true;
  renderVerify();
  try {
    verify.status = await invoke('units_fetch');
    verify.net = 'online';
    verify.problem = null;
  } catch (error) {
    verify.net = 'offline';
    verify.status = (await invoke('units_status').catch(() => null)) ?? verify.status;
    if (!verify.status) verify.problem = String(error);
  }
  verify.fetching = false;
}

function useOwnList() {
  invoke('plugin:dialog|open', {
    options: {
      multiple: false,
      title: 'Choose a units list',
      filters: [{ name: 'Produced units list', extensions: ['bin'] }],
    },
  })
    .then((path) => {
      // Picking the file is the whole request: it is read and verified here, not behind a button.
      if (!path) return null;
      return invoke('units_use_file', { path });
    })
    .then((status) => {
      if (!status) return;
      verify.status = status;
      verify.mode = 'file';
      verify.problem = null;
      return ensureChallenge();
    })
    .catch((error) => {
      verify.problem = String(error);
    })
    .then(renderVerify);
}

async function drawChallenge() {
  const { svg } = await invoke('units_challenge');
  // The SVG is written here rather than fetched, so painting it needs no remote origin.
  el('verify-qr').innerHTML = svg;
}

function renderVerify() {
  const loaded = Boolean(verify.status);
  const { origin, fetchedAt, file } = verify.status ?? {};

  el('verify-list-online').classList.toggle('ksh__seg--on', loaded && verify.mode === 'online');
  el('verify-list-file').classList.toggle('ksh__seg--on', loaded && verify.mode === 'file');
  // With nothing on the computer, "Fetched list" is not a choice to make: the button below fetches it.
  el('verify-list-online').disabled = !loaded;
  el('verify-net').textContent = loaded && verify.net !== 'offline' ? 'Online' : 'No network';

  const line = el('verify-list-line');
  if (verify.fetching) {
    line.textContent = 'Fetching the produced units list…';
  } else if (loaded) {
    const verified = document.createElement('span');
    verified.className = 'ksh__sha--ok';
    verified.textContent = 'signature verified';
    // A file name is text, however the list got here.
    line.replaceChildren(
      verified,
      ` · ${origin === 'file' ? file : `fetched ${dayDate(fetchedAt)}${origin === 'cached' ? ', no network since' : ''}`}`,
    );
  } else {
    const none = document.createElement('span');
    none.className = 'ksh__sha-state--bad';
    none.textContent = 'No list on this computer';
    line.replaceChildren(none, ' · load one from file, or fetch it once you are online');
  }

  // Only the reason something failed: the line above already says what to do about it.
  el('verify-note').hidden = !verify.problem;
  el('verify-note-text').textContent = String(verify.problem ?? '');

  const button = el('btn-camera');
  button.textContent = loaded
    ? 'Enable camera'
    : verify.fetching
      ? 'Fetching units list…'
      : 'Load units list';
  button.disabled = verify.fetching;
  // Solid orange means "go", and this screen can only say that with a list checked against.
  button.classList.toggle('ksh__btn--primary', loaded);
}

// A "5M" webcam asked for nothing gets its biggest mode: 2592x1944 over USB arrives seconds apart,
// which is a black preview and a code nobody can ever scan. 640x480 is more than the 420 px the
// decoder window uses, and it is a mode every webcam has. ideal rather than an exact 640 because a
// constraint one camera cannot satisfy means no camera at all.
const CAMERA_MODE = { width: { ideal: 640 }, height: { ideal: 480 }, frameRate: { ideal: 30 } };

async function cameraStart(deviceId) {
  const generation = ++verify.generation;
  try {
    verify.stream = await navigator.mediaDevices.getUserMedia({
      video: { ...CAMERA_MODE, ...(deviceId ? { deviceId: { exact: deviceId } } : {}) },
      audio: false,
    });
  } catch (error) {
    verify.problem = `This computer has no camera the app could open: ${error}`;
    renderVerify();
    return;
  }
  verify.problem = null;
  verify.problemDetail = null;
  renderScan();
  show('screen-scan');
  // Nothing between the answer and the screen. play() takes a second or two, and on a WebKitGTK
  // without the gstreamer plugins it never settles at all; the frame is there to aim at either way,
  // and a picture that cannot be shown is said in the note box of a screen you can already read.
  const video = el('scan-video');
  video.srcObject = verify.stream;
  video.play().catch((error) => {
    // Pointing the element at another camera rejects the play() of the load it replaces with an
    // AbortError we caused ourselves; the watchdog is what says so if no frame ever arrives.
    if (generation !== verify.generation || error?.name === 'AbortError') return;
    verify.problem = `The camera is open but its picture cannot be shown here: ${error}`;
    renderScan();
  });
  await listCameras();
  startDecoding();
}

async function listCameras() {
  const devices = (await navigator.mediaDevices.enumerateDevices().catch(() => [])).filter(
    (device) => device.kind === 'videoinput',
  );
  const track = verify.stream?.getVideoTracks()[0];
  // The same device id is spelled with braces on some platforms and without them on others.
  const current = String(track?.getSettings?.().deviceId ?? '')
    .replace(/^\{/, '')
    .replace(/\}$/, '');
  const select = el('scan-camera');
  select.replaceChildren(
    ...devices.map((device, index) => {
      const option = document.createElement('option');
      option.value = device.deviceId;
      option.selected = device.deviceId === current;
      option.textContent = device.label || `Camera ${index + 1}`;
      return option;
    }),
  );
  select.disabled = devices.length < 2;
}

function startDecoding() {
  const video = el('scan-video');
  const canvas = document.createElement('canvas');
  // 420 px is enough for a code on a device screen, and it is what every frame costs.
  const size = 420;
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext('2d', { willReadFrequently: true });

  clearTimeout(verify.noPicture);
  if (verify.timer) clearInterval(verify.timer);
  // A webcam that opens and never delivers a frame is indistinguishable from a covered lens, so say
  // it out loud rather than keep a black frame and a person holding a Shell up to it.
  verify.noPictureSent = false;
  verify.noPicture = setTimeout(() => {
    if (video.readyState >= 2 || state.screen !== 'screen-scan') return;
    // A second ask: sitting at readyState 0 until played again is on the record for WebKitGTK.
    video.srcObject = verify.stream;
    video.play().catch(() => {});
    verify.noPicture = setTimeout(() => {
      if (video.readyState >= 2 || state.screen !== 'screen-scan') return;
      verify.noPictureSent = true;
      verify.problem = 'This camera opened but sent no picture. Try another one below.';
      verify.problemDetail = videoState(video);
      renderScan();
    }, 4000);
  }, 3000);
  verify.frames = 0;
  verify.openedAt = performance.now();
  verify.window = { at: performance.now(), frames: 0 };
  verify.stopWatch?.();
  verify.stopWatch = watchFrames(video, () => {
    verify.frames += 1;
  });
  verify.timer = setInterval(() => {
    if (state.screen !== 'screen-scan' || video.readyState < 2) return;
    if (verify.noPictureSent) {
      // A slow camera is not a dead one: the first frame takes back the accusation.
      verify.noPictureSent = false;
      verify.problem = null;
      verify.problemDetail = null;
      renderScan();
    }
    // Rate over a window, not the gap since the last frame: a camera at one picture a second has no
    // gap long enough to look broken and still cannot be scanned. Measured, so a camera that speeds up
    // takes the sentence back.
    if (performance.now() - verify.window.at > 2000) {
      const seconds = (performance.now() - verify.window.at) / 1000;
      const fps = (verify.frames - verify.window.frames) / seconds;
      verify.window = { at: performance.now(), frames: verify.frames };
      const slow = verify.frames >= 2 && fps < 3;
      if (slow !== verify.slowSaid) {
        verify.slowSaid = slow;
        verify.problem = slow
          ? `This camera manages about ${fps.toFixed(1)} pictures a second, too slow to read a code. Try another one below.`
          : null;
        verify.problemDetail = slow ? videoState(video) : null;
        renderScan();
      }
    }
    context.drawImage(video, 0, 0, size, size);
    const code = window.jsQR?.(context.getImageData(0, 0, size, size).data, size, size, {
      inversionAttempts: 'dontInvert',
    });
    if (code?.data) scanAnswer(code.data);
  }, 120);
}

async function scanAnswer(text) {
  stopCamera();
  let verdict;
  try {
    verdict = await invoke('units_check', { text });
  } catch (error) {
    verdict = { outcome: 'refused', reason: String(error) };
  }
  verify.verdict = verdict;
  renderVerdict();
  show('screen-verdict');
}

/** The scan screen speaks for itself: whatever goes wrong with the camera happens in front of it. */
function renderScan() {
  const problem = el('scan-problem');
  problem.hidden = !verify.problem;
  problem.textContent = String(verify.problem ?? '');
  const detail = el('scan-problem-detail');
  detail.hidden = !verify.problemDetail;
  detail.textContent = String(verify.problemDetail ?? '');
}

/** What the media element itself says, for a report from a machine this app has never seen: the
mode the camera agreed to is the difference between a dark room and a driver doing 0.3 fps. */
function videoState(video) {
  const track = verify.stream?.getVideoTracks()[0];
  return [
    `readyState ${video.readyState}`,
    video.videoWidth ? `${video.videoWidth}×${video.videoHeight}` : 'no size',
    `track ${track?.readyState ?? 'gone'}`,
    `${verify.frames} frame${verify.frames === 1 ? '' : 's'} in ${Math.round((performance.now() - verify.openedAt) / 1000)}s`,
    video.error ? String(video.error) : '',
  ]
    .filter(Boolean)
    .join(' · ');
}

/**
 * Counts the frames the element really receives, by whichever API this engine has. Without one of
 * these a camera that manages one picture every three seconds looks exactly like a working one.
 */
function watchFrames(video, onFrame) {
  if (video.requestVideoFrameCallback) {
    let handle = video.requestVideoFrameCallback(function loop() {
      onFrame();
      handle = video.requestVideoFrameCallback(loop);
    });
    return () => video.cancelVideoFrameCallback?.(handle);
  }
  const track = verify.stream?.getVideoTracks()[0];
  if (track?.addEventListener) {
    track.addEventListener('frame', onFrame);
    return () => track.removeEventListener('frame', onFrame);
  }
  return null;
}

function renderVerdict() {
  const { outcome, reason, uid } = verify.verdict ?? {
    outcome: 'refused',
    reason: 'nothing was scanned',
  };
  const updated = verify.status
    ? `Produced units list last updated ${listDate(verify.status.build)}`
    : '';

  const shown =
    outcome === 'authentic'
      ? {
          icon: 'img/success.svg',
          title: 'Your Shell is authentic',
          text: 'It signed the challenge correctly and it is on our produced units list.',
          sub: `${updated} · checked on this computer`,
          button: 'Done',
        }
      : outcome === 'notRegistered'
        ? {
            icon: 'img/untrustworthy.svg',
            title: 'Your Shell is not on our produced units list',
            text: `The response was signed correctly, but no entry matches ${uid}. We can go through the process together.`,
            sub: updated,
            button: 'Learn more',
          }
        : {
            icon: 'img/fail.svg',
            title: 'Shell was not verified',
            text: `Something went wrong: ${reason}. Go to Settings/Shell/Verify on your Shell and try again.`,
            sub: updated,
            button: 'Try again',
          };

  el('verdict-icon').src = shown.icon;
  el('verdict-title').textContent = shown.title;
  el('verdict-text').textContent = shown.text;
  el('verdict-sub').textContent = shown.sub;
  el('btn-verdict').textContent = shown.button;
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
el('card-verify').addEventListener('click', openVerify);
el('btn-verify-back').addEventListener('click', () => show('screen-home'));
el('verify-list-online').addEventListener('click', async () => {
  if (verify.mode === 'online' && verify.status) return;
  verify.mode = 'online';
  verify.problem = null;
  await fetchList();
  renderVerify();
});
el('verify-list-file').addEventListener('click', useOwnList);
el('btn-camera').addEventListener('click', async () => {
  if (verify.status) {
    cameraStart();
    return;
  }
  await fetchList();
  renderVerify();
});
el('btn-scan-back').addEventListener('click', () => {
  stopCamera();
  show('screen-verify');
});
el('scan-camera').addEventListener('change', async (event) => {
  const device = event.target.value;
  stopCamera();
  await cameraStart(device);
});
el('btn-verdict').addEventListener('click', () => {
  const outcome = verify.verdict?.outcome;
  if (outcome === 'notRegistered') {
    // Out to the desktop browser: the help page is where that process continues, not here.
    invoke('plugin:opener|open_url', { url: HELP_VERIFY_URL }).catch((error) =>
      console.warn(`could not open ${HELP_VERIFY_URL}: ${error}`),
    );
    return;
  }
  if (outcome === 'authentic') show('screen-home');
  else openVerify();
});
listen('shell://connected', () => refresh());
listen('shell://disconnected', () => {
  if (!el('screen-transfer').hidden && !state.transferError)
    // No device, no write: the percentage would sit there frozen.
    state.transferError = 'The device was disconnected.';
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
// Read before the device, so the first render can mention a previous run.
invoke('load_update_record')
  .then((record) => {
    state.record = record ?? null;
  })
  .catch(() => {})
  .then(refresh)
  .then(() => loadReleases())
  .then(preload);

// Polled as well as pushed: the watcher's first event can arrive before this document listens.
setInterval(async () => {
  if (state.scanning || state.busy) return;
  state.scanning = true;
  try {
    const found = await invoke('list_shells');
    const path = found.length ? found[0].path : null;
    const seen = state.device?.path ?? null;
    if (path !== seen || (path && !state.config)) await refresh();
  } catch {
    // Unreadable for a moment while USB settles; the next tick sees it.
  } finally {
    state.scanning = false;
  }
}, 1500);
