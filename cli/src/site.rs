//! The static site: the viewer that the build embeds, plus data/atlas.json.
//! `build` writes it to a folder; `serve` and `studio` answer HTTP requests
//! from the live map, mount the MCP server at `/mcp`, and answer the
//! studio's API under `/api/`.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::body::{to_bytes, Body};
use axum::extract::Request;
use axum::http::{header, StatusCode};
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
    let (mcp, stop_mcp) = crate::mcp::http_service(studio.live.clone(), addr);
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
    let app = Router::new()
        .nest_service("/mcp", mcp)
        .fallback(move |req: Request| {
            let studio = studio.clone();
            async move { route(req, studio).await }
        });
    eprintln!("acu: serving {what} at http://{addr}/ and MCP at http://{addr}/mcp, {writes}  (Ctrl-C stops)");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            stop_mcp();
            stopper.stop_children();
        })
        .await?;
    Ok(())
}

/// Sends a request under `/api/` to the studio, and any other to
/// [`respond`].
async fn route(req: Request, studio: Arc<Studio>) -> Response {
    let path = req.uri().path().to_string();
    // In the studio, every answer needs the server's own Host, so a page
    // that DNS rebinding points here reads neither the map nor the API.
    // `acu serve` keeps the map open to any name, as forwarded ports need,
    // and guards only the API, below.
    // The API's own guard, below, checks the Host of an API call.
    if studio.mode == studio::Mode::Studio && !path.starts_with("/api/") {
        if let Err(r) = studio.check_host(req.headers()) {
            return json_reply(r);
        }
    }
    let Some(call) = path.strip_prefix("/api/") else {
        let atlas = studio.live.get().ok();
        return respond(&path, atlas.as_deref());
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

/// Makes a JSON response that no browser caches.
fn json_reply((status, body): studio::Reply) -> Response {
    reply(
        status,
        "application/json",
        serde_json::to_vec(&body).unwrap_or_default(),
    )
}

/// Answers one request: the data at `DATA_PATH`, else a file of the viewer.
fn respond(path: &str, atlas: Option<&Atlas>) -> Response {
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
            let body = if path == "index.html" {
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
/// page. A static site from `acu build` lacks it, so its viewer never calls
/// the API and never logs a failed request.
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

    /// Sends one request through [`route`] to a studio on 127.0.0.1:4321.
    async fn send(host: &str, path: &str, body: Vec<u8>) -> StatusCode {
        send_to(studio::Mode::Studio, host, path, body).await
    }

    /// Sends one request through [`route`] to a server of the given mode
    /// on 127.0.0.1:4321.
    async fn send_to(mode: studio::Mode, host: &str, path: &str, body: Vec<u8>) -> StatusCode {
        let studio = Arc::new(Studio::new(
            Arc::new(Live::empty()),
            mode,
            "127.0.0.1:4321".parse().unwrap(),
            None,
            None,
        ));
        let req = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::HOST, host)
            .body(Body::from(body))
            .unwrap();
        route(req, studio).await.status()
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
        assert_eq!(
            send("127.0.0.1:4321", "/data/atlas.json", vec![]).await,
            StatusCode::NOT_FOUND
        );
        // acu serve keeps the map open to a forwarded port's name.
        assert_eq!(
            send_to(studio::Mode::Serve, "localhost:9999", "/index.html", vec![]).await,
            StatusCode::OK
        );
        assert_eq!(
            send_to(studio::Mode::Serve, "localhost:9999", "/api/state", vec![]).await,
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn the_served_page_carries_the_api_mark_and_the_built_one_does_not() {
        let page = VIEWER.get_file("index.html").unwrap().contents();
        assert!(!String::from_utf8_lossy(page).contains("acu-api"));
        let served = String::from_utf8(mark_api(page)).unwrap();
        assert_eq!(served.matches(API_MARK).count(), 1);
    }
}
