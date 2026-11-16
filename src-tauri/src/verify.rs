//! Commands around the produced units list: get it, keep it, ask a Shell to sign for itself.
//!
//! The list is cached under the app's own directory because the verify flow has to work without a
//! network - the mockups call that state "fetched …, no network since" - and a cached list is only
//! as good as the signature checked again on every read, never on the memory of one.

use crate::net;
use crate::units::{self, UnitList, UnitsError, LIST_ASSET_NAME};
use crate::ur;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Where the produced units list is published.
const UNITS_REPO: &str = "keycard-tech/shell-units";

/// Where the list the screen is describing came from.
const ORIGIN_ONLINE: &str = "online";
const ORIGIN_FILE: &str = "file";
const ORIGIN_CACHED: &str = "cached";

#[derive(Default)]
pub struct Units(Mutex<Session>);

#[derive(Default, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UnitsStatus {
    /// Build date the list header carries, which is how the site names a list.
    build: Option<u32>,
    origin: Option<String>,
    /// The day the app last downloaded it, not the day it was built.
    fetched_at: Option<i64>,
    file: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Challenge {
    /// What the QR says, so a person reading it over someone's shoulder can compare.
    text: String,
    svg: String,
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Verdict {
    /// The signature came from a unit on the list.
    Authentic,
    /// Well-formed and correctly signed, by a unit that never left the factory.
    NotRegistered { uid: String },
    /// Something that is not an answer to this session's challenge.
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

/// Days since the epoch: the view turns it into `2026-11-13`, so no calendar code lives here.
fn today() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() / 86_400)
        .unwrap_or_default() as i64
}

/// A cached list is read again rather than trusted: an edited file says so rather than being believed.
fn restore(app: &AppHandle, held: &mut Session) -> Result<(), String> {
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
    /// The challenge a screen was drawn for: an answer is checked against it and nothing else.
    answered: bool,
}

impl Session {
    /// A list has been loaded, from a download or from a file the person chose.
    ///
    /// It does not touch `challenge`: the verify screen draws its QR before the list is even asked
    /// for, so a list that arrives afterwards is the normal case, and clearing the challenge there
    /// left the code on screen belonging to nothing -- every scan then answered "this verify session
    /// has no challenge to check against" against a genuine Shell (2026-11-13).
    fn note_list(&mut self, list: UnitList, state: UnitsStatus) {
        self.list = Some(list);
        self.state = state;
    }
}

#[tauri::command]
pub fn units_status(app: AppHandle, units: tauri::State<'_, Units>) -> Result<UnitsStatus, String> {
    let mut session = units
        .0
        .lock()
        .map_err(|_| "verify state is busy".to_string())?;
    if session.list.is_none() {
        restore(&app, &mut session)?;
    }
    Ok(session.state.clone())
}

/// The newest produced units list, downloaded and verified; absence is a refusal like any other.
///
/// The API names the release, and its rate limit is not a reason to have no list, so the same file is
/// also tried at the release's stable download path. Both answers are verified by their signature, so
/// falling back asks nothing of trust.
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

/// Download the newest list and keep it; a failure leaves the loaded list in place.
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
        fetched_at: Some(today()),
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

    let mut session = units
        .0
        .lock()
        .map_err(|_| "verify state is busy".to_string())?;
    session.note_list(list, state.clone());
    Ok(state)
}

/// A chosen file, checked with the same pinned key: the source changes, the rules do not.
#[tauri::command]
pub fn units_use_file(
    app: AppHandle,
    units: tauri::State<'_, Units>,
    path: String,
) -> Result<UnitsStatus, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    let list = units::load_list(&bytes).map_err(|e| format!("that file was refused: {e}"))?;
    let build = list.build;
    let file = Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or(path);

    let mut session = units
        .0
        .lock()
        .map_err(|_| "verify state is busy".to_string())?;
    session.note_list(
        list,
        UnitsStatus {
            build: Some(build),
            origin: Some(ORIGIN_FILE.to_string()),
            fetched_at: Some(today()),
            file: Some(file),
        },
    );
    let _ = app;
    Ok(session.state.clone())
}

/// A fresh challenge, and the QR the Shell will read it from.
#[tauri::command]
pub fn units_challenge(units: tauri::State<'_, Units>) -> Result<Challenge, String> {
    let mut session = units
        .0
        .lock()
        .map_err(|_| "verify state is busy".to_string())?;
    let challenge = units::new_challenge();
    let text = ur::ur_encode(&units::encode_init(&challenge));
    let svg = challenge_svg(&text)?;
    session.challenge = Some(challenge);
    session.answered = false;
    Ok(Challenge { text, svg })
}

/// Read a scanned answer; the challenge is spent once, so a scan cannot be a second guess.
#[tauri::command]
pub fn units_check(units: tauri::State<'_, Units>, text: String) -> Result<Verdict, String> {
    let mut session = units
        .0
        .lock()
        .map_err(|_| "verify state is busy".to_string())?;
    let challenge = session
        .challenge
        .ok_or_else(|| "this verify session has no challenge to check against".to_string())?;
    if session.answered {
        return Ok(Verdict::Refused {
            reason: "this session has already been answered; start the check again".into(),
        });
    }
    if session.list.is_none() {
        return Ok(Verdict::Refused {
            reason: "no units list has been loaded on this computer".into(),
        });
    }
    let payload = ur::ur_decode(&text).map_err(|e| e.to_string())?;
    let list = session.list.as_ref().expect("checked just above");
    let verdict = match units::check_answer(list, &challenge, &payload) {
        Ok(_) => {
            session.answered = true;
            Verdict::Authentic
        }
        Err(UnitsError::NotRegistered { uid }) => {
            session.answered = true;
            Verdict::NotRegistered { uid }
        }
        Err(UnitsError::Refusal(reason)) => Verdict::Refused { reason },
    };
    Ok(verdict)
}

/// The QR code as inline SVG: the CSP allows no remote image, and an SVG string needs none.
pub fn challenge_svg(text: &str) -> Result<String, String> {
    use fast_qr::convert::svg::SvgBuilder;
    use fast_qr::convert::{Builder, Shape};
    use fast_qr::qr::QRBuilder;

    let code = QRBuilder::new(text.to_string())
        .build()
        .map_err(|e| format!("the challenge did not fit a QR code: {e}"))?;
    Ok(SvgBuilder::default()
        .margin(2)
        .shape(Shape::RoundedSquare)
        .to_str(&code))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The order `openVerify` runs in: a challenge first, so the screen is never a blank tile, and
    /// the list behind it. Wiping the challenge when the list landed was enough to fail every real
    /// Shell, because by the time the answer was scanned the code it answered was gone.
    #[test]
    fn a_list_arriving_after_the_challenge_leaves_the_challenge_alone() {
        let mut session = Session::default();
        let challenge = units::new_challenge();
        session.challenge = Some(challenge);

        let bytes = std::fs::read(format!(
            "{}/scratch/units-20261112.bin",
            std::env::var("HOME").unwrap_or_default()
        ));
        let Ok(bytes) = bytes else {
            // Same reason the vector test skips: the factory list is not in the repository.
            return;
        };
        let list = units::load_list(&bytes).expect("the published list loads");
        session.note_list(list, UnitsStatus::default());

        assert_eq!(
            session.challenge,
            Some(challenge),
            "the screen is still showing this code"
        );
    }
}
