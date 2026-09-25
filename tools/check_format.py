#!/usr/bin/env python3
"""Check Rust formatting without making the ordinary edit loop scan the whole crate."""

from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
FORMAT_POLICY_FILES = {"rustfmt.toml", ".rustfmt.toml"}


def git_paths(*arguments: str) -> list[str]:
    """Return NUL-delimited repository paths from one read-only Git query."""

    result = subprocess.run(
        ["git", *arguments, "-z"],
        cwd=ROOT,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        detail = result.stderr.decode(errors="replace").strip()
        raise RuntimeError(detail or f"git {' '.join(arguments)} failed")
    return [
        raw.decode(errors="surrogateescape")
        for raw in result.stdout.split(b"\0")
        if raw
    ]


def changed_repository_paths() -> list[str]:
    """Return staged, unstaged, and untracked paths once, including deletions."""

    return sorted({
        *git_paths("diff", "--name-only", "--diff-filter=ACMRD"),
        *git_paths("diff", "--cached", "--name-only", "--diff-filter=ACMRD"),
        *git_paths("ls-files", "--others", "--exclude-standard"),
    })


def changed_rust_files(relative_paths: list[str]) -> list[Path]:
    """Return existing changed Rust files in stable order."""

    return sorted(
        ROOT / relative
        for relative in relative_paths
        if relative.endswith(".rs") and (ROOT / relative).is_file()
    )


def formatting_policy_changed(relative_paths: list[str]) -> bool:
    """Return whether a global rustfmt policy file changed and requires full validation."""

    return any(relative in FORMAT_POLICY_FILES for relative in relative_paths)


def rust_edition() -> str:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return str(manifest["package"]["edition"])


def changed_format_command(paths: list[Path]) -> list[str]:
    """Build one rustfmt invocation that checks only the explicitly changed files."""

    return [
        "rustfmt",
        "--edition",
        rust_edition(),
        "--check",
        "--color",
        "never",
        "--config",
        "skip_children=true",
        *(str(path.relative_to(ROOT)) for path in paths),
    ]


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Check changed Rust files by default, or the full crate explicitly."
    )
    parser.add_argument(
        "--all",
        action="store_true",
        help="run Cargo's canonical full-crate formatting check",
    )
    return parser.parse_args(argv)


def run(command: list[str]) -> int:
    result = subprocess.run(command, cwd=ROOT, check=False)
    if result.returncode != 0:
        print(f"reproduce: {' '.join(command)}", file=sys.stderr)
    return result.returncode


def main() -> int:
    args = parse_args()
    if args.all:
        return run(["cargo", "fmt", "--check"])
    try:
        relative_paths = changed_repository_paths()
    except (OSError, RuntimeError, UnicodeError, tomllib.TOMLDecodeError) as error:
        print(f"format-check: {error}", file=sys.stderr)
        return 2
    if formatting_policy_changed(relative_paths):
        return run(["cargo", "fmt", "--check"])
    paths = changed_rust_files(relative_paths)
    if not paths:
        return 0
    try:
        return run(changed_format_command(paths))
    except (OSError, KeyError, tomllib.TOMLDecodeError) as error:
        print(f"format-check: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
