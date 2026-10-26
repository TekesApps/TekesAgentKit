"""Read-only sampled OS RSS for explicitly owned performance process trees.

PID and OS start identity are retained. This cannot observe allocator peaks or
guarantee seeing children shorter than the requested sampling interval. No
process is signalled and no command arguments or environment are read.
"""
from __future__ import annotations

import json
import os
import ctypes
import sys
from functools import lru_cache
from pathlib import Path
import subprocess
import threading
import time
from contextlib import contextmanager


@lru_cache(maxsize=1)
def _pidpath_function():
    library = ctypes.CDLL('/usr/lib/libproc.dylib', use_errno=True)
    function = library.proc_pidpath
    function.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
    function.restype = ctypes.c_int
    return function


def executable_path(pid):
    """Read the executable identity, independent of Node's mutable process title."""
    if sys.platform != 'darwin':
        return None
    buffer = ctypes.create_string_buffer(4096)
    if _pidpath_function()(pid, buffer, len(buffer)) <= 0:
        return None
    return os.fsdecode(buffer.value)


def process_table():
    result = subprocess.run(['/bin/ps', '-axo', 'pid=,ppid=,rss=,lstart=,comm='],
        check=True, capture_output=True, text=True, timeout=5,
        env={'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','LC_ALL':'C','LANG':'C'})
    rows = {}
    for line in result.stdout.splitlines():
        fields = line.split(None, 8)
        if len(fields) != 9:
            raise ValueError('OS process row missing PID/start identity/RSS/command')
        pid, ppid, rss = map(int, fields[:3])
        rows[pid] = {'pid':pid, 'ppid':ppid, 'rssKiB':rss,
                     'startIdentity':' '.join(fields[3:8]), 'command':fields[8],
                     'executablePath':executable_path(pid)}
    return rows


def classify(rows, roots, known, *, pi_embedded=False):
    """Resolve nearest explicit owner, preserving previously observed orphans."""
    result = {}
    for pid, row in rows.items():
        identity = (pid, row['startIdentity'])
        current, visited, owner = pid, set(), None
        while current in rows and current not in visited:
            visited.add(current)
            candidate = rows[current]
            root = roots.get(current)
            if root and root['startIdentity'] == candidate['startIdentity']:
                owner = root['role']; break
            current = candidate['ppid']
        if owner is None:
            owner = known.get(identity)
        if owner is None:
            continue
        # Pi is embedded in the AgentKit endpoint but its Node RPC and tool
        # descendants belong to harness RSS, not the Rust consumer budget.
        if owner == 'agentkit' and pi_embedded:
            current, visited = pid, set()
            while current in rows and current not in visited:
                visited.add(current)
                executable = rows[current].get('executablePath') or rows[current]['command']
                if Path(executable).name == 'node':
                    owner = 'harness'; break
                if current in roots:
                    break
                current = rows[current]['ppid']
        result[pid] = dict(row, role=owner)
    return result


class ProcessSampler:
    def __init__(self, output, *, pi_embedded=False, interval_ms=50):
        if interval_ms != 50:
            raise ValueError('the preregistered interval is exactly 50ms')
        self.output = Path(output)
        self.pi_embedded = pi_embedded
        self.interval_ns = interval_ms * 1_000_000
        self.roots, self.known = {}, {}
        # Root launch + registration must be atomic with sampler classification.
        # _sample takes an OS snapshot before acquiring this lock; a snapshot taken
        # before spawn omits the root safely, while a snapshot taken during spawn
        # waits here until the explicit root identity is registered.
        self.lock = threading.RLock()
        self.stop_event = threading.Event()
        self.thread = None
        self.samples = 0
        self.first_ns = self.last_ns = None
        self.max_interval_ns = 0
        self.peaks = {'agentkit':0,'harness':0,'provider':0}
        self.error = None

    @contextmanager
    def root_launch_guard(self):
        """Guard only the short Popen→register window for an owned root.

        Never hold this across process IO, readiness, or the process lifetime.
        """
        with self.lock:
            yield

    def register(self, process_or_pid, role):
        if role not in self.peaks:
            raise ValueError('unknown owned process role')
        pid = process_or_pid if isinstance(process_or_pid, int) else process_or_pid.pid
        row = process_table().get(pid)
        if row is None:
            raise ValueError('owned root exited before process sampling registration')
        with self.lock:
            self.roots[pid] = {'role':role,'startIdentity':row['startIdentity']}
        return {'pid':pid,'role':role,'startIdentity':row['startIdentity']}

    def start(self):
        if self.thread is not None:
            raise RuntimeError('sampler already started')
        self.output.parent.mkdir(parents=True, exist_ok=True)
        self.stream = self.output.open('x', encoding='utf-8')
        self.thread = threading.Thread(target=self._run, name='agentkit-owned-rss', daemon=True)
        self.thread.start()

    def _sample(self):
        rows = process_table()
        observed_ns = time.monotonic_ns()
        with self.lock:
            owned = classify(rows, self.roots, self.known, pi_embedded=self.pi_embedded)
            for row in owned.values():
                self.known[(row['pid'],row['startIdentity'])] = row['role']
            totals = {role:sum(row['rssKiB'] for row in owned.values() if row['role']==role)
                      for role in self.peaks}
            for role, total in totals.items():
                self.peaks[role] = max(self.peaks[role], total)
            if self.first_ns is None:
                self.first_ns = observed_ns
            if self.last_ns is not None:
                self.max_interval_ns = max(self.max_interval_ns, observed_ns-self.last_ns)
            self.last_ns = observed_ns
            self.samples += 1
        self.stream.write(json.dumps({'monotonicNs':observed_ns,
            'treeRssKiB':totals,'processes':list(owned.values())},ensure_ascii=False)+'\n')
        self.stream.flush()

    def _run(self):
        deadline = time.monotonic_ns()
        try:
            while not self.stop_event.is_set():
                self._sample()
                deadline += self.interval_ns
                self.stop_event.wait(max(0, (deadline-time.monotonic_ns())/1_000_000_000))
        except Exception as error:
            self.error = type(error).__name__ + ': ' + str(error)
        finally:
            self.stream.close()

    def stop(self):
        self.stop_event.set()
        if self.thread:
            self.thread.join(timeout=10)
            if self.thread.is_alive():
                raise RuntimeError('owned RSS sampler did not retire')
        if self.error:
            raise RuntimeError('OS RSS sampling failed: ' + self.error)
        if not self.samples:
            raise ValueError('no process samples were recorded')
        return {'sampleCount':self.samples,'requestedIntervalMs':50,
            'maxObservedIntervalNs':self.max_interval_ns,'sampledTreePeakRssKiB':dict(self.peaks),
            'measurement':'simultaneous OS resident-size tree sum, not allocator peak',
            'shortProcessLimit':'children shorter than 50ms may be missed; Pi --version probes are a known short-lived source',
            'startIdentityResolution':'OS lstart in whole seconds; PID plus start time/command/ancestry retained',
            'knownProcesses':[{'pid':pid,'startIdentity':start,'role':role}
                              for (pid,start),role in self.known.items()],
            'roots':dict(self.roots),'firstMonotonicNs':self.first_ns,'lastMonotonicNs':self.last_ns}
