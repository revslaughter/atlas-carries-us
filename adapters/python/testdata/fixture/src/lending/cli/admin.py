"""The lendctl command, built with Typer."""

import typer

app = typer.Typer()
users = typer.Typer()


@app.command()
def sync_all() -> None:
    """Syncs every store."""


@users.command("add")
def add_user(name: str) -> None:
    """Adds a member."""


app.add_typer(users, name="users")
