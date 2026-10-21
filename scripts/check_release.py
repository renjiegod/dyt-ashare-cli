#!/usr/bin/env python3
"""Fail fast when release code changes omit the package version bump."""
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def cargo_version(path: pathlib.Path) -> str:
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("version ="):
            return line.split("=", 1)[1].strip().strip('"')
    raise SystemExit("Cargo.toml has no package version")


def lock_version(path: pathlib.Path) -> str:
    lines = path.read_text(encoding="utf-8").splitlines()
    for index, line in enumerate(lines):
        if line == 'name = "dyt-ashare-cli"':
            for candidate in lines[index + 1 : index + 4]:
                if candidate.startswith("version ="):
                    return candidate.split("=", 1)[1].strip().strip('"')
    raise SystemExit("Cargo.lock has no dyt-ashare-cli package entry")


def changed_files(base: str) -> list[str]:
    names: set[str] = set()
    for command in (
        ["git", "diff", "--name-only", f"{base}...HEAD"],
        ["git", "diff", "--name-only"],
        ["git", "diff", "--cached", "--name-only"],
        ["git", "ls-files", "--others", "--exclude-standard"],
    ):
        result = subprocess.run(
            command,
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        names.update(line for line in result.stdout.splitlines() if line)
    return sorted(names)


def main() -> int:
    current = cargo_version(ROOT / "Cargo.toml")
    locked = lock_version(ROOT / "Cargo.lock")
    if current != locked:
        raise SystemExit(
            f"Cargo.toml version {current} does not match Cargo.lock version {locked}; run cargo check once to refresh Cargo.lock"
        )

    base = os.environ.get("DYT_RELEASE_BASE", "origin/main")
    try:
        files = changed_files(base)
    except subprocess.CalledProcessError:
        files = []
    if not files:
        print(f"release check: {current}; no branch diff against {base}")
        return 0

    base_toml = subprocess.run(
        ["git", "show", f"{base}:Cargo.toml"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    base_version = next(
        line.split("=", 1)[1].strip().strip('"')
        for line in base_toml.splitlines()
        if line.startswith("version =")
    )
    release_paths = ("src/", "skills/", "scripts/package.py", "Cargo.toml", "Cargo.lock")
    release_change = any(path.startswith(release_paths) for path in files)
    version_changed = "Cargo.toml" in files and current != base_version
    if release_change and not version_changed:
        raise SystemExit(
            f"release check: {', '.join(files)} changes release code but package version remains {current}; bump Cargo.toml and refresh Cargo.lock"
        )
    print(f"release check: {current}; base={base_version}; version_changed={version_changed}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
