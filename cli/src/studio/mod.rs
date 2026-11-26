//! The studio: a JSON API under `/api/` beside the viewer. With it a person
//! edits the tour, the regions and the project text of atlas.yaml in the
//! browser, and runs the adapters again. `acu serve -c atlas.yaml` offers
//! the editor; `acu studio` also lets the person pick a folder or clone a
//! repository, proposes a first atlas.yaml, and extracts the map.
//! docs/studio.md describes each call.
//!
//! The API answers only requests for a loopback address. A request that
//! changes something must carry the header `X-Acu-Studio: 1` and a JSON
//! body, so a web page on another site cannot send one.

pub mod edit;
pub mod setup;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::http::{header, HeaderMap, Method, StatusCode};
use serde_json::{json, Value};

use crate::mcp::Live;

/// The header that every changing request carries. A browser sends a
/// custom header across sites only after a preflight, which acu never
/// answers, so a page elsewhere cannot set it.
pub const GUARD_HEADER: &str = "x-acu-studio";

/// The largest request body that the API reads.
pub const MAX_BODY: usize = 4 << 20;

/// Which command runs the server.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// `acu serve`: one project, fixed at the start.
    Serve,
    /// `acu studio`: the person picks the project in the browser.
    Studio,
}

impl Mode {
    /// The mode's name in the API.
    fn name(self) -> &'static str {
        match self {
            Mode::Serve => "serve",
            Mode::Studio => "studio",
        }
    }
}

/// One background job: an extraction, the opening of a project, or a
/// clone.
#[derive(Clone, Debug)]
pub struct Job {
    /// The job's number, from 1.
    pub id: u64,
    /// What the job does: extract, open or clone.
    pub kind: &'static str,
    /// running, done or failed.
    pub state: &'static str,
    /// The lines of progress so far, at most [`MAX_LOG`].
    pub log: Vec<String>,
    /// Why the job failed.
    pub error: Option<String>,
    /// The folder that a clone wrote.
    pub path: Option<String>,
}

/// The most lines of progress that a job keeps; older lines go.
pub const MAX_LOG: usize = 400;

impl Job {
    /// The job as JSON for the API.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "kind": self.kind,
            "state": self.state,
            "log": self.log,
            "error": self.error,
            "path": self.path,
        })
    }
}

/// The state that the API shares across requests.
pub struct Studio {
    /// The map on show.
    pub live: Arc<Live>,
    /// The command that runs the server.
    pub mode: Mode,
    /// The address that the server listens on.
    pub addr: SocketAddr,
    /// The folder that the folder picker may show; `acu studio` only.
    pub browse_root: Option<PathBuf>,
    /// The folder that receives clones; `acu studio` only.
    pub workspace: Option<PathBuf>,
    /// The last job, running or finished.
    pub job: Mutex<Option<Job>>,
}

/// An answer of the API: a status and a JSON body.
pub type Reply = (StatusCode, Value);

/// Makes an error answer whose text starts with `acu:`.
pub fn fail(status: StatusCode, msg: impl Into<String>) -> Reply {
    let mut m: String = msg.into();
    if !m.starts_with("acu:") {
        m = format!("acu: {m}");
    }
    (status, json!({ "error": m }))
}

impl Studio {
    /// Makes the shared state.
    pub fn new(
        live: Arc<Live>,
        mode: Mode,
        addr: SocketAddr,
        browse_root: Option<PathBuf>,
        workspace: Option<PathBuf>,
    ) -> Studio {
        Studio {
            live,
            mode,
            addr,
            browse_root,
            workspace,
            job: Mutex::new(None),
        }
    }

    /// Lists the `Host` values that the API accepts: the loopback names and
    /// the listening address, each with the port.
    fn hosts(&self) -> Vec<String> {
        let port = self.addr.port();
        let ip = self.addr.ip();
        let mut v = vec![
            format!("localhost:{port}"),
            format!("127.0.0.1:{port}"),
            format!("[::1]:{port}"),
        ];
        let own = if ip.is_ipv6() {
            format!("[{ip}]:{port}")
        } else {
            format!("{ip}:{port}")
        };
        if !v.contains(&own) {
            v.push(own);
        }
        v
    }

    /// Checks that a request may reach the API: its `Host` names this
    /// server, its `Origin` (when sent) names this server, and a changing
    /// request carries the guard header and a JSON body.
    pub fn admit(&self, method: &Method, headers: &HeaderMap) -> Result<(), Reply> {
        let hosts = self.hosts();
        let host = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if !hosts.iter().any(|h| h.eq_ignore_ascii_case(host)) {
            return Err(fail(
                StatusCode::FORBIDDEN,
                "the studio answers only requests for its own loopback address; open it at the address that acu printed",
            ));
        }
        if let Some(origin) = headers.get(header::ORIGIN) {
            let o = origin.to_str().unwrap_or("");
            if !hosts
                .iter()
                .any(|h| o.eq_ignore_ascii_case(&format!("http://{h}")))
            {
                return Err(fail(
                    StatusCode::FORBIDDEN,
                    "the studio refuses requests from other sites",
                ));
            }
        }
        if method != Method::GET && method != Method::HEAD {
            // Fetch Metadata: a browser marks a request from another site.
            let site = headers
                .get("sec-fetch-site")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("same-origin");
            if site != "same-origin" && site != "none" {
                return Err(fail(
                    StatusCode::FORBIDDEN,
                    "the studio refuses changing requests from other sites",
                ));
            }
            if headers.get(header::ORIGIN).is_none() {
                return Err(fail(
                    StatusCode::FORBIDDEN,
                    "a changing request needs an Origin header that names the studio",
                ));
            }
            if headers.get(GUARD_HEADER).and_then(|v| v.to_str().ok()) != Some("1") {
                return Err(fail(
                    StatusCode::FORBIDDEN,
                    "a changing request needs the header X-Acu-Studio: 1",
                ));
            }
            let json = headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("application/json"));
            if !json {
                return Err(fail(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "send the request body as application/json",
                ));
            }
        }
        Ok(())
    }

    /// Returns the last job.
    pub fn job(&self) -> Option<Job> {
        self.job.lock().ok().and_then(|j| j.clone())
    }

    /// Starts a job on its own thread, unless one runs already. `work`
    /// gets a function that logs one line, and returns the folder that a
    /// clone wrote, or nothing.
    pub fn start_job(
        self: &Arc<Self>,
        kind: &'static str,
        work: impl FnOnce(&Studio, &(dyn Fn(&str) + Sync)) -> Result<Option<String>, String>
            + Send
            + 'static,
    ) -> Reply {
        let id = {
            let Ok(mut slot) = self.job.lock() else {
                return fail(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "a lock broke; restart acu",
                );
            };
            if slot.as_ref().is_some_and(|j| j.state == "running") {
                return fail(
                    StatusCode::CONFLICT,
                    "another job runs now; wait until it ends, then try again",
                );
            }
            let id = slot.as_ref().map_or(1, |j| j.id + 1);
            *slot = Some(Job {
                id,
                kind,
                state: "running",
                log: Vec::new(),
                error: None,
                path: None,
            });
            id
        };
        let me = self.clone();
        std::thread::spawn(move || {
            let log = |line: &str| {
                if let Ok(mut slot) = me.job.lock() {
                    if let Some(j) = slot.as_mut().filter(|j| j.id == id) {
                        j.log.push(line.to_string());
                        if j.log.len() > MAX_LOG {
                            let cut = j.log.len() - MAX_LOG;
                            j.log.drain(..cut);
                        }
                    }
                }
            };
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&me, &log)))
                .unwrap_or_else(|_| Err("acu: the job stopped after an internal error".into()));
            if let Ok(mut slot) = me.job.lock() {
                if let Some(j) = slot.as_mut().filter(|j| j.id == id) {
                    match out {
                        Ok(path) => {
                            j.state = "done";
                            j.path = path;
                        }
                        Err(e) => {
                            eprintln!("{e}");
                            j.state = "failed";
                            j.error = Some(e);
                        }
                    }
                }
            }
        });
        let job = self.job().map(|j| j.to_json());
        (StatusCode::ACCEPTED, json!({ "job": job }))
    }

    /// Describes the server for the editor: the mode, whether writes work,
    /// the open project, the last job, and what the setup screen offers.
    pub fn state(&self) -> Value {
        let atlas = self.live.get().ok();
        let project = atlas.as_ref().and_then(|a| {
            let path = a.config_path()?;
            let name = a
                .snapshot()
                .ok()
                .and_then(|s| s.doc["project"]["name"].as_str().map(String::from));
            Some(json!({
                "config": path.display().to_string(),
                "dir": path.parent().map(|d| d.display().to_string()),
                "name": name,
            }))
        });
        let writable = atlas.as_ref().is_some_and(|a| a.writable());
        let why = match &atlas {
            Some(a) => a.why_read_only().map(String::from),
            None => Some(crate::mcp::atlas::NO_PROJECT.to_string()),
        };
        json!({
            "mode": self.mode.name(),
            "map": atlas.is_some(),
            "writable": writable,
            "why": why,
            "project": project,
            "job": self.job().map(|j| j.to_json()),
            "setup": self.mode == Mode::Studio,
            "browseRoot": self.browse_root.as_ref().map(|p| p.display().to_string()),
            "workspace": self.workspace.as_ref().map(|p| p.display().to_string()),
        })
    }
}

/// Parses a request body as a JSON object.
pub fn body(bytes: &[u8]) -> Result<serde_json::Map<String, Value>, Reply> {
    if bytes.len() > MAX_BODY {
        return Err(fail(
            StatusCode::PAYLOAD_TOO_LARGE,
            "the request body exceeds 4 MB",
        ));
    }
    if bytes.is_empty() {
        return Ok(serde_json::Map::new());
    }
    match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(fail(
            StatusCode::BAD_REQUEST,
            "send a JSON object as the request body",
        )),
        Err(e) => Err(fail(
            StatusCode::BAD_REQUEST,
            format!("the request body holds JSON that does not parse: {e}"),
        )),
    }
}

/// Answers one API request. `path` comes without the leading `/api/`.
pub async fn handle(
    studio: Arc<Studio>,
    method: Method,
    path: &str,
    headers: &HeaderMap,
    bytes: &[u8],
) -> Reply {
    if let Err(r) = studio.admit(&method, headers) {
        return r;
    }
    let get = method == Method::GET;
    let known = match path {
        "state" | "config" | "job" => get,
        "tour" | "regions" | "project" | "match" | "extract" | "folders" | "propose" | "open"
        | "clone" => method == Method::POST,
        _ => false,
    };
    if !known {
        return fail(
            StatusCode::NOT_FOUND,
            format!("the API has no {method} /api/{path}; docs/studio.md lists the calls"),
        );
    }
    if get {
        return match path {
            "state" => (StatusCode::OK, studio.state()),
            "job" => (
                StatusCode::OK,
                json!({ "job": studio.job().map(|j| j.to_json()) }),
            ),
            _ => {
                let s = studio.clone();
                blocking(move || edit::config(&s)).await
            }
        };
    }
    let args = match body(bytes) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let s = studio.clone();
    match path {
        "tour" => blocking(move || edit::save_tour(&s, &args)).await,
        "regions" => blocking(move || edit::save_regions(&s, &args)).await,
        "project" => blocking(move || edit::save_project(&s, &args)).await,
        "match" => blocking(move || edit::matching(&s, &args)).await,
        "extract" => edit::extract(&studio),
        "folders" => blocking(move || setup::folders(&s, &args)).await,
        "propose" => blocking(move || setup::propose(&s, &args)).await,
        "open" => setup::open(&studio, &args),
        "clone" => setup::clone(&studio, &args),
        _ => fail(StatusCode::NOT_FOUND, "no such call"),
    }
}

/// Runs a call that reads files or checks a map on a thread that may
/// block.
async fn blocking(f: impl FnOnce() -> Reply + Send + 'static) -> Reply {
    tokio::task::spawn_blocking(f).await.unwrap_or_else(|e| {
        fail(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("a call stopped: {e}"),
        )
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::http::HeaderValue;

    /// Makes a studio over a slot, for the address 127.0.0.1:4321.
    pub fn studio(live: Live, mode: Mode) -> Arc<Studio> {
        Arc::new(Studio::new(
            Arc::new(live),
            mode,
            "127.0.0.1:4321".parse().unwrap(),
            None,
            None,
        ))
    }

    /// Headers of a request from the studio's own page.
    pub fn own(post: bool) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_static("127.0.0.1:4321"));
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://127.0.0.1:4321"),
        );
        if post {
            h.insert(GUARD_HEADER, HeaderValue::from_static("1"));
            h.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
        }
        h
    }

    #[test]
    fn the_guard_admits_only_the_own_page() {
        let s = studio(Live::empty(), Mode::Studio);
        assert!(s.admit(&Method::GET, &own(false)).is_ok());
        assert!(s.admit(&Method::POST, &own(true)).is_ok());
        let mut h = own(true);
        h.insert(header::HOST, HeaderValue::from_static("evil.example:4321"));
        assert_eq!(
            s.admit(&Method::POST, &h).unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        let mut h = own(true);
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.example"),
        );
        assert_eq!(
            s.admit(&Method::GET, &h).unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        let mut h = own(true);
        h.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert_eq!(
            s.admit(&Method::POST, &h).unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        let mut h = own(true);
        h.remove(header::ORIGIN);
        assert_eq!(
            s.admit(&Method::POST, &h).unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        let mut h = own(true);
        h.remove(GUARD_HEADER);
        assert_eq!(
            s.admit(&Method::POST, &h).unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        let mut h = own(true);
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));
        assert_eq!(
            s.admit(&Method::POST, &h).unwrap_err().0,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        let mut h = own(false);
        h.insert(header::HOST, HeaderValue::from_static("localhost:4321"));
        h.remove(header::ORIGIN);
        assert!(s.admit(&Method::GET, &h).is_ok());
    }

    #[test]
    fn bodies_must_hold_an_object() {
        assert!(body(b"").unwrap().is_empty());
        assert!(body(b"{\"a\":1}").is_ok());
        assert_eq!(body(b"[1]").unwrap_err().0, StatusCode::BAD_REQUEST);
        assert_eq!(body(b"{").unwrap_err().0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_calls_say_where_the_list_lives() {
        let s = studio(Live::empty(), Mode::Serve);
        let (code, v) = handle(s.clone(), Method::GET, "nothing", &own(false), b"").await;
        assert_eq!(code, StatusCode::NOT_FOUND);
        assert!(v["error"].as_str().unwrap().contains("docs/studio.md"));
        let (code, v) = handle(s, Method::GET, "state", &own(false), b"").await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(v["mode"], "serve");
        assert_eq!(v["map"], false);
    }
}
