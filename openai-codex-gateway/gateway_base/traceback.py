from __future__ import annotations

import sys
import traceback
from typing import Callable, TextIO


def make_traceback_printer(
    *,
    traceback_printer: Callable[..., None] = traceback.print_exc,
    error_stream: TextIO | None = None,
) -> Callable[[], None]:
    stream = sys.stderr if error_stream is None else error_stream

    def print_traceback() -> None:
        traceback_printer(file=stream)

    return print_traceback
