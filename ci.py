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

GAMEPLAY_SCOPES = ("all", "workshop", "survival", "progression", "ore", "foundry")
FOCUSED_GAMEPLAY_ALIASES = {
    "workshop": "test-gameplay-workshop",
    "survival": "test-gameplay-survival",
    "progression": "test-gameplay-progression",
    "ore": "test-gameplay-ore",
    "foundry": "test-gameplay-foundry",
}


def cargo(alias: str) -> list[str]:
    return ["cargo", alias]


def audit_plan() -> list[tuple[str, list[str]]]:
    """Run the broad maintained runtime checkpoint without optional long-horizon/lint shapes."""

    return [
        ("format", ["cargo", "fmt", "--check"]),
        ("core", cargo("test-fast")),
        ("gameplay", cargo("test-gameplay")),
        (
            "gameplay command policy",
            [sys.executable, "tools/check_gameplay_aliases.py", "--static"],
        ),
    ]


def gameplay_plan(scope: str) -> list[tuple[str, list[str]]]:
    if scope == "all":
        return [
            ("gameplay", cargo("test-gameplay")),
            (
                "gameplay command policy",
                [sys.executable, "tools/check_gameplay_aliases.py", "--static"],
            ),
        ]

    alias = FOCUSED_GAMEPLAY_ALIASES[scope]
    return [
        (f"gameplay {scope}", cargo(alias)),
        (
            "gameplay command policy",
            [sys.executable, "tools/check_gameplay_aliases.py", "--static"],
        ),
    ]


def plan_for(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    if args.preset == "audit":
        return audit_plan()

    plan = [("format", ["cargo", "fmt", "--check"])]
    has_explicit_lane = any(
        (args.core, args.soak, args.gameplay, args.shaders, args.docs, args.lint)
    )
    if args.soak:
        plan.append(("core + soak", cargo("test-all")))
    elif args.core or not has_explicit_lane:
        plan.append(("core", cargo("test-fast")))
    if args.gameplay:
        plan.extend(gameplay_plan(args.gameplay))
    if args.shaders:
        plan.append(("shaders", cargo("test-shaders")))
    if args.docs:
        plan.append(("docs", cargo("test-doc")))
    if args.lint:
        plan.append(("clippy", cargo("test-lint")))
    return plan


def run_stage(index: int, total: int, label: str, command: list[str]) -> float | None:
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
        print(f"reproduce: {' '.join(command)}", file=sys.stderr)
        print(f"unable to start command: {error}", file=sys.stderr)
        return None
    elapsed = time.perf_counter() - started
    if result.returncode == 0:
        print(f"PASS ({elapsed:.1f}s)")
        return elapsed

    print(f"FAIL ({elapsed:.1f}s)")
    print(f"reproduce: {' '.join(command)}", file=sys.stderr)
    if result.stdout.strip():
        print(result.stdout.rstrip(), file=sys.stderr)
    if result.stderr.strip():
        print(result.stderr.rstrip(), file=sys.stderr)
    return None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run concise local verification without hosted CI or implicit change detection."
    )
    parser.add_argument(
        "preset",
        nargs="?",
        choices=("gate", "audit"),
        default="gate",
        help="fast routine gate or broad maintained runtime checkpoint",
    )
    parser.add_argument(
        "--lint",
        action="store_true",
        help="add production-library Clippy",
    )
    parser.add_argument(
        "--core",
        action="store_true",
        help="include the ordinary core behavior suite alongside explicitly selected lanes",
    )
    parser.add_argument("--soak", action="store_true", help="include ignored core soak tests")
    parser.add_argument(
        "--gameplay",
        nargs="?",
        const="all",
        choices=GAMEPLAY_SCOPES,
        metavar="SCOPE",
        help=(
            "include gameplay verification; omit SCOPE for all maintained targets or choose "
            "workshop, survival, progression, ore, or foundry for a focused edit-loop gate"
        ),
    )
    parser.add_argument("--shaders", action="store_true", help="include WGSL validation")
    parser.add_argument("--docs", action="store_true", help="include documentation build")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved stages without executing them",
    )
    args = parser.parse_args()
    if args.preset == "audit" and any(
        (args.core, args.lint, args.soak, args.gameplay, args.shaders, args.docs)
    ):
        parser.error(
            "audit has a fixed runtime scope; run change-scoped lint/docs/shader lanes separately"
        )
    return args


def main() -> int:
    args = parse_args()
    plan = plan_for(args)
    if args.dry_run:
        for label, command in plan:
            print(f"{label}: {' '.join(command)}")
        return 0

    started = time.perf_counter()
    timings: list[tuple[str, float]] = []
    print(f"local-ci {args.preset}: {len(plan)} stage(s)")
    try:
        for index, (label, command) in enumerate(plan, start=1):
            elapsed = run_stage(index, len(plan), label, command)
            if elapsed is None:
                return 1
            timings.append((label, elapsed))
    except KeyboardInterrupt:
        print("\nINTERRUPTED", file=sys.stderr)
        return 130
    total_elapsed = time.perf_counter() - started
    slowest_label, slowest_elapsed = max(timings, key=lambda item: item[1])
    print(
        f"PASS total ({total_elapsed:.1f}s; slowest={slowest_label} {slowest_elapsed:.1f}s)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
