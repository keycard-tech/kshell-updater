use crabcamera::headless::session::SessionHandle;
use crabcamera::headless::{list_devices, CaptureConfig, Frame, HeadlessSession};
use crabcamera::CameraFormat;
use image::{ExtendedColorType, ImageEncoder, RgbImage};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{ipc, AppHandle, Emitter, Manager, State};

const EVENT_CODE: &str = "scan://code";

pub const SCAN_PX: u32 = 480;

const JPEG_QUALITY: u8 = 70;

const FRAME_WAIT: Duration = Duration::from_millis(2000);

const SECOND_FRAME_WAIT: Duration = Duration::from_millis(3000);

fn said(what: impl AsRef<str>) {
    log::info!("[scan] {}", what.as_ref());
}

fn detail(what: impl AsRef<str>) {
    log::debug!("[scan]     {}", what.as_ref());
}

#[derive(Clone, Default)]
pub struct Scanner(Arc<Mutex<Option<Arc<SessionHandle>>>>);

impl Scanner {
    fn session(&self) -> Result<Arc<SessionHandle>, String> {
        self.0
            .lock()
            .map_err(|_| "scan state is busy".to_string())?
            .clone()
            .ok_or_else(|| String::from("no camera is being scanned"))
    }

    fn give_up(&self) {
        if let Some(session) = self.0.lock().ok().and_then(|mut held| held.take()) {
            if let Err(why) = session.stop(FRAME_WAIT) {
                log::warn!("[scan] would not stop the camera: {why}");
            }
            detail("camera released");
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Camera {
    id: String,
    label: String,
}

#[derive(Clone)]
struct Node {
    camera: Camera,
    modes: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    cameras: Vec<Camera>,
    device: String,
}

fn listed() -> Result<Vec<Node>, String> {
    let listed = list_devices().map_err(|why| format!("the cameras would not list: {why}"))?;
    said(format!("{} camera(s) listed", listed.len()));
    for found in &listed {
        detail(format!(
            "{}: {} [{}] {}",
            found.id,
            found.name,
            if found.is_available { "free" } else { "busy" },
            modes_of(&found.supports_formats)
        ));
    }
    Ok(listed
        .iter()
        .enumerate()
        .map(|(nth, found)| Node {
            modes: found.supports_formats.len(),
            camera: Camera {
                id: found.id.clone(),
                label: if found.name.trim().is_empty() {
                    format!("Camera {}", nth + 1)
                } else {
                    found.name.clone()
                },
            },
        })
        .collect())
}

fn modes_of(formats: &[CameraFormat]) -> String {
    if formats.is_empty() {
        return String::from("no mode claimed");
    }
    formats
        .iter()
        .map(|mode| {
            format!(
                "{}x{} {}@{}",
                mode.width, mode.height, mode.format_type, mode.fps
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn told(frame: &Frame) -> String {
    let pixels = frame.width as usize * frame.height as usize;
    format!(
        "{}x{}, {} bytes ({:.1} per pixel), frame #{}",
        frame.width,
        frame.height,
        frame.data.len(),
        frame.data.len() as f64 / pixels.max(1) as f64,
        frame.sequence
    )
}

fn ms(since: Duration) -> u64 {
    since.as_millis() as u64
}

fn open_one(id: &str) -> Result<SessionHandle, String> {
    let attempted = |why: String| {
        if let Ok(formats) = crabcamera::headless::list_formats(id) {
            log::warn!(
                "[scan] {id}, as crabcamera lists it: {}",
                modes_of(&formats)
            );
        }
        why
    };

    let asked = CameraFormat::new(1280, 720, 30.0);
    detail(format!(
        "{id}: opening, asking {}x{} @ {} fps",
        asked.width, asked.height, asked.fps
    ));
    let began = Instant::now();
    let session = HeadlessSession::open(CaptureConfig::new(id.to_string(), asked))
        .map_err(|why| attempted(format!("open: {why}")))?;
    let opened = ms(began.elapsed());

    let began = Instant::now();
    session
        .start()
        .map_err(|why| attempted(format!("start: {why}")))?;
    let started = ms(began.elapsed());

    let first = session
        .get_frame(SECOND_FRAME_WAIT)
        .map_err(|why| format!("first frame: {why}"))?
        .ok_or_else(|| {
            attempted(format!(
                "no frame within {} ms, {} dropped",
                SECOND_FRAME_WAIT.as_millis(),
                session.dropped_frames().unwrap_or_default()
            ))
        })?;
    said(format!(
        "{id}: {opened} ms to open, {started} ms to start, then {} — {}",
        told(&first),
        anatomy(&first.data)
    ));
    Ok(session)
}

fn anatomy(data: &[u8]) -> String {
    let head = data
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    let coded = data
        .windows(3)
        .position(|at| at == [0xFF, 0xD8, 0xFF])
        .map_or_else(|| String::from("nowhere"), |at| at.to_string());
    let padding = data.iter().rev().take_while(|byte| **byte == 0).count();
    format!(
        "starts {head}, jpeg marker at {coded}, {padding} of {} bytes are padding",
        data.len()
    )
}

fn refused(why: &str) -> String {
    log::warn!("[scan] refused: {why}");
    let lower = why.to_lowercase();
    if lower.contains("busy") || lower.contains("in use") {
        String::from("Another app is using the camera. Close it and try again.")
    } else {
        String::from("The camera did not start. Try another one below.")
    }
}

pub fn probe(device: Option<String>) -> Result<String, String> {
    let nodes = listed()?;
    let order = match &device {
        Some(id) => vec![id.clone()],
        None => tries(&nodes, None),
    };
    if order.is_empty() {
        return Err(String::from("no camera is connected"));
    }
    let (session, id) = open_any(&order)?;
    let answer = match session.get_frame(FRAME_WAIT) {
        Ok(Some(frame)) => format!("{} — {}", told(&frame), anatomy(&frame.data)),
        Ok(None) => String::from("no frame after opening"),
        Err(why) => format!("a frame would not come: {why}"),
    };
    session
        .stop(FRAME_WAIT)
        .map_err(|why| format!("would not stop: {why}"))?;
    Ok(format!("{id} gave {answer}"))
}

fn open_any(order: &[String]) -> Result<(SessionHandle, String), String> {
    let mut refusals = Vec::new();
    for id in order {
        match open_one(id) {
            Ok(session) => return Ok((session, id.clone())),
            Err(why) => {
                refusals.push(format!("{id}: {why}"));
                said(format!("{id}: not this one, going on"));
            }
        }
    }
    Err(refused(&refusals.join(" then ")))
}

fn offered(nodes: &[Node]) -> Vec<Camera> {
    let mut offered: Vec<Camera> = Vec::new();
    for node in most_modes(nodes) {
        if !offered.iter().any(|seen| seen.label == node.camera.label) {
            offered.push(node.camera.clone());
        }
    }
    offered
}

fn most_modes(nodes: &[Node]) -> Vec<Node> {
    let mut most = nodes.to_vec();
    most.sort_by_key(|node| std::cmp::Reverse(node.modes));
    most
}

fn tries(nodes: &[Node], wanted: Option<&str>) -> Vec<String> {
    let same_name = wanted
        .and_then(|id| {
            nodes
                .iter()
                .find(|node| node.camera.id == id)
                .map(|wanted| {
                    most_modes(
                        &nodes
                            .iter()
                            .filter(|node| node.camera.label == wanted.camera.label)
                            .cloned()
                            .collect::<Vec<_>>(),
                    )
                    .into_iter()
                    .map(|node| node.camera.id)
                    .collect::<Vec<_>>()
                })
        })
        .unwrap_or_default();
    let mut tries: Vec<String> = Vec::new();
    let names = wanted
        .map(str::to_string)
        .into_iter()
        .chain(same_name)
        .chain(most_modes(nodes).into_iter().map(|node| node.camera.id));
    for id in names {
        if !tries.contains(&id) {
            tries.push(id);
        }
    }
    tries
}

#[tauri::command]
pub async fn scan_start(app: AppHandle, device: Option<String>) -> Result<Scan, String> {
    let scanner = app.state::<Scanner>().inner().clone();
    match &device {
        Some(id) => said(format!("asked to scan {id}")),
        None => said("asked to scan, any camera"),
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<Scan, String> {
        let nodes = listed()?;
        if nodes.is_empty() {
            log::warn!("[scan] nothing to open: no camera is connected");
            return Err(String::from(
                "No camera is connected. Plug one in and try again.",
            ));
        }
        scanner.give_up();
        let (session, running) = open_any(&tries(&nodes, device.as_deref()))?;
        said(format!("{running}: scanning"));
        *scanner
            .0
            .lock()
            .map_err(|_| "scan state is busy".to_string())? = Some(Arc::new(session));
        let cameras = offered(&nodes);
        let shown = nodes
            .iter()
            .find(|node| node.camera.id == running)
            .and_then(|running| {
                cameras
                    .iter()
                    .find(|camera| camera.label == running.camera.label)
            })
            .cloned();
        Ok(Scan {
            cameras,
            device: shown.map_or(running, |camera| camera.id),
        })
    })
    .await
    .map_err(|why| format!("opening the camera panicked: {why}"))?
}

#[tauri::command]
pub async fn scan_frame(app: AppHandle) -> Result<ipc::Response, String> {
    let session = app.state::<Scanner>().session()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<ipc::Response, String> {
        let began = Instant::now();
        let frame = match session.get_frame(FRAME_WAIT) {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                log::warn!(
                    "[scan] no frame within {} ms, {} dropped",
                    FRAME_WAIT.as_millis(),
                    session.dropped_frames().unwrap_or_default()
                );
                return Err(String::from(
                    "The camera sent no picture. Try another one below.",
                ));
            }
            Err(why) => {
                log::warn!("[scan] a frame would not come: {why}");
                return Err(String::from(
                    "The camera sent no picture. Try another one below.",
                ));
            }
        };
        let waited = ms(began.elapsed());
        let telling = told(&frame);

        let began = Instant::now();
        let picture = picture_of(frame.width, frame.height, frame.data)?;
        let converted = ms(began.elapsed());

        let began = Instant::now();
        let square = square(&picture);
        let cropped = ms(began.elapsed());

        let began = Instant::now();
        let code = code_of(&square);
        let read = ms(began.elapsed());

        let mut jpeg = Vec::new();
        let began = Instant::now();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
            .write_image(
                square.as_raw(),
                square.width(),
                square.height(),
                ExtendedColorType::Rgb8,
            )
            .map_err(|why| format!("could not encode the picture: {why}"))?;
        let encoded = ms(began.elapsed());

        detail(format!(
            "{telling}: waited {waited} ms, converted {converted}, cropped {cropped}, read {read}, \
             jpeg {encoded}, {} bytes of jpeg{}",
            jpeg.len(),
            match &code {
                Some(code) => format!(" read \"{}\"", shorten(code)),
                None => " no code".to_string(),
            }
        ));
        if let Some(code) = code {
            let _ = app.emit(EVENT_CODE, code);
        }
        Ok(ipc::Response::new(jpeg))
    })
    .await
    .map_err(|why| why.to_string())?
}

#[tauri::command]
pub fn scan_stop(scanner: State<'_, Scanner>) {
    said("asked to stop");
    scanner.give_up();
}

pub fn picture_of(width: u32, height: u32, data: Vec<u8>) -> Result<RgbImage, String> {
    let pixels = width as usize * height as usize;
    let bytes = data.len();
    let said = |because: &str| {
        log::warn!(
            "[scan] {width}x{height} of {bytes} bytes ({:.1} per pixel) is {because}",
            bytes as f64 / pixels.max(1) as f64,
        )
    };

    if data.starts_with(&[0xFF, 0xD8]) {
        if let Some(picture) = coded(&data) {
            detail(String::from("a frame of coded jpeg, padded by the driver"));
            return Ok(picture);
        }
    }
    let picture = if bytes == pixels * 3 {
        RgbImage::from_raw(width, height, data)
    } else if bytes == pixels * 2 {
        yuyv(width, height, &data)
    } else if bytes == pixels {
        shades(width, height, &data)
    } else {
        coded(&data).or_else(|| {
            said("neither RGB, YUYV, luma nor a coded picture");
            None
        })
    };
    picture.ok_or_else(|| {
        said("a picture this app cannot read");
        String::from("The camera sent a picture this app could not read.")
    })
}

fn coded(data: &[u8]) -> Option<RgbImage> {
    image::load_from_memory(data)
        .map(|picture| picture.to_rgb8())
        .ok()
}

fn yuyv(width: u32, height: u32, data: &[u8]) -> Option<RgbImage> {
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for line in data.chunks_exact(width as usize * 2) {
        for pair in line.chunks_exact(4) {
            let (d, e) = (i32::from(pair[1]) - 128, i32::from(pair[3]) - 128);
            for luma in [i32::from(pair[0]), i32::from(pair[2])] {
                let c = luma - 16;
                rgb.push(tinted(298 * c + 409 * e + 128));
                rgb.push(tinted(298 * c - 100 * d - 208 * e + 128));
                rgb.push(tinted(298 * c + 516 * d + 128));
            }
        }
    }
    RgbImage::from_raw(width, height, rgb)
}

fn tinted(scaled: i32) -> u8 {
    u8::try_from(scaled >> 8).unwrap_or(if scaled >> 8 < 0 { 0 } else { 255 })
}

fn shades(width: u32, height: u32, data: &[u8]) -> Option<RgbImage> {
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for shade in data {
        rgb.extend_from_slice(&[*shade, *shade, *shade]);
    }
    RgbImage::from_raw(width, height, rgb)
}

fn shorten(text: &str) -> String {
    let head: String = text.chars().take(24).collect();
    if head.chars().count() == text.chars().count() {
        head
    } else {
        format!("{head}…")
    }
}

pub fn square(rgb: &RgbImage) -> RgbImage {
    let side = rgb.width().min(rgb.height());
    let cut = image::imageops::crop_imm(
        rgb,
        (rgb.width() - side) / 2,
        (rgb.height() - side) / 2,
        side,
        side,
    )
    .to_image();
    image::imageops::resize(
        &cut,
        SCAN_PX,
        SCAN_PX,
        image::imageops::FilterType::Triangle,
    )
}

pub fn code_of(picture: &RgbImage) -> Option<String> {
    let grey = image::DynamicImage::ImageRgb8(picture.clone()).into_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(grey);
    prepared
        .detect_grids()
        .into_iter()
        .find_map(|grid| grid.decode().ok().map(|(_, text)| text))
}

#[cfg(test)]
mod tests {
    use super::{offered, picture_of, square, tries, yuyv, Camera, Node};

    fn nodes() -> Vec<Node> {
        [
            ("Integrated Camera", 3),
            ("Integrated Camera", 47),
            ("Integrated Camera", 3),
            ("External Camera", 44),
        ]
        .iter()
        .enumerate()
        .map(|(nth, (label, modes))| Node {
            modes: *modes,
            camera: Camera {
                id: nth.to_string(),
                label: (*label).to_string(),
            },
        })
        .collect()
    }

    #[test]
    fn nodes_sharing_a_name_are_offered_once_and_the_capturing_one_leads() {
        let cameras = offered(&nodes());
        assert_eq!(
            cameras
                .iter()
                .map(|camera| camera.label.as_str())
                .collect::<Vec<_>>(),
            ["Integrated Camera", "External Camera"]
        );
        assert_eq!(
            cameras
                .iter()
                .map(|camera| camera.id.as_str())
                .collect::<Vec<_>>(),
            ["1", "3"]
        );
    }

    #[test]
    fn a_chosen_camera_is_tried_through_its_own_nodes_first() {
        assert_eq!(tries(&nodes(), Some("0")), ["0", "1", "2", "3"]);
        assert_eq!(tries(&nodes(), None), ["1", "3", "0", "2"]);
    }

    #[test]
    fn neutral_yuyv_is_grey_on_both_pixels_of_the_pair() {
        let picture = yuyv(2, 1, &[128, 128, 128, 128]).expect("two pixels");
        assert_eq!(picture.get_pixel(0, 0).0, [130, 130, 130]);
        assert_eq!(picture.get_pixel(1, 0).0, [130, 130, 130]);
    }

    #[test]
    fn one_chroma_sample_colours_both_pixels_it_is_given_to() {
        let picture = yuyv(2, 1, &[128, 0, 128, 255]).expect("two pixels");
        for x in 0..2 {
            assert_eq!(picture.get_pixel(x, 0).0, [255, 77, 0]);
        }
    }

    #[test]
    fn a_picture_is_read_off_its_bytes_per_pixel() {
        assert_eq!(
            picture_of(1, 1, vec![10, 20, 30])
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [10, 20, 30]
        );
        assert_eq!(
            picture_of(2, 1, vec![128, 128, 128, 128]).unwrap().width(),
            2
        );
        assert_eq!(
            picture_of(1, 1, vec![77]).unwrap().get_pixel(0, 0).0,
            [77, 77, 77]
        );
        assert!(picture_of(2, 2, vec![1, 2, 3]).is_err());
    }

    #[test]
    fn an_empty_frame_is_the_green_a_person_sees() {
        assert_eq!(
            picture_of(2, 2, vec![0u8; 8]).unwrap().get_pixel(0, 0).0,
            [0, 135, 0]
        );
    }

    #[test]
    fn a_padded_jpeg_is_read_as_a_picture_not_as_static() {
        use image::ImageEncoder;
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
            .write_image(
                image::RgbImage::from_pixel(4, 4, image::Rgb([20, 200, 20])).as_raw(),
                4,
                4,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        jpeg.resize(64 * 64 * 2, 0);
        assert_eq!(picture_of(64, 64, jpeg).unwrap().width(), 4);
    }

    #[test]
    fn the_centre_square_of_a_wide_frame_keeps_what_was_in_the_middle() {
        let mut wide = vec![0u8; 16 * 9];
        for (offset, byte) in wide.iter_mut().enumerate() {
            let (x, y) = ((offset % 16) as u32, (offset / 16) as u32);
            *byte = u8::from((8..16).contains(&x) && (4..9).contains(&y) && (x + y) % 2 == 0);
        }
        let square = square(&picture_of(16, 9, wide).unwrap());
        assert_eq!(
            (square.width(), square.height()),
            (super::SCAN_PX, super::SCAN_PX)
        );
    }
}
