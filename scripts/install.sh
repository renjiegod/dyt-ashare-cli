#!/bin/sh
set -eu
case "$(uname -s)/$(uname -m)" in
 Darwin/arm64) target=aarch64-apple-darwin;;
 Darwin/x86_64) target=x86_64-apple-darwin;;
 Linux/x86_64) target=x86_64-unknown-linux-gnu;;
 *) echo 'Unsupported platform; download a compatible portable bundle.' >&2; exit 1;;
esac
# The installer uses system Python only for metadata parsing; the installed CLI has no runtime dependency.
command -v python3 >/dev/null || { echo 'First installation needs python3 or use the portable binary.' >&2; exit 1; }
task_url="${DYT_CN_RELEASE_URL:-https://data.doyoutrade.cloud/releases/dyt-cn/stable-$target.json}"
task_tmp=$(mktemp -d)
trap 'rm -rf "$task_tmp"' EXIT HUP INT TERM
curl --proto '=https' --tlsv1.2 -fL --max-redirs 0 "$task_url" -o "$task_tmp/release.json"
python3 - "$task_tmp" "$target" <<'PY'
import hashlib,json,pathlib,subprocess,sys,urllib.parse
p=pathlib.Path(sys.argv[1]);m=json.loads(json.loads((p/'release.json').read_text())['payload'])
if m['target']!=sys.argv[2]:raise SystemExit('Target mismatch')
a=next(a for a in m['files'] if a['path']=='dyt-cn')
if not 0<a['bytes']<=134217728 or urllib.parse.urlsplit(a['url']).scheme!='https':raise SystemExit('Invalid bootstrap asset')
subprocess.run(['curl','--proto','=https','--tlsv1.2','-fL','--max-redirs','0',a['url'],'-o',str(p/'dyt-cn')],check=True)
b=(p/'dyt-cn').read_bytes()
if len(b)!=a['bytes'] or hashlib.sha256(b).hexdigest()!=a['sha256']:raise SystemExit('Bootstrap checksum mismatch')
(p/'dyt-cn').chmod(0o755)
PY
DYT_CN_CHILD=1 "$task_tmp/dyt-cn" update --manifest "$task_tmp/release.json"
task_root="${DYT_CN_HOME:-$HOME/.local/share/dyt-cn}"
"$task_root/bin/dyt-cn" skills install
printf '\nAdd %s/bin to PATH, then run dyt-cn auth login.\n' "$task_root"
