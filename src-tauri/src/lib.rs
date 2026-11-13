pub mod image;
pub mod net;
pub mod releases;
mod shell;

use serde::{Deserialize, Serialize};
use shell::{AppConfiguration, DeviceInfo, Shell};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// `HidApi` is Send but not Sync, and opening the same device twice confuses the platform
/// drivers, so exactly one instance is shared and every access takes the lock.
struct Usb(Mutex<hidapi::HidApi>);

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
    images: HashMap<String, Vec<u8>>,
    hashes: HashMap<String, String>,
}

impl Loaded {
    fn put_image(&self, kind: &str, bytes: Vec<u8>) {
        self.0
            .lock()
            .expect("loaded state poisoned")
            .images
            .insert(kind.to_string(), bytes);
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
}

/// The picker's listing. `verified` is false when a release's hash list could not be read, which is
/// said once instead of leaving every row to look independently broken.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub files: Vec<releases::ReleaseFile>,
    /// Files the release carries but the app will not offer, for want of a published hash. A count,
    /// so a short list is visibly a choice rather than a truncated one.
    pub unofferable: usize,
    pub verified: bool,
    pub note: Option<String>,
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
    let (files, unofferable) = releases::offerable(files);
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
        return Err(format!(
            "checksum mismatch for {name}: published {expected}, downloaded {actual}"
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
    Ok(Catalogue {
        verified: failures.is_empty(),
        note: (!failures.is_empty()).then(|| failures.join("; ")),
        files,
        unofferable,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DownloadRequest {
    kind: String,
    url: String,
}

#[tauri::command]
async fn download_release(
    app: AppHandle,
    request: DownloadRequest,
) -> Result<image::ImageInfo, String> {
    let index = app.state::<Loaded>().hashes();
    let (info, bytes) = download_verified(&request.kind, &request.url, &index).await?;
    app.state::<Loaded>().put_image(&request.kind, bytes);
    Ok(info)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PickedFile {
    kind: String,
    path: String,
}

#[tauri::command]
async fn pick_image_file(app: AppHandle, file: PickedFile) -> Result<image::ImageInfo, String> {
    let PickedFile { kind, path } = file;
    let message = path.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("could not read {message}: {e}"))?;
    let info = inspect(&kind, &bytes)?;
    app.state::<Loaded>().put_image(&kind, bytes);
    Ok(info)
}

#[tauri::command]
async fn list_shells(app: AppHandle) -> Result<Vec<DeviceInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let usb = app.state::<Usb>();
        let api = usb.0.lock().map_err(|e| e.to_string())?;
        Ok(shell::list_devices(&api))
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

/// Hotplug is polled rather than event driven. hidapi 2.x exposes no portable refresh, and a
/// fresh context costs a few milliseconds against the half second a person waits to see the
/// "connected" badge after plugging a cable in.
fn watch_presence(app: AppHandle) {
    std::thread::spawn(move || {
        let mut present: Vec<DeviceInfo> = Vec::new();
        loop {
            let now = match hidapi::HidApi::new() {
                Ok(api) => shell::list_devices(&api),
                // A USB subsystem that vanishes mid-session is rare; keep the last known state
                // and try again rather than reporting the Shell unplugged.
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
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
        .setup(|app| {
            app.manage(Usb(Mutex::new(hidapi::HidApi::new()?)));
            app.manage(Loaded::default());
            watch_presence(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_shells,
            shell_app_configuration,
            list_releases,
            download_release,
            pick_image_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keycard Shell Updater");
}
