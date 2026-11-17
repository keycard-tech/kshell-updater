pub mod bytewords;
pub mod image;
pub mod net;
pub mod releases;
pub mod scan;
mod shell;
pub mod units;
pub mod ur;
pub mod verify;

use image::Kind;
use serde::{Deserialize, Serialize};
use shell::{AppConfiguration, DeviceInfo, Shell};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

struct SharedUsb(Mutex<hidapi::HidApi>);

impl SharedUsb {
    fn locked(&self) -> Result<std::sync::MutexGuard<'_, hidapi::HidApi>, String> {
        self.0.lock().map_err(|e| e.to_string())
    }
}

#[derive(Clone, Default)]
struct CancelFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

#[derive(Default)]
struct PreparedImages(Mutex<PreparedImageStore>);

#[derive(Default)]
struct PreparedImageStore {
    bytes_by_key: HashMap<String, Vec<u8>>,
    published_hash_by_release: HashMap<String, String>,
}

impl PreparedImages {
    fn store(&self) -> std::sync::MutexGuard<'_, PreparedImageStore> {
        self.0.lock().expect("prepared image store poisoned")
    }

    fn bytes_of(&self, key: &str) -> Option<Vec<u8>> {
        self.store().bytes_by_key.get(key).cloned()
    }

    fn remember(&self, key: &str, bytes: Vec<u8>) {
        self.store().bytes_by_key.insert(key.to_string(), bytes);
    }

    fn remember_hashes(&self, index: HashMap<String, String>) {
        self.store().published_hash_by_release.extend(index);
    }

    fn published_hashes(&self) -> HashMap<String, String> {
        self.store().published_hash_by_release.clone()
    }

    fn release_publishing(&self, hash: &str) -> Option<String> {
        let inner = self.store();
        inner
            .published_hash_by_release
            .iter()
            .find(|(_, published)| published.as_str() == hash)
            .map(|(release, _)| release.clone())
    }
}

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

pub struct Listing {
    pub files: Vec<releases::ReleaseFile>,
    pub index: HashMap<String, String>,
    pub failures: Vec<String>,
}

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

pub async fn catalogue_for(kind: Kind) -> Result<Listing, String> {
    let json = net::get_json(endpoint(kind))
        .await
        .map_err(|e| e.to_string())?;
    let mut files = releases::release_files(&json, kind.name());
    let mut index = HashMap::new();
    let mut failures = Vec::new();
    for hash_url in releases::sha256_urls(&json) {
        match net::get_text(&hash_url).await {
            Ok(text) => index.extend(releases::parse_sha256_index(&text)),
            Err(e) => failures.push(e.to_string()),
        }
    }
    releases::attach_sha256(&mut files, &index);
    let offered = releases::offerable(files);
    if offered.dropped_without_hash > 0 {
        eprintln!(
            "{} {kind} files offer nothing, for want of a published hash",
            offered.dropped_without_hash
        );
    }
    Ok(Listing {
        files: releases::latest(&offered.files),
        index,
        failures,
    })
}

pub async fn build_catalogue(kind: &str) -> Result<Catalogue, String> {
    let listing = catalogue_for(Kind::parse(kind)?).await?;
    Ok(catalogue(listing.files, listing.failures))
}

pub async fn download_verified(
    kind: Kind,
    url: &str,
    index: &HashMap<String, String>,
) -> Result<(image::ImageInfo, Vec<u8>), String> {
    let name = asset_name(url);
    let expected = releases::lookup_sha256(index, &name)
        .ok_or_else(|| format!("no published hash for {name}, so it cannot be verified"))?;
    let bytes = net::get_image(url).await.map_err(|e| e.to_string())?;
    let actual = kind.published_hash(&bytes).map_err(|e| e.to_string())?;
    if actual != expected {
        eprintln!("checksum mismatch for {name}: published {expected}, downloaded {actual}");
        return Err(String::from(
            "This file does not match its published checksum.",
        ));
    }
    let info = kind.inspect(&bytes)?;
    Ok((info, bytes))
}

fn asset_name(url: &str) -> String {
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
    let listing = catalogue_for(Kind::parse(&kind)?)
        .await
        .inspect_err(|error| eprintln!("{kind} release list refused: {error}"))?;
    app.state::<PreparedImages>().remember_hashes(listing.index);
    Ok(catalogue(listing.files, listing.failures))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FileRef {
    kind: String,
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub info: image::ImageInfo,
    pub key: String,
}

pub const EVENT_PRELOADED: &str = "shell://preloaded";

#[tauri::command]
async fn preload_images(app: AppHandle, files: Vec<FileRef>) -> Result<Vec<Prepared>, String> {
    let index = app.state::<PreparedImages>().published_hashes();
    let mut prepared = Vec::new();
    for file in files {
        let outcome = download_verified(Kind::parse(&file.kind)?, &file.url, &index).await;
        let (key, ok, note, entry) = match outcome {
            Ok((mut info, bytes)) => {
                info.matches_release = Some(asset_name(&file.url));
                app.state::<PreparedImages>().remember(&file.url, bytes);
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
pub const NEEDS_CONFIRM: &str = "confirm:";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FlashRequest {
    kind: String,
    key: String,
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
        .state::<PreparedImages>()
        .bytes_of(&request.key)
        .ok_or_else(|| {
            format!(
                "{} was never prepared, so there is nothing to transfer",
                kind.name()
            )
        })?;
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
    let usb = app.state::<SharedUsb>();
    let stopping = app.state::<CancelFlag>().0.clone();
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
        let arriving_database = info.version.parse::<u32>().unwrap_or(u32::MAX);
        if arriving_database < on_device.db_version {
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
        let mut api = usb.locked()?;
        let device = shell::only_device(&mut api).ok_or("no Keycard Shell connected")?;
        let opened = Shell::open(&api, &device.path).map_err(|e| e.to_string())?;
        opened
            .load(
                ins,
                &bytes,
                |done| {
                    if progress_worth_sending(done, total, reported) {
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
    report_back(&usb)
}

fn read_configuration(usb: &SharedUsb) -> Result<AppConfiguration, String> {
    let mut api = usb.locked()?;
    let device = shell::only_device(&mut api).ok_or("no Keycard Shell connected")?;
    Shell::open(&api, &device.path)
        .map_err(|e| e.to_string())?
        .get_app_configuration()
        .map_err(explain)
}

const RECONNECT_TRIES: usize = 40;
const RECONNECT_PAUSE: std::time::Duration = std::time::Duration::from_millis(500);

fn report_back(usb: &SharedUsb) -> Result<AppConfiguration, String> {
    let mut last = String::from("no answer");
    for _ in 0..RECONNECT_TRIES {
        match read_configuration(usb) {
            Ok(config) => return Ok(config),
            Err(error) => last = error,
        }
        std::thread::sleep(RECONNECT_PAUSE);
    }
    eprintln!("no configuration after the transfer: {last}");
    Err(String::from(
        "The Shell did not report its version after restarting.",
    ))
}

fn explain(error: shell::ShellError) -> String {
    use shell::ShellError;
    match error {
        ShellError::StatusWord(code) if code == shell::SW_USER_DECLINED => {
            String::from("The transfer was declined on the Shell's screen.")
        }
        ShellError::StatusWord(code) => {
            eprintln!("shell rejected the command: SW {code:#06x}");
            String::from("The Shell refused the update.")
        }
        ShellError::Cancelled => {
            String::from("The transfer was cancelled. Nothing more was written.")
        }
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

#[tauri::command]
fn cancel_flash(state: tauri::State<'_, CancelFlag>) {
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
    info.matches_release = app
        .state::<PreparedImages>()
        .release_publishing(&info.sha256);
    let key = format!("file://{message}");
    app.state::<PreparedImages>().remember(&key, bytes);
    Ok(Prepared { info, key })
}

#[tauri::command]
async fn list_shells(app: AppHandle) -> Result<Vec<DeviceInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let usb = app.state::<SharedUsb>();
        let mut api = usb.locked()?;
        Ok(shell::list_devices(&mut api))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn shell_app_configuration(app: AppHandle, path: String) -> Result<AppConfiguration, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let usb = app.state::<SharedUsb>();
        let api = usb.locked()?;
        let shell = Shell::open(&api, &path).map_err(|e| e.to_string())?;
        shell.get_app_configuration().map_err(explain)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub const EVENT_CONNECTED: &str = "shell://connected";
pub const EVENT_DISCONNECTED: &str = "shell://disconnected";

const PRESENCE_POLL: std::time::Duration = std::time::Duration::from_millis(500);
const PROGRESS_STEPS: usize = 100;

fn progress_worth_sending(done: usize, total: usize, reported: usize) -> bool {
    done == total || done - reported >= total / PROGRESS_STEPS
}

fn watch_presence(app: AppHandle) {
    std::thread::spawn(move || {
        let mut present: Vec<DeviceInfo> = Vec::new();
        loop {
            let now = {
                let usb = app.state::<SharedUsb>();
                let Ok(mut api) = usb.locked() else {
                    return;
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
            std::thread::sleep(PRESENCE_POLL);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init()
        .ok();
    log::info!(
        "[app] kshell-updater {} on {} {}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(SharedUsb(Mutex::new(hidapi::HidApi::new()?)));
            app.manage(CancelFlag::default());
            app.manage(PreparedImages::default());
            app.manage(verify::Units::default());
            app.manage(scan::Scanner::default());
            if let Ok(dir) = app.path().app_config_dir() {
                net::keep_reads_in(dir.join("reads"));
            }
            watch_presence(app.handle().clone());
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
            verify::units_check,
            scan::scan_start,
            scan::scan_frame,
            scan::scan_stop
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keycard Shell Updater");
}
