//! The HTTP API: routes that turn requests into library calls.

use axum::extract::{Json, Path};
use axum::routing::{any, get, post};
use axum::Router;
use lending::domain::{Book, BookId};

/// Builds the router of the whole API.
pub fn router() -> Router {
    let admin = Router::new()
        .route("/reindex", post(reindex))
        .route("/ping", get(|| async { "pong" }));
    Router::new()
        .route("/books", get(list_books).post(add_book))
        .route("/books/{id}", get(show_book).delete(remove_book))
        .nest("/api", api())
        .nest("/admin", admin)
        .merge(health())
        .route_service("/static", "files")
}

/// The routes under /api.
fn api() -> Router {
    Router::new().route("/search", any(search))
}

/// The health check.
fn health() -> Router {
    Router::new().route("/health", get(|| async {}))
}

/// Lists every book.
async fn list_books() -> Json<Vec<Book>> {
    Json(vec![])
}

/// Adds a book.
async fn add_book(Json(book): Json<Book>) -> Json<Book> {
    Json(book)
}

/// Shows one book.
async fn show_book(Path(id): Path<u32>) -> Json<Option<Book>> {
    let _ = BookId(id);
    Json(None)
}

/// Removes one book.
async fn remove_book(Path(id): Path<u32>) {
    let _ = id;
}

/// Rebuilds the search index.
async fn reindex() {}

/// Searches the books.
async fn search() -> Json<Vec<Book>> {
    list_books().await
}
