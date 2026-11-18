use crate::net;
use crate::units::{self, UnitList, UnitsError, LIST_ASSET_NAME};
use crate::ur;
use fast_qr::qr::QRBuilder;
use fast_qr::{QRCode, ECL};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const UNITS_REPO: &str = "keycard-tech/shell-units";

const ORIGIN_ONLINE: &str = "online";
const ORIGIN_FILE: &str = "file";
const ORIGIN_CACHED: &str = "cached";

fn with_session<T>(
    units: &Units,
    run: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = units
        .0
        .lock()
        .map_err(|_| "verify state is busy".to_string())?;
    run(&mut guard)
}

#[derive(Default)]
pub struct Units(Mutex<Session>);

#[derive(Default, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UnitsStatus {
    build: Option<u32>,
    origin: Option<String>,
    fetched_at: Option<i64>,
    file: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    text: String,
    svg: String,
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Verdict {
    Authentic,
    NotRegistered { uid: String },
    Refused { reason: String },
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    build: u32,
    origin: String,
    fetched_at: i64,
    file: Option<String>,
}

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn cache_path(dir: &Path) -> PathBuf {
    dir.join("units-state.json")
}

fn list_path(dir: &Path, build: u32) -> PathBuf {
    dir.join(format!("units-{build}.bin"))
}

fn days_since_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() / 86_400)
        .unwrap_or_default() as i64
}

fn restore_cached_list(app: &AppHandle, held: &mut Session) -> Result<(), String> {
    let directory = dir(app)?;
    let Ok(body) = std::fs::read(cache_path(&directory)) else {
        return Ok(());
    };
    let cache: Cache = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(list_path(&directory, cache.build)).map_err(|e| e.to_string())?;
    let list = units::load_list(&bytes).map_err(|e| e.to_string())?;
    held.list = Some(list);
    held.state = UnitsStatus {
        build: Some(cache.build),
        origin: Some(ORIGIN_CACHED.to_string()),
        fetched_at: Some(cache.fetched_at),
        file: cache.file,
    };
    Ok(())
}

#[derive(Default)]
struct Session {
    list: Option<UnitList>,
    state: UnitsStatus,
    challenge: Option<[u8; 32]>,
    answered: bool,
}

impl Session {
    fn use_list(&mut self, list: UnitList, state: UnitsStatus) {
        self.list = Some(list);
        self.state = state;
    }
}

#[tauri::command]
pub fn units_status(app: AppHandle, units: tauri::State<'_, Units>) -> Result<UnitsStatus, String> {
    with_session(&units, |session| {
        if session.list.is_none() {
            restore_cached_list(&app, session)?;
        }
        Ok(session.state.clone())
    })
}

pub async fn newest_list() -> Result<(Vec<u8>, UnitList), String> {
    let mut urls = Vec::with_capacity(2);
    let api = format!("https://api.github.com/repos/{UNITS_REPO}/releases/latest");
    if let Ok(release) = net::get_json(&api).await {
        if let Some(url) = release["assets"]
            .as_array()
            .and_then(|assets| {
                assets
                    .iter()
                    .find(|asset| asset["name"].as_str() == Some(LIST_ASSET_NAME))
            })
            .and_then(|asset| asset["browser_download_url"].as_str())
        {
            urls.push(url.to_string());
        }
    }
    urls.push(format!(
        "https://github.com/{UNITS_REPO}/releases/latest/download/{LIST_ASSET_NAME}"
    ));

    let mut problem = "no units list could be reached".to_string();
    for url in urls {
        match net::get_bytes(&url, net::MAX_LIST_BYTES).await {
            Ok(bytes) => {
                return units::load_list(&bytes)
                    .map(|list| (bytes, list))
                    .map_err(|e| format!("the downloaded units list was refused: {e}"));
            }
            Err(error) => problem = error.to_string(),
        }
    }
    Err(problem)
}

#[tauri::command]
pub async fn units_fetch(
    app: AppHandle,
    units: tauri::State<'_, Units>,
) -> Result<UnitsStatus, String> {
    let (bytes, list) = newest_list().await?;
    let build = list.build;
    let directory = dir(&app)?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    std::fs::write(list_path(&directory, build), &bytes)
        .map_err(|e| format!("could not cache the units list: {e}"))?;

    let state = UnitsStatus {
        build: Some(build),
        origin: Some(ORIGIN_ONLINE.to_string()),
        fetched_at: Some(days_since_epoch()),
        file: None,
    };
    let cache = Cache {
        build,
        origin: ORIGIN_ONLINE.to_string(),
        fetched_at: state.fetched_at.unwrap_or_default(),
        file: None,
    };
    std::fs::write(
        cache_path(&directory),
        serde_json::to_vec_pretty(&cache).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    with_session(&units, |session| {
        session.use_list(list, state.clone());
        Ok(state)
    })
}

#[tauri::command]
pub fn units_use_file(units: tauri::State<'_, Units>, path: String) -> Result<UnitsStatus, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    let list = units::load_list(&bytes).map_err(|e| format!("that file was refused: {e}"))?;
    let build = list.build;
    let file = Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or(path);

    with_session(&units, |session| {
        session.use_list(
            list,
            UnitsStatus {
                build: Some(build),
                origin: Some(ORIGIN_FILE.to_string()),
                fetched_at: Some(days_since_epoch()),
                file: Some(file),
            },
        );
        Ok(session.state.clone())
    })
}

#[tauri::command]
pub fn units_challenge(
    units: tauri::State<'_, Units>,
    renew: Option<bool>,
) -> Result<Challenge, String> {
    with_session(&units, |session| {
        if renew.unwrap_or(false) || session.challenge.is_none() || session.answered {
            session.challenge = Some(units::new_challenge());
            session.answered = false;
        }
        let challenge = session.challenge.ok_or("no challenge could be made")?;
        let text = ur::ur_encode(&units::encode_init(&challenge));
        let svg = challenge_svg(&text)?;
        Ok(Challenge { text, svg })
    })
}

#[tauri::command]
pub fn units_check(units: tauri::State<'_, Units>, text: String) -> Result<Verdict, String> {
    with_session(&units, |session| {
        let challenge = session
            .challenge
            .ok_or("this verify session has no challenge to check against")?;
        if session.answered {
            return Ok(Verdict::Refused {
                reason: "this session has already been answered; start the check again".into(),
            });
        }
        let Some(list) = session.list.as_ref() else {
            return Ok(Verdict::Refused {
                reason: "no units list has been loaded on this computer".into(),
            });
        };
        let payload = ur::ur_decode(&text).map_err(|e| e.to_string())?;
        Ok(match units::check_answer(list, &challenge, &payload) {
            Ok(_) => {
                session.answered = true;
                Verdict::Authentic
            }
            Err(UnitsError::NotRegistered { uid }) => {
                session.answered = true;
                Verdict::NotRegistered { uid }
            }
            Err(UnitsError::Refusal(reason)) => Verdict::Refused { reason },
        })
    })
}

const QUIET_ZONE_MODULES: usize = 4;

#[tauri::command]
pub fn link_code(url: String) -> Result<String, String> {
    challenge_svg(&url)
}

pub fn challenge_svg(text: &str) -> Result<String, String> {
    use fast_qr::convert::svg::SvgBuilder;
    use fast_qr::convert::{Builder, Shape};

    Ok(SvgBuilder::default()
        .margin(QUIET_ZONE_MODULES)
        .shape(Shape::Square)
        .to_str(&challenge_code(text)?))
}

/// Error level L, as shell-units docs/format.md asks for. fast_qr defaults to quartile, which spends
/// a fifth of the symbol on redundancy the Shell's camera does not need and buys smaller modules with
/// it - and that camera has a narrow focus range, so a denser code fails like one that is too small.
fn challenge_code(text: &str) -> Result<QRCode, String> {
    QRBuilder::new(text.to_string())
        .ecl(ECL::L)
        .build()
        .map_err(|e| format!("the challenge did not fit a QR code: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_challenge_code_is_sparse_enough_for_the_shell_to_focus_on() {
        let text = format!("UR:DEV-AUTH/{}", "ae".repeat(42));
        let code = challenge_code(&text).expect("a challenge fits a QR code");
        assert_eq!(
            code.size, 37,
            "a challenge answer takes {} modules; quartile error correction would make it 41, \
             and the Shell's camera has a narrow focus range",
            code.size,
        );
    }

    #[test]
    fn a_list_arriving_after_the_challenge_leaves_the_challenge_alone() {
        let mut session = Session::default();
        let challenge = units::new_challenge();
        session.challenge = Some(challenge);

        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("src-tauri is under the repository")
                .join("tests/fixtures/units-latest.bin"),
        )
        .expect("run `cargo run --example units` to write tests/fixtures/units-latest.bin");
        let list = units::load_list(&bytes).expect("the published list loads");
        session.use_list(list, UnitsStatus::default());

        assert_eq!(
            session.challenge,
            Some(challenge),
            "the screen is still showing this code"
        );
    }
}
