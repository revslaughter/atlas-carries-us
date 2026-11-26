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
    let addr = studio.addr;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("listen on {addr}"))?;
    let addr = listener.local_addr().unwrap_or(addr);
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
        })
        .await?;
    Ok(())
}

/// Sends a request under `/api/` to the studio, and any other to
/// [`respond`].
async fn route(req: Request, studio: Arc<Studio>) -> Response {
    let path = req.uri().path().to_string();
    let Some(call) = path.strip_prefix("/api/") else {
        let atlas = studio.live.get().ok();
        return respond(&path, atlas.as_deref());
    };
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
            reply(StatusCode::OK, mime.as_ref(), f.contents().to_vec())
        }
        None => reply(
            StatusCode::NOT_FOUND,
            "text/plain",
            b"You find nothing at this address.".to_vec(),
        ),
    }
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
