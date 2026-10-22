#!/usr/bin/env python3
"""Opt-in macOS Pinyin integration probe; never changes the user's input source.

Posts hardware-key events only to the isolated demo PID. No Unicode insertion,
paste, AX value replacement or synthetic marked-text call is used. This is
software-IME evidence, not physical-keyboard/candidate-window acceptance.
"""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time


class MacInput:
    def __init__(self):
        self.cf = c.CDLL('/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation')
        self.cg = c.CDLL('/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics')
        self.tis = c.CDLL('/System/Library/Frameworks/Carbon.framework/Carbon')
        self.cf.CFRelease.argtypes = [c.c_void_p]
        self.cf.CFStringGetCString.argtypes = [c.c_void_p, c.c_char_p, c.c_long, c.c_uint32]
        self.cf.CFStringGetCString.restype = c.c_bool
        self.tis.TISCopyCurrentKeyboardInputSource.restype = c.c_void_p
        self.tis.TISGetInputSourceProperty.argtypes = [c.c_void_p, c.c_void_p]
        self.tis.TISGetInputSourceProperty.restype = c.c_void_p
        self.cg.CGPreflightPostEventAccess.restype = c.c_bool
        self.cg.CGEventCreateKeyboardEvent.argtypes = [c.c_void_p, c.c_uint16, c.c_bool]
        self.cg.CGEventCreateKeyboardEvent.restype = c.c_void_p
        self.cg.CGEventPostToPid.argtypes = [c.c_int, c.c_void_p]
        self.cg.CGEventSetFlags.argtypes = [c.c_void_p, c.c_uint64]

    def source(self):
        source = self.tis.TISCopyCurrentKeyboardInputSource()
        if not source:
            raise RuntimeError('current input source unavailable')
        try:
            key = c.c_void_p.in_dll(self.tis, 'kTISPropertyInputSourceID').value
            value = self.tis.TISGetInputSourceProperty(source, key)
            buf = c.create_string_buffer(1024)
            if not value or not self.cf.CFStringGetCString(value, buf, len(buf), 0x08000100):
                raise RuntimeError('input-source ID unavailable')
            return buf.value.decode('utf-8')
        finally:
            self.cf.CFRelease(source)

    def key(self, pid, code):
        for down in (True, False):
            event = self.cg.CGEventCreateKeyboardEvent(None, code, down)
            if not event:
                raise RuntimeError('keyboard event allocation failed')
            try:
                self.cg.CGEventSetFlags(event, 0)
                self.cg.CGEventPostToPid(pid, event)
            finally:
                self.cf.CFRelease(event)
            time.sleep(0.03)


def run(demo, *, native_app=None, log=None, observe_frontmost=False):
    if sys.platform != 'darwin':
        raise RuntimeError('macOS is required')
    mac = MacInput()
    expected = 'com.apple.inputmethod.SCIM.ITABC'
    before = mac.source()
    if before != expected:
        raise RuntimeError(f'current source is {before}; select Simplified Pinyin before this opt-in test')
    if not mac.cg.CGPreflightPostEventAccess():
        raise RuntimeError('event-post access unavailable; this tool does not request permissions')
    if native_app is not None:
        if log is None:
            raise ValueError('--native-app requires a fresh --log')
        from native_ime_probe import run_app_ime
        return run_app_ime(demo, native_app, log, mac, before,
                           observe_frontmost=observe_frontmost)
    if log is not None or observe_frontmost:
        raise ValueError('--log/--observe-frontmost require --native-app')
    process = subprocess.Popen([str(demo), '--components', '--transcript-content',
                                '--system-ime-selftest'], stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, bufsize=1)
    lines = queue.Queue()

    def read():
        try:
            for line in process.stdout:
                lines.put(line)
        finally:
            lines.put(None)

    threading.Thread(target=read, daemon=True).start()
    seen = set()
    passed = False
    deadline = time.monotonic() + 25
    try:
        while True:
            line = lines.get(timeout=max(0.01, deadline - time.monotonic()))
            if line is None:
                break
            print(line, end='', flush=True)
            if time.monotonic() >= deadline:
                raise RuntimeError('IME probe timed out')
            for stage, keys in [('TYPE', (45, 34, 4, 0, 31)), ('COMMIT', (49,))]:
                if line.strip() != f'selftest: Log("SYSTEM_IME_{stage}_READY")':
                    continue
                if stage in seen or mac.source() != expected:
                    raise RuntimeError('duplicate stage or input source changed; refusing key injection')
                seen.add(stage)
                for code in keys:
                    if process.poll() is not None:
                        raise RuntimeError('owned demo exited before key injection')
                    mac.key(process.pid, code)
            passed |= line.startswith('NATIVE_AGENT_SYSTEM_IME_PROBE_OK ')
        status = process.wait(timeout=max(0.01, deadline - time.monotonic()))
        after = mac.source()
        if status != 0 or not passed or seen != {'TYPE', 'COMMIT'} or after != before:
            raise RuntimeError(f'IME probe failed: exit={status}, stages={sorted(seen)}, source={after}')
        return {'softwareIme': True, 'pidTargetedEvents': True, 'physicalKeyboard': False,
                'candidateWindow': False, 'sourceBefore': before, 'sourceAfter': after,
                'binarySha256': hashlib.sha256(demo.read_bytes()).hexdigest()}
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--demo', required=True, type=Path)
    parser.add_argument('--receipt', type=Path)
    parser.add_argument('--native-app', type=Path, help='Exact APPL bundle for a normal LaunchServices run')
    parser.add_argument('--log', type=Path, help='Fresh native log; existing evidence is never overwritten')
    parser.add_argument('--observe-frontmost', action='store_true', help='Read-only app-identity trace')
    args = parser.parse_args()
    if args.receipt and args.receipt.exists():
        raise ValueError('preserve existing IME receipt; choose a new path')
    result = run(args.demo.resolve(strict=True), native_app=args.native_app,
                 log=args.log, observe_frontmost=args.observe_frontmost)
    body = json.dumps(result, ensure_ascii=False, indent=2) + '\n'
    if args.receipt:
        with args.receipt.open('x') as output:
            output.write(body)
    print(body, end='')


if __name__ == '__main__':
    main()
