import signal
import subprocess

from .find_vacht import find_vacht_bin


def start():
    proc = subprocess.Popen(
        [find_vacht_bin()],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if proc.stdout:
        print("got data", proc.stdout.readlines())
        proc.send_signal(signal.SIGKILL)
