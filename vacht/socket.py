import asyncio
import os
import socket
from dataclasses import dataclass
from typing import AsyncGenerator

SOCKET_PATH = "vacht.sock"


@dataclass(frozen=True)
class LocalSocket:
    sock: socket.socket

    @staticmethod
    async def session() -> AsyncGenerator[
        tuple[socket.socket, asyncio.AbstractEventLoop]
    ]:
        if os.path.exists(SOCKET_PATH):
            os.remove(SOCKET_PATH)

        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.setblocking(False)

        # we connect
        loop = asyncio.get_event_loop()
        await loop.sock_connect(sock, SOCKET_PATH)

        yield (sock, loop)

        # then close
        sock.close()
