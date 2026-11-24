"""Extra rules that live in a namespace package."""

from lending.library import Library


def late_fee(lib: Library, days: int) -> int:
    """Charges one coin per late day."""
    return days
