#!/usr/bin/env python3
"""Run or list exact Rust tests without silently accepting an empty selector."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[1]
TEST_LINE = re.compile(r"^(?P<name>.+): test$")
ZERO_TESTS = re.compile(r"\brunning 0 tests\b")


def cargo_command(args: argparse.Namespace) -> list[str]:
    command = ["cargo", "test", "--quiet", "--locked"]
    if args.target == "lib":
        command.append("--lib")
    else:
        command.extend(("--test", args.target))
    if args.features:
        command.extend(("--features", args.features))
    if args.list:
        command.extend(("--", "--list"))
        return command
    command.extend((args.name, "--", "--exact"))
    if args.ignored:
        command.append("--ignored")
    if args.nocapture:
        command.append("--nocapture")
    return command


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run one exact cached Rust test, or list the authoritative live test catalog."
    )
    parser.add_argument("name", nargs="?", help="fully qualified exact test name")
    parser.add_argument("--list", action="store_true", help="list tests instead of executing one")
    parser.add_argument("--target", default="lib", help="Cargo test target name; defaults to lib")
    parser.add_argument("--features", help="Cargo feature set required by the selected target")
    parser.add_argument("--ignored", action="store_true", help="select an ignored exact test")
    parser.add_argument("--nocapture", action="store_true", help="show exact-test output")
    args = parser.parse_args()
    if not args.list and not args.name:
        parser.error("an exact test name is required unless --list is used")
    if args.list and (args.ignored or args.nocapture):
        parser.error("--ignored and --nocapture apply only to exact execution")
    return args


def main() -> int:
    args = parse_args()
    command = cargo_command(args)
    environment = os.environ.copy()
    environment.setdefault("CARGO_TERM_COLOR", "never")
    started = time.perf_counter()
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )
    elapsed = time.perf_counter() - started
    if result.returncode != 0:
        print(f"FAIL ({elapsed:.1f}s)", file=sys.stderr)
        print(f"reproduce: {' '.join(command)}", file=sys.stderr)
        if result.stdout.strip():
            print(result.stdout.rstrip(), file=sys.stderr)
        if result.stderr.strip():
            print(result.stderr.rstrip(), file=sys.stderr)
        return result.returncode

    if args.list:
        names = [
            match.group("name")
            for line in result.stdout.splitlines()
            if (match := TEST_LINE.match(line)) is not None
        ]
        if args.name:
            names = [name for name in names if args.name in name]
        for name in names:
            print(name)
        print(f"{len(names)} test(s)")
        return 0

    if ZERO_TESTS.search(result.stdout):
        print(f"FAIL exact test not found: {args.name}", file=sys.stderr)
        print(f"catalog: python tools/run_test.py --list {args.name}", file=sys.stderr)
        return 2

    if args.nocapture and result.stdout.strip():
        print(result.stdout.rstrip())
    print(f"PASS {args.target}::{args.name} ({elapsed:.1f}s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
