import os
import socket
import subprocess

from .find_vacht import find_vacht_bin
from .socket import LocalSocket


async def start():
    if os.name == "nt":
        raise RuntimeError("unsupported platform: nt")

    proc = subprocess.Popen(
        [find_vacht_bin(), "run"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    print("Waiting for connection...")
    async for sock, loop in LocalSocket.session():
        sock.sendall(b"0")
        print(await loop.sock_recv(sock, 1))

        sock.close()

    proc.terminate()
