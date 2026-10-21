#!/usr/bin/env python3
"""Package a CLI and Skills; sign the exact UTF-8 payload with an offline Ed25519 key."""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tempfile, time
try:
    import tomllib
except ModuleNotFoundError:
    tomllib = None
# Signed channel metadata stays valid for seven days. A leaked signing key is
# therefore usable for at most a week, and the stable manifest has to be
# re-signed and re-uploaded before it expires even when no binary changed.
VALIDITY_SECONDS=7*86400
p=argparse.ArgumentParser()
p.add_argument('--binary',required=True,type=pathlib.Path)
p.add_argument('--target',required=True)
p.add_argument('--version',help='Release version; defaults to Cargo.toml and must match it when supplied')
p.add_argument('--base-url',required=True)
p.add_argument('--key',required=True,type=pathlib.Path)
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
# The bootstrap installers are intentionally outside the signed per-target
# manifest: they select the host target and then consume that manifest. Keep
# them beside stable-*.json in the package output so a release upload cannot
# accidentally omit the URLs advertised in README/DEVELOPING.md.
for name in ['install.sh', 'install.ps1']:
 src=root/'scripts'/name
 dest=a.out/name
 shutil.copy2(src,dest)
payload=json.dumps(dict(version=version,target=a.target,expires_at=int(time.time())+VALIDITY_SECONDS,files=files),separators=(',',':'),ensure_ascii=False)
with tempfile.TemporaryDirectory() as tmp:
 plain=pathlib.Path(tmp)/'payload';sig=pathlib.Path(tmp)/'signature';plain.write_bytes(payload.encode())
 subprocess.run([os.getenv('DYT_OPENSSL','openssl'),'pkeyutl','-sign','-rawin','-inkey',str(a.key),'-in',str(plain),'-out',str(sig)],check=True)
 bootstrap=files[0]
 envelope=json.dumps(dict(bootstrap_url=bootstrap['url'],bootstrap_bytes=bootstrap['bytes'],bootstrap_sha256=bootstrap['sha256'],payload=payload,signature=sig.read_bytes().hex()),ensure_ascii=False).encode()
(prefix/'release.json').write_bytes(envelope)
(a.out/f'stable-{a.target}.json').write_bytes(envelope)
print(a.out/f'stable-{a.target}.json')
