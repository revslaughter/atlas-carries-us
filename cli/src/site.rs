//! The static site: the viewer that the build embeds, plus data/atlas.json.
//! `build` writes it to a folder; `serve` answers HTTP requests from the live
//! map, and mounts the MCP server at `/mcp`.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::Response;
use axum::Router;
use include_dir::{include_dir, Dir};

use crate::mcp::Atlas;

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

/// Serves the viewer, the live map's data and the MCP server at `/mcp` on an
/// address until Ctrl-C. The data comes from the live map on each request,
/// so a draft from an MCP write tool, or a fresh `acu extract` of a served
/// atlas.json, shows on reload.
pub async fn serve(addr: SocketAddr, atlas: Arc<Atlas>) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("listen on {addr}"))?;
    let addr = listener.local_addr().unwrap_or(addr);
    let (mcp, stop_mcp) = crate::mcp::http_service(atlas.clone(), addr);
    let writes = if atlas.writable() { "on" } else { "off" };
    let app = Router::new()
        .nest_service("/mcp", mcp)
        .fallback(move |uri: Uri| {
            let atlas = atlas.clone();
            async move { respond(uri, &atlas) }
        });
    eprintln!("acu: serving the map at http://{addr}/ and MCP at http://{addr}/mcp, writes {writes}  (Ctrl-C stops)");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            stop_mcp();
        })
        .await?;
    Ok(())
}

/// Answers one request: the data at `DATA_PATH`, else a file of the viewer.
fn respond(uri: Uri, atlas: &Atlas) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path == DATA_PATH {
        let bytes = atlas.snapshot().map(|s| s.bytes.clone());
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
fn reply(status: StatusCode, mime: &str, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .unwrap()
}
