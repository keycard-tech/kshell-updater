pub mod bytewords;
pub mod image;
pub mod net;
pub mod releases;
mod shell;
pub mod units;
pub mod ur;
pub mod verify;

use serde::{Deserialize, Serialize};
use shell::{AppConfiguration, DeviceInfo, Shell};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::webview::{PermissionKind, PermissionResponse, WebviewWindowBuilder};
use tauri::{AppHandle, Emitter, Manager};

/// `HidApi` is Send but not Sync, and opening the same device twice confuses the platform
/// drivers, so exactly one instance is shared and every access takes the lock.
struct Usb(Mutex<hidapi::HidApi>);

/// Set by `cancel_flash` and read between chunks. An `Arc` because the transfer loop holds one while
/// the command that sets it has returned: Cancel has to reach a write that is already running.
#[derive(Clone, Default)]
struct Cancel(std::sync::Arc<std::sync::atomic::AtomicBool>);

/// Images that have passed their checks and are waiting to be flashed, plus the published hashes
/// read alongside the release listing.
///
/// Bytes stay on this side of the bridge. They arrive from a download or a file dialog and the device
/// needs them, so a round trip through the renderer would only offer a chance to change what gets
/// flashed. The hash index lives here for the same reason: a download is checked against what this
/// process read from GitHub, not against a value handed back by the view.
#[derive(Default)]
struct Loaded(Mutex<LoadedState>);

#[derive(Default)]
struct LoadedState {
    /// Verified bytes by key: the release URL for an online file, `file://<path>` for one picked from
    /// disk. Keyed per file rather than per kind because both firmware variants are held at once and
    /// the image that gets flashed has to be the one the row is showing.
    images: HashMap<String, Vec<u8>>,
    hashes: HashMap<String, String>,
}

impl Loaded {
    /// A copy, not a take: a cancelled or declined transfer must not throw away bytes the person
    /// would otherwise have to fetch again to retry.
    fn image_bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.0
            .lock()
            .expect("loaded state poisoned")
            .images
            .get(key)
            .cloned()
    }

    fn put_image(&self, key: &str, bytes: Vec<u8>) {
        self.0
            .lock()
            .expect("loaded state poisoned")
            .images
            .insert(key.to_string(), bytes);
    }

    fn merge_hashes(&self, index: HashMap<String, String>) {
        self.0
            .lock()
            .expect("loaded state poisoned")
            .hashes
            .extend(index);
    }

    fn hashes(&self) -> HashMap<String, String> {
        self.0.lock().expect("loaded state poisoned").hashes.clone()
    }

    /// The release a file from disk came from, when its hash is one this app published. Names in the
    /// hash lists differ from asset names, so this reads as the list wrote it.
    fn release_for_hash(&self, hash: &str) -> Option<String> {
        let inner = self.0.lock().expect("loaded state poisoned");
        inner
            .hashes
            .iter()
            .find(|(_, published)| published.as_str() == hash)
            .map(|(name, _)| name.clone())
    }
}

/// The picker's listing. `verified` is false when a release's hash list could not be read, which is
/// said once instead of leaving every row to look independently broken.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub files: Vec<releases::ReleaseFile>,
    /// Files the release carries but the app will not offer, for want of a published hash. A count,
    /// so a short list is visibly a choice rather than a truncated one.
    pub unofferable: usize,
    pub verified: bool,
    pub note: Option<String>,
}

fn catalogue_shape(
    files: Vec<releases::ReleaseFile>,
    unofferable: usize,
    failures: Vec<String>,
) -> Catalogue {
    Catalogue {
        files,
        unofferable,
        verified: failures.is_empty(),
        note: (!failures.is_empty()).then(|| failures.join("; ")),
    }
}

/// The listing the command returns, exposed so `examples/releases.rs --fixtures` writes the JSON the
/// renderer actually receives rather than a hand-written approximation of it.
pub async fn build_catalogue(kind: &str) -> Result<Catalogue, String> {
    let (files, unofferable, _index, failures) = catalogue_for(kind).await?;
    Ok(catalogue_shape(files, unofferable, failures))
}

// per_page because the API's default of 30 silently truncates: keycard-shell has 32 releases, so a
// default request would drop the oldest without saying so.
fn endpoint(kind: &str) -> Result<&'static str, String> {
    match kind {
        "firmware" => {
            Ok("https://api.github.com/repos/keycard-tech/keycard-shell/releases?per_page=100")
        }
        "database" => {
            Ok("https://api.github.com/repos/keycard-tech/keycard-eth-db/releases?per_page=100")
        }
        other => Err(format!("unknown image kind {other}")),
    }
}

/// Listing for one kind, with the hash published for each file. Public so `examples/releases.rs` can
/// run the same code as the command without a window. Returns the offerable files, how many were
/// dropped for want of a published hash, the hash index, and any hash list that could not be read.
pub async fn catalogue_for(
    kind: &str,
) -> Result<
    (
        Vec<releases::ReleaseFile>,
        usize,
        HashMap<String, String>,
        Vec<String>,
    ),
    String,
> {
    let json = net::get_json(endpoint(kind)?)
        .await
        .map_err(|e| e.to_string())?;
    let mut files = releases::release_files(&json, kind);
    let mut index = HashMap::new();
    let mut failures = Vec::new();
    for hash_url in releases::sha256_urls(&json) {
        match net::get_text(&hash_url).await {
            Ok(text) => index.extend(releases::parse_sha256_index(&text)),
            // One unreachable list should not blank the picker: the files it covers arrive without a
            // hash and are then refused at download time.
            Err(e) => failures.push(e.to_string()),
        }
    }
    releases::attach_sha256(&mut files, &index);
    let (all, unofferable) = releases::offerable(files);
    // Older builds are dropped here rather than hidden in the view, so nothing downstream can offer
    // one by accident. The full hash index is kept, because it is what identifies a file from disk.
    let files = releases::latest(&all);
    Ok((files, unofferable, index, failures))
}

/// Downloads an image and confirms it is the published bytes. The rule is in [`image`]: neither image
/// hashes as a plain `sha256sum`, so a mismatch means the file was replaced, truncated, or is not what
/// the release says it is. A file no list covers is refused rather than flashed unchecked.
pub async fn download_verified(
    kind: &str,
    url: &str,
    index: &HashMap<String, String>,
) -> Result<(image::ImageInfo, Vec<u8>), String> {
    let name = filename(url);
    let expected = releases::lookup_sha256(index, &name)
        .ok_or_else(|| format!("no published hash for {name}, so it cannot be verified"))?;
    let bytes = net::get_image(url).await.map_err(|e| e.to_string())?;
    let actual = match kind {
        "firmware" => image::firmware_hash(&bytes),
        "database" => image::database_hash(&bytes),
        other => return Err(format!("unknown image kind {other}")),
    }
    .map_err(|e| e.to_string())?;
    if actual != expected {
        // Two 64-character hashes are what a developer would want and what a person updating a
        // device cannot use, so they go to the log and the row gets one sentence.
        eprintln!("checksum mismatch for {name}: published {expected}, downloaded {actual}");
        return Err(String::from(
            "This file does not match its published checksum.",
        ));
    }
    let info = inspect(kind, &bytes)?;
    Ok((info, bytes))
}

/// Reads an image chosen with the file dialog. Structural checks only — a file from disk has nothing
/// to match a published hash against unless it happens to be a release asset.
fn inspect(kind: &str, bytes: &[u8]) -> Result<image::ImageInfo, String> {
    match kind {
        "firmware" => image::inspect_firmware(bytes),
        "database" => image::inspect_database(bytes),
        other => return Err(format!("unknown image kind {other}")),
    }
    .map_err(|e| e.to_string())
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
    let (files, unofferable, index, failures) = catalogue_for(&kind).await?;
    app.state::<Loaded>().merge_hashes(index);
    Ok(catalogue_shape(files, unofferable, failures))
}

/// One file the view wants prepared: the same URL the listing gave it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileRef {
    kind: String,
    url: String,
}

/// Verified bytes plus the key `flash_image` needs to reach them. The bytes deliberately do not
/// travel to the renderer; the key is only a name.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub info: image::ImageInfo,
    pub key: String,
}

pub const EVENT_PRELOADED: &str = "shell://preloaded";

/// Fetch and check every offered file at start-up, so pressing Transfer only ever writes and never
/// waits. Each file is verified against the published hash list before it is kept, and one failing
/// file does not stop the others: the row for it says so and From file still works.
#[tauri::command]
async fn preload_images(app: AppHandle, files: Vec<FileRef>) -> Result<Vec<Prepared>, String> {
    let index = app.state::<Loaded>().hashes();
    let mut prepared = Vec::new();
    for file in files {
        let outcome = download_verified(&file.kind, &file.url, &index).await;
        match outcome {
            Ok((mut info, bytes)) => {
                info.matches_release = Some(filename(&file.url));
                app.state::<Loaded>().put_image(&file.url, bytes);
                let _ = app.emit(
                    EVENT_PRELOADED,
                    Preloaded {
                        key: file.url.clone(),
                        ok: true,
                        note: None,
                    },
                );
                prepared.push(Prepared {
                    info,
                    key: file.url,
                });
            }
            Err(error) => {
                let _ = app.emit(
                    EVENT_PRELOADED,
                    Preloaded {
                        key: file.url.clone(),
                        ok: false,
                        note: Some(error),
                    },
                );
            }
        }
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
    let kind = request.kind.clone();
    let bytes = app
        .state::<Loaded>()
        .image_bytes(&request.key)
        .ok_or_else(|| format!("{kind} was never prepared, so there is nothing to flash"))?;
    // Re-read from the bytes rather than trusting what the view believes it picked.
    let info = inspect(&kind, &bytes)?;
    tauri::async_runtime::spawn_blocking(move || flash(app, kind, bytes, info, request))
        .await
        .map_err(|e| e.to_string())?
}

fn flash(
    app: AppHandle,
    kind: String,
    bytes: Vec<u8>,
    info: image::ImageInfo,
    request: FlashRequest,
) -> Result<AppConfiguration, String> {
    let usb = app.state::<Usb>();
    let stopping = app.state::<Cancel>().0.clone();
    stopping.store(false, std::sync::atomic::Ordering::Relaxed);
    let on_device = read_configuration(&usb)?;

    if kind == "firmware" {
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
    let ins = if kind == "firmware" {
        shell::INS_LOAD_FIRMWARE
    } else {
        shell::INS_LOAD_DATABASE
    };
    let progress_kind = kind.clone();
    let mut reported = 0usize;
    {
        let mut api = usb.0.lock().map_err(|e| e.to_string())?;
        let device = shell::list_devices(&mut api)
            .into_iter()
            .next()
            .ok_or("no Keycard Shell connected")?;
        let opened = Shell::open(&api, &device.path).map_err(|e| e.to_string())?;
        opened
            .load(
                ins,
                &bytes,
                |done| {
                    // One event per percent. A firmware is 2595 chunks and the bridge has better
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
    let device = shell::list_devices(&mut api)
        .into_iter()
        .next()
        .ok_or("no Keycard Shell connected")?;
    Shell::open(&api, &device.path)
        .map_err(|e| e.to_string())?
        .get_app_configuration()
        .map_err(explain)
}

fn report_back(usb: &Usb) -> Result<AppConfiguration, String> {
    // The device is gone while it restarts, so a failure here is expected and only worth reporting
    // once the wait is over.
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
        // The literal is `SW_USER_DECLINED`, whose arm above already took the name.
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

/// Unconfirmed update: written before the first byte, so a half-written Shell is known later.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecord {
    pub serial: String,
    pub firmware: Option<String>,
    pub database: Option<String>,
}

fn record_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("update-state.json"))
}

#[tauri::command]
fn save_update_record(app: AppHandle, record: UpdateRecord) -> Result<(), String> {
    let path = record_path(&app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| format!("could not write {}: {e}", path.display()))
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
        Err(error) => Err(format!("could not read {}: {error}", path.display())),
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

/// Stop after the chunk in flight. Stopping mid-chunk would leave the image half written with no way
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
        .map_err(|e| format!("could not read {message}: {e}"))?;
    let mut info = inspect(&kind, &bytes)?;
    // A file from disk is recognised by its hash alone: the person may have renamed it, and the
    // download list is the only thing that knows what a published image hashes to.
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
        shell.get_app_configuration().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub const EVENT_CONNECTED: &str = "shell://connected";
pub const EVENT_DISCONNECTED: &str = "shell://disconnected";

/// Hotplug is polled rather than event driven: hidapi 2.x has no portable hotplug callback, and a
/// re-scan costs milliseconds against the half second a person waits to see "connected" after
/// plugging a cable in or powering the Shell on.
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
            // `"create": false` in tauri.conf.json, because a permission the webview is never asked
            // for is a permission the webview never grants: the camera handler can only be attached
            // while the window is being built. Everything else still comes from the config.
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
