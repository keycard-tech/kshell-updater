//! HTTPS to GitHub, and nothing else.
//!
//! Downloads happen here rather than in the renderer because the CSP allows no remote `connect-src`,
//! and because an image must be hashed by the same code that hands it to the device. Unauthenticated
//! GitHub API use is capped at 60 requests an hour per address.

use reqwest::header::ACCEPT;
use thiserror::Error;

/// Upper bound on a produced units list: 4096 rows today, and room for years of them.
pub const MAX_LIST_BYTES: usize = 8 * 1024 * 1024;

/// Upper bound on a release image. The firmware partition is [`crate::image::FW_SIZE`]; a database is
/// far smaller. Anything larger is not what we asked for.
pub const MAX_IMAGE_BYTES: usize = crate::image::FW_SIZE;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("could not reach {url}: {source}")]
    Request {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{url} responded {status}")]
    Status { url: String, status: u16 },
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

/// One client for the run, so connections are pooled and the builder runs once.
fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            // GitHub answers a request without a User-Agent with an HTML error page.
            .user_agent(concat!("kshell-updater/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("a default HTTP client builds")
    })
}

async fn get(url: &str, accept: &str, max: usize) -> Result<Vec<u8>, NetError> {
    let response = client()
        .get(url)
        .header(ACCEPT, accept)
        .send()
        .await
        .map_err(|source| NetError::Request {
            url: url.to_string(),
            source,
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(NetError::Status {
            url: url.to_string(),
            status: status.as_u16(),
        });
    }
    // The size of an image is known before it arrives, so a wrong length is refused rather than
    // buffered first and reported later as a hash mismatch.
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

/// A release asset that is not an image: the produced units list is a signed binary of its own.
pub async fn get_bytes(url: &str, max: usize) -> Result<Vec<u8>, NetError> {
    get(url, "application/octet-stream", max).await
}
