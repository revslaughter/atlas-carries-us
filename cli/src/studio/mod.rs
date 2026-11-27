//! The studio: a JSON API under `/api/` beside the viewer. With it a person
//! edits the tour, the regions and the project text of atlas.yaml in the
//! browser, and runs the adapters again. `acu serve -c atlas.yaml` offers
//! the editor; `acu studio` also lets the person pick a folder or clone a
//! repository, proposes a first atlas.yaml, and extracts the map.
//! docs/studio.md describes each call.
//!
//! The API answers only requests for a loopback address that carry the
//! session token (see [`crate::secret`]). A request that changes something
//! must also carry the header `X-Acu-Studio: 1` and a JSON body, so a web
//! page on another site cannot send one.

pub mod edit;
pub mod folder;
pub mod setup;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::http::{header, HeaderMap, Method, StatusCode};
use serde_json::{json, Value};

use crate::mcp::Live;
use crate::secret::Secret;

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
    /// The folder that the job opened or cloned.
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
    /// The address that the server listens on: the one asked for, then the
    /// one that the system gave after the bind (which fills in port 0).
    addr: std::sync::RwLock<SocketAddr>,
    /// The folder that the folder picker may show; `acu studio` only.
    pub browse_root: Option<PathBuf>,
    /// The folder that receives clones; `acu studio` only.
    pub workspace: Option<PathBuf>,
    /// The last job, running or finished.
    pub job: Mutex<Option<Job>>,
    /// The process group of a running clone, which acu stops when it shuts
    /// down.
    child: Mutex<Option<u32>>,
    /// The session token that every API and MCP call must carry.
    pub secret: Secret,
    /// The forwarded `Host` values, each `name:port`, that `acu serve`
    /// admits for pages and data on a loopback address.
    allow_hosts: Vec<String>,
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
    /// Makes the shared state with a session token and no forwarded names.
    pub fn new(
        live: Arc<Live>,
        mode: Mode,
        addr: SocketAddr,
        browse_root: Option<PathBuf>,
        workspace: Option<PathBuf>,
        secret: Secret,
    ) -> Studio {
        Studio {
            live,
            mode,
            addr: std::sync::RwLock::new(addr),
            browse_root,
            workspace,
            job: Mutex::new(None),
            child: Mutex::new(None),
            secret,
            allow_hosts: Vec::new(),
        }
    }

    /// Sets the forwarded names that pages and data admit; each holds the
    /// form that [`parse_allow_host`] gives.
    pub fn with_allow_hosts(mut self, hosts: Vec<String>) -> Studio {
        self.allow_hosts = hosts;
        self
    }

    /// Returns the forwarded names that pages and data admit.
    pub fn allow_hosts(&self) -> &[String] {
        &self.allow_hosts
    }

    /// Returns the address that the server listens on.
    pub fn addr(&self) -> SocketAddr {
        match self.addr.read() {
            Ok(a) => *a,
            Err(p) => *p.into_inner(),
        }
    }

    /// Records the address that the bind gave, so the `Host` check names
    /// the real port.
    pub fn set_addr(&self, addr: SocketAddr) {
        match self.addr.write() {
            Ok(mut a) => *a = addr,
            Err(p) => *p.into_inner() = addr,
        }
    }

    /// Lists the `Host` values that the API and the MCP server accept: the
    /// loopback names and the listening address, each with the port.
    pub fn hosts(&self) -> Vec<String> {
        let addr = self.addr();
        let port = addr.port();
        let ip = addr.ip();
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

    /// Checks that a request's `Host` names this server, so a page that a
    /// foreign name points at this computer (DNS rebinding) reads nothing.
    pub fn check_host(&self, headers: &HeaderMap) -> Result<(), Reply> {
        let host = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if self.hosts().iter().any(|h| h.eq_ignore_ascii_case(host)) {
            return Ok(());
        }
        Err(fail(
            StatusCode::FORBIDDEN,
            "the server answers only requests for its own address; open it at the address that acu printed",
        ))
    }

    /// Says whether every page and the data need a known `Host`: always in
    /// the studio, and in `acu serve` on a loopback address. `acu serve` on
    /// an address that other computers reach answers any name.
    pub fn checks_pages(&self) -> bool {
        self.mode == Mode::Studio || self.addr().ip().is_loopback()
    }

    /// Checks the `Host` of a page or the data: the server's own names, or
    /// a name from `--allow-host`.
    pub fn check_page_host(&self, headers: &HeaderMap) -> Result<(), Reply> {
        if !self.checks_pages() || self.check_host(headers).is_ok() {
            return Ok(());
        }
        let host = headers
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if !host.is_empty() && self.allow_hosts.iter().any(|h| host_matches(h, host)) {
            return Ok(());
        }
        let hint = match self.mode {
            Mode::Serve => format!(
                "the server answers only requests for its own address; open it at the address that acu printed, or start acu serve again with --allow-host {} for a forwarded name",
                if host.is_empty() { "<name[:port]>" } else { host }
            ),
            Mode::Studio => "the server answers only requests for its own address; open it at the address that acu printed".to_string(),
        };
        Err(fail(StatusCode::FORBIDDEN, hint))
    }

    /// Checks that a request carries the session token, in the cookie or
    /// as a bearer token.
    pub fn check_token(&self, headers: &HeaderMap) -> Result<(), Reply> {
        if self.secret.admits(headers, self.addr().port()) {
            return Ok(());
        }
        Err(fail(
            StatusCode::UNAUTHORIZED,
            "the request lacks the session token; open the address with ?token= that acu printed, or send the header Authorization: Bearer <token>",
        ))
    }

    /// Checks that an MCP request carries the session token as a bearer
    /// token. The cookie does not count here.
    pub fn check_bearer(&self, headers: &HeaderMap) -> Result<(), Reply> {
        if self.secret.admits_bearer(headers) {
            return Ok(());
        }
        Err(fail(
            StatusCode::UNAUTHORIZED,
            "the MCP request lacks the session token; send the header Authorization: Bearer <token> with the token that acu printed",
        ))
    }

    /// Checks that a request's `Origin`, when it sends one, names this
    /// server.
    pub fn check_origin(&self, headers: &HeaderMap) -> Result<(), Reply> {
        let Some(origin) = headers.get(header::ORIGIN) else {
            return Ok(());
        };
        let o = origin.to_str().unwrap_or("");
        if self
            .hosts()
            .iter()
            .any(|h| o.eq_ignore_ascii_case(&format!("http://{h}")))
        {
            return Ok(());
        }
        Err(fail(
            StatusCode::FORBIDDEN,
            "the server refuses requests from other sites; open it at the address that acu printed",
        ))
    }

    /// Checks that a request may reach the API: its `Host` names this
    /// server, its `Origin` (when sent) names this server, it carries the
    /// session token, and a changing request carries the guard header and
    /// a JSON body.
    pub fn admit(&self, method: &Method, headers: &HeaderMap) -> Result<(), Reply> {
        self.check_host(headers)?;
        self.check_origin(headers)?;
        // After Origin, so a request that names another site meets 403.
        self.check_token(headers)?;
        if method != Method::GET && method != Method::HEAD {
            // Fetch Metadata: a browser marks a request from another site.
            let site = headers
                .get("sec-fetch-site")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("same-origin");
            if site != "same-origin" && site != "none" {
                return Err(fail(
                    StatusCode::FORBIDDEN,
                    "the studio refuses changing requests from other sites; make the change in the studio's own page",
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
                .is_some_and(|v| {
                    // The media type alone, before any parameter such as charset.
                    let media = v.split(';').next().unwrap_or("").trim();
                    media.eq_ignore_ascii_case("application/json")
                });
            if !json {
                return Err(fail(
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "send the request body as application/json",
                ));
            }
        }
        Ok(())
    }

    /// Records the process group of a running clone, or clears it.
    pub fn set_child(&self, pid: Option<u32>) {
        match self.child.lock() {
            Ok(mut c) => *c = pid,
            Err(p) => *p.into_inner() = pid,
        }
    }

    /// Stops a running clone and the processes it started, when acu shuts
    /// down.
    pub fn stop_children(&self) {
        let pid = match self.child.lock() {
            Ok(c) => *c,
            Err(p) => *p.into_inner(),
        };
        if let Some(pid) = pid {
            setup::kill_pgid(pid);
        }
    }

    /// Returns the last job.
    pub fn job(&self) -> Option<Job> {
        match self.job.lock() {
            Ok(j) => j.clone(),
            Err(p) => p.into_inner().clone(),
        }
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

/// Checks one `--allow-host` value and writes it in the form that a browser
/// sends as `Host`: a name or an address in lower case, with `:` and the
/// port when the address names one, such as `myhost.example:8080`,
/// `[fd00::1]:4321`, or `x-4321.app.github.dev` for an HTTPS forward on the
/// default port.
pub fn parse_allow_host(v: &str) -> Result<String, String> {
    let bad = || {
        format!(
            "--allow-host {v}: give a name with an optional port, such as myhost.example:4321, [fd00::1]:4321 or x-4321.app.github.dev"
        )
    };
    let v = v.trim().to_ascii_lowercase();
    // An IPv6 address sits in brackets; a port may follow the bracket.
    let (name, port) = match v.rfind(']') {
        Some(end) => match &v[end + 1..] {
            "" => (v.as_str(), None),
            rest => (&v[..end + 1], Some(rest.strip_prefix(':').ok_or_else(bad)?)),
        },
        None => match v.split_once(':') {
            Some((n, p)) => (n, Some(p)),
            None => (v.as_str(), None),
        },
    };
    let port = match port {
        None => None,
        Some(p) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
            match p.parse::<u16>() {
                Ok(n) if n > 0 => Some(n),
                _ => return Err(bad()),
            }
        }
        Some(_) => return Err(bad()),
    };
    let name = if let Some(inner) = name.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
        let ip: std::net::Ipv6Addr = inner.parse().map_err(|_| bad())?;
        match ip.to_ipv4_mapped() {
            // A browser writes an IPv4-mapped address in hex groups.
            Some(_) => {
                let g = ip.segments();
                format!("[::ffff:{:x}:{:x}]", g[6], g[7])
            }
            None => format!("[{ip}]"),
        }
    } else {
        let ok = !name.is_empty()
            && name.len() <= 253
            && !name.starts_with(['-', '.'])
            && !name.ends_with('-')
            && !name.contains("..")
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'));
        if !ok {
            return Err(bad());
        }
        name.to_string()
    };
    Ok(match port {
        Some(p) => format!("{name}:{p}"),
        None => name,
    })
}

/// Says whether a request's `Host` equals an `--allow-host` value. A
/// browser leaves out the default port (80 or 443), so `name:443` also
/// matches a bare `name`.
fn host_matches(allowed: &str, host: &str) -> bool {
    if allowed.eq_ignore_ascii_case(host) {
        return true;
    }
    [":80", ":443"].iter().any(|p| {
        allowed
            .strip_suffix(p)
            .is_some_and(|bare| bare.eq_ignore_ascii_case(host))
    })
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
        "extract" => blocking(move || edit::extract(&s)).await,
        "folders" => blocking(move || setup::folders(&s, &args)).await,
        "propose" => blocking(move || setup::propose(&s, &args)).await,
        "open" => blocking(move || setup::open(&s, &args)).await,
        "clone" => blocking(move || setup::clone(&s, &args)).await,
        _ => fail(
            StatusCode::NOT_FOUND,
            "the API has no such call; docs/studio.md lists the calls",
        ),
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

    /// The session token of every test studio.
    pub fn secret() -> Secret {
        Secret::fixed(crate::secret::tests::TEST, crate::secret::tests::COOKIE)
    }

    /// Makes a studio over a slot, for the address 127.0.0.1:4321.
    pub fn studio(live: Live, mode: Mode) -> Arc<Studio> {
        Arc::new(Studio::new(
            Arc::new(live),
            mode,
            "127.0.0.1:4321".parse().unwrap(),
            None,
            None,
            secret(),
        ))
    }

    /// Headers of a request from the studio's own page, with its cookie.
    pub fn own(post: bool) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_static("127.0.0.1:4321"));
        h.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("acu-token-4321={}", crate::secret::tests::COOKIE))
                .unwrap(),
        );
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
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/jsonp"),
        );
        assert_eq!(
            s.admit(&Method::POST, &h).unwrap_err().0,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        assert!(s.admit(&Method::POST, &h).is_ok());
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
    fn the_guard_follows_the_port_that_the_bind_gave() {
        let s = Studio::new(
            Arc::new(Live::empty()),
            Mode::Studio,
            "127.0.0.1:0".parse().unwrap(),
            None,
            None,
            secret(),
        );
        s.set_addr("127.0.0.1:4321".parse().unwrap());
        assert!(s.admit(&Method::GET, &own(false)).is_ok());
    }

    #[test]
    fn the_api_needs_the_session_token() {
        let s = studio(Live::empty(), Mode::Serve);
        let mut h = own(true);
        h.remove(header::COOKIE);
        let (code, v) = s.admit(&Method::GET, &h).unwrap_err();
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        assert!(v["error"].as_str().unwrap().contains("?token="));
        assert_eq!(
            s.admit(&Method::POST, &h).unwrap_err().0,
            StatusCode::UNAUTHORIZED
        );
        h.insert(
            header::COOKIE,
            HeaderValue::from_str(&format!("acu-token-4321={}", crate::secret::tests::TEST))
                .unwrap(),
        );
        assert_eq!(
            s.admit(&Method::GET, &h).unwrap_err().0,
            StatusCode::UNAUTHORIZED
        );
        h.remove(header::COOKIE);
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", crate::secret::tests::TEST)).unwrap(),
        );
        assert!(s.admit(&Method::POST, &h).is_ok());
        // A foreign Host or Origin fails first, with 403, whatever the token.
        h.remove(header::AUTHORIZATION);
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.example"),
        );
        assert_eq!(
            s.admit(&Method::POST, &h).unwrap_err().0,
            StatusCode::FORBIDDEN
        );
        h.insert(header::HOST, HeaderValue::from_static("evil.example:4321"));
        assert_eq!(
            s.admit(&Method::GET, &h).unwrap_err().0,
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn pages_admit_forwarded_names_only_in_acu_serve_on_loopback() {
        let host = |v: &'static str| {
            let mut h = HeaderMap::new();
            h.insert(header::HOST, HeaderValue::from_static(v));
            h
        };
        let serve = Studio::new(
            Arc::new(Live::empty()),
            Mode::Serve,
            "127.0.0.1:4321".parse().unwrap(),
            None,
            None,
            secret(),
        )
        .with_allow_hosts(vec!["myhost.example:8080".into()]);
        assert!(serve.checks_pages());
        assert!(serve.check_page_host(&host("127.0.0.1:4321")).is_ok());
        assert!(serve.check_page_host(&host("LOCALHOST:4321")).is_ok());
        assert!(serve.check_page_host(&host("myhost.example:8080")).is_ok());
        assert!(serve.check_page_host(&host("MyHost.Example:8080")).is_ok());
        let (code, v) = serve
            .check_page_host(&host("myhost.example:8081"))
            .unwrap_err();
        assert_eq!(code, StatusCode::FORBIDDEN);
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("--allow-host myhost.example:8081"));
        assert!(serve.check_page_host(&host("evil.example:4321")).is_err());
        assert!(serve.check_page_host(&HeaderMap::new()).is_err());
        // The API keeps its own names; a forwarded name reaches no call.
        assert_eq!(
            serve
                .check_host(&host("myhost.example:8080"))
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
        let open = Studio::new(
            Arc::new(Live::empty()),
            Mode::Serve,
            "0.0.0.0:4321".parse().unwrap(),
            None,
            None,
            secret(),
        );
        assert!(!open.checks_pages());
        assert!(open.check_page_host(&host("anything.example")).is_ok());
        let studio = studio(Live::empty(), Mode::Studio);
        assert!(studio.checks_pages());
        let (_, v) = studio
            .check_page_host(&host("evil.example:4321"))
            .unwrap_err();
        assert!(!v["error"].as_str().unwrap().contains("--allow-host"));
    }

    #[test]
    fn allow_host_takes_a_name_and_a_port() {
        assert_eq!(
            parse_allow_host("MyHost.Example:8080").unwrap(),
            "myhost.example:8080"
        );
        assert_eq!(parse_allow_host("10.0.0.5:4321").unwrap(), "10.0.0.5:4321");
        assert_eq!(
            parse_allow_host("[FD00:0::1]:4321").unwrap(),
            "[fd00::1]:4321"
        );
        assert_eq!(
            parse_allow_host("x-4321.app.github.dev:443").unwrap(),
            "x-4321.app.github.dev:443"
        );
        assert_eq!(
            parse_allow_host("X-4321.app.github.dev").unwrap(),
            "x-4321.app.github.dev"
        );
        assert_eq!(parse_allow_host("[::1]").unwrap(), "[::1]");
        assert_eq!(
            parse_allow_host("[::ffff:10.0.0.5]:80").unwrap(),
            "[::ffff:a00:5]:80"
        );
        assert_eq!(parse_allow_host("my_app:8080").unwrap(), "my_app:8080");
        assert!(host_matches("x.example:443", "x.example"));
        assert!(host_matches("x.example:80", "X.Example"));
        assert!(host_matches("x.example", "x.example"));
        assert!(!host_matches("x.example", "x.example:8080"));
        assert!(!host_matches("x.example:8080", "x.example"));
        for bad in [
            "myhost:",
            "[::1]x",
            "[::1]:",
            ":4321",
            "myhost:0",
            "myhost:65536",
            "myhost:+80",
            "my host:80",
            "-x:80",
            "x-:80",
            ".x:80",
            "a..b:80",
            "fd00::1:80",
            "[nope]:80",
            "http://x:80",
            "x/y:80",
        ] {
            assert!(parse_allow_host(bad).is_err(), "{bad}");
        }
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
