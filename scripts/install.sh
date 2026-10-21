#!/bin/sh
set -eu

no_modify_path=0
for arg in "$@"; do
  case "$arg" in
    --no-modify-path)
      no_modify_path=1
      ;;
    -h|--help)
      cat <<'EOF'
DYT AShare installer

Usage: install.sh [--no-modify-path]

  --no-modify-path  install the CLI without changing shell configuration files
EOF
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      echo "Run with --help for usage." >&2
      exit 2
      ;;
  esac
done

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

path_entry="$task_root/bin"
path_configured=0
config_file=''
shell_value=${SHELL:-sh}
current_shell=${shell_value##*/}
xdg_config_home=${XDG_CONFIG_HOME:-$HOME/.config}
zsh_config_home=${ZDOTDIR:-$HOME}

case "$current_shell" in
  fish) path_command="fish_add_path \"$path_entry\"" ;;
  *) path_command="export PATH=\"${path_entry}:\$PATH\"" ;;
esac

select_existing_config() {
  for candidate in "$@"; do
    if [ -f "$candidate" ]; then
      config_file=$candidate
      return 0
    fi
  done
  return 1
}

case "$current_shell" in
  zsh)
    if ! select_existing_config \
      "$zsh_config_home/.zshrc" \
      "$zsh_config_home/.zshenv" \
      "$xdg_config_home/zsh/.zshrc" \
      "$xdg_config_home/zsh/.zshenv"; then
      config_file="$zsh_config_home/.zshrc"
    fi
    ;;
  bash)
    if ! select_existing_config \
      "$HOME/.bashrc" \
      "$HOME/.bash_profile" \
      "$HOME/.profile" \
      "$xdg_config_home/bash/.bashrc" \
      "$xdg_config_home/bash/.bash_profile"; then
      config_file="$HOME/.bashrc"
    fi
    ;;
  fish)
    if ! select_existing_config "$HOME/.config/fish/config.fish"; then
      config_file="$HOME/.config/fish/config.fish"
    fi
    ;;
  sh|ash)
    config_file="$HOME/.profile"
    ;;
  *)
    config_file=''
    ;;
esac

add_path_to_config() {
  if [ -f "$config_file" ] && grep -Fqx "$path_command" "$config_file"; then
    path_configured=1
    return 0
  fi

  config_dir=${config_file%/*}
  if [ ! -d "$config_dir" ] && ! mkdir -p "$config_dir" 2>/dev/null; then
    return 1
  fi
  if [ ! -e "$config_file" ] && ! : > "$config_file" 2>/dev/null; then
    return 1
  fi
  if [ ! -w "$config_file" ]; then
    return 1
  fi

  {
    printf '\n# dyt-ashare\n'
    printf '%s\n' "$path_command"
  } >> "$config_file"
  path_configured=1
}

if [ "$no_modify_path" -eq 1 ]; then
  printf '\nSkipped PATH configuration (--no-modify-path).\n'
elif [ -z "$config_file" ]; then
  printf '\nCould not identify a shell configuration file for %s.\n' "$current_shell" >&2
  printf 'Add this line manually:\n  %s\n' "$path_command"
else
  path_in_current=0
  case ":${PATH:-}:" in
    *":$path_entry:"*)
      path_in_current=1
      path_configured=1
      ;;
    *)
      if ! add_path_to_config; then
        printf '\nCould not update %s. Add this line manually:\n  %s\n' \
          "$config_file" "$path_command" >&2
      fi
      ;;
  esac
  if [ "$path_configured" -eq 1 ] && [ "$path_in_current" -eq 0 ]; then
    printf '\nPATH is configured in %s. Run `exec $SHELL` or open a new terminal.\n' "$config_file"
  fi
fi

printf 'Run: dytcn auth login\n'
