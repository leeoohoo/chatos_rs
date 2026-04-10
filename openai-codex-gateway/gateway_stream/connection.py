from __future__ import annotations

from typing import Any, Callable


def make_close_connection_setter(
    *,
    target: Any,
    attr_name: str = "close_connection",
) -> Callable[[bool], None]:
    def set_close_connection(value: bool) -> None:
        setattr(target, attr_name, value)

    return set_close_connection


def make_close_connection_marker(
    *,
    set_close_connection: Callable[[bool], None],
) -> Callable[[], None]:
    def mark_close_connection() -> None:
        set_close_connection(True)

    return mark_close_connection
