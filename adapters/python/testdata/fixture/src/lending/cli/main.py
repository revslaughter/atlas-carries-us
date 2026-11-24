"""The lendd command, built with click."""

import click

from .. import open_library


@click.group()
def cli() -> None:
    """Runs the lending library from a shell."""


@cli.command()
@click.argument("book_id")
@click.argument("member")
def borrow(book_id: str, member: str) -> None:
    """Lends a book."""
    open_library().borrow(book_id, member)


@cli.group()
def report() -> None:
    """Prints reports."""


@report.command("count")
def count_cmd() -> None:
    """Prints how many books sit on loan."""
    click.echo(open_library().size)


@click.command()
def import_command() -> None:
    """Imports books from the old system."""


cli.add_command(import_command)
