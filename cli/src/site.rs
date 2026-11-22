//! The static site: the viewer that the build embeds, plus data/atlas.json.
//! `build` writes it to a folder; `serve` answers HTTP requests from memory.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::Response;
use axum::Router;
use include_dir::{include_dir, Dir};

static VIEWER: Dir<'_> = include_dir!("$OUT_DIR/viewer");

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

/// Where `serve` gets its data: bytes held in memory, or a file read on each
/// request (so a fresh `acu extract` shows on reload).
pub enum DataSource {
    Memory(Vec<u8>),
    File(PathBuf),
}

pub async fn serve(addr: SocketAddr, data: DataSource) -> Result<()> {
    let data = Arc::new(data);
    let app = Router::new().fallback(move |uri: Uri| {
        let data = data.clone();
        async move { respond(uri, &data) }
    });
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("listen on {addr}"))?;
    eprintln!("acu: serving the map at http://{addr}/  (Ctrl-C stops)");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn respond(uri: Uri, data: &DataSource) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path == DATA_PATH {
        let bytes = match data {
            DataSource::Memory(b) => Ok(b.clone()),
            DataSource::File(p) => std::fs::read(p),
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

fn reply(status: StatusCode, mime: &str, body: Vec<u8>) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .unwrap()
}
