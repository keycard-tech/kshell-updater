use crate::net;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const EVENT: &str = "net://reachable";

const PROBE: &str = "https://github.com";
const PROBE_WAIT: Duration = Duration::from_secs(3);
const HOME_POLL: Duration = Duration::from_secs(20);
const AWAY_POLL: Duration = Duration::from_secs(3);

fn page() -> &'static OnceLock<AppHandle> {
    static PAGE: OnceLock<AppHandle> = OnceLock::new();
    &PAGE
}

fn reached() -> &'static AtomicBool {
    static REACHED: AtomicBool = AtomicBool::new(true);
    &REACHED
}

pub fn heard(online: bool) {
    if reached().swap(online, Ordering::Relaxed) == online {
        return;
    }
    log::info!(
        "[net] {}",
        if online {
            "github.com answers again"
        } else {
            "nothing answers"
        }
    );
    if let Some(app) = page().get() {
        let _ = app.emit(EVENT, online);
    }
}

async fn asks(url: &str) -> bool {
    net::client()
        .head(url)
        .timeout(PROBE_WAIT)
        .send()
        .await
        .is_ok()
}

async fn ask() -> bool {
    let online = asks(PROBE).await;
    heard(online);
    online
}

pub fn watch(app: AppHandle) {
    let _ = page().set(app);
    std::thread::spawn(|| loop {
        let online = tauri::async_runtime::block_on(ask());
        std::thread::sleep(if online { HOME_POLL } else { AWAY_POLL });
    });
}

#[tauri::command]
pub async fn network_status() -> bool {
    ask().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::block_on;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn any_answer_at_all_is_reaching_and_a_closed_port_is_not() {
        let serving = TcpListener::bind("127.0.0.1:0").expect("a port to answer on");
        let asked = format!("http://{}/", serving.local_addr().unwrap());
        std::thread::spawn(move || {
            for mut one in serving.incoming().flatten() {
                // Reading first, as `net`'s stand-in api does: answering a request that is still on
                // its way truncates the send, and the client's complaint is about that, not about
                // the answer.
                let _ = one.read(&mut [0_u8; 1024]);
                let _ = one.write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n");
            }
        });
        let gone = {
            let closed = TcpListener::bind("127.0.0.1:0").expect("a port to close");
            closed.local_addr().expect("the port it took")
        };

        assert!(block_on(asks(&asked)), "a 403 is an answer");
        assert!(
            !block_on(asks(&format!("http://{gone}/"))),
            "nothing answering is the thing being watched for",
        );
    }
}
