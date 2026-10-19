#!/usr/bin/env python3
"""Read-only consumption of an explicitly selected TekesUI development snapshot."""
import argparse, hashlib, json, shutil, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--source',type=Path,required=True)
p.add_argument('--capture',action='store_true',help='Explicitly replace the development source manifest; never a published dependency pin')
a=p.parse_args();src=a.source.resolve(strict=True);lock=ROOT/'ui/tekesui-source.json'
if lock.exists() and json.loads(lock.read_text())['kind']=='published-git-revision':
    p.error('published pin is resolved by Cargo; capture/path preparation is disabled')
crates={'controls','scene','tokens','text','render','platform-macos','motion','design-export'}
if a.capture:
    names={'Cargo.toml','LICENSE','design/tokens.json','design/gallery/brands.json','design/behavior/scroll.json'}
    listed=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=src).decode().split('\0')
    names.update(n for n in listed if any(n.startswith('crates/tekes-ui-'+c+'/') for c in crates) and (src/n).is_file() and not (src/n).is_symlink())
    for name in ('design/tokens.json','design/gallery/brands.json'):
        document=json.loads((src/name).read_text())
        names.update(document.get('merge',{}).get('inputs_sha256',{}))
        names.update(document.get('inputs_sha256',{}))
    files={n:hashlib.sha256((src/n).read_bytes()).hexdigest() for n in sorted(names)}
    digest=hashlib.sha256(json.dumps(files,sort_keys=True,separators=(',',':')).encode()).hexdigest()
    data={'kind':'development-snapshot-not-release','baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=src,text=True).strip(),'sha256':digest,'files':files}
    lock.write_text(json.dumps(data,indent=2)+'\n')
data=json.loads(lock.read_text());dest=ROOT/'ui/.deps/tekesui'
assert data['kind']=='development-snapshot-not-release'
assert hashlib.sha256(json.dumps(data['files'],sort_keys=True,separators=(',',':')).encode()).hexdigest()==data['sha256'],'manifest digest differs'
for name in data['files']:
    relative=Path(name)
    assert not relative.is_absolute() and '..' not in relative.parts and str(relative)==name,'unsafe snapshot path'

if dest.exists():
    # Never replace a different snapshot silently or accept unlisted source.
    assert {str(path.relative_to(dest)) for path in dest.rglob('*') if path.is_file()}==set(data['files']),'unlisted snapshot files'
    assert not any(path.is_symlink() for path in dest.rglob('*')),'snapshot symlink'

    for name,digest in data['files'].items():
        assert (dest/name).is_file() and hashlib.sha256((dest/name).read_bytes()).hexdigest()==digest, 'snapshot differs; use a new explicit source snapshot'
else:
    staging=dest.with_name('tekesui-staging')
    assert not staging.exists(),str(staging)
    staging.mkdir(parents=True)
    try:
        for name,digest in data['files'].items():
            relative=Path(name)
            assert not relative.is_absolute() and '..' not in relative.parts
            assert (src/relative).resolve().is_relative_to(src),'source escapes explicit root'
            value=(src/relative).read_bytes()
            assert hashlib.sha256(value).hexdigest()==digest,'source drift: '+name
            target=staging/relative;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(value)
        staging.rename(dest)
    except BaseException:
        shutil.rmtree(staging);raise
print('TEKESUI_SNAPSHOT_READY',data['sha256'],dest)
