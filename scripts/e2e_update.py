#!/usr/bin/env python3
"""Real two-version installation/update/rollback against an anchored release channel."""
import functools, hashlib, http.server, json, os, pathlib, shutil, subprocess, tempfile, threading
root=pathlib.Path(__file__).resolve().parent.parent
def call(args,**kwargs):return subprocess.run([str(x) for x in args],check=True,**kwargs)
with tempfile.TemporaryDirectory(prefix='dyt-e2e-') as temp:
 t=pathlib.Path(temp)
 buildenv=dict(os.environ,CARGO_TARGET_DIR=str(t/'target'))
 # Separate source copy prevents a test from modifying the development checkout.
 source=t/'source';source.mkdir()
 for name in ['Cargo.toml','Cargo.lock','build.rs','src','skills','.cargo']:
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
 def package(version,anchor=True):
  (source/'Cargo.toml').write_text(original.replace(f'version = "{original_version}"',f'version = "{version}"',1))
  # Version-only package changes update the lockfile in this disposable copy.
  call(['cargo','build','--quiet'],cwd=source,env=buildenv)
  call(['python3' if os.name!='nt' else 'python',source/'scripts/package.py','--binary',t/'target/debug'/exe,'--target',target,'--version',version,'--base-url',base,'--out',out])
  # The client fetches the anchor from <base>/v<version>/releases/<version>/<target>.sha256.
  if anchor:
   served=out/f'v{version}'/'releases'/version
   served.mkdir(parents=True,exist_ok=True)
   shutil.copyfile(out/'releases'/version/f'{target}.sha256',served/f'{target}.sha256')
  return out/version/target/'release.json'
 def publish_channel(version):
  shutil.copyfile(out/version/target/'release.json',out/f'channel-{target}.json')
 try:
  for version in ['0.1.0','0.2.0']:package(version)
  # 0.3.0 is packaged without its anchor so the missing-anchor case can be exercised.
  package('0.3.0',anchor=False)
  publish_channel('0.1.0')
  # The channel manifest is unsigned and has no expiry; the repository anchor pins its bytes.
  manifest=json.loads((out/'0.1.0'/target/'release.json').read_text())
  assert 'payload' not in manifest and 'signature' not in manifest and 'expires_at' not in manifest
  anchor=(out/'v0.1.0'/'releases'/'0.1.0'/f'{target}.sha256').read_text().split()[0]
  assert anchor==hashlib.sha256((out/'0.1.0'/target/'release.json').read_bytes()).hexdigest()
  assert (out/'install.sh').exists()
  assert (out/'install.ps1').exists()
  assert (out/'0.1.0'/target/exe).stat().st_mode & 0o111 or os.name == 'nt'
  user_home=t/'user-home'; user_home.mkdir()
  # Existing agent roots exercise the canonical file plus adapter links.
  for agent in ['.claude', '.codex', '.cursor', '.workbuddy-ai', '.workbuddy']:
   (user_home/agent).mkdir()
  env=dict(os.environ,DYT_ASHARE_HOME=str(t/'installed'),DYT_ASHARE_NO_UPDATE='1',HOME=str(user_home),USERPROFILE=str(user_home),CODEX_HOME=str(user_home/'.codex'),DYT_ASHARE_ANCHOR_URL=base)
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
   call(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',root/'scripts/install.ps1','-ManifestUrl',f'{base}/channel-{target}.json'],env=env)
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
  assert doctor['updates']['channel']['verification']=='sha256+anchor'
  # A tampered channel manifest no longer matches the repository anchor.
  tampered=json.loads((out/'0.2.0'/target/'release.json').read_text())
  tampered['bootstrap_bytes']=int(tampered['bootstrap_bytes'])+1
  (t/'tampered.json').write_text(json.dumps(tampered))
  failed=cli('update','--manifest',str(t/'tampered.json'),fail=True)
  assert 'anchor' in failed.stderr.lower(), failed.stderr
  assert cli('--version').stdout.strip()=='dytcn 0.2.0'
  # An unreachable anchor is rejected too; there is no unanchored fallback.
  failed=cli('update','--manifest',str(out/'0.3.0'/target/'release.json'),fail=True)
  assert 'anchor' in failed.stderr.lower(), failed.stderr
  assert cli('--version').stdout.strip()=='dytcn 0.2.0'
  # A valid anchor must not let a corrupted asset replace the active installation.
  package('0.3.0')
  served=out/'0.3.0'/target/exe;original_binary=served.read_bytes();served.write_bytes(b'corrupted asset')
  cli('update','--manifest',str(out/'0.3.0'/target/'release.json'),fail=True);served.write_bytes(original_binary)
  assert cli('--version').stdout.strip()=='dytcn 0.2.0'
  cli('update','rollback');assert cli('--version').stdout.strip()=='dytcn 0.1.0'
  assert json.loads(cli('update','--background').stdout)['reason']=='disabled'
  cli('update','enable')
  env.pop('DYT_ASHARE_NO_UPDATE')
  # The same command used by the schedule performs an unattended installation.
  publish_channel('0.2.0')
  env['DYT_ASHARE_RELEASE_URL']=f'{base}/channel-{target}.json'
  status=t/'installed/update-status.json';status.unlink()
  cli('update','--background');assert cli('--version').stdout.strip()=='dytcn 0.2.0'
  if os.name=='nt':
   task=json.loads((t/'installed/scheduled-task.json').read_text(encoding='utf-8-sig'))['name']
   call(['schtasks','/Query','/TN',task])
  print('PASS: installation, activation, edited Skills, anchor verification, rollback and unattended update')
 finally:
  if os.name=='nt':
   record=t/'installed/scheduled-task.json'
   if record.exists():
    task=json.loads(record.read_text(encoding='utf-8-sig'))['name'];subprocess.run(['schtasks','/Delete','/TN',task,'/F'],check=False)
   # Clean the temporary install's user PATH entry.
   binpath=str(t/'installed/bin').replace("'","''")
   subprocess.run(['powershell','-NoProfile','-Command',f"$p=[Environment]::GetEnvironmentVariable('Path','User');[Environment]::SetEnvironmentVariable('Path',(($p -split ';' | Where-Object {{$_ -ne '{binpath}'}}) -join ';'),'User')"],check=True)
  server.shutdown();server.server_close()
