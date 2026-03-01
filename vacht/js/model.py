from __future__ import annotations

import asyncio
import inspect
from dataclasses import dataclass
from enum import Enum
from typing import TYPE_CHECKING, Any, Awaitable, Callable, Literal, TypeAlias

if TYPE_CHECKING:
    from ..isolate import Isolate


GLOBAL_ID = 0


# branded type
class PythonId(int):
    """A universal identifier."""

    @staticmethod
    def unique() -> PythonId:
        """Create a universal identifier."""
        global GLOBAL_ID
        me = GLOBAL_ID  # cheap copy
        GLOBAL_ID += 1

        return me  # type: ignore


if TYPE_CHECKING:
    # branded type
    class V8Id(int): ...
else:
    V8Id = int


@dataclass(frozen=True)
class Context:
    action_id: PythonId
    isolate: "Isolate"


@dataclass(frozen=True, kw_only=True)
class Action:
    id: PythonId
    runner: Callable[[Context, Value], Awaitable[Serializable]]


@dataclass(frozen=True)
class Value:
    """Represents a v8 value."""

    isolate: "Isolate"
    """The tied isolate reference."""

    id: V8Id
    """The arena entry ID of this value.

    Upon calling `drop()`, the server looks up the the ID of this
    value and deallocates it.
    """

    def orchestrate(self) -> ValueOrchestrator:
        """Orchestrate the data for more efficient Python <-> V8 transformation."""
        return ValueOrchestrator(value=self, tasks=[])

    async def drop(self):
        """Drops the value.

        It is recommended to call this function to free some memory on the server.
        """
        await self.isolate._drop(self.id)

    @staticmethod
    def _rust(isolate: "Isolate", id: int) -> Value:
        return Value(isolate=isolate, id=id)  # pyright: ignore [reportArgumentType]


class ManipulationTaskType(Enum):
    As = 0
    Index = 1
    IndexKey = 2


class CastTo(Enum):
    Bool = 0
    Int = 1
    Str = 2


Manipulation: TypeAlias = (
    tuple[Literal[ManipulationTaskType.As], CastTo]
    | tuple[Literal[ManipulationTaskType.Index], int]
    | tuple[Literal[ManipulationTaskType.IndexKey], str]
)


@dataclass
class ValueOrchestrator:
    value: Value
    tasks: list[Manipulation]

    def as_bool(self) -> ValueOrchestrator:
        self.tasks.append((ManipulationTaskType.As, CastTo.Bool))
        return self

    def as_int(self) -> ValueOrchestrator:
        self.tasks.append((ManipulationTaskType.As, CastTo.Int))
        return self

    def as_str(self) -> ValueOrchestrator:
        self.tasks.append((ManipulationTaskType.As, CastTo.Str))
        return self

    def index(self, key: int | str) -> ValueOrchestrator:
        if isinstance(key, int):
            self.tasks.append((ManipulationTaskType.Index, key))
        else:
            self.tasks.append((ManipulationTaskType.IndexKey, key))
        return self

    async def run(self, ctx: Context):
        self.value.isolate


def function(
    fn: Callable[[Context, Value], Awaitable[Serializable]]
    | Callable[[Context, Value], Serializable],
) -> Action:
    if not inspect.iscoroutinefunction(fn):

        async def runner(ctx, arg) -> Serializable:
            return await asyncio.to_thread(fn, ctx, arg)  # type: ignore

        return Action(id=PythonId.unique(), runner=runner)

    else:
        return Action(id=PythonId.unique(), runner=fn)


Serializable: TypeAlias = (
    str
    | int
    | float
    | bool
    | dict[str, "Serializable"]
    | list["Serializable"]
    | Value
    | None
)
