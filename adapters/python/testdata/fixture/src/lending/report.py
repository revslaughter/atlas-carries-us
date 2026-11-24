"""Prints a report of the loans."""

from .library import Library
from . import open_library


def main() -> None:
    """Prints how many books sit on loan."""
    lib: Library = open_library()
    print(lib.size)


if __name__ == "__main__":
    main()
