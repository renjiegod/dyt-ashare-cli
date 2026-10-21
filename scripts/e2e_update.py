#!/usr/bin/env python3
"""Real two-version installation/update/rollback, using an isolated test trust key and HTTP server."""
import functools, http.server, json, os, pathlib, shutil, subprocess, tempfile, threading
root=pathlib.Path(__file__).resolve().parent.parent
openssl=os.getenv('DYT_OPENSSL','openssl')
def call(args,**kwargs):return subprocess.run([str(x) for x in args],check=True,**kwargs)
with tempfile.TemporaryDirectory(prefix='dyt-e2e-') as temp:
 t=pathlib.Path(temp); key=t/'key.pem'
 call([openssl,'genpkey','-algorithm','ED25519','-out',key])
 public=subprocess.check_output([openssl,'pkey','-in',str(key),'-pubout','-outform','DER'])[-32:].hex()
 buildenv=dict(os.environ,DYT_ASHARE_RELEASE_PUBLIC_KEY=public,CARGO_TARGET_DIR=str(t/'target'))
 # Separate source copy prevents a test from modifying the development checkout or release key.
 source=t/'source';source.mkdir()
 for name in ['Cargo.toml','Cargo.lock','build.rs','release-public-key.hex','src','skills','.cargo']:
  p=root/name
  if p.is_dir():shutil.copytree(p,source/name)
  else:shutil.copyfile(p,source/name)
 (source/'scripts').mkdir()
 shutil.copyfile(root/'scripts/package.py',source/'scripts/package.py')
 shutil.copyfile(root/'scripts/install.sh',source/'scripts/install.sh')
 shutil.copyfile(root/'scripts/install.ps1',source/'scripts/install.ps1')
 target=subprocess.check_output(['rustc','-vV'],text=True).split('host: ')[1].splitlines()[0]
 exe='dytcn.exe' if os.name=='nt' else 'dytcn'
 out=t/'web';out.mkdir()
 class Handler(http.server.SimpleHTTPRequestHandler):
  def log_message(self,*args):pass
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(out)))
 thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
 base=f'http://127.0.0.1:{server.server_port}'
 original=(source/'Cargo.toml').read_text()
 original_version=next(line.split('=',1)[1].strip().strip('"') for line in original.splitlines() if line.startswith('version ='))
 try:
  for version in ['0.1.0','0.2.0']:
   (source/'Cargo.toml').write_text(original.replace(f'version = "{original_version}"',f'version = "{version}"',1))
   # Version-only package changes update the lockfile in this disposable copy.
   call(['cargo','build','--quiet'],cwd=source,env=buildenv)
   call(['python3' if os.name!='nt' else 'python',source/'scripts/package.py','--binary',t/'target/debug'/exe,'--target',target,'--version',version,'--base-url',base,'--key',key,'--out',out])
  assert (out/'install.sh').exists()
  assert (out/'install.ps1').exists()
  assert (out/version/target/exe).stat().st_mode & 0o111 or os.name == 'nt'
  user_home=t/'user-home'; user_home.mkdir()
  # Existing agent roots exercise the canonical file plus adapter links.
  for agent in ['.claude', '.codex', '.cursor', '.workbuddy-ai', '.workbuddy']:
   (user_home/agent).mkdir()
  env=dict(os.environ,DYT_ASHARE_HOME=str(t/'installed'),DYT_ASHARE_NO_UPDATE='1',HOME=str(user_home),USERPROFILE=str(user_home),CODEX_HOME=str(user_home/'.codex'))
  launcher=t/'installed/bin'/exe
  bootstrap=out/'0.1.0'/target/exe
  if os.name!='nt':bootstrap.chmod(0o755)
  def cli(*args,fail=False):
   p=subprocess.run([str(launcher),*args],env=env,capture_output=True,text=True)
   if fail:assert p.returncode!=0,(p.stdout,p.stderr)
   else:assert p.returncode==0,(p.stdout,p.stderr)
   return p
  if os.name=='nt':
   # Exercise the actual PowerShell installer, including user PATH and scheduled task registration.
   call(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',root/'scripts/install.ps1','-ManifestUrl',f'{base}/0.1.0/{target}/release.json'],env=env)
  else:call([bootstrap,'update','--manifest',out/'0.1.0'/target/'release.json'],env=env)
  assert cli('--version').stdout.strip()=='dytcn 0.1.0'
  canonical=user_home/'.agents/skills/dyt-ashare/SKILL.md'
  assert canonical.exists(), canonical
  assert (user_home/'.claude/skills/dyt-ashare/SKILL.md').exists()
  assert (user_home/'.codex/skills/dyt-ashare/SKILL.md').exists()
  assert (user_home/'.cursor/skills/dyt-ashare/SKILL.md').exists()
  assert (user_home/'.workbuddy-ai/skills/dyt-ashare/SKILL.md').exists()
  assert (user_home/'.workbuddy/skills/dyt-ashare/SKILL.md').exists()
  edited=canonical;edited.write_text('user owned change')
  cli('update','--manifest',str(out/'0.2.0'/target/'release.json'))
  assert cli('--version').stdout.strip()=='dytcn 0.2.0'
  assert edited.read_text()=='user owned change'
  doctor=json.loads(cli('doctor').stdout)
  assert doctor['updates']['installed']['version']=='0.2.0'
  assert doctor['skills_status']['pending']
  # Tampering a signed payload cannot change active.json.
  bad=json.loads((out/'0.2.0'/target/'release.json').read_text());bad['payload']=bad['payload'].replace('0.2.0','0.3.0')
  (t/'bad.json').write_text(json.dumps(bad));cli('update','--manifest',str(t/'bad.json'),fail=True)
  assert cli('--version').stdout.strip()=='dytcn 0.2.0'
  # A valid signature must not let a corrupted asset replace the active installation.
  signed=json.loads((out/'0.2.0'/target/'release.json').read_text())
  payload=json.loads(signed['payload']);payload['version']='0.3.0'
  plain=t/'payload';plain.write_bytes(json.dumps(payload,separators=(',',':')).encode())
  sig=t/'sig';call([openssl,'pkeyutl','-sign','-rawin','-inkey',key,'-in',plain,'-out',sig])
  signed=dict(payload=plain.read_text(),signature=sig.read_bytes().hex());(t/'bad-asset.json').write_text(json.dumps(signed))
  served=out/'0.2.0'/target/exe;original_binary=served.read_bytes();served.write_bytes(b'corrupted asset')
  cli('update','--manifest',str(t/'bad-asset.json'),fail=True);served.write_bytes(original_binary)
  assert cli('--version').stdout.strip()=='dytcn 0.2.0'
  cli('update','rollback');assert cli('--version').stdout.strip()=='dytcn 0.1.0'
  assert json.loads(cli('update','--background').stdout)['reason']=='disabled'
  cli('update','enable')
  env.pop('DYT_ASHARE_NO_UPDATE')
  # The same command used by the schedule performs an unattended installation.
  env['DYT_ASHARE_RELEASE_URL']=f'{base}/0.2.0/{target}/release.json'
  status=t/'installed/update-status.json';status.unlink()
  cli('update','--background');assert cli('--version').stdout.strip()=='dytcn 0.2.0'
  if os.name=='nt':
   task=json.loads((t/'installed/scheduled-task.json').read_text(encoding='utf-8-sig'))['name']
   call(['schtasks','/Query','/TN',task])
  print('PASS: installation, activation, edited Skills, signature rejection, rollback and unattended update')
 finally:
  if os.name=='nt':
   record=t/'installed/scheduled-task.json'
   if record.exists():
    task=json.loads(record.read_text(encoding='utf-8-sig'))['name'];subprocess.run(['schtasks','/Delete','/TN',task,'/F'],check=False)
   # Clean the temporary install's user PATH entry.
   binpath=str(t/'installed/bin').replace("'","''")
   subprocess.run(['powershell','-NoProfile','-Command',f"$p=[Environment]::GetEnvironmentVariable('Path','User');[Environment]::SetEnvironmentVariable('Path',(($p -split ';' | Where-Object {{$_ -ne '{binpath}'}}) -join ';'),'User')"],check=True)
  server.shutdown();server.server_close()
