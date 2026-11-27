//! The static site: the viewer that the build embeds, plus data/atlas.json.
//! `build` writes it to a folder; `serve` and `studio` answer HTTP requests
//! from the live map, mount the MCP server at `/mcp`, and answer the
//! studio's API under `/api/`. Both calls need the session token of
//! [`crate::secret`].

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::body::{to_bytes, Body};
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use include_dir::{include_dir, Dir};

use crate::mcp::Atlas;
use crate::studio::{self, Studio};

/// The built viewer, embedded in acu at compile time.
static VIEWER: Dir<'_> = include_dir!("$OUT_DIR/viewer");

/// Where the site keeps the data, relative to its root.
pub const DATA_PATH: &str = "data/atlas.json";

/// Writes the viewer and the data into `out`.
pub fn build(out: &Path, data: &[u8]) -> Result<usize> {
    std::fs::create_dir_all(out).with_context(|| format!("create {}", out.display()))?;
    let mut n = 0;
    fn walk(dir: &Dir<'_>, out: &Path, n: &mut usize) -> Result<()> {
        for f in dir.files() {
            let target = out.join(f.path());
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&target, f.contents())
                .with_context(|| format!("write {}", target.display()))?;
            *n += 1;
        }
        for d in dir.dirs() {
            walk(d, out, n)?;
        }
        Ok(())
    }
    walk(&VIEWER, out, &mut n)?;
    let target = out.join(DATA_PATH);
    std::fs::create_dir_all(target.parent().unwrap())?;
    std::fs::write(&target, data).with_context(|| format!("write {}", target.display()))?;
    Ok(n + 1)
}

/// Serves the viewer, the live map's data, the studio's API at `/api/` and
/// the MCP server at `/mcp` on an address until Ctrl-C. The data comes from
/// the live map on each request, so a save in the editor, a draft from an
/// MCP write tool, or a fresh `acu extract` of a served atlas.json shows on
/// reload.
pub async fn serve(studio: Arc<Studio>) -> Result<()> {
    let addr = studio.addr();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("listen on {addr}"))?;
    let addr = listener.local_addr().unwrap_or(addr);
    studio.set_addr(addr);
    let writes = match studio.live.get() {
        Ok(a) if a.writable() => "writes on",
        Ok(_) => "writes off",
        Err(_) => "no project open yet",
    };
    let what = match studio.mode {
        studio::Mode::Studio => "the studio",
        studio::Mode::Serve => "the map",
    };
    let stopper = studio.clone();
    let (app, stop_mcp) = app(studio, addr);
    for line in start_lines(&stopper, addr, what, writes) {
        eprintln!("{line}");
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            stop_mcp();
            stopper.stop_children();
        })
        .await?;
    Ok(())
}

/// Makes the server's routes: the MCP server at `/mcp` behind the token
/// check, and [`route`] for every other address. Also returns the function
/// that stops the MCP server's sessions.
fn app(studio: Arc<Studio>, addr: SocketAddr) -> (Router, impl FnOnce() + Send + 'static) {
    let (mcp, stop_mcp) = crate::mcp::http_service(studio.live.clone(), addr);
    let guard = studio.clone();
    // The MCP library checks Host and Origin itself; acu adds the token,
    // as a bearer token only.
    let mcp = Router::new()
        .fallback_service(mcp)
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            let studio = guard.clone();
            async move {
                match studio.check_bearer(req.headers()) {
                    Ok(()) => next.run(req).await,
                    Err(r) => mcp_refusal(r, req.headers()),
                }
            }
        }));
    let app = Router::new()
        .nest("/mcp", mcp)
        .fallback(move |req: Request| {
            let studio = studio.clone();
            async move { route(req, studio).await }
        });
    (app, stop_mcp)
}

/// Makes the 401 answer of the MCP server. `WWW-Authenticate` names the
/// bearer scheme and no authorization server, so a client asks its user
/// for the token instead of starting a sign-in. A sent but wrong token
/// adds `error="invalid_token"`.
fn mcp_refusal(r: studio::Reply, headers: &header::HeaderMap) -> Response {
    let sent = crate::secret::bearers(headers).next().is_some();
    let mut res = json_reply(r);
    let value = if sent {
        "Bearer realm=\"acu\", error=\"invalid_token\""
    } else {
        "Bearer realm=\"acu\""
    };
    res.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        header::HeaderValue::from_static(value),
    );
    res
}

/// Writes the lines that acu prints at start: the address with the token,
/// how an MCP client sends the token, and, for `acu serve` on a loopback
/// address, how to admit a forwarded name.
fn start_lines(studio: &Studio, addr: SocketAddr, what: &str, writes: &str) -> Vec<String> {
    let token = studio.secret.token();
    let mut v = vec![
        format!("acu: serving {what} at http://{addr}/?token={token} and MCP at http://{addr}/mcp, {writes}  (Ctrl-C stops)"),
        format!("acu: open the address above, with its token; an MCP client sends the header \"Authorization: Bearer {token}\"; set {} to keep one token across restarts", crate::secret::ENV),
    ];
    if studio.mode == studio::Mode::Serve && studio.checks_pages() {
        let names = if studio.allow_hosts().is_empty() {
            String::new()
        } else {
            format!(" and {}", studio.allow_hosts().join(", "))
        };
        v.push(format!("acu: pages answer only the loopback names of port {}{names}; for a forwarded name, start acu serve again with --allow-host <name:port>", addr.port()));
    }
    v
}

/// Sends a request under `/api/` to the studio, and any other to
/// [`respond`]. A page address with `?token=` trades the token for a
/// cookie first.
async fn route(req: Request, studio: Arc<Studio>) -> Response {
    let path = req.uri().path().to_string();
    // Every page needs a known Host in the studio, and in acu serve on a
    // loopback address, so a page that DNS rebinding points here reads
    // nothing. acu serve on another address answers any name. For an API
    // call, the API's guard below checks the Host against its own names.
    let api = path.starts_with("/api/");
    if !api {
        if let Err(r) = studio.check_page_host(req.headers()) {
            return json_reply(r);
        }
        if let Some(r) = trade_token(&studio, &path, req.uri().query()) {
            return r;
        }
        // The studio's data holds the open project, so it needs the token.
        if studio.mode == studio::Mode::Studio && path.trim_start_matches('/') == DATA_PATH {
            if let Err(r) = studio.check_token(req.headers()) {
                return json_reply(r);
            }
        }
    }
    let Some(call) = path.strip_prefix("/api/") else {
        let atlas = studio.live.get().ok();
        // Only a page with the token learns of the API, so a page without
        // it never calls the API and logs no refused request.
        let api = studio.check_token(req.headers()).is_ok();
        return respond(&path, atlas.as_deref(), api);
    };
    // Refuse a foreign request before reading its body.
    if let Err(r) = studio.admit(req.method(), req.headers()) {
        return json_reply(r);
    }
    let (parts, body) = req.into_parts();
    let bytes = match to_bytes(body, studio::MAX_BODY).await {
        Ok(b) => b,
        Err(_) => {
            return json_reply(studio::fail(
                StatusCode::PAYLOAD_TOO_LARGE,
                "the request body exceeds 4 MB",
            ))
        }
    };
    let r = studio::handle(studio, parts.method, call, &parts.headers, &bytes).await;
    json_reply(r)
}

/// Trades a `?token=` in a page address for the session cookie: a good
/// token gets the cookie and a redirect to the address without it; a wrong
/// one gets 401. An address without the token gets `None`.
fn trade_token(studio: &Studio, path: &str, query: Option<&str>) -> Option<Response> {
    let (token, rest) = crate::secret::take_token(query?);
    let token = token?;
    if !studio.secret.matches(token) {
        return Some(json_reply(studio::fail(
            StatusCode::UNAUTHORIZED,
            "the address holds a wrong or old token; open the address that acu printed last",
        )));
    }
    // Only a path on this server: "//host" or a backslash could send the
    // browser to another site.
    let path = if path.starts_with("//") || path.contains('\\') {
        "/"
    } else {
        path
    };
    let to = if rest.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{rest}")
    };
    let mut r = reply(StatusCode::SEE_OTHER, "text/plain", Vec::new());
    let h = r.headers_mut();
    if let (Ok(loc), Ok(cookie)) = (
        header::HeaderValue::from_str(&to),
        header::HeaderValue::from_str(&studio.secret.set_cookie(studio.addr().port())),
    ) {
        h.insert(header::LOCATION, loc);
        h.insert(header::SET_COOKIE, cookie);
    }
    Some(r)
}

/// Makes a JSON response that no browser caches.
fn json_reply((status, body): studio::Reply) -> Response {
    reply(
        status,
        "application/json",
        serde_json::to_vec(&body).unwrap_or_default(),
    )
}

/// Answers one request: the data at `DATA_PATH`, else a file of the viewer.
/// With `api`, the viewer's page carries [`API_MARK`].
fn respond(path: &str, atlas: Option<&Atlas>, api: bool) -> Response {
    let path = path.trim_start_matches('/');
    if path == DATA_PATH {
        let bytes = match atlas {
            Some(a) => a.snapshot().map(|s| s.bytes.clone()),
            None => Err("no project open yet".to_string()),
        };
        return match bytes {
            Ok(b) => reply(StatusCode::OK, "application/json", b),
            Err(e) => reply(
                StatusCode::NOT_FOUND,
                "text/plain",
                format!("read the data: {e}").into_bytes(),
            ),
        };
    }
    let path = if path.is_empty() { "index.html" } else { path };
    match VIEWER.get_file(path) {
        Some(f) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let body = if path == "index.html" && api {
                mark_api(f.contents())
            } else {
                f.contents().to_vec()
            };
            reply(StatusCode::OK, mime.as_ref(), body)
        }
        None => reply(
            StatusCode::NOT_FOUND,
            "text/plain",
            b"You find nothing at this address.".to_vec(),
        ),
    }
}

/// The tag that tells the viewer a server with the studio's API serves the
/// page, and that the request carried the session token. A static site from
/// `acu build`, and a page without the token, lack it, so their viewer never
/// calls the API and never logs a failed request.
pub const API_MARK: &str = "<meta name=\"acu-api\" content=\"1\" />";

/// Adds [`API_MARK`] to the viewer's page, right after `<head>`.
fn mark_api(page: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(page);
    text.replacen("<head>", &format!("<head>{API_MARK}"), 1)
        .into_bytes()
}

/// Makes a response that no browser caches, so a reload shows fresh data.
/// No other site may frame it or read its type differently.
fn reply(status: StatusCode, mime: &str, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::REFERRER_POLICY, "no-referrer")
        .header(header::CONTENT_SECURITY_POLICY, "frame-ancestors 'none'")
        .body(Body::from(body))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::Live;
    use crate::secret::tests::TEST;

    /// Makes a server of the given mode with no project, on `addr`.
    fn server(mode: studio::Mode, addr: &str) -> Arc<Studio> {
        Arc::new(Studio::new(
            Arc::new(Live::empty()),
            mode,
            addr.parse().unwrap(),
            None,
            None,
            studio::tests::secret(),
        ))
    }

    /// Sends one request through [`route`] with a Host, and an optional
    /// cookie that holds the token.
    async fn get(studio: Arc<Studio>, host: &str, uri: &str, cookie: bool) -> Response {
        let mut b = Request::builder()
            .method("GET")
            .uri(uri)
            .header(header::HOST, host);
        if cookie {
            b = b.header(header::COOKIE, format!("acu-token-4321={TEST}"));
        }
        route(b.body(Body::empty()).unwrap(), studio).await
    }

    /// Sends one request through [`route`] to a studio on 127.0.0.1:4321.
    async fn send(host: &str, path: &str, body: Vec<u8>) -> StatusCode {
        send_to(studio::Mode::Studio, host, path, body).await
    }

    /// Sends one POST through [`route`] to a server of the given mode on
    /// 127.0.0.1:4321.
    async fn send_to(mode: studio::Mode, host: &str, path: &str, body: Vec<u8>) -> StatusCode {
        let req = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::HOST, host)
            .body(Body::from(body))
            .unwrap();
        route(req, server(mode, "127.0.0.1:4321")).await.status()
    }

    #[tokio::test]
    async fn a_foreign_host_reads_nothing_and_sends_no_body() {
        // A body over the limit would give 413 if acu read it first.
        let big = vec![b'x'; studio::MAX_BODY + 10];
        assert_eq!(
            send("evil.example", "/api/tour", big).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send("evil.example", "/data/atlas.json", vec![]).await,
            StatusCode::FORBIDDEN
        );
        // A foreign Host with a token in the address gets no cookie.
        let r = get(
            server(studio::Mode::Studio, "127.0.0.1:4321"),
            "evil.example",
            &format!("/?token={TEST}"),
            false,
        )
        .await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        assert!(r.headers().get(header::SET_COOKIE).is_none());
        // acu serve on a loopback address checks the Host of pages too.
        assert_eq!(
            send_to(studio::Mode::Serve, "localhost:9999", "/index.html", vec![]).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send_to(studio::Mode::Serve, "localhost:9999", "/api/state", vec![]).await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn acu_serve_shows_pages_to_allowed_names_and_on_open_addresses() {
        let forwarded = Arc::new(
            Studio::new(
                Arc::new(Live::empty()),
                studio::Mode::Serve,
                "127.0.0.1:4321".parse().unwrap(),
                None,
                None,
                studio::tests::secret(),
            )
            .with_allow_hosts(vec!["localhost:9999".into()]),
        );
        let r = get(forwarded.clone(), "localhost:9999", "/index.html", false).await;
        assert_eq!(r.status(), StatusCode::OK);
        let r = get(forwarded.clone(), "localhost:9998", "/index.html", false).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        // The API keeps its own names, token or not.
        let r = get(forwarded, "localhost:9999", "/api/state", true).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
        let open = server(studio::Mode::Serve, "0.0.0.0:4321");
        let r = get(open, "anything.example", "/index.html", false).await;
        assert_eq!(r.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_good_token_in_the_address_becomes_a_cookie() {
        let s = server(studio::Mode::Studio, "127.0.0.1:4321");
        let r = get(
            s.clone(),
            "127.0.0.1:4321",
            &format!("/?token={TEST}"),
            false,
        )
        .await;
        assert_eq!(r.status(), StatusCode::SEE_OTHER);
        assert_eq!(r.headers()[header::LOCATION], "/");
        assert_eq!(
            r.headers()[header::SET_COOKIE],
            format!("acu-token-4321={TEST}; Path=/; HttpOnly; SameSite=Strict").as_str()
        );
        assert_eq!(r.headers()[header::REFERRER_POLICY], "no-referrer");
        let r = get(
            s.clone(),
            "localhost:4321",
            &format!("/index.html?a=1&token={TEST}&b=2"),
            false,
        )
        .await;
        assert_eq!(r.headers()[header::LOCATION], "/index.html?a=1&b=2");
        // A path of two slashes would leave the server.
        let r = get(
            s.clone(),
            "127.0.0.1:4321",
            &format!("//evil.example/?token={TEST}"),
            false,
        )
        .await;
        assert_eq!(r.headers()[header::LOCATION], "/");
        let r = get(s.clone(), "127.0.0.1:4321", "/?token=wrong", false).await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        assert!(r.headers().get(header::SET_COOKIE).is_none());
        let r = get(s, "127.0.0.1:4321", "/?tokens=1", false).await;
        assert_eq!(r.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn the_studio_guards_its_data_and_acu_serve_does_not() {
        let s = server(studio::Mode::Studio, "127.0.0.1:4321");
        let r = get(s.clone(), "127.0.0.1:4321", "/data/atlas.json", false).await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        // With the cookie, the request passes; no project shows yet.
        let r = get(s.clone(), "127.0.0.1:4321", "/data/atlas.json", true).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        let r = get(s.clone(), "127.0.0.1:4321", "/index.html", false).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert!(!page_text(r).await.contains(API_MARK));
        let r = get(s, "127.0.0.1:4321", "/", true).await;
        assert!(page_text(r).await.contains(API_MARK));
        let serve = server(studio::Mode::Serve, "127.0.0.1:4321");
        let r = get(serve.clone(), "127.0.0.1:4321", "/data/atlas.json", false).await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        let r = get(serve, "127.0.0.1:4321", "/api/state", false).await;
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    }

    /// Reads a response's body as text.
    async fn page_text(r: Response) -> String {
        let b = to_bytes(r.into_body(), usize::MAX).await.unwrap();
        String::from_utf8_lossy(&b).to_string()
    }

    #[tokio::test]
    async fn mcp_needs_the_token() {
        let s = server(studio::Mode::Serve, "127.0.0.1:0");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        s.set_addr(addr);
        let (app, stop) = app(s, addr);
        let task = tokio::spawn(async move { axum::serve(listener, app).await });
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;
        let ask = |auth: Option<String>| {
            let mut req = format!(
                "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
                init.len()
            );
            if let Some(a) = auth {
                req.push_str(&format!("Authorization: {a}\r\n"));
            }
            req.push_str("\r\n");
            req.push_str(init);
            async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut c = tokio::net::TcpStream::connect(addr).await.unwrap();
                c.write_all(req.as_bytes()).await.unwrap();
                let mut all = Vec::new();
                c.read_to_end(&mut all).await.unwrap();
                String::from_utf8_lossy(&all).to_string()
            }
        };
        let status = |r: &str| r[9..12].to_string();
        let r = ask(None).await;
        assert_eq!(status(&r), "401");
        assert!(
            r.contains("www-authenticate: Bearer realm=\"acu\"\r\n"),
            "{r}"
        );
        let r = ask(Some("Bearer wrong".into())).await;
        assert_eq!(status(&r), "401");
        assert!(r.contains("error=\"invalid_token\""), "{r}");
        // The cookie that opens the API does not open MCP.
        let r = ask(Some(format!(
            "Basic x\r\nCookie: acu-token-{}={TEST}",
            addr.port()
        )))
        .await;
        assert_eq!(status(&r), "401");
        let r = ask(Some(format!("Bearer {TEST}"))).await;
        assert_eq!(status(&r), "200", "{r}");
        stop();
        task.abort();
    }

    #[test]
    fn the_start_lines_give_the_token_and_the_forwarding_hint() {
        let addr: SocketAddr = "127.0.0.1:4321".parse().unwrap();
        let s = server(studio::Mode::Serve, "127.0.0.1:4321");
        let v = start_lines(&s, addr, "the map", "writes on");
        assert!(v[0].starts_with(&format!(
            "acu: serving the map at http://127.0.0.1:4321/?token={TEST} "
        )));
        assert!(v[1].contains(&format!("Authorization: Bearer {TEST}")));
        assert!(v[2].contains("--allow-host"));
        let s = server(studio::Mode::Studio, "127.0.0.1:4321");
        assert_eq!(
            start_lines(&s, addr, "the studio", "no project open yet").len(),
            2
        );
        let open: SocketAddr = "0.0.0.0:4321".parse().unwrap();
        let s = server(studio::Mode::Serve, "0.0.0.0:4321");
        assert_eq!(start_lines(&s, open, "the map", "writes off").len(), 2);
        assert!(v.iter().all(|l| l.starts_with("acu: ")));
    }

    #[test]
    fn the_served_page_carries_the_api_mark_and_the_built_one_does_not() {
        let page = VIEWER.get_file("index.html").unwrap().contents();
        assert!(!String::from_utf8_lossy(page).contains("acu-api"));
        let served = String::from_utf8(mark_api(page)).unwrap();
        assert_eq!(served.matches(API_MARK).count(), 1);
    }
}
