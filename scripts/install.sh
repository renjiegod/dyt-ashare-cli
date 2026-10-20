#!/bin/sh
set -eu
case "$(uname -s)/$(uname -m)" in
 Darwin/arm64) target=aarch64-apple-darwin;;
 Darwin/x86_64) target=x86_64-apple-darwin;;
 Linux/x86_64) target=x86_64-unknown-linux-gnu;;
 *) echo 'Unsupported platform; download a compatible portable bundle.' >&2; exit 1;;
esac
command -v curl >/dev/null || { echo 'First installation needs curl or use the portable binary.' >&2; exit 1; }
task_url="${DYT_ASHARE_RELEASE_URL:-https://data.doyoutrade.cloud/releases/dyt-ashare/stable-$target.json}"
task_tmp=$(mktemp -d)
trap 'rm -rf "$task_tmp"' EXIT HUP INT TERM
curl --proto '=https' --tlsv1.2 -fL --max-redirs 0 "$task_url" -o "$task_tmp/release.json"
json_string() { sed -n -E "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"([^\"]*)\".*/\1/p" "$task_tmp/release.json"; }
json_number() { sed -n -E "s/.*\"$1\"[[:space:]]*:[[:space:]]*([0-9]+).*/\1/p" "$task_tmp/release.json"; }
file_bytes() { wc -c < "$1" | tr -d '[:space:]'; }
file_sha256() {
  if command -v shasum >/dev/null; then
    shasum -a 256 "$1" | awk '{print $1}'
  elif command -v sha256sum >/dev/null; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v openssl >/dev/null; then
    openssl dgst -sha256 -r "$1" | awk '{print $1}'
  else
    echo 'First installation needs shasum, sha256sum or openssl.' >&2
    exit 1
  fi
}
bootstrap_url=$(json_string bootstrap_url)
bootstrap_bytes=$(json_number bootstrap_bytes)
bootstrap_sha256=$(json_string bootstrap_sha256)
case "$bootstrap_url" in https://*) ;; *) echo 'Invalid bootstrap URL.' >&2; exit 1;; esac
case "$bootstrap_bytes" in ''|*[!0-9]*) echo 'Invalid bootstrap size.' >&2; exit 1;; esac
[ "$bootstrap_bytes" -gt 0 ] && [ "$bootstrap_bytes" -le 134217728 ] || { echo 'Invalid bootstrap size.' >&2; exit 1; }
[ "${#bootstrap_sha256}" -eq 64 ] || { echo 'Invalid bootstrap checksum.' >&2; exit 1; }
case "$bootstrap_sha256" in *[!0-9a-fA-F]*) echo 'Invalid bootstrap checksum.' >&2; exit 1;; esac
curl --proto '=https' --tlsv1.2 -fL --max-redirs 0 "$bootstrap_url" -o "$task_tmp/dytcn"
[ "$(file_bytes "$task_tmp/dytcn")" = "$bootstrap_bytes" ] || { echo 'Bootstrap size mismatch.' >&2; exit 1; }
[ "$(file_sha256 "$task_tmp/dytcn")" = "$bootstrap_sha256" ] || { echo 'Bootstrap checksum mismatch.' >&2; exit 1; }
chmod 755 "$task_tmp/dytcn"
DYT_ASHARE_CHILD=1 "$task_tmp/dytcn" update --manifest "$task_tmp/release.json"
task_root="${DYT_ASHARE_HOME:-$HOME/.local/share/dyt-ashare}"
"$task_root/bin/dytcn" skills install
printf '\nAdd %s/bin to PATH, then run dytcn auth login.\n' "$task_root"
