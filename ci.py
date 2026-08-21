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

GAMEPLAY_TARGETS = {
    "workshop": "gameplay_workshop",
    "survival": "gameplay_survival",
    "progression": "gameplay_progression",
    "ore": "gameplay_ore",
    "foundry": "gameplay_foundry",
}
GAMEPLAY_SCOPES = ("all", *GAMEPLAY_TARGETS)


def cargo(alias: str) -> list[str]:
    return ["cargo", alias]


def quick_plan() -> list[tuple[str, list[str]]]:
    """Run the build-free edit-loop checks that are safe after every coherent text edit."""

    return [
        ("format", ["cargo", "fmt", "--check"]),
        (
            "repository contracts",
            [sys.executable, "tools/check_authority_docs.py"],
        ),
        ("local CI contracts", [sys.executable, "tools/test_ci.py"]),
    ]


def audit_plan() -> list[tuple[str, list[str]]]:
    """Run the broad maintained runtime checkpoint without optional long-horizon/lint shapes."""

    return [
        *quick_plan(),
        ("core", cargo("test-fast")),
        ("gameplay", gameplay_command("all")),
    ]


def gameplay_command(scope: str, *, nocapture: bool = False) -> list[str]:
    command = ["cargo", "test", "--quiet", "--locked", "--features", "test-gameplay"]
    targets = GAMEPLAY_TARGETS.values() if scope == "all" else [GAMEPLAY_TARGETS[scope]]
    for target in targets:
        command.extend(("--test", target))
    if nocapture:
        command.extend(("--", "--nocapture"))
    return command


def gameplay_plan(scope: str) -> list[tuple[str, list[str]]]:
    label = "gameplay" if scope == "all" else f"gameplay {scope}"
    return [(label, gameplay_command(scope))]


def exact_gameplay_command(name: str, *, ignored: bool = False) -> list[str]:
    command = [
        sys.executable,
        "tools/run_test.py",
        "--target",
        "gameplay_workshop",
        "--features",
        "test-gameplay",
        "--nocapture",
    ]
    if ignored:
        command.append("--ignored")
    command.append(name)
    return command


def report_plan() -> list[tuple[str, list[str]]]:
    return [
        (
            "workshop exploration",
            exact_gameplay_command("gameplay_harness_exploratory_report", ignored=True),
        ),
        (
            "workshop agency",
            exact_gameplay_command("gameplay_maintained_agency_counterfactuals"),
        ),
        ("survival probe", gameplay_command("survival", nocapture=True)),
        ("progression probe", gameplay_command("progression", nocapture=True)),
        ("ore probe", gameplay_command("ore", nocapture=True)),
        ("foundry probe", gameplay_command("foundry", nocapture=True)),
    ]


def plan_for(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    if args.preset == "quick":
        return quick_plan()
    if args.preset == "audit":
        return audit_plan()
    if args.preset == "report":
        return report_plan()

    plan = quick_plan()
    has_explicit_lane = any(
        (args.core, args.soak, args.gameplay, args.shaders, args.rustdoc, args.lint)
    )
    if args.soak:
        plan.append(("core + soak", cargo("test-all")))
    elif args.core:
        plan.append(("core", cargo("test-fast")))
    elif not has_explicit_lane:
        plan.append(("compile", cargo("check-fast")))
    if args.gameplay:
        plan.extend(gameplay_plan(args.gameplay))
    if args.shaders:
        plan.append(("shaders", cargo("test-shaders")))
    if args.rustdoc:
        plan.append(("rustdoc", cargo("test-doc")))
    if args.lint:
        plan.append(("clippy", cargo("test-lint")))
    return plan


def run_stage(
    index: int,
    total: int,
    label: str,
    command: list[str],
    *,
    echo_success: bool = False,
) -> float | None:
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
        if echo_success and result.stdout.strip():
            print(result.stdout.rstrip())
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
        choices=("quick", "gate", "audit", "report"),
        default="quick",
        help=(
            "build-free edit-loop check, coherent compile/test gate, broad maintained checkpoint, "
            "or explicit gameplay report"
        ),
    )
    parser.add_argument(
        "--lint",
        action="store_true",
        help="add production-library Clippy",
    )
    parser.add_argument(
        "--core",
        action="store_true",
        help="include the complete ordinary core behavior suite alongside selected specialized lanes",
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
            "workshop, survival, progression, ore, or foundry for a focused coherent gate"
        ),
    )
    parser.add_argument("--shaders", action="store_true", help="include WGSL validation")
    parser.add_argument(
        "--rustdoc",
        action="store_true",
        help="build Rust API documentation when that surface changed",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved stages without executing them",
    )
    args = parser.parse_args()
    if args.soak and args.core:
        parser.error("--soak already includes complete core behavior")
    if args.preset == "quick" and any(
        (args.core, args.lint, args.soak, args.gameplay, args.shaders, args.rustdoc)
    ):
        parser.error("quick is intentionally build-free and does not accept build-producing flags")
    if args.preset == "audit" and any(
        (args.core, args.lint, args.soak, args.gameplay, args.shaders, args.rustdoc)
    ):
        parser.error(
            "audit has a fixed runtime scope; run change-scoped lint/rustdoc/shader lanes separately"
        )
    if args.preset == "report" and any(
        (args.core, args.lint, args.soak, args.gameplay, args.shaders, args.rustdoc)
    ):
        parser.error("report is a fixed exploratory lane and does not accept gate flags")
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
            elapsed = run_stage(
                index,
                len(plan),
                label,
                command,
                echo_success=args.preset == "report",
            )
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
