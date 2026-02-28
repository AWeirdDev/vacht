from __future__ import annotations

import asyncio
import inspect
from dataclasses import dataclass
from typing import TYPE_CHECKING, Awaitable, Callable, TypeAlias

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
    id: V8Id

    def perform(self) -> ValuePerformance:
        return ValuePerformance(tasks=[])

    async def drop(self):
        # TODO
        # await self.isolate.drop(self.id)
        ...

    @staticmethod
    def _rust(isolate: "Isolate", id: int) -> Value:
        return Value(isolate=isolate, id=id)  # pyright: ignore [reportArgumentType]


@dataclass(kw_only=True)
class ValuePerformance:
    tasks: list

    async def send(self, ctx: Context): ...


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
