"""Delivers books in two ways, so the resolver must not pick one.

A name or attribute that holds a Courier in one place and a Drone in
another has no single type. A call through it gets no exact edge.
"""

from .domain import LOAN_DAYS


class Courier:
    """Carries books by hand."""

    def run(self) -> None:
        """Delivers by hand."""


class Drone:
    """Carries books by air."""

    def run(self) -> None:
        """Delivers by air."""


carrier = Courier()
if LOAN_DAYS > 7:
    carrier = Drone()

backup = Courier()

fixed = Courier()
"""A carrier that nothing rebinds."""


def deliver() -> None:
    """Sends today's books with whichever carrier the module holds."""
    carrier.run()


def use_backup() -> None:
    """Sends books with the backup carrier."""
    backup.run()


def switch_backup() -> None:
    """Makes a drone the backup carrier."""
    global backup
    backup = Drone()


def send_fixed() -> None:
    """Sends books with the carrier that never changes."""
    fixed.run()


class Dispatch:
    """Holds a carrier that one method swaps."""

    def __init__(self) -> None:
        """Starts with a courier."""
        self.carrier = Courier()

    def swap(self) -> None:
        """Swaps to a drone."""
        self.carrier = Drone()

    def go(self) -> None:
        """Sends the books with the current carrier."""
        self.carrier.run()
