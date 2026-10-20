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
 buildenv=dict(os.environ,DYT_CN_RELEASE_PUBLIC_KEY=public,CARGO_TARGET_DIR=str(t/'target'))
 # Separate source copy prevents a test from modifying the development checkout or release key.
 source=t/'source';source.mkdir()
 for name in ['Cargo.toml','Cargo.lock','build.rs','release-public-key.hex','src','skills','.cargo']:
  p=root/name
  if p.is_dir():shutil.copytree(p,source/name)
  else:shutil.copyfile(p,source/name)
 target=subprocess.check_output(['rustc','-vV'],text=True).split('host: ')[1].splitlines()[0]
 exe='dyt-cn.exe' if os.name=='nt' else 'dyt-cn'
 out=t/'web';out.mkdir()
 class Handler(http.server.SimpleHTTPRequestHandler):
  def log_message(self,*args):pass
 server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(out)))
 thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
 base=f'http://127.0.0.1:{server.server_port}'
 original=(source/'Cargo.toml').read_text()
 try:
  for version in ['0.1.0','0.2.0']:
   (source/'Cargo.toml').write_text(original.replace('version = "0.1.0"',f'version = "{version}"',1))
   # Version-only package changes update the lockfile in this disposable copy.
   call(['cargo','build','--quiet'],cwd=source,env=buildenv)
   call(['python3' if os.name!='nt' else 'python',root/'scripts/package.py','--binary',t/'target/debug'/exe,'--target',target,'--version',version,'--base-url',base,'--key',key,'--out',out])
  env=dict(os.environ,DYT_CN_HOME=str(t/'installed'),DYT_CN_NO_UPDATE='1')
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
   call(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',root/'scripts/install.ps1','-ManifestUrl',f'{base}/0.1.0/{target}/release.json','-NoSkills'],env=env)
  else:call([bootstrap,'update','--manifest',out/'0.1.0'/target/'release.json'],env=env)
  assert cli('--version').stdout.strip()=='dyt-cn 0.1.0'
  skills=t/'user-skills';cli('skills','install','--dir',str(skills))
  edited=skills/'dyt-cn-research/SKILL.md';edited.write_text('user owned change')
  cli('update','--manifest',str(out/'0.2.0'/target/'release.json'))
  assert cli('--version').stdout.strip()=='dyt-cn 0.2.0'
  assert edited.read_text()=='user owned change'
  doctor=json.loads(cli('doctor').stdout)
  assert doctor['updates']['installed']['version']=='0.2.0'
  assert doctor['skills_status']['pending']
  # Tampering a signed payload cannot change active.json.
  bad=json.loads((out/'0.2.0'/target/'release.json').read_text());bad['payload']=bad['payload'].replace('0.2.0','0.3.0')
  (t/'bad.json').write_text(json.dumps(bad));cli('update','--manifest',str(t/'bad.json'),fail=True)
  assert cli('--version').stdout.strip()=='dyt-cn 0.2.0'
  cli('update','rollback');assert cli('--version').stdout.strip()=='dyt-cn 0.1.0'
  assert json.loads(cli('update','--background').stdout)['reason']=='disabled'
  cli('update','enable')
  env.pop('DYT_CN_NO_UPDATE')
  # The same command used by the schedule performs an unattended installation.
  env['DYT_CN_RELEASE_URL']=f'{base}/0.2.0/{target}/release.json'
  status=t/'installed/update-status.json';status.unlink()
  cli('update','--background');assert cli('--version').stdout.strip()=='dyt-cn 0.2.0'
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
