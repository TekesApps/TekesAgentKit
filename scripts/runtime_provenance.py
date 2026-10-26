"""Read-only identity checks for already-built Kernel acceptance programs."""
import hashlib
import json
from pathlib import Path
import subprocess


def kernel_programs(root, target=None, supervisor=None, worker=None, expected_version=None):
    if (supervisor is None) != (worker is None):
        raise ValueError('supply both Kernel supervisor and worker programs')
    if supervisor is not None and target is not None:
        raise ValueError('explicit Kernel programs cannot be combined with a target directory')
    build = Path(target) if target is not None else Path(root) / 'target'
    supervisor = Path(supervisor if supervisor is not None else build / 'debug/tekes-supervisor').resolve(strict=True)
    worker = Path(worker if worker is not None else build / 'debug/tekes-worker').resolve(strict=True)
    result = subprocess.run([str(supervisor), '--describe-build'], check=True,
                            capture_output=True, text=True, timeout=10)
    description = json.loads(result.stdout)
    if not isinstance(description, dict) or description.get('identifier') != 'com.tekes.kernel.supervisor' or description.get('format') != 1:
        raise ValueError('unexpected Kernel supervisor build description')
    version = description.get('version')
    if not isinstance(version, str) or not version.strip():
        raise ValueError('Kernel build description has no version')
    if expected_version is not None and version != expected_version:
        raise ValueError(f'Kernel build version {version!r} does not match {expected_version!r}')
    receipt = {
        'supervisor': str(supervisor), 'supervisorSha256': hashlib.sha256(supervisor.read_bytes()).hexdigest(),
        'worker': str(worker), 'workerSha256': hashlib.sha256(worker.read_bytes()).hexdigest(),
        'build': description,
    }
    return supervisor, worker, receipt
