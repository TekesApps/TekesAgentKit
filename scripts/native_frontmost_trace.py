"""Read-only frontmost-app identity trace for isolated macOS native probes.

Uses lsappinfo's front/info queries only: no activation, focus, key or window
operations. It records application identity, never window titles or contents.
"""
import json
from pathlib import Path
import re
import subprocess
import threading
import time


def frontmost_identity():
    asn = subprocess.check_output(['/usr/bin/lsappinfo', 'front'], text=True, timeout=2).strip()
    if not re.fullmatch(r'ASN:0x[0-9a-fA-F]+-0x[0-9a-fA-F]+:', asn):
        raise ValueError('frontmost ASN unavailable')
    text = subprocess.check_output(['/usr/bin/lsappinfo', 'info', '-only', 'pid,bundleid,name', asn], text=True, timeout=2)
    pid = re.search(r'^"pid"=(\d+)$', text, re.MULTILINE)
    bundle = re.search(r'^"CFBundleIdentifier"="([^"\n]*)"$', text, re.MULTILINE)
    if pid is None:
        raise ValueError('frontmost PID unavailable')
    return {'asn': asn, 'pid': int(pid[1]), 'bundleId': bundle[1] if bundle else None}


class FrontmostTrace:
    def __init__(self, output):
        self.output = Path(output)
        self.stop = threading.Event()
        self.last = None
        self.thread = None

    def sample(self):
        try:
            identity = frontmost_identity()
        except (OSError, ValueError, subprocess.SubprocessError):
            identity = {'observationUnavailable': True}
        if identity != self.last:
            self.last = identity
            with self.output.open('a') as out:
                out.write(json.dumps({'unixSeconds': time.time(), **identity}, sort_keys=True) + '\n')

    def loop(self):
        while not self.stop.wait(0.2):
            self.sample()

    def __enter__(self):
        self.output.write_text('')
        self.sample()
        self.thread = threading.Thread(target=self.loop, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *exception):
        self.stop.set()
        self.thread.join(5)
        if not self.thread.is_alive():
            self.sample()
