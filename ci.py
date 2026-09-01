#!/usr/bin/env python3
"""Fast, explicit local verification runner for Deep Hearth's repository-owned gates."""

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

GAMEPLAY_CONTRACTS_TARGET = "gameplay_contracts"
GAMEPLAY_REPORT_EXAMPLE = "gameplay-report"
GAMEPLAY_TARGETS = {
    "workshop": "gameplay_workshop",
    "survival": "gameplay_survival",
    "progression": "gameplay_progression",
    "ore": "gameplay_ore",
    "foundry": "gameplay_foundry",
}
GAMEPLAY_AUDIT_TARGETS = (GAMEPLAY_CONTRACTS_TARGET, *GAMEPLAY_TARGETS.values())
GAMEPLAY_TESTS = {
    "workshop": "workshop_contract_tests::gameplay_harness_gate",
    "survival": "gameplay_survival_provisioning_probe",
    "progression": "gameplay_primitive_progression_probe",
    "ore": "gameplay_ore_preparation_probe",
    "foundry": "gameplay_foundry_probe",
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
FAILED_RERUN_TARGET = re.compile(r"to rerun pass `(?P<target>--lib|--test [A-Za-z0-9_-]+)`")
RUST_TEST_RESULT = re.compile(
    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored;"
)
ORDINARY_GAMEPLAY_REPORT_PREFIXES = (
    "PLAYER FANTASY ",
    "CONTENT registry_schema=",
    "CONTENT ACQUISITION EDGES ",
    "EVIDENCE CONTRACT ",
)
ORDINARY_PROBE_REVIEW_PREFIXES = (
    ("survival-provisioning", "SURVIVAL EXPERIENCE "),
    ("primitive-progression", "PROGRESSION FALLBACK "),
    ("primitive-progression", "PROGRESSION EXPERIENCE "),
)
FOCUSED_VISIBLE_REPLAY_SEED = re.compile(
    r"\b(?:anchor|coverage|organic):(0x[0-9A-Fa-f]+)"
)
GAMEPLAY_REPLAY_ROOTS = re.compile(
    r"\bworld_root=(?P<world>\S+)\s+behavior_root=(?P<behavior>\S+)"
)
WORKSHOP_PLAN_SUMMARY = re.compile(
    r"\bplan=(?P<plan>\S+)\s+anchors=(?P<anchors>\d+)\s+variation=(?P<variation>\d+)\s+custom=(?P<custom>\d+)"
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


def gameplay_replay_summary(stdout: str) -> str | None:
    """Return one compact reproduction token from captured focused-gameplay output."""

    for line in stdout.splitlines():
        if line.startswith("PROBE INPUT ") and " replay=" in line:
            roots = GAMEPLAY_REPLAY_ROOTS.search(line)
            if roots is not None and roots.group("world") != "explicit":
                return f"roots={roots.group('world')}/{roots.group('behavior')}"
            replay = line.split(" replay=", 1)[1]
            if roots is not None:
                return f"roots={roots.group('world')}/{roots.group('behavior')}; replay={replay}"
            return f"replay={replay}"
        if line.startswith("HARNESS INPUT "):
            plan = WORKSHOP_PLAN_SUMMARY.search(line)
            if plan is not None and plan.group("plan") == "maintained":
                return f"maintained={plan.group('anchors')}"
            if plan is not None and plan.group("plan") == "custom":
                return f"custom={plan.group('custom')}"
            match = GAMEPLAY_REPLAY_ROOTS.search(line)
            if match is not None:
                return f"roots={match.group('world')}/{match.group('behavior')}"
    return None


def combined_test_summary(stdout: str) -> str | None:
    """Return the core/gameplay split for the one-graph broad test command."""

    matches = list(RUST_TEST_RESULT.finditer(stdout))
    if len(matches) < 2:
        return rust_test_summary(stdout)
    core, *gameplay = matches
    gameplay_passed = sum(int(match.group("passed")) for match in gameplay)
    detail = f"{core.group('passed')} core + {gameplay_passed} gameplay"
    ignored = int(core.group("ignored")) + sum(
        int(match.group("ignored")) for match in gameplay
    )
    if ignored:
        detail += f", {ignored} ignored"
    return detail


def concise_gameplay_report(stdout: str, environ=None) -> str:
    """Keep the current ordinary-player experience; verbose mode retains capability diagnostics."""

    environment = os.environ if environ is None else environ
    if environment.get("DEEP_HEARTH_GAMEPLAY_VERBOSE") is not None or environment.get(
        "DEEP_HEARTH_GAMEPLAY_TRACE"
    ) is not None:
        return stdout.rstrip()
    lines = stdout.splitlines()
    ordinary_probe_inputs = {
        probe: [
            line
            for line in lines
            if line.startswith(f"PROBE INPUT name={probe} ")
        ]
        for probe, _prefix in ORDINARY_PROBE_REVIEW_PREFIXES
    }
    visible_seeds = {
        probe: {
            match.group(1).upper()
            for line in probe_lines
            for match in FOCUSED_VISIBLE_REPLAY_SEED.finditer(line)
        }
        for probe, probe_lines in ordinary_probe_inputs.items()
    }
    ordinary_input_lines = {
        line for probe_lines in ordinary_probe_inputs.values() for line in probe_lines
    }
    return "\n".join(
        line
        for line in lines
        if line.startswith(ORDINARY_GAMEPLAY_REPORT_PREFIXES)
        or line.startswith("EVALUATION SCOPE kind=ordinary-play ")
        or line in ordinary_input_lines
        or (
            any(
                line.startswith(prefix)
                and any(
                    f"SEED={seed}" in line.upper()
                    for seed in visible_seeds.get(probe, set())
                )
                for probe, prefix in ORDINARY_PROBE_REVIEW_PREFIXES
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
    if command == cargo("test-core"):
        failed = FAILED_TEST.findall(combined)
        if failed:
            return f"python tools/run_test.py {failed[-1]}"
    gameplay_targets = (*GAMEPLAY_AUDIT_TARGETS, *GAMEPLAY_TARGETS.values())
    if any(target in command for target in gameplay_targets):
        failed = FAILED_TEST.findall(combined)
        if failed:
            rerun_targets = FAILED_RERUN_TARGET.findall(combined)
            if rerun_targets and rerun_targets[-1] == "--lib":
                return f"python tools/run_test.py {failed[-1]}"
            if rerun_targets and rerun_targets[-1].startswith("--test "):
                target = rerun_targets[-1].removeprefix("--test ")
                return f"python tools/run_test.py --target {target} {failed[-1]}"
            return "python ci.py audit --gameplay"
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
        plan.append(("core", cargo("test-core")))
    if scope == "gameplay":
        plan.append(("gameplay", gameplay_command("all")))
    return plan


def combined_test_command() -> list[str]:
    """Run core and gameplay tests in one Cargo graph with one shared feature fingerprint."""

    command = [
        "cargo",
        "test",
        "--quiet",
        "--locked",
        "--features",
        "test-gameplay",
        "--lib",
    ]
    for target in GAMEPLAY_AUDIT_TARGETS:
        command.extend(("--test", target))
    return command


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
    if scope == "all":
        return gameplay_targets_command(GAMEPLAY_AUDIT_TARGETS, nocapture=nocapture)
    return gameplay_targets_command(
        (GAMEPLAY_TARGETS[scope],),
        test_filter=GAMEPLAY_TESTS[scope],
        # Focused gates capture stdout in ci.py, then surface only the short replay token on success.
        # On failure the same output preserves the exact world needed to reproduce the problem.
        nocapture=True,
    )


def gameplay_plan(scope: str) -> list[tuple[str, list[str]]]:
    label = "gameplay" if scope == "all" else f"gameplay {scope}"
    return [(label, gameplay_command(scope))]


def report_plan() -> list[tuple[str, list[str]]]:
    return [
        (
            "gameplay report",
            [
                "cargo",
                "run",
                "--quiet",
                "--locked",
                "--profile",
                "test",
                "--example",
                GAMEPLAY_REPORT_EXAMPLE,
                "--features",
                "test-gameplay",
            ],
        )
    ]


def bca_review_plan(
    since: str,
    paths: list[str],
    *,
    changed_only: bool = True,
) -> list[tuple[str, list[str]]]:
    """Run the pinned history-aware BCA review over changed source or a requested hotspot scope."""

    command = [
        sys.executable,
        "tools/check_bca.py",
        "review",
    ]
    label = "BCA hotspot review"
    if changed_only:
        command.append("--changed")
        label = "BCA changed-source review"
    command.extend(("--since", since))
    for path in paths:
        command.extend(("--path", path))
    return [(label, command)]


def audit_plan_for_args(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    if args.all:
        return audit_plan("all")
    if args.core:
        return audit_plan("core")
    if args.gameplay:
        return audit_plan("gameplay")
    raise ValueError("audit requires an explicit scope: use `--core`, `--gameplay`, or `--all`")


def gate_plan(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    """Resolve exactly one build-producing proof without repeating the separate quick lane."""

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

    if args.soak:
        return [("soak", cargo("test-soak"))]
    if args.gameplay:
        return gameplay_plan(args.gameplay)
    if args.shaders:
        return [("shaders", cargo("test-shaders"))]
    if args.rustdoc:
        return [("rustdoc", cargo("test-doc"))]
    if args.lint:
        return [("clippy", cargo("test-lint"))]
    return [("compile", cargo("check-fast"))]


def plan_for(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    if args.preset == "quick":
        return quick_plan()
    if args.preset == "gate":
        return gate_plan(args)
    if args.preset == "audit":
        return audit_plan_for_args(args)
    if args.preset == "report":
        return report_plan()
    if args.preset == "bca":
        return bca_review_plan(
            args.since,
            args.path,
            changed_only=not args.hotspots,
        )
    raise ValueError(f"unknown local-CI preset: {args.preset}")


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
        details = [detail] if detail is not None else []
        if replay := gameplay_replay_summary(result.stdout):
            details.append(replay)
        suffix = f"; {'; '.join(details)}" if details else ""
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


def build_parser() -> argparse.ArgumentParser:
    """Build the local-CI command surface without mixing in preset policy validation."""

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
    parser.add_argument(
        "--hotspots",
        action="store_true",
        help=(
            "with the bca preset, review current history-aware hotspots in the requested source "
            "scope instead of limiting the report to changed maintained Rust source"
        ),
    )
    return parser


def has_build_lane_option(args: argparse.Namespace) -> bool:
    return any(
        (args.all, args.core, args.lint, args.soak, args.gameplay, args.shaders, args.rustdoc)
    )


def validate_quick_options(parser: argparse.ArgumentParser, args: argparse.Namespace) -> None:
    if has_build_lane_option(args):
        parser.error("quick is intentionally build-free and does not accept build-producing flags")


def validate_audit_options(parser: argparse.ArgumentParser, args: argparse.Namespace) -> None:
    if any((args.lint, args.soak, args.shaders, args.rustdoc)):
        parser.error(
            "audit has a fixed runtime scope; run change-scoped lint/rustdoc/shader lanes separately"
        )
    if not any((args.all, args.core, args.gameplay)):
        parser.error("audit requires an explicit scope: --core, --gameplay, or --all")
    if args.gameplay not in (None, "all"):
        parser.error(
            "focused gameplay belongs in gate; audit --gameplay always means all maintained gameplay targets"
        )


def validate_gate_options(parser: argparse.ArgumentParser, args: argparse.Namespace) -> None:
    if args.core:
        parser.error("complete core behavior is audit-only; use `python ci.py audit --core`")
    if args.all:
        parser.error("broad verification is audit-only; use `python ci.py audit --all`")
    if args.gameplay == "all":
        parser.error(
            "gate requires an explicit gameplay scope; use `python ci.py audit --gameplay` for all targets"
        )


def validate_report_options(parser: argparse.ArgumentParser, args: argparse.Namespace) -> None:
    if has_build_lane_option(args):
        parser.error("report is a fixed exploratory lane and does not accept gate flags")


def validate_bca_options(parser: argparse.ArgumentParser, args: argparse.Namespace) -> None:
    if has_build_lane_option(args):
        parser.error("bca review is build-free and does not accept build-producing flags")


def validate_preset_options(parser: argparse.ArgumentParser, args: argparse.Namespace) -> None:
    validators = {
        "quick": validate_quick_options,
        "gate": validate_gate_options,
        "audit": validate_audit_options,
        "report": validate_report_options,
        "bca": validate_bca_options,
    }
    validators[args.preset](parser, args)
    if args.preset != "bca" and (args.since != "HEAD" or args.path or args.hotspots):
        parser.error("--since, --path, and --hotspots are valid only with the bca preset")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = build_parser()
    args = parser.parse_args(argv)
    validate_preset_options(parser, args)
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
