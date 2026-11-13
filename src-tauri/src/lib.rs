mod shell;

use shell::{AppConfiguration, DeviceInfo, Shell};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// `HidApi` is Send but not Sync, and opening the same device twice confuses the platform
/// drivers, so exactly one instance is shared and every access takes the lock.
struct Usb(Mutex<hidapi::HidApi>);

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
        .setup(|app| {
            app.manage(Usb(Mutex::new(hidapi::HidApi::new()?)));
            watch_presence(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_shells,
            shell_app_configuration
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keycard Shell Updater");
}
