#!/usr/bin/env python3
"""Package a CLI and Skills into an unsigned channel manifest and its repository anchor."""
import argparse, hashlib, json, os, pathlib, shutil
try:
    import tomllib
except ModuleNotFoundError:
    tomllib = None
p=argparse.ArgumentParser()
p.add_argument('--binary',required=True,type=pathlib.Path)
p.add_argument('--target',required=True)
p.add_argument('--version',help='Release version; defaults to Cargo.toml and must match it when supplied')
p.add_argument('--base-url',required=True)
p.add_argument('--out',required=True,type=pathlib.Path)
a=p.parse_args()
root=pathlib.Path(__file__).resolve().parent.parent
if tomllib is not None:
    package_version=tomllib.loads((root/'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
else:
    package_version=next(line.split('=',1)[1].strip().strip('"') for line in (root/'Cargo.toml').read_text(encoding='utf-8').splitlines() if line.startswith('version ='))
version=a.version or package_version
if version != package_version:
    raise SystemExit(f'--version {version} does not match Cargo.toml version {package_version}; bump Cargo.toml first')
prefix=a.out/version/a.target
prefix.mkdir(parents=True,exist_ok=True)
files=[]
for name,src in [('dytcn.exe' if 'windows' in a.target else 'dytcn',a.binary)]+[(str(s.relative_to(root)).replace(os.sep,'/'),s) for s in sorted((root/'skills').glob('*/SKILL.md'))]:
    dest=prefix/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src,dest)
    data=dest.read_bytes()
    files.append(dict(path=name,url=f'{a.base_url.rstrip("/")}/{version}/{a.target}/{name}',bytes=len(data),sha256=hashlib.sha256(data).hexdigest()))
bootstrap=files[0]
manifest=dict(version=version,target=a.target,bootstrap_url=bootstrap['url'],bootstrap_bytes=bootstrap['bytes'],bootstrap_sha256=bootstrap['sha256'],files=files)
# The manifest is deterministic: the same artifacts always produce the same
# bytes, and therefore the same repository anchor.
raw=json.dumps(manifest,indent=2,sort_keys=True).encode()
(prefix/'release.json').write_bytes(raw)
(a.out/f'channel-{a.target}.json').write_bytes(raw)
anchor=a.out/'releases'/version
anchor.mkdir(parents=True,exist_ok=True)
(anchor/f'{a.target}.sha256').write_text(f"{hashlib.sha256(raw).hexdigest()}  channel-{a.target}.json\n")
# The bootstrap installers are intentionally outside the channel manifest: they
# select the host target and then consume that manifest. Keep them beside the
# channel file so a release upload cannot accidentally omit the URLs
# advertised in README/DEVELOPING.md.
for name in ['install.sh', 'install.ps1']:
    shutil.copy2(root/'scripts'/name,a.out/name)
print(a.out/f'channel-{a.target}.json')
