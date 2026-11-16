use reqwest::header::{HeaderMap, ACCEPT, ETAG, IF_NONE_MATCH};
use reqwest::StatusCode;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const MAX_LIST_BYTES: usize = 8 * 1024 * 1024;

pub const MAX_IMAGE_BYTES: usize = crate::image::FIRMWARE_PARTITION_SIZE;

/// One line with everything the error carries, because reqwest's own Display stops at `error sending
/// request for url (https://…)`: the part that says whether it was DNS, a proxy, a certificate or a
/// reset lives further down the chain, and it is the only part that answers `why is it offline`. A
/// link that only repeats the url is left out, since the message starts with it.
fn chain(url: &str, error: &reqwest::Error) -> String {
    let mut links = Vec::new();
    let mut next: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(one) = next {
        let text = one.to_string();
        if !text.contains(url) {
            links.push(text);
        }
        next = one.source();
    }
    links.join(": ")
}

#[derive(Debug, Error)]
pub enum NetError {
    #[error("could not reach {url}: {}", chain(url, source))]
    Request {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{url} responded {status}")]
    Status { url: String, status: u16 },
    #[error(
        "{url} is past GitHub's allowance for this network, which renews in {minutes} minutes"
    )]
    RateLimited { url: String, minutes: u64 },
    #[error("{url} returned {found} bytes, more than the {max} byte limit")]
    TooLarge {
        url: String,
        found: usize,
        max: usize,
    },
    #[error("{url} was not the JSON expected: {source}")]
    Decode {
        url: String,
        #[source]
        source: serde_json::Error,
    },
}

/// Bodies up to this size are kept so the next read can be a conditional one. A request answered 304
/// costs nothing against the allowance, and the allowance is what runs out.
const CACHE_MAX_BYTES: usize = 512 * 1024;

/// What a 403 that names no reset is held for: the whole window, rather than another request now.
const DEFAULT_ALLOWANCE_MINUTES: u64 = 60;

#[derive(Debug, Clone)]
struct Held {
    etag: Option<String>,
    body: Vec<u8>,
}

/// Where a read that carried an etag is kept between runs: the wait for GitHub should not cost the
/// list the app already read, and a revalidation of what is on disk costs nothing against it.
fn cache_dir() -> &'static OnceLock<PathBuf> {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    &DIR
}

pub fn keep_reads_in(dir: PathBuf) {
    std::fs::create_dir_all(&dir).ok();
    let _ = cache_dir().set(dir);
}

fn entry(url: &str) -> Option<PathBuf> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.as_bytes() {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
    }
    Some(cache_dir().get()?.join(format!("{hash:016x}")))
}

fn read_disk(url: &str) -> Option<Held> {
    let key = entry(url)?;
    let body = std::fs::read(&key).ok()?;
    let etag = std::fs::read(key.with_extension("etag"))
        .ok()
        .map(|raw| String::from_utf8_lossy(&raw).trim().to_string())
        .filter(|one| !one.is_empty());
    Some(Held { etag, body })
}

fn keep_on_disk(url: &str, held: &Held) {
    let Some(key) = entry(url) else { return };
    std::fs::write(&key, &held.body).ok();
    if let Some(etag) = &held.etag {
        std::fs::write(key.with_extension("etag"), etag).ok();
    }
}

fn remember(url: &str, held: Held) {
    if let Ok(mut kept) = cache().lock() {
        kept.insert(url.to_string(), held.clone());
    }
    keep_on_disk(url, &held);
}

/// What is known about a url, from memory first and then from the last run.
fn held_of(url: &str) -> Option<Held> {
    if let Some(one) = cache().lock().ok().and_then(|kept| kept.get(url).cloned()) {
        return Some(one);
    }
    let from_disk = read_disk(url)?;
    if let Ok(mut kept) = cache().lock() {
        kept.insert(url.to_string(), from_disk.clone());
    }
    Some(from_disk)
}

#[cfg(test)]
fn forget_memory() {
    if let Ok(mut kept) = cache().lock() {
        kept.clear();
    }
}

fn cache() -> &'static Mutex<HashMap<String, Held>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Held>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// When this network's GitHub allowance comes back, in seconds since the epoch; zero sends requests.
fn allowance_reset() -> &'static AtomicU64 {
    static RESET: AtomicU64 = AtomicU64::new(0);
    &RESET
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

fn header_unix(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|one| one.to_str().ok())
        .and_then(|one| one.trim().parse().ok())
}

fn is_allowance(headers: &HeaderMap) -> bool {
    header_unix(headers, "x-ratelimit-remaining") == Some(0) || headers.get("retry-after").is_some()
}

fn reset_after_refusal(headers: &HeaderMap, now: u64) -> u64 {
    header_unix(headers, "retry-after")
        .map(|secs| now + secs)
        .or_else(|| header_unix(headers, "x-ratelimit-reset").filter(|reset| *reset > now))
        .unwrap_or(now + DEFAULT_ALLOWANCE_MINUTES * 60)
}

fn minutes_until(reset: u64, now: u64) -> u64 {
    reset.saturating_sub(now).div_ceil(60)
}

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("kshell-updater/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("a default HTTP client builds")
    })
}

async fn get(url: &str, accept: &str, max: usize) -> Result<Vec<u8>, NetError> {
    let held = held_of(url);

    let blocked_until = allowance_reset().load(Ordering::Relaxed);
    if blocked_until > now_unix() {
        // The allowance of this network is spent. Anything already read is served as it stands, and
        // without one the wait is reported: asking again inside the window cannot succeed.
        if let Some(one) = held {
            return Ok(one.body);
        }
        return Err(NetError::RateLimited {
            url: url.to_string(),
            minutes: minutes_until(blocked_until, now_unix()),
        });
    }

    let mut request = client().get(url).header(ACCEPT, accept);
    if let Some(Held {
        etag: Some(etag),
        body: _,
    }) = held.as_ref()
    {
        request = request.header(IF_NONE_MATCH, etag);
    }

    let response = request.send().await.map_err(|source| NetError::Request {
        url: url.to_string(),
        source,
    })?;
    let status = response.status();
    let headers = response.headers().clone();

    if status == StatusCode::NOT_MODIFIED {
        if let Some(one) = held {
            return Ok(one.body);
        }
    }
    if status == StatusCode::FORBIDDEN && is_allowance(&headers) {
        let reset = reset_after_refusal(&headers, now_unix());
        allowance_reset().store(reset, Ordering::Relaxed);
        return Err(NetError::RateLimited {
            url: url.to_string(),
            minutes: minutes_until(reset, now_unix()),
        });
    }
    if !status.is_success() {
        return Err(NetError::Status {
            url: url.to_string(),
            status: status.as_u16(),
        });
    }
    if let Some(declared) = response.content_length() {
        if declared > max as u64 {
            return Err(NetError::TooLarge {
                url: url.to_string(),
                found: declared as usize,
                max,
            });
        }
    }
    let body = response.bytes().await.map_err(|source| NetError::Request {
        url: url.to_string(),
        source,
    })?;
    if body.len() > max {
        return Err(NetError::TooLarge {
            url: url.to_string(),
            found: body.len(),
            max,
        });
    }

    allowance_reset().store(0, Ordering::Relaxed);
    if body.len() <= CACHE_MAX_BYTES {
        remember(
            url,
            Held {
                etag: headers
                    .get(ETAG)
                    .and_then(|one| one.to_str().ok())
                    .map(str::to_string),
                body: body.to_vec(),
            },
        );
    }
    Ok(body.to_vec())
}

pub async fn get_json(url: &str) -> Result<serde_json::Value, NetError> {
    let body = get(url, "application/vnd.github+json", 4 * 1024 * 1024).await?;
    serde_json::from_slice(&body).map_err(|source| NetError::Decode {
        url: url.to_string(),
        source,
    })
}

pub async fn get_text(url: &str) -> Result<String, NetError> {
    let body = get(url, "text/plain", 64 * 1024).await?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

pub async fn get_image(url: &str) -> Result<Vec<u8>, NetError> {
    get(url, "application/octet-stream", MAX_IMAGE_BYTES).await
}

pub async fn get_bytes(url: &str, max: usize) -> Result<Vec<u8>, NetError> {
    get(url, "application/octet-stream", max).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::net::TcpStream;
    use std::sync::Arc;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                reqwest::header::HeaderName::from_lowercase(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }

    /// A stand-in for the API: it answers, revalidates, refuses, and counts what it was asked for,
    /// because "we stopped asking" is the whole point of the cache and the wait.
    struct Api {
        asked: Arc<Mutex<Vec<String>>>,
        base: String,
    }

    impl Api {
        fn url(&self, tail: &str) -> String {
            format!("{}{}", self.base, tail)
        }
    }

    impl Api {
        fn scripted(script: Vec<String>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a port to serve on");
            let base = format!("http://{}/", listener.local_addr().unwrap());
            let asked: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
            let counted = Arc::clone(&asked);
            let thread = std::thread::spawn(move || {
                let mut script = script.into_iter();
                for mut stream in listener.incoming().flatten() {
                    let request = read_request(&mut stream);
                    let _ = counted.lock().map(|mut one| one.push(request.clone()));
                    let answer = script
                        .next()
                        .unwrap_or_else(|| "HTTP/1.1 500 Sold Out\r\n\r\n".to_string());
                    let _ = stream.write_all(answer.as_bytes());
                }
            });
            let _ = thread;
            Self { asked, base }
        }

        fn times_asked(&self) -> usize {
            self.asked.lock().map(|one| one.len()).unwrap_or_default()
        }
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut buf = [0_u8; 1024];
        let mut text = String::new();
        while !text.contains("\r\n\r\n") {
            let Ok(read) = stream.read(&mut buf) else {
                break;
            };
            if read == 0 {
                break;
            }
            text.push_str(&String::from_utf8_lossy(&buf[..read]));
        }
        text
    }

    #[test]
    fn a_list_already_read_is_revalidated_and_survives_the_allowance_running_out() {
        let later = now_unix() + 120;
        let kept_in = std::env::temp_dir().join(format!("kshell-reads-{}", std::process::id()));
        keep_reads_in(kept_in.clone());
        let api = Api::scripted(vec![
            format!("HTTP/1.1 200 OK\r\netag: \"one\"\r\nx-ratelimit-reset: {later}\r\n\r\nfirst"),
            "HTTP/1.1 304 Not Modified\r\n\r\n".to_string(),
            "HTTP/1.1 304 Not Modified\r\n\r\n".to_string(),
            format!(
                "HTTP/1.1 403 Forbidden\r\nx-ratelimit-remaining: 0\r\nx-ratelimit-reset: {later}\r\n\r\nover the limit"
            ),
        ]);

        let read = api.url("latest");
        let first = block_on(get_text(&read)).expect("the first read answers");
        assert_eq!(first, "first");
        assert!(
            !api.asked.lock().unwrap()[0].contains("if-none-match"),
            "nothing is held yet, so the first read asks plainly",
        );

        assert_eq!(
            block_on(get_text(&read)).expect("a cached read answers"),
            "first",
            "a 304 answers from what is held",
        );
        assert!(
            api.asked.lock().unwrap()[1].contains("if-none-match: \"one\""),
            "the second read revalidates with the etag it was given",
        );

        forget_memory();
        assert_eq!(
            block_on(get_text(&read)).expect("what the last run kept answers"),
            "first",
            "a restart inside the wait still has the list it read",
        );
        assert!(
            api.asked.lock().unwrap()[2].contains("if-none-match: \"one\""),
            "what the last run kept revalidates with the etag it was given",
        );

        let other = api.url("other");
        let refused =
            block_on(get_text(&other)).expect_err("a refusal of a thing never read is an error");
        assert!(
            matches!(&refused, NetError::RateLimited { minutes, .. } if *minutes == 2),
            "the refusal says when the allowance returns, got {refused}",
        );

        assert_eq!(
            block_on(get_text(&read)).expect("a held copy answers while the allowance is out"),
            "first",
            "the list on screen does not disappear while GitHub refuses the network",
        );
        assert!(block_on(get_text(&api.url("fresh"))).is_err());
        assert_eq!(
            api.times_asked(),
            4,
            "while the allowance is out nothing else is asked of it",
        );
        std::fs::remove_dir_all(&kept_in).ok();
    }

    /// reqwest needs a tokio reactor, which is what the app itself runs on.
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tauri::async_runtime::block_on(future)
    }

    #[test]
    fn a_refusal_that_names_the_renewal_is_waited_out_to_the_minute() {
        let now = 1_800_000_000;
        let refusal = headers(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1800001200"),
        ]);
        assert!(is_allowance(&refusal));
        let reset = reset_after_refusal(&refusal, now);
        assert_eq!(reset, now + 1200);
        assert_eq!(minutes_until(reset, now), 20);
    }

    #[test]
    fn a_refusal_that_names_nothingwaits_the_window_instead_of_firing_again() {
        let now = 1_800_000_000;
        let reset = reset_after_refusal(&headers(&[("x-ratelimit-remaining", "0")]), now);
        assert_eq!(minutes_until(reset, now), DEFAULT_ALLOWANCE_MINUTES);
    }

    #[test]
    fn a_refusal_about_something_else_is_left_as_it_is() {
        assert!(!is_allowance(&headers(&[("x-ratelimit-remaining", "41")])));
        assert!(!is_allowance(&headers(&[])));
    }

    #[test]
    fn a_renewal_already_past_does_not_hold_the_app_offline_forever() {
        let now = 1_800_000_000;
        let reset = reset_after_refusal(&headers(&[("x-ratelimit-reset", "1799999000")]), now);
        assert_eq!(minutes_until(reset, now), DEFAULT_ALLOWANCE_MINUTES);
    }
}
