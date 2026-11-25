// Z-SHOT — Copyright (C) 2026 R. Trunk
// SPDX-License-Identifier: GPL-3.0-or-later

//! Update check: asks GitHub for the latest release of Z-SHOT and compares its version
//! with this build. It only informs; nothing is downloaded or installed.
//!
//! Runs on its own thread: once shortly after start, then at most once a day (the time
//! of the last check is kept in `update.json` next to the album, so restarts don't
//! cause extra requests). A manual check ("Jetzt prüfen") always asks right away.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const API_HOST: &str = "api.github.com";
const API_PATH: &str = "/repos/RayTrunk/Z-SHOT/releases/latest";
/// Release pages outside this prefix are never opened.
pub const RELEASES_URL: &str = "https://github.com/RayTrunk/Z-SHOT/releases";
const CHECK_INTERVAL_SECS: i64 = 24 * 60 * 60;
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
const WAKE_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Not checked yet (or checks are switched off).
    Unknown,
    Checking,
    UpToDate,
    /// GitHub has no published release (yet), or the repository is not public.
    NoRelease,
    Available { version: String, url: String },
    Failed(String),
}

/// Handle owned by the UI.
pub struct Checker {
    status: Arc<Mutex<Status>>,
    enabled: Arc<AtomicBool>,
    check_now: Sender<()>,
}

impl Checker {
    /// Starts the checker thread. `repaint` is called whenever the status changes.
    pub fn start(dir: PathBuf, enabled: bool, repaint: impl Fn() + Send + 'static) -> Self {
        let cache_path = dir.join("update.json");
        let cache = Cache::load(&cache_path);
        let status = Arc::new(Mutex::new(cache.status()));
        let enabled = Arc::new(AtomicBool::new(enabled));
        let (tx, rx) = mpsc::channel::<()>();

        let thread_status = Arc::clone(&status);
        let thread_enabled = Arc::clone(&enabled);
        std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let mut cache = cache;
                let mut wait = FIRST_CHECK_DELAY;
                loop {
                    let manual = match rx.recv_timeout(wait) {
                        Ok(()) => true,
                        Err(RecvTimeoutError::Timeout) => false,
                        Err(RecvTimeoutError::Disconnected) => return,
                    };
                    wait = WAKE_INTERVAL;
                    let due = now_secs() - cache.last_check >= CHECK_INTERVAL_SECS;
                    if !manual && !(thread_enabled.load(Ordering::Relaxed) && due) {
                        continue;
                    }
                    set(&thread_status, Status::Checking, &repaint);
                    let result = fetch_latest();
                    cache.last_check = now_secs();
                    let status = match result {
                        Ok(Some((version, url))) => {
                            cache.latest = Some(version);
                            cache.url = Some(url);
                            cache.status()
                        }
                        Ok(None) => {
                            cache.latest = None;
                            cache.url = None;
                            Status::NoRelease
                        }
                        Err(e) => {
                            crate::log::log(format!("Update-Prüfung fehlgeschlagen: {e}"));
                            // Try again at the next hourly wake-up, not only tomorrow.
                            cache.last_check = 0;
                            Status::Failed(e)
                        }
                    };
                    cache.save(&cache_path);
                    set(&thread_status, status, &repaint);
                }
            })
            .expect("update thread");

        Self { status, enabled, check_now: tx }
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or(Status::Unknown)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn check_now(&self) {
        let _ = self.check_now.send(());
    }
}

fn set(status: &Mutex<Status>, value: Status, repaint: &impl Fn()) {
    if let Ok(mut s) = status.lock() {
        *s = value;
    }
    repaint();
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// What the last check found, so a restart shows it without asking GitHub again.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Cache {
    last_check: i64,
    latest: Option<String>,
    url: Option<String>,
}

impl Cache {
    fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    fn status(&self) -> Status {
        match (&self.latest, &self.url) {
            (Some(v), Some(url)) if is_newer(v, env!("CARGO_PKG_VERSION")) => {
                Status::Available { version: v.clone(), url: url.clone() }
            }
            (Some(_), _) => Status::UpToDate,
            _ => Status::Unknown,
        }
    }
}

/// `"v1.2.3"`, `"1.2"` or `"1.2.3-beta"` → `(1, 2, 3)`; anything else → `None`.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().unwrap_or(Some(0))?;
    let patch = parts.next().unwrap_or(Some(0))?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Parses the GitHub answer: `Some((version, page))` for a usable release.
fn parse_release(body: &str) -> Result<Option<(String, String)>, String> {
    let release: Release = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let Some((a, b, c)) = parse_version(&release.tag_name) else {
        return Err(format!("unbekannte Version: {}", release.tag_name));
    };
    let url = if release.html_url.starts_with(RELEASES_URL) {
        release.html_url
    } else {
        RELEASES_URL.to_string()
    };
    Ok(Some((format!("{a}.{b}.{c}"), url)))
}

fn fetch_latest() -> Result<Option<(String, String)>, String> {
    let (code, body) = http_get(API_HOST, API_PATH)?;
    match code {
        200 => parse_release(&body),
        // No release yet, or the repository is private.
        404 => Ok(None),
        403 | 429 => Err(format!("GitHub-Limit erreicht (HTTP {code})")),
        other => Err(format!("HTTP {other}")),
    }
}

// ---------------------------------------------------------------------------
// HTTPS GET via WinHTTP (part of Windows, uses the system proxy and certificates)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn http_get(host: &str, path: &str) -> Result<(u32, String), String> {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Networking::WinHttp::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }
    let fail = |what: &str| format!("{what} (Fehler {})", unsafe { GetLastError() });

    let agent = wide(concat!("Z-SHOT/", env!("CARGO_PKG_VERSION")));
    let host_w = wide(host);
    let path_w = wide(path);
    let verb = wide("GET");
    let headers = wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n");

    unsafe {
        let session = Handle(WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        ));
        if session.0.is_null() {
            return Err(fail("WinHttpOpen"));
        }
        WinHttpSetTimeouts(session.0, 10_000, 10_000, 10_000, 15_000);
        let connect = Handle(WinHttpConnect(session.0, host_w.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT as u16, 0));
        if connect.0.is_null() {
            return Err(fail("Verbindung"));
        }
        let request = Handle(WinHttpOpenRequest(
            connect.0,
            verb.as_ptr(),
            path_w.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err(fail("Anfrage"));
        }
        if WinHttpSendRequest(request.0, headers.as_ptr(), u32::MAX, std::ptr::null(), 0, 0, 0) == 0 {
            return Err(fail("Keine Verbindung zu GitHub"));
        }
        if WinHttpReceiveResponse(request.0, std::ptr::null_mut()) == 0 {
            return Err(fail("Keine Antwort von GitHub"));
        }
        let mut code: u32 = 0;
        let mut len = std::mem::size_of::<u32>() as u32;
        if WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            std::ptr::null(),
            &mut code as *mut u32 as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err(fail("Statuscode"));
        }
        let mut body = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let mut read: u32 = 0;
            if WinHttpReadData(request.0, buf.as_mut_ptr() as *mut c_void, buf.len() as u32, &mut read) == 0 {
                return Err(fail("Lesen"));
            }
            if read == 0 || body.len() > 1 << 20 {
                break;
            }
            body.extend_from_slice(&buf[..read as usize]);
        }
        Ok((code, String::from_utf8_lossy(&body).into_owned()))
    }
}

// ---------------------------------------------------------------------------
// Linux: HTTPS GET via ureq (rustls with the Mozilla root certificates)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn http_get(host: &str, path: &str) -> Result<(u32, String), String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .user_agent(concat!("Z-SHOT/", env!("CARGO_PKG_VERSION")))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(&format!("https://{host}{path}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| format!("Keine Verbindung zu GitHub ({e})"))?;
    let code = u32::from(response.status().as_u16());
    let body = response
        .body_mut()
        .with_config()
        .limit(1 << 20)
        .read_to_string()
        .map_err(|e| format!("Lesen ({e})"))?;
    Ok((code, body))
}

#[cfg(not(any(windows, target_os = "linux")))]
fn http_get(_host: &str, _path: &str) -> Result<(u32, String), String> {
    Err("nicht unterstützt".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(is_newer("v0.9.1-beta", "0.9.0"));
        assert!(!is_newer("v0.9.0", "0.9.0"));
        assert!(!is_newer("0.8.12", "0.9.0"));
        assert!(!is_newer("nightly", "0.9.0"));
        assert_eq!(parse_version("1.2.3.4"), None);
    }

    #[test]
    fn release_json_is_read_and_foreign_links_are_replaced() {
        let ok = r#"{"tag_name":"v1.0.0","html_url":"https://github.com/RayTrunk/Z-SHOT/releases/tag/v1.0.0","draft":false,"prerelease":false}"#;
        assert_eq!(
            parse_release(ok).unwrap(),
            Some(("1.0.0".into(), "https://github.com/RayTrunk/Z-SHOT/releases/tag/v1.0.0".into()))
        );
        let foreign = r#"{"tag_name":"v1.0.0","html_url":"https://example.com/evil"}"#;
        assert_eq!(parse_release(foreign).unwrap().unwrap().1, RELEASES_URL);
        let pre = r#"{"tag_name":"v2.0.0","html_url":"x","prerelease":true}"#;
        assert_eq!(parse_release(pre).unwrap(), None);
    }

    /// Needs the network: `cargo test -- --ignored live_`
    #[test]
    #[ignore]
    fn live_github_request() {
        let (code, body) = http_get(API_HOST, "/repos/rust-lang/rust/releases/latest").unwrap();
        assert_eq!(code, 200, "{body}");
        let release: Release = serde_json::from_str(&body).unwrap();
        assert!(parse_version(&release.tag_name).is_some(), "{}", release.tag_name);
        println!("rust-lang/rust: {}", release.tag_name);
        println!("Z-SHOT: {:?}", fetch_latest());
    }
}
