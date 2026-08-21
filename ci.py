#!/usr/bin/env python3
"""Fast, explicit local verification runner for a single-developer workspace."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
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
GAMEPLAY_SCOPE_BY_TARGET = {target: scope for scope, target in GAMEPLAY_TARGETS.items()}
FAILED_TEST = re.compile(r"^    (?P<name>[A-Za-z0-9_:]+)$", re.MULTILINE)
FAILED_GAMEPLAY_TARGET = re.compile(r"to rerun pass `--test (?P<target>gameplay_[a-z_]+)`")


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


def repair_hint(command: list[str], stdout: str, stderr: str) -> str | None:
    """Return the narrow follow-up command for a failed broad executable lane when detectable."""

    combined = f"{stdout}\n{stderr}"
    if command == cargo("test-fast"):
        failed = FAILED_TEST.findall(combined)
        if failed:
            return f"python tools/run_test.py {failed[-1]}"
    if "test-gameplay" in command:
        failed_targets = FAILED_GAMEPLAY_TARGET.findall(combined)
        if failed_targets:
            target = failed_targets[-1]
            failed = FAILED_TEST.findall(combined)
            if failed:
                return f"python tools/run_test.py --target {target} {failed[-1]}"
            scope = GAMEPLAY_SCOPE_BY_TARGET.get(target)
            if scope is not None:
                return f"python ci.py gate --gameplay {scope}"
    return None


def audit_plan(scope: str) -> list[tuple[str, list[str]]]:
    """Run an explicitly selected broad runtime audit surface."""

    if scope not in ("core", "gameplay", "all"):
        raise ValueError(f"unknown audit scope: {scope}")

    plan = quick_plan()
    if scope in ("core", "all"):
        plan.append(("core", cargo("test-fast")))
    if scope in ("gameplay", "all"):
        plan.append(("gameplay", gameplay_command("all")))
    return plan


def gameplay_targets_command(scopes: tuple[str, ...], *, nocapture: bool = False) -> list[str]:
    command = ["cargo", "test", "--quiet", "--locked", "--features", "test-gameplay"]
    for scope in scopes:
        command.extend(("--test", GAMEPLAY_TARGETS[scope]))
    if nocapture:
        command.extend(("--", "--nocapture"))
    return command


def gameplay_command(scope: str, *, nocapture: bool = False) -> list[str]:
    scopes = tuple(GAMEPLAY_TARGETS) if scope == "all" else (scope,)
    return gameplay_targets_command(scopes, nocapture=nocapture)


def gameplay_plan(scope: str) -> list[tuple[str, list[str]]]:
    label = "gameplay" if scope == "all" else f"gameplay {scope}"
    return [(label, gameplay_command(scope))]


def exact_gameplay_command(name: str, *, ignored: bool = False) -> list[str]:
    command = [
        sys.executable,
        "tools/run_test.py",
        "--target",
        "gameplay_workshop",
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
            exact_gameplay_command("agency::gameplay_maintained_agency_counterfactuals"),
        ),
        (
            "focused probes",
            gameplay_targets_command(
                ("survival", "progression", "ore", "foundry"),
                nocapture=True,
            ),
        ),
    ]


def plan_for(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    if args.preset == "quick":
        return quick_plan()
    if args.preset == "audit":
        if args.all:
            return audit_plan("all")
        if args.core:
            return audit_plan("core")
        if args.gameplay:
            return audit_plan("gameplay")
        raise ValueError(
            "audit requires an explicit scope: use `--core`, `--gameplay`, or `--all`"
        )
    if args.preset == "report":
        return report_plan()

    if args.all:
        raise ValueError("broad verification is audit-only; use `python ci.py audit --all`")
    if args.core:
        raise ValueError("complete core behavior is audit-only; use `python ci.py audit --core`")
    if args.gameplay == "all":
        raise ValueError(
            "all-gameplay verification is audit-only; use `python ci.py audit --gameplay`"
        )

    selected_lanes = sum(
        bool(selected)
        for selected in (
            args.soak,
            args.gameplay,
            args.shaders,
            args.rustdoc,
            args.lint,
        )
    )
    if selected_lanes > 1:
        raise ValueError("gate accepts exactly one build-producing lane at a time")

    plan = quick_plan()
    if args.soak:
        plan.append(("soak", cargo("test-soak")))
    elif args.gameplay:
        plan.extend(gameplay_plan(args.gameplay))
    elif args.shaders:
        plan.append(("shaders", cargo("test-shaders")))
    elif args.rustdoc:
        plan.append(("rustdoc", cargo("test-doc")))
    elif args.lint:
        plan.append(("clippy", cargo("test-lint")))
    else:
        plan.append(("compile", cargo("check-fast")))
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
    environment["CARGO_TERM_COLOR"] = "never"
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
    if hint := repair_hint(command, result.stdout, result.stderr):
        print(f"repair: {hint}", file=sys.stderr)
    return None


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
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
    lane = parser.add_mutually_exclusive_group()
    lane.add_argument(
        "--lint",
        action="store_true",
        help="run production-library Clippy as the gate's single build lane",
    )
    lane.add_argument(
        "--all",
        action="store_true",
        help="run both maintained audit surfaces; valid only with the audit preset",
    )
    lane.add_argument(
        "--core",
        action="store_true",
        help="run the complete ordinary core behavior suite as an explicit audit-only lane",
    )
    lane.add_argument(
        "--soak",
        action="store_true",
        help="run ignored long-horizon soak tests as the gate's single build lane",
    )
    lane.add_argument(
        "--gameplay",
        nargs="?",
        const="all",
        choices=GAMEPLAY_SCOPES,
        metavar="SCOPE",
        help=(
            "run gameplay verification; gate requires an explicit focused scope, while audit accepts "
            "omitted SCOPE/all for every maintained gameplay target"
        ),
    )
    lane.add_argument(
        "--shaders",
        action="store_true",
        help="run WGSL validation as the gate's single build lane",
    )
    lane.add_argument(
        "--rustdoc",
        action="store_true",
        help="build Rust API documentation as the gate's single build lane",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved stages without executing them",
    )
    args = parser.parse_args(argv)
    if args.preset == "quick" and any(
        (args.all, args.core, args.lint, args.soak, args.gameplay, args.shaders, args.rustdoc)
    ):
        parser.error("quick is intentionally build-free and does not accept build-producing flags")
    if args.preset == "audit" and any((args.lint, args.soak, args.shaders, args.rustdoc)):
        parser.error(
            "audit has a fixed runtime scope; run change-scoped lint/rustdoc/shader lanes separately"
        )
    if args.preset == "audit" and not any((args.all, args.core, args.gameplay)):
        parser.error("audit requires an explicit scope: --core, --gameplay, or --all")
    if args.preset == "audit" and args.gameplay not in (None, "all"):
        parser.error(
            "focused gameplay belongs in gate; audit --gameplay always means all maintained gameplay targets"
        )
    if args.preset == "gate" and args.core:
        parser.error("complete core behavior is audit-only; use `python ci.py audit --core`")
    if args.preset == "gate" and args.all:
        parser.error("broad verification is audit-only; use `python ci.py audit --all`")
    if args.preset == "gate" and args.gameplay == "all":
        parser.error(
            "gate requires an explicit gameplay scope; use `python ci.py audit --gameplay` for all targets"
        )
    if args.preset == "report" and any(
        (args.all, args.core, args.lint, args.soak, args.gameplay, args.shaders, args.rustdoc)
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
