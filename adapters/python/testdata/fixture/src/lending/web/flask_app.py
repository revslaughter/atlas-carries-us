"""The Flask app: a blueprint of book routes and an app factory."""

from flask import Blueprint, Flask, jsonify

from ..library import Library

books = Blueprint("books", __name__, url_prefix="/books")


@books.route("/<book_id>")
def show(book_id: str):
    """Shows one book."""
    return jsonify(id=book_id)


@books.route("/", methods=["POST"])
def add():
    """Adds a book."""
    return jsonify(ok=True)


def about():
    """Says what the service does."""
    return "lending"


def create_app(lib: Library) -> Flask:
    """Builds the app and mounts the book routes under /api."""
    app = Flask(__name__)
    app.register_blueprint(books, url_prefix="/api/books")
    app.add_url_rule("/about", view_func=about)

    @app.get("/health")
    def health():
        """Reports the number of loans."""
        return jsonify(loans=lib.size)

    return app
