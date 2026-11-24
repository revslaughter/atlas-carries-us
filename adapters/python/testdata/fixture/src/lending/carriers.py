"""Delivers books in two ways, so the resolver must not pick one.

A name or attribute that holds a Courier in one place and a Drone in
another has no single type. A call through it gets no exact edge.
"""

from contextlib import nullcontext

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

looped = Courier()
for looped in [Drone()]:
    pass

wrapped = Courier()
with nullcontext(Drone()) as wrapped:
    pass

counted = Courier()
counted += 1

added = Courier()
imported = Courier()
walrused = Courier()
deleted = Courier()
del deleted


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


def use_looped() -> None:
    """Sends books with the carrier that a module-level loop rebinds."""
    looped.run()


def use_wrapped() -> None:
    """Sends books with the carrier that a module-level with rebinds."""
    wrapped.run()


def use_counted() -> None:
    """Sends books with the carrier that a module-level += rebinds."""
    counted.run()


def bump_added() -> None:
    """Rebinds added with += through global."""
    global added
    added += 1


def use_added() -> None:
    """Sends books with added."""
    added.run()


def import_over() -> None:
    """Rebinds imported with an import through global."""
    global imported
    import lending.domain as imported  # noqa: F401


def use_imported() -> None:
    """Sends books with imported."""
    imported.run()


def walrus_over() -> None:
    """Rebinds walrused with := through global."""
    global walrused
    (walrused := Drone())


def use_walrused() -> None:
    """Sends books with walrused."""
    walrused.run()


def use_deleted() -> None:
    """Reads a name that del removed."""
    deleted.run()


class Tally:
    """Holds a carrier that one method bumps with +=."""

    def __init__(self) -> None:
        """Starts with a courier."""
        self.carrier = Courier()

    def bump(self) -> None:
        """Adds to the carrier."""
        self.carrier += 1

    def go(self) -> None:
        """Sends the books with the current carrier."""
        self.carrier.run()


class Fleet:
    """Holds a carrier that a subclass may replace."""

    def __init__(self) -> None:
        """Starts with a courier."""
        self.carrier = Courier()

    def go(self) -> None:
        """Sends the books with the current carrier."""
        self.carrier.run()


class AirFleet(Fleet):
    """A fleet whose carrier flies."""

    def fly(self) -> None:
        """Swaps to a drone."""
        self.carrier = Drone()


def bump_local() -> None:
    """Sends books with a local carrier that += rebinds."""
    van = Courier()
    van += 1
    van.run()


def relay() -> None:
    """Sends books with a local carrier that a nested function rebinds through nonlocal."""
    van = Courier()

    def take_off() -> None:
        """Swaps the local carrier for a drone."""
        nonlocal van
        van = Drone()

    take_off()
    van.run()


def pick_last() -> None:
    """Sends books with a local carrier that := inside a comprehension rebinds."""
    van = Courier()
    _ = [van := Drone() for _ in range(1)]
    van.run()


def send_local() -> None:
    """Sends books with a local carrier that nothing rebinds."""
    van = Courier()
    van.run()
