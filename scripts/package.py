#!/usr/bin/env python3
"""Package a CLI and Skills; sign the exact UTF-8 payload with an offline Ed25519 key."""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, tempfile, time
p=argparse.ArgumentParser()
p.add_argument('--binary',required=True,type=pathlib.Path)
p.add_argument('--target',required=True)
p.add_argument('--version',required=True)
p.add_argument('--base-url',required=True)
p.add_argument('--key',required=True,type=pathlib.Path)
p.add_argument('--out',required=True,type=pathlib.Path)
a=p.parse_args()
root=pathlib.Path(__file__).resolve().parent.parent
prefix=a.out/a.version/a.target
prefix.mkdir(parents=True,exist_ok=True)
files=[]
for name,src in [('dytcn.exe' if 'windows' in a.target else 'dytcn',a.binary)]+[(str(s.relative_to(root)).replace(os.sep,'/'),s) for s in sorted((root/'skills').glob('*/SKILL.md'))]:
 dest=prefix/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dest)
 data=dest.read_bytes()
 files.append(dict(path=name,url=f'{a.base_url.rstrip("/")}/{a.version}/{a.target}/{name}',bytes=len(data),sha256=hashlib.sha256(data).hexdigest()))
payload=json.dumps(dict(version=a.version,target=a.target,expires_at=int(time.time())+14*86400,files=files),separators=(',',':'),ensure_ascii=False)
with tempfile.TemporaryDirectory() as tmp:
 plain=pathlib.Path(tmp)/'payload';sig=pathlib.Path(tmp)/'signature';plain.write_bytes(payload.encode())
 subprocess.run([os.getenv('DYT_OPENSSL','openssl'),'pkeyutl','-sign','-rawin','-inkey',str(a.key),'-in',str(plain),'-out',str(sig)],check=True)
 bootstrap=files[0]
 envelope=json.dumps(dict(bootstrap_url=bootstrap['url'],bootstrap_bytes=bootstrap['bytes'],bootstrap_sha256=bootstrap['sha256'],payload=payload,signature=sig.read_bytes().hex()),ensure_ascii=False).encode()
(prefix/'release.json').write_bytes(envelope)
(a.out/f'stable-{a.target}.json').write_bytes(envelope)
print(a.out/f'stable-{a.target}.json')
