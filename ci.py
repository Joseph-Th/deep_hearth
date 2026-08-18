#!/usr/bin/env python3
"""Fast, explicit local verification runner for a single-developer workspace."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parent


def cargo(alias: str) -> list[str]:
    return ["cargo", alias]


def plan_for(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    if args.preset == "hardening":
        return [
            ("format", ["cargo", "fmt", "--check"]),
            ("clippy all", cargo("test-lint-all")),
            ("core + soak", cargo("test-all")),
            ("gameplay selectors", [sys.executable, "tools/check_gameplay_aliases.py"]),
            ("gameplay", cargo("test-gameplay")),
            ("shaders", cargo("test-shaders")),
            ("docs", cargo("test-doc")),
        ]

    soak = args.soak or args.preset == "full"
    gameplay = args.gameplay or args.preset == "full"
    shaders = args.shaders
    docs = args.docs

    plan = [
        ("format", ["cargo", "fmt", "--check"]),
        (
            "core + soak" if soak else "core",
            cargo("test-all" if soak else "test-fast"),
        ),
    ]
    if args.lint:
        plan.append(("clippy", cargo("test-lint")))
    if gameplay:
        plan.append(("gameplay selectors", [sys.executable, "tools/check_gameplay_aliases.py"]))
        plan.append(("gameplay", cargo("test-gameplay")))
    if shaders:
        plan.append(("shaders", cargo("test-shaders")))
    if docs:
        plan.append(("docs", cargo("test-doc")))
    return plan


def run_stage(index: int, total: int, label: str, command: list[str]) -> bool:
    started = time.perf_counter()
    print(f"[{index}/{total}] {label} ... ", end="", flush=True)
    environment = os.environ.copy()
    environment.setdefault("CARGO_TERM_COLOR", "never")
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            env=environment,
            text=True,
            capture_output=True,
            check=False,
        )
    except OSError as error:
        elapsed = time.perf_counter() - started
        print(f"FAIL ({elapsed:.1f}s)")
        print(f"command: {' '.join(command)}", file=sys.stderr)
        print(f"unable to start command: {error}", file=sys.stderr)
        return False
    elapsed = time.perf_counter() - started
    if result.returncode == 0:
        print(f"PASS ({elapsed:.1f}s)")
        return True

    print(f"FAIL ({elapsed:.1f}s)")
    print(f"command: {' '.join(command)}", file=sys.stderr)
    if result.stdout.strip():
        print(result.stdout.rstrip(), file=sys.stderr)
    if result.stderr.strip():
        print(result.stderr.rstrip(), file=sys.stderr)
    return False


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run concise local verification without hosted CI or implicit change detection."
    )
    parser.add_argument(
        "preset",
        nargs="?",
        choices=("gate", "full", "hardening"),
        default="gate",
        help="gate is the fast local correctness path; full adds soak and gameplay coverage",
    )
    parser.add_argument(
        "--lint",
        action="store_true",
        help="add production-library Clippy; hardening always runs all-target/all-feature Clippy",
    )
    parser.add_argument("--soak", action="store_true", help="include ignored core soak tests")
    parser.add_argument("--gameplay", action="store_true", help="include the gameplay harness")
    parser.add_argument("--shaders", action="store_true", help="include WGSL validation")
    parser.add_argument("--docs", action="store_true", help="include documentation build")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved stages without executing them",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    plan = plan_for(args)
    if args.dry_run:
        for label, command in plan:
            print(f"{label}: {' '.join(command)}")
        return 0

    started = time.perf_counter()
    print(f"local-ci {args.preset}: {len(plan)} stage(s)")
    try:
        for index, (label, command) in enumerate(plan, start=1):
            if not run_stage(index, len(plan), label, command):
                return 1
    except KeyboardInterrupt:
        print("\nINTERRUPTED", file=sys.stderr)
        return 130
    print(f"PASS total ({time.perf_counter() - started:.1f}s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
