from __future__ import annotations

import asyncio
import struct
from dataclasses import dataclass
from enum import Enum

from .js import Value

SOCKET_PATH = "./vacht0.sock"


# event types
class RustEventType(Enum):
    Error = 0
    Closing = 1
    JsException = 2
    JsValue = 3


class PythonEventType(Enum):
    CloseIsolate = 0
    RunScript = 1
    DropValue = 2


# exceptions
class ServerError(RuntimeError):
    """The server sent back an internal server error."""


class ClosingError(RuntimeError):
    """The isolate is closing before the expected response is received."""


@dataclass
class JsException(RuntimeError):
    name: str
    message: str
    stack: str

    def __post_init__(self):
        super().__init__(self.stack)


class Deserializer:
    __slots__ = ("reader",)

    def __init__(self, reader: asyncio.StreamReader):
        self.reader = reader

    async def get_u8(self) -> int:
        return (await self.reader.readexactly(1))[0]

    async def get_u32(self) -> int:
        return struct.unpack("<I", await self.reader.read(4))[0]

    async def get_u64(self) -> int:
        return struct.unpack("<Q", await self.reader.read(8))[0]

    async def read_string(self) -> str:
        length = await self.get_u32()
        return (await self.reader.readexactly(length)).decode("utf-8")

    async def read_event_type(self) -> RustEventType:
        return RustEventType(await self.get_u8())


class Serializer:
    __slots__ = ("writer",)

    def __init__(self, writer: asyncio.StreamWriter):
        self.writer = writer

    def set_u8(self, data: int):
        self.writer.write(bytearray([data]))

    def set_u32(self, data: int):
        self.writer.write(struct.pack("<I", data))

    def set_u64(self, data: int):
        self.writer.write(struct.pack("<Q", data))

    def write_string(self, content: str):
        data = content.encode("utf-8")
        self.set_u32(len(data))
        self.writer.write(data)

    def write_event_type(self, event: PythonEventType):
        self.set_u8(event.value)

    async def commit(self):
        await self.writer.drain()

    async def close(self):
        """Close the session."""
        self.writer.close()
        await self.writer.wait_closed()


class Isolate:
    """Represents an isolate connection."""

    __slots__ = ("des", "ser")

    des: Deserializer
    ser: Serializer

    def __init__(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        self.des = Deserializer(reader)
        self.ser = Serializer(writer)

    @staticmethod
    async def connect(path: str = SOCKET_PATH) -> Isolate:
        reader, writer = await asyncio.open_unix_connection(path)
        return Isolate(reader, writer)

    async def run(self, source: str) -> Value:
        """Run Javascript code.

        Raises:
            ClosingError: The isolate is closing before the expected response is received.
        """
        self.ser.write_event_type(PythonEventType.RunScript)
        self.ser.write_string(source)
        await self.ser.commit()

        match await self.des.read_event_type():
            case RustEventType.Closing:
                raise ClosingError("closing before getting run result")

            case RustEventType.Error:
                err = await self.des.read_string()
                raise ServerError(f"server error: {err}")

            case RustEventType.JsException:
                name = await self.des.read_string()
                message = await self.des.read_string()
                stack = await self.des.read_string()
                raise JsException(name, message, stack)

            case RustEventType.JsValue:
                idx = await self.des.get_u64()
                return Value._rust(self, id=idx)

    async def _drop(self, idx: int):
        """Drop value of index `idx`. (internal)

        No errors will be raised even if the value doesn't exist.
        """
        self.ser.write_event_type(PythonEventType.DropValue)
        self.ser.set_u64(idx)
        await self.ser.commit()

    async def drop(self, value: Value):
        """Drop a value.

        No errors will be raised even if the value doesn't exist.

        Args:
            value: The value to drop.
        """
        await value.drop()

    async def close(self):
        """Close the isolate connection.

        The isolate instance will also be dropped from the server.
        """
        # we'll say goodbye gracefully
        self.ser.write_event_type(PythonEventType.CloseIsolate)
        await self.ser.commit()
        await self.ser.close()

    def __repr__(self):
        return "Isolate()"
