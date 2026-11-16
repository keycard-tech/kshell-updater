pub mod bytewords;
pub mod image;
pub mod net;
pub mod releases;
mod shell;
pub mod units;
pub mod ur;
pub mod verify;

use image::Kind;
use serde::{Deserialize, Serialize};
use shell::{AppConfiguration, DeviceInfo, Shell};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::webview::{PermissionKind, PermissionResponse, WebviewWindowBuilder};
use tauri::{AppHandle, Emitter, Manager};

/// `HidApi` is Send but not Sync, and opening one device twice confuses the platform drivers: one
/// instance is shared and every access takes the lock.
struct Usb(Mutex<hidapi::HidApi>);

/// Set by `cancel_flash`, read between chunks. An `Arc` because the transfer loop holds one after
/// the command that sets it has returned.
#[derive(Clone, Default)]
struct Cancel(std::sync::Arc<std::sync::atomic::AtomicBool>);

/// Images that passed their checks and are waiting to be flashed, plus the published hashes read
/// alongside the release listing. Bytes stay on this side of the bridge: a round trip through the
/// renderer would only offer a chance to change what gets flashed.
#[derive(Default)]
struct Loaded(Mutex<LoadedState>);

#[derive(Default)]
struct LoadedState {
    /// By key: the release URL for a download, `file://<path>` for a file picked from disk. Per file
    /// rather than per kind, because both firmware variants are held at once.
    images: HashMap<String, Vec<u8>>,
    hashes: HashMap<String, String>,
}

impl Loaded {
    fn held(&self) -> std::sync::MutexGuard<'_, LoadedState> {
        self.0.lock().expect("loaded state poisoned")
    }

    /// A copy, not a take: a cancelled or declined transfer must not throw away bytes a retry needs.
    fn image_bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.held().images.get(key).cloned()
    }

    fn put_image(&self, key: &str, bytes: Vec<u8>) {
        self.held().images.insert(key.to_string(), bytes);
    }

    fn merge_hashes(&self, index: HashMap<String, String>) {
        self.held().hashes.extend(index);
    }

    fn hashes(&self) -> HashMap<String, String> {
        self.held().hashes.clone()
    }

    /// The release a file from disk came from, when its hash is a published one. Names in the hash
    /// lists differ from asset names, so this reads as the list wrote it.
    fn release_for_hash(&self, hash: &str) -> Option<String> {
        let inner = self.held();
        inner
            .hashes
            .iter()
            .find(|(_, published)| published.as_str() == hash)
            .map(|(name, _)| name.clone())
    }
}

/// The picker's listing. `verified` is false when a release's hash list could not be read, which is
/// said once rather than leaving every row to look independently broken.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub files: Vec<releases::ReleaseFile>,
    pub verified: bool,
    pub note: Option<String>,
}

fn catalogue(files: Vec<releases::ReleaseFile>, failures: Vec<String>) -> Catalogue {
    Catalogue {
        files,
        verified: failures.is_empty(),
        note: (!failures.is_empty()).then(|| failures.join("; ")),
    }
}

/// Files offered to the view, the hash index behind them, and any hash list that could not be read.
pub struct Listing {
    pub files: Vec<releases::ReleaseFile>,
    pub index: HashMap<String, String>,
    pub failures: Vec<String>,
}

/// The API's default of 30 would truncate: keycard-shell has 32 releases.
fn endpoint(kind: Kind) -> &'static str {
    match kind {
        Kind::Firmware => {
            "https://api.github.com/repos/keycard-tech/keycard-shell/releases?per_page=100"
        }
        Kind::Database => {
            "https://api.github.com/repos/keycard-tech/keycard-eth-db/releases?per_page=100"
        }
    }
}

/// Listing for one kind, with the published hash of each file. Public so `examples/releases.rs` runs
/// the same code as the command without a window.
pub async fn catalogue_for(kind: Kind) -> Result<Listing, String> {
    let json = net::get_json(endpoint(kind))
        .await
        .map_err(|e| e.to_string())?;
    let mut files = releases::release_files(&json, kind.name());
    let mut index = HashMap::new();
    let mut failures = Vec::new();
    for hash_url in releases::sha256_urls(&json) {
        match net::get_text(&hash_url).await {
            // One unreachable list should not blank the picker: its files arrive hashless and are
            // then refused at download time.
            Ok(text) => index.extend(releases::parse_sha256_index(&text)),
            Err(e) => failures.push(e.to_string()),
        }
    }
    releases::attach_sha256(&mut files, &index);
    // Older builds are dropped here rather than hidden in the view. The full hash index is kept: it
    // is what identifies a file picked from disk.
    let (all, dropped) = releases::offerable(files);
    if dropped > 0 {
        eprintln!(
            "{} {kind} files offer nothing, for want of a published hash",
            dropped
        );
    }
    Ok(Listing {
        files: releases::latest(&all),
        index,
        failures,
    })
}

/// The listing the command returns, exposed so `examples/releases.rs --fixtures` writes the JSON the
/// renderer actually receives rather than an approximation of it.
pub async fn build_catalogue(kind: &str) -> Result<Catalogue, String> {
    let listing = catalogue_for(Kind::parse(kind)?).await?;
    Ok(catalogue(listing.files, listing.failures))
}

/// Downloads an image and confirms it is the published bytes. The rule is in [`image`]: neither
/// image hashes as a plain `sha256sum`. A file no list covers is refused rather than flashed
/// unchecked.
pub async fn download_verified(
    kind: Kind,
    url: &str,
    index: &HashMap<String, String>,
) -> Result<(image::ImageInfo, Vec<u8>), String> {
    let name = filename(url);
    let expected = releases::lookup_sha256(index, &name)
        .ok_or_else(|| format!("no published hash for {name}, so it cannot be verified"))?;
    let bytes = net::get_image(url).await.map_err(|e| e.to_string())?;
    let actual = kind.hash(&bytes).map_err(|e| e.to_string())?;
    if actual != expected {
        // Two 64-character hashes are what a developer wants and what a person updating a device
        // cannot use, so they go to the log and the row gets one sentence.
        eprintln!("checksum mismatch for {name}: published {expected}, downloaded {actual}");
        return Err(String::from(
            "This file does not match its published checksum.",
        ));
    }
    let info = kind.inspect(&bytes)?;
    Ok((info, bytes))
}

fn filename(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_string()
}

#[tauri::command]
async fn list_releases(app: AppHandle, kind: String) -> Result<Catalogue, String> {
    let listing = catalogue_for(Kind::parse(&kind)?).await?;
    app.state::<Loaded>().merge_hashes(listing.index);
    Ok(catalogue(listing.files, listing.failures))
}

/// One file the view wants prepared: the same URL the listing gave it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileRef {
    kind: String,
    url: String,
}

/// Verified bytes plus the key `flash_image` needs to reach them. The bytes do not travel to the
/// renderer; the key is only a name.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub info: image::ImageInfo,
    pub key: String,
}

pub const EVENT_PRELOADED: &str = "shell://preloaded";

/// Fetch and check every offered file at start-up, so pressing Transfer only ever writes and never
/// waits. One failing file does not stop the others: its row says so and From file still works.
#[tauri::command]
async fn preload_images(app: AppHandle, files: Vec<FileRef>) -> Result<Vec<Prepared>, String> {
    let index = app.state::<Loaded>().hashes();
    let mut prepared = Vec::new();
    for file in files {
        let outcome = download_verified(Kind::parse(&file.kind)?, &file.url, &index).await;
        let (key, ok, note, entry) = match outcome {
            Ok((mut info, bytes)) => {
                info.matches_release = Some(filename(&file.url));
                app.state::<Loaded>().put_image(&file.url, bytes);
                let entry = Prepared {
                    info,
                    key: file.url.clone(),
                };
                (file.url, true, None, Some(entry))
            }
            Err(error) => (file.url, false, Some(error), None),
        };
        let _ = app.emit(EVENT_PRELOADED, Preloaded { key, ok, note });
        prepared.extend(entry);
    }
    Ok(prepared)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preloaded {
    pub key: String,
    pub ok: bool,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PickedFile {
    kind: String,
    path: String,
}

pub const EVENT_FLASH_PROGRESS: &str = "shell://flash-progress";
/// A destructive choice the view has to ask about first. A command's error is a String, so the
/// prefix is the contract: `confirm:variant-switch`, `confirm:downgrade`.
pub const NEEDS_CONFIRM: &str = "confirm:";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlashRequest {
    kind: String,
    /// Which prepared file to write, from `Prepared.key`.
    key: String,
    /// omni and btc-only hold different signing keys; switching wipes what the other build stored.
    #[serde(default)]
    allow_variant_switch: bool,
    #[serde(default)]
    allow_downgrade: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashProgress {
    pub kind: String,
    pub done: usize,
    pub total: usize,
}

#[tauri::command]
async fn flash_image(app: AppHandle, request: FlashRequest) -> Result<AppConfiguration, String> {
    let kind = Kind::parse(&request.kind)?;
    let bytes = app
        .state::<Loaded>()
        .image_bytes(&request.key)
        .ok_or_else(|| {
            format!(
                "{} was never prepared, so there is nothing to flash",
                kind.name()
            )
        })?;
    // Re-read from the bytes rather than trusting what the view believes it picked.
    let info = kind.inspect(&bytes)?;
    tauri::async_runtime::spawn_blocking(move || flash(app, kind, bytes, info, request))
        .await
        .map_err(|e| e.to_string())?
}

fn flash(
    app: AppHandle,
    kind: Kind,
    bytes: Vec<u8>,
    info: image::ImageInfo,
    request: FlashRequest,
) -> Result<AppConfiguration, String> {
    let usb = app.state::<Usb>();
    let stopping = app.state::<Cancel>().0.clone();
    stopping.store(false, std::sync::atomic::Ordering::Relaxed);
    let on_device = read_configuration(&usb)?;

    if kind == Kind::Firmware {
        if !request.allow_variant_switch
            && matches!((on_device.fw_variant, info.fw_variant), (Some(have), Some(want)) if have != want)
        {
            return Err(format!("{NEEDS_CONFIRM}variant-switch"));
        }
        if !request.allow_downgrade
            && image::firmware_version_older(&info.version, &on_device.fw_version)
        {
            return Err(format!("{NEEDS_CONFIRM}downgrade"));
        }
    } else if !request.allow_downgrade {
        // A database version is its date; an older list would drop tokens the device already has.
        let candidate = info.version.parse::<u32>().unwrap_or(u32::MAX);
        if candidate < on_device.db_version {
            return Err(format!("{NEEDS_CONFIRM}downgrade"));
        }
    }

    let total = bytes.len();
    let ins = match kind {
        Kind::Firmware => shell::INS_LOAD_FIRMWARE,
        Kind::Database => shell::INS_LOAD_DATABASE,
    };
    let progress_kind = kind.name().to_string();
    let mut reported = 0usize;
    {
        let mut api = usb.0.lock().map_err(|e| e.to_string())?;
        let device = shell::only_device(&mut api).ok_or("no Keycard Shell connected")?;
        let opened = Shell::open(&api, &device.path).map_err(|e| e.to_string())?;
        opened
            .load(
                ins,
                &bytes,
                |done| {
                    // One event per percent: a firmware is 2595 chunks and the bridge has better
                    // things to carry than all of them.
                    if done == total || done - reported >= total / 100 {
                        reported = done;
                        let _ = app.emit(
                            EVENT_FLASH_PROGRESS,
                            FlashProgress {
                                kind: progress_kind.clone(),
                                done,
                                total,
                            },
                        );
                    }
                },
                || stopping.load(std::sync::atomic::Ordering::Relaxed),
            )
            .map_err(explain)?;
    }
    // A flashed firmware reboots, so "did it work" is only answerable by connecting again.
    report_back(&usb)
}

fn read_configuration(usb: &Usb) -> Result<AppConfiguration, String> {
    let mut api = usb.0.lock().map_err(|e| e.to_string())?;
    let device = shell::only_device(&mut api).ok_or("no Keycard Shell connected")?;
    Shell::open(&api, &device.path)
        .map_err(|e| e.to_string())?
        .get_app_configuration()
        .map_err(explain)
}

fn report_back(usb: &Usb) -> Result<AppConfiguration, String> {
    // The device is gone while it restarts, so a failure here is expected until the wait is over.
    let mut last = String::from("no answer");
    for _ in 0..40 {
        match read_configuration(usb) {
            Ok(config) => return Ok(config),
            Err(error) => last = error,
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    eprintln!("no configuration after the flash: {last}");
    Err(String::from(
        "The Shell did not report its version after restarting.",
    ))
}

/// What the person sees: status words and OS numbers go to the log, not to the screen.
fn explain(error: shell::ShellError) -> String {
    use shell::ShellError;
    match error {
        ShellError::StatusWord(code) if code == shell::SW_USER_DECLINED => {
            String::from("The flash was declined on the Shell's screen.")
        }
        ShellError::StatusWord(code) => {
            eprintln!("shell rejected the command: SW {code:#06x}");
            String::from("The Shell refused the update.")
        }
        ShellError::Cancelled => String::from(CANCELLED),
        ShellError::Timeout => String::from("The Shell did not answer in time."),
        ShellError::Framing(why) => {
            eprintln!("framing error: {why}");
            String::from("The Shell sent an answer that could not be read.")
        }
        other => {
            eprintln!("shell error: {other}");
            String::from("Could not reach the Shell over USB.")
        }
    }
}

/// The one error string the view matches on instead of showing.
const CANCELLED: &str = "cancelled";

/// An update that has not finished: written before the first byte, so a half-written Shell is known
/// later.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecord {
    pub serial: String,
    pub firmware: Option<String>,
    pub database: Option<String>,
}

fn record_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("update-state.json"))
}

fn io_error(action: &str, path: &std::path::Path, error: &std::io::Error) -> String {
    format!("could not {action} {}: {error}", path.display())
}

#[tauri::command]
fn save_update_record(app: AppHandle, record: UpdateRecord) -> Result<(), String> {
    let path = record_path(&app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io_error("create", dir, &e))?;
    }
    let body = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| io_error("write", &path, &e))
}

#[tauri::command]
fn load_update_record(app: AppHandle) -> Result<Option<UpdateRecord>, String> {
    let path = record_path(&app)?;
    match std::fs::read(&path) {
        Ok(body) => serde_json::from_slice(&body)
            .map(Some)
            // A record this process cannot read would claim an update history it cannot substantiate.
            .map_err(|e| format!("could not read {}: {e}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error("read", &path, &error)),
    }
}

#[tauri::command]
fn clear_update_record(app: AppHandle) -> Result<(), String> {
    match std::fs::remove_file(record_path(&app)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// Stop after the chunk in flight: stopping mid-chunk would leave the image half written with no way
/// to tell the Shell so, and it answers the next command as a broken firmware.
#[tauri::command]
fn cancel_flash(state: tauri::State<'_, Cancel>) {
    state.0.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[tauri::command]
async fn pick_image_file(app: AppHandle, file: PickedFile) -> Result<Prepared, String> {
    let PickedFile { kind, path } = file;
    let message = path.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| io_error("read", std::path::Path::new(&message), &e))?;
    let mut info = Kind::parse(&kind)?.inspect(&bytes)?;
    // A file from disk is recognised by its hash alone: it may have been renamed, and the download
    // list is the only thing that knows what a published image hashes to.
    info.matches_release = app.state::<Loaded>().release_for_hash(&info.sha256);
    let key = format!("file://{message}");
    app.state::<Loaded>().put_image(&key, bytes);
    Ok(Prepared { info, key })
}

#[tauri::command]
async fn list_shells(app: AppHandle) -> Result<Vec<DeviceInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let usb = app.state::<Usb>();
        let mut api = usb.0.lock().map_err(|e| e.to_string())?;
        Ok(shell::list_devices(&mut api))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn shell_app_configuration(app: AppHandle, path: String) -> Result<AppConfiguration, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let usb = app.state::<Usb>();
        let api = usb.0.lock().map_err(|e| e.to_string())?;
        let shell = Shell::open(&api, &path).map_err(|e| e.to_string())?;
        shell.get_app_configuration().map_err(explain)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub const EVENT_CONNECTED: &str = "shell://connected";
pub const EVENT_DISCONNECTED: &str = "shell://disconnected";

/// Hotplug is polled: hidapi 2.x has no portable hotplug callback, and a re-scan costs milliseconds
/// against the half second a person waits to see "connected".
fn watch_presence(app: AppHandle) {
    std::thread::spawn(move || {
        let mut present: Vec<DeviceInfo> = Vec::new();
        loop {
            let now = {
                let usb = app.state::<Usb>();
                let mut api = match usb.0.lock() {
                    Ok(api) => api,
                    Err(_) => return,
                };
                shell::list_devices(&mut api)
            };
            if now != present {
                if now.is_empty() {
                    let _ = app.emit(EVENT_DISCONNECTED, ());
                } else {
                    let _ = app.emit(EVENT_CONNECTED, &now);
                }
                present = now;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // The renderer calls `plugin:dialog|open` through `invoke` rather than the plugin's JS API,
        // which would mean vendoring its api-iife.js into the frontend.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(Usb(Mutex::new(hidapi::HidApi::new()?)));
            app.manage(Cancel::default());
            app.manage(Loaded::default());
            app.manage(verify::Units::default());
            watch_presence(app.handle().clone());
            // `"create": false` in tauri.conf.json: the camera handler can only be attached while the
            // window is being built. Everything else still comes from the config.
            let config = app.config().clone();
            let window = config
                .app
                .windows
                .first()
                .ok_or("tauri.conf.json declares no window")?;
            WebviewWindowBuilder::from_config(&*app, window)?
                .on_permission_request(|_, kind| match kind {
                    PermissionKind::Camera => PermissionResponse::Allow,
                    _ => PermissionResponse::Deny,
                })
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_shells,
            shell_app_configuration,
            list_releases,
            preload_images,
            pick_image_file,
            flash_image,
            cancel_flash,
            save_update_record,
            load_update_record,
            clear_update_record,
            verify::units_status,
            verify::units_fetch,
            verify::units_use_file,
            verify::units_challenge,
            verify::units_check
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keycard Shell Updater");
}
