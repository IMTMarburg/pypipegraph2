"""Borrowed and adapted from 'rich'

Copyright 2020 Will McGugan, Florian Finkernagel

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

"""
from types import TracebackType
from typing import Callable, Dict, Iterable, List, Optional, Type
from traceback import walk_tb
import inspect
import sys
import os
from dataclasses import dataclass, field

_load_cwd = os.path.abspath(os.getcwd())


@dataclass
class Frame:
    filename: str
    lineno: int
    name: str
    locals: Dict[str, str]
    source: str 


@dataclass
class Stack:
    exc_type: str
    exc_value: str
    is_cause: bool = False
    frames: List[Frame] = field(default_factory=list)


class Trace:
    def __init__(
        self,
        exc_type: Type[BaseException],
        exc_value: BaseException,
        traceback: TracebackType,
    ):
        """Extract traceback information.

        Args:
            exc_type (Type[BaseException]): Exception type.
            exc_value (BaseException): Exception value.
            traceback (TracebackType): Python Traceback object.

        Returns:
            Trace: A Trace instance which you can use to construct a `Traceback`.
        """

        stacks: List[Stack] = []
        is_cause = False

        while True:
            stack = Stack(
                exc_type=str(exc_type.__name__),
                exc_value=str(exc_value),
                is_cause=is_cause,
            )

            stacks.append(stack)
            append = stack.frames.append

            for frame_summary, line_no in walk_tb(traceback):
                try:
                    if (
                        inspect.getsourcefile(frame_summary) == sys.argv[0]
                    ):  # current script, not absolute
                        filename = os.path.join(_load_cwd, sys.argv[0])
                    else:
                        filename = inspect.getabsfile(frame_summary)
                except:
                    filename = frame_summary.f_code.co_filename
                    if filename and not filename.startswith("<"):
                        filename = os.path.abspath(filename) if filename else "?"
                try:
                    with open(filename, "rb") as op:
                        source = op.read().decode("utf-8", errors="replace")

                except:
                    source = ""
                frame = Frame(
                    filename=filename,
                    lineno=line_no,
                    name=frame_summary.f_code.co_name,
                    locals={
                        key: str(value) for key, value in frame_summary.f_locals.items()
                    },
                    source=source,
                )
                append(frame)

            cause = getattr(exc_value, "__cause__", None)
            if cause and cause.__traceback__:
                exc_type = cause.__class__
                exc_value = cause
                traceback = cause.__traceback__
                if traceback:
                    is_cause = True
                    continue

            cause = exc_value.__context__
            if (
                cause
                and cause.__traceback__
                and not getattr(exc_value, "__suppress_context__", False)
            ):
                exc_type = cause.__class__
                exc_value = cause
                traceback = cause.__traceback__
                if traceback:
                    is_cause = False
                    continue
            # No cover, code is reached but coverage doesn't recognize it.
            break  # pragma: no cover

        self.stacks = stacks