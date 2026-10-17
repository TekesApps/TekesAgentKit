#!/usr/bin/env python3
"""Compile current, unmodified Swift contract against the same JSON cases as Rust."""
import argparse
import re
from pathlib import Path
import subprocess
import tempfile
parser=argparse.ArgumentParser()
parser.add_argument('--clientkit-root',type=Path,required=True)
args=parser.parse_args()
root=Path(__file__).resolve().parent.parent
contract=args.clientkit_root/'Sources/Contract/SessionEndpoint'
sources=sorted(str(p) for p in contract.glob('*.swift') if p.name not in {'SessionGitService.swift','WorkspaceServiceEndpoint.swift'})
if not sources: raise SystemExit('ClientKit contract sources not found')
registry=(contract/'SessionEventRegistry.generated.swift').read_text()
entries=re.findall(r'type: "([^"]+)", projection: \.(\w+)',registry)
rust_registry=(root/'crates/contract/src/registry.rs').read_text()
required_text,surface_text=rust_registry.split('pub const SURFACE_EVENTS')
assert re.findall(r'"([^"]+)"',required_text)==[name for name,_ in entries], 'required event registry drift'
assert re.findall(r'"([^"]+)"',surface_text)==[name for name,kind in entries if kind=='surface'], 'surface event registry drift'

with tempfile.TemporaryDirectory(prefix='agentkit-swift-parity-') as tmp:
    # Swift top-level source requires main.swift when compiled with multiple files.
    main=Path(tmp)/'main.swift'; main.write_bytes((root/'scripts/clientkit-parity.swift').read_bytes())
    binary=Path(tmp)/'parity'
    subprocess.run(['swiftc','-module-name','TekesClientContract','-package-name','TekesClientKit',*sources,str(main),'-o',str(binary)],check=True)
    subprocess.run([str(binary),str(root/'crates/contract/tests/fixtures/parity.json')],check=True)
    subprocess.run(['cargo','test','--locked','-p','tekes-agent-contract','--test','parity'],cwd=root,check=True)
