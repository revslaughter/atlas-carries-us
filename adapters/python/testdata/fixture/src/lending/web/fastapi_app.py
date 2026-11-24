"""The FastAPI app: loan routes on a router, mounted under /v1."""

from fastapi import APIRouter, FastAPI

from ..domain import Loan
from ..library import Library
from ..store import MemoryStore

app = FastAPI()
loans = APIRouter(prefix="/loans")
library = Library(MemoryStore())


@loans.get("/{member}")
async def list_loans(member: str) -> list[Loan]:
    """Lists one member's loans."""
    return library.held_by(member)


@loans.post("")
async def borrow(book_id: str, member: str) -> Loan:
    """Lends a book."""
    return library.borrow(book_id, member)


@app.api_route("/ping", methods=["GET", "HEAD"])
def ping() -> str:
    """Answers a health check."""
    return "pong"


app.include_router(loans, prefix="/v1")
