#!/usr/bin/env python3
"""Fast, explicit local verification runner for a single-developer workspace."""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parent

GAMEPLAY_AUDIT_TARGET = "gameplay_audit"
GAMEPLAY_TESTS = {
    "workshop": "workshop_contract_tests::gameplay_harness_gate",
    "survival": "focused::gameplay_survival_provisioning_probe",
    "progression": "focused::gameplay_primitive_progression_probe",
    "ore": "focused::gameplay_ore_preparation_probe",
    "foundry": "focused::gameplay_foundry_probe",
}


def configure_report_replay_environment(
    environ,
    *,
    randbits=secrets.randbits,
) -> tuple[str, str]:
    """Give exploratory gameplay a fresh bounded sample unless the caller requested a replay."""

    variation_key = "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED"
    behavior_key = "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED"
    if variation_key not in environ:
        environ[variation_key] = f"0x{randbits(64):016X}"
    if behavior_key not in environ:
        environ[behavior_key] = f"0x{randbits(64):016X}"
    variation = environ[variation_key]
    behavior = environ[behavior_key]
    return variation, behavior


GAMEPLAY_SCOPES = ("all", *GAMEPLAY_TESTS)
FAILED_TEST = re.compile(r"^    (?P<name>[A-Za-z0-9_:]+)$", re.MULTILINE)
FAILED_RERUN_TARGET = re.compile(
    r"to rerun pass `(?P<target>--lib|--test gameplay_audit)`"
)
RUST_TEST_RESULT = re.compile(
    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored;"
)
GAMEPLAY_REPORT_PREFIXES = (
    "HARNESS INPUT ",
    "CONTENT ",
    "EVIDENCE CONTRACT ",
    "CAPABILITY HIGHLIGHT ",
    "SAMPLE ",
    "WORKSHOP CAPABILITY ",
    "CAPABILITY SYSTEMS ",
    "WORKSHOP EXPERIENCE REVIEW ",
    "AGENCY INPUT ",
    "AGENCY SUMMARY ",
    "FOCUSED REPORT INPUT ",
    "PROBE INPUT ",
)
FOCUSED_REVIEW_PREFIXES = (
    "SURVIVAL REVIEW ",
    "PROGRESSION FALLBACK ",
    "PROGRESSION REVIEW ",
    "ORE REVIEW ",
    "FOUNDRY REVIEW ",
)
FOCUSED_VISIBLE_REPLAY_SEED = re.compile(
    r"\b(?:anchor|coverage|organic):(0x[0-9A-Fa-f]+)"
)


def cargo(alias: str) -> list[str]:
    return ["cargo", alias]


def rust_test_summary(stdout: str) -> str | None:
    """Return one compact count for successful Rust test output, if present."""

    matches = list(RUST_TEST_RESULT.finditer(stdout))
    if not matches:
        return None
    passed = sum(int(match.group("passed")) for match in matches)
    ignored = sum(int(match.group("ignored")) for match in matches)
    detail = f"{passed} test{'s' if passed != 1 else ''}"
    if ignored:
        detail += f", {ignored} ignored"
    return detail


def combined_test_summary(stdout: str) -> str | None:
    """Return the core/gameplay split for the one-graph broad test command."""

    matches = list(RUST_TEST_RESULT.finditer(stdout))
    if len(matches) != 2:
        return rust_test_summary(stdout)
    core, gameplay = matches
    detail = f"{core.group('passed')} core + {gameplay.group('passed')} gameplay"
    ignored = int(core.group("ignored")) + int(gameplay.group("ignored"))
    if ignored:
        detail += f", {ignored} ignored"
    return detail


def concise_gameplay_report(stdout: str, environ=None) -> str:
    """Keep high-signal aggregates plus the focused reference and organic outcomes."""

    environment = os.environ if environ is None else environ
    if environment.get("DEEP_HEARTH_GAMEPLAY_VERBOSE") is not None or environment.get(
        "DEEP_HEARTH_GAMEPLAY_TRACE"
    ) is not None:
        return stdout.rstrip()
    lines = stdout.splitlines()
    focused_visible_seeds = {
        match.group(1).upper()
        for line in lines
        if line.startswith("PROBE INPUT ")
        for match in FOCUSED_VISIBLE_REPLAY_SEED.finditer(line)
    }
    return "\n".join(
        line
        for line in lines
        if line.startswith(GAMEPLAY_REPORT_PREFIXES)
        or (
            line.startswith(FOCUSED_REVIEW_PREFIXES)
            and any(
                f"SEED={seed}" in line.upper()
                for seed in focused_visible_seeds
            )
        )
    )


def quick_plan() -> list[tuple[str, list[str]]]:
    """Run the build-free edit-loop checks that are safe after every coherent text edit."""

    return [
        ("format", ["cargo", "fmt", "--check"]),
        (
            "complexity ratchet",
            [sys.executable, "tools/check_bca.py", "check"],
        ),
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
    if GAMEPLAY_AUDIT_TARGET in command:
        failed = FAILED_TEST.findall(combined)
        if failed:
            rerun_targets = FAILED_RERUN_TARGET.findall(combined)
            if rerun_targets and rerun_targets[-1] == "--lib":
                return f"python tools/run_test.py {failed[-1]}"
            return (
                "python tools/run_test.py "
                f"--target {GAMEPLAY_AUDIT_TARGET} {failed[-1]}"
            )
        for scope, test_name in GAMEPLAY_TESTS.items():
            if test_name in command:
                return f"python ci.py gate --gameplay {scope}"
    return None


def audit_plan(scope: str) -> list[tuple[str, list[str]]]:
    """Run an explicitly selected broad runtime audit surface."""

    if scope not in ("core", "gameplay", "all"):
        raise ValueError(f"unknown audit scope: {scope}")

    plan = quick_plan()
    if scope == "all":
        plan.append(("core + gameplay", combined_test_command()))
        return plan
    if scope == "core":
        plan.append(("core", cargo("test-fast")))
    if scope == "gameplay":
        plan.append(("gameplay", gameplay_command("all")))
    return plan


def combined_test_command() -> list[str]:
    """Run core and gameplay tests in one Cargo graph with one shared feature fingerprint."""

    return [
        "cargo",
        "test",
        "--quiet",
        "--locked",
        "--features",
        "test-gameplay",
        "--lib",
        "--test",
        GAMEPLAY_AUDIT_TARGET,
    ]


def gameplay_targets_command(
    targets: tuple[str, ...],
    *,
    test_filter: str | None = None,
    nocapture: bool = False,
) -> list[str]:
    command = [
        "cargo",
        "test",
        "--quiet",
        "--locked",
        "--features",
        "test-gameplay",
    ]
    for target in targets:
        command.extend(("--test", target))
    test_args: list[str] = []
    if test_filter is not None:
        command.append(test_filter)
        test_args.append("--exact")
    if nocapture:
        test_args.append("--nocapture")
    if test_args:
        command.append("--")
        command.extend(test_args)
    return command


def gameplay_command(scope: str, *, nocapture: bool = False) -> list[str]:
    test_filter = None if scope == "all" else GAMEPLAY_TESTS[scope]
    return gameplay_targets_command(
        (GAMEPLAY_AUDIT_TARGET,),
        test_filter=test_filter,
        nocapture=nocapture,
    )


def gameplay_plan(scope: str) -> list[tuple[str, list[str]]]:
    label = "gameplay" if scope == "all" else f"gameplay {scope}"
    return [(label, gameplay_command(scope))]


def exact_gameplay_command(target: str, name: str, *, ignored: bool = False) -> list[str]:
    command = [
        sys.executable,
        "tools/run_test.py",
        "--target",
        target,
        "--nocapture",
    ]
    if ignored:
        command.append("--ignored")
    command.append(name)
    return command


def report_plan() -> list[tuple[str, list[str]]]:
    return [
        (
            "gameplay report",
            exact_gameplay_command(GAMEPLAY_AUDIT_TARGET, "gameplay_report", ignored=True),
        )
    ]


def bca_review_plan(since: str, paths: list[str]) -> list[tuple[str, list[str]]]:
    """Run the pinned history-aware BCA review over maintained source changed from a base revision."""

    command = [
        sys.executable,
        "tools/check_bca.py",
        "review",
        "--changed",
        "--since",
        since,
    ]
    for path in paths:
        command.extend(("--path", path))
    return [("BCA changed-source review", command)]


def plan_for(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    if args.preset == "quick":
        return quick_plan()
    if args.preset == "bca":
        return bca_review_plan(args.since, args.path)
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


def execute_stage(command: list[str]) -> tuple[subprocess.CompletedProcess[str] | None, float, OSError | None]:
    started = time.perf_counter()
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
        return None, time.perf_counter() - started, error
    return result, time.perf_counter() - started, None


def report_stage(
    index: int,
    total: int,
    label: str,
    command: list[str],
    execution: tuple[subprocess.CompletedProcess[str] | None, float, OSError | None],
    *,
    echo_success: bool = False,
    announced: bool = False,
) -> float | None:
    result, elapsed, start_error = execution
    if not announced:
        print(f"[{index}/{total}] {label} ... ", end="")
    if start_error is not None:
        print(f"FAIL ({elapsed:.1f}s)")
        print(f"reproduce: {' '.join(command)}", file=sys.stderr)
        print(f"unable to start command: {start_error}", file=sys.stderr)
        return None
    assert result is not None
    if result.returncode == 0:
        detail = (
            combined_test_summary(result.stdout)
            if label == "core + gameplay"
            else rust_test_summary(result.stdout)
        )
        suffix = f"; {detail}" if detail is not None else ""
        print(f"PASS ({elapsed:.1f}s{suffix})")
        if echo_success and result.stdout.strip():
            output = (
                concise_gameplay_report(result.stdout)
                if label == "gameplay report"
                else result.stdout.rstrip()
            )
            if output:
                print(output)
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


def run_stage(
    index: int,
    total: int,
    label: str,
    command: list[str],
    *,
    echo_success: bool = False,
) -> float | None:
    print(f"[{index}/{total}] {label} ... ", end="", flush=True)
    return report_stage(
        index,
        total,
        label,
        command,
        execute_stage(command),
        echo_success=echo_success,
        announced=True,
    )


def run_parallel_stages(
    stages: list[tuple[str, list[str]]],
    *,
    total: int,
    start_index: int,
) -> list[tuple[str, float]] | None:
    """Run independent build-free stages concurrently and report them in stable plan order."""

    with ThreadPoolExecutor(max_workers=len(stages)) as executor:
        executions = list(executor.map(lambda stage: execute_stage(stage[1]), stages))
    timings: list[tuple[str, float]] = []
    failed = False
    for offset, ((label, command), execution) in enumerate(zip(stages, executions, strict=True)):
        elapsed = report_stage(start_index + offset, total, label, command, execution)
        if elapsed is None:
            failed = True
        else:
            timings.append((label, elapsed))
    return None if failed else timings


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run concise local verification without hosted CI or implicit change detection."
    )
    parser.add_argument(
        "preset",
        nargs="?",
        choices=("quick", "gate", "audit", "report", "bca"),
        default="quick",
        help=(
            "build-free edit-loop check, coherent compile/test gate, broad maintained checkpoint, "
            "explicit gameplay report, or advisory changed-source BCA review"
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
    parser.add_argument(
        "--since",
        default="HEAD",
        help="git revision used as the BCA changed-source comparison base",
    )
    parser.add_argument(
        "--path",
        action="append",
        default=[],
        help="restrict BCA review to a source scope; repeat for multiple scopes",
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
    if args.preset == "bca" and any(
        (args.all, args.core, args.lint, args.soak, args.gameplay, args.shaders, args.rustdoc)
    ):
        parser.error("bca review is build-free and does not accept build-producing flags")
    if args.preset != "bca" and (args.since != "HEAD" or args.path):
        parser.error("--since and --path are valid only with the bca preset")
    return args


def main() -> int:
    args = parse_args()
    if args.preset == "report":
        configure_report_replay_environment(os.environ)
    plan = plan_for(args)
    if args.dry_run:
        for label, command in plan:
            print(f"{label}: {' '.join(command)}")
        return 0

    started = time.perf_counter()
    timings: list[tuple[str, float]] = []
    print(f"local-ci {args.preset}: {len(plan)} stage(s)")
    try:
        quick = quick_plan()
        quick_count = len(quick) if plan[: len(quick)] == quick else 0
        if quick_count:
            quick_timings = run_parallel_stages(
                plan[:quick_count],
                total=len(plan),
                start_index=1,
            )
            if quick_timings is None:
                return 1
            timings.extend(quick_timings)

        for index, (label, command) in enumerate(plan[quick_count:], start=quick_count + 1):
            elapsed = run_stage(
                index,
                len(plan),
                label,
                command,
                echo_success=args.preset in ("report", "bca"),
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
