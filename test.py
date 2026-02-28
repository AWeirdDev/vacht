import asyncio
import struct
from enum import Enum

SOCKET_PATH = "/tmp/vacht.sock"


class RustEventType(Enum):
    Error = 0
    Closing = 1
    JsException = 2


class PythonEventType(Enum):
    CloseIsolate = 0
    RunScript = 1


class Deserializer:
    __slots__ = ("reader",)

    def __init__(self, reader: asyncio.StreamReader):
        self.reader = reader

    async def get_u8(self) -> int:
        return (await self.reader.readexactly(1))[0]

    async def get_u32(self) -> int:
        return struct.unpack("<I", await self.reader.read(4))[0]

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
        self.writer.write(bytearray(data))

    def set_u32(self, data: int):
        self.writer.write(struct.pack("<I", data))

    def write_string(self, content: str):
        data = content.encode("utf-8")
        self.set_u32(len(data))
        self.writer.write(data)

    def write_event_type(self, event: PythonEventType):
        self.set_u8(event.value)

    async def commit(self):
        await self.writer.drain()

    async def close(self):
        self.writer.close()
        await self.writer.wait_closed()


async def main():
    reader, writer = await asyncio.open_unix_connection(SOCKET_PATH)

    ser = Serializer(writer)
    des = Deserializer(reader)

    ser.write_event_type(PythonEventType.RunScript)
    ser.write_string("'hello world'")
    await ser.commit()

    match await des.read_event_type():
        case RustEventType.Error:
            print("got error", await des.read_string())

        case RustEventType.Closing:
            print("closing isolate")

        case RustEventType.JsException:
            print("got js exception")
            name = await des.read_string()
            message = await des.read_string()
            stack = await des.read_string()
            print(name, message, stack)

    print("done")

    await ser.close()


asyncio.run(main())
