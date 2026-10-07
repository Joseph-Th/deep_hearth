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

from tools.cargo_env import local_cargo_environment
from tools.command_output import (
    FAILURE_HEAD_LINES,
    FAILURE_TAIL_LINES,
    bounded_failure_output,
    bounded_failure_streams,
    rust_test_result_counts,
)
from tools.gameplay_targets import (
    GAMEPLAY_AGENCY_TARGET,
    GAMEPLAY_AUDIT_TARGET,
    GAMEPLAY_BEHAVIOR_ENV,
    GAMEPLAY_CONTRACTS_TARGET,
    GAMEPLAY_FEATURE,
    gameplay_evidence_mode,
    GAMEPLAY_REPORT_MODE_ENV,
    GAMEPLAY_SCOPE_SPECS,
    GAMEPLAY_TARGETS,
    GAMEPLAY_TESTS,
    GAMEPLAY_VARIATION_ENV,
    GAMEPLAY_VARIATION_SCOPE_ENV,
    GAMEPLAY_ROUTINE_VARIATION_SCOPES,
)
from tools.gameplay_report_summary import concise_gameplay_report
from tools.replay_seed import parse_replay_seed


ROOT = Path(__file__).resolve().parent

GAMEPLAY_REPORT_EXAMPLE = "gameplay-report"
SCOPED_REPORT_EXAMPLES = {
    "workshop": "gameplay-workshop-report",
    "survival": "gameplay-survival-report",
    "progression": "gameplay-progression-report",
    "settlement": "gameplay-settlement-report",
    "woodworking": "gameplay-woodworking-report",
    "power-provider": "gameplay-power-report",
}
REPORT_BEHAVIOR_SCOPES = {
    "all",
    *(scope for scope, spec in GAMEPLAY_SCOPE_SPECS.items() if spec.uses_behavior_seed),
}
GAMEPLAY_SEED_ENV_KEYS = (
    "DEEP_HEARTH_GAMEPLAY_SEEDS",
    GAMEPLAY_VARIATION_ENV,
    GAMEPLAY_BEHAVIOR_ENV,
    GAMEPLAY_VARIATION_SCOPE_ENV,
)
REUSED_TEST_REPORTS = (
    frozenset(GAMEPLAY_SCOPE_SPECS) - frozenset(SCOPED_REPORT_EXAMPLES)
) | {"agency"}


def configure_gameplay_replay_environment(
    environ,
    *,
    variation_override: str | None,
    behavior_override: str | None,
    use_behavior_seed: bool,
    randbits=secrets.randbits,
) -> tuple[str, str]:
    """Apply or generate replayable gameplay roots before any Cargo process starts."""

    variation_key = GAMEPLAY_VARIATION_ENV
    behavior_key = GAMEPLAY_BEHAVIOR_ENV

    if variation_override is not None:
        variation = variation_override
    elif variation_key in environ:
        try:
            variation = parse_replay_seed(environ[variation_key])
        except argparse.ArgumentTypeError as error:
            raise ValueError(f"{variation_key}: {error}") from error
    else:
        variation = f"0x{randbits(64):016X}"
    environ[variation_key] = variation

    if not use_behavior_seed:
        environ.pop(behavior_key, None)
        return variation, "unused"

    if behavior_override is not None:
        behavior = behavior_override
    elif behavior_key in environ:
        try:
            behavior = parse_replay_seed(environ[behavior_key])
        except argparse.ArgumentTypeError as error:
            raise ValueError(f"{behavior_key}: {error}") from error
    else:
        behavior = f"0x{randbits(64):016X}"
    environ[behavior_key] = behavior
    return variation, behavior


def configure_report_replay_environment(
    args: argparse.Namespace,
    environ,
    *,
    randbits=secrets.randbits,
) -> tuple[str, str]:
    """Apply report replay roots through the shared gameplay sampling contract."""

    return configure_gameplay_replay_environment(
        environ,
        variation_override=args.variation_seed,
        behavior_override=args.behavior_seed,
        use_behavior_seed=args.scope in REPORT_BEHAVIOR_SCOPES,
        randbits=randbits,
    )


def configure_report_mode_environment(args: argparse.Namespace, environ) -> None:
    """Enable expanded sampling when a scoped report reuses a focused test artifact."""

    if args.scope in REUSED_TEST_REPORTS:
        environ[GAMEPLAY_REPORT_MODE_ENV] = "1"
    else:
        environ.pop(GAMEPLAY_REPORT_MODE_ENV, None)


def gameplay_uses_behavior_seed(args: argparse.Namespace) -> bool | None:
    """Return whether the selected gameplay lane accepts an actor-policy replay root."""

    if args.preset == "report":
        return args.scope in REPORT_BEHAVIOR_SCOPES
    if args.preset == "gate" and args.gameplay not in (None, "contracts"):
        return GAMEPLAY_SCOPE_SPECS[args.gameplay].uses_behavior_seed
    if args.preset == "audit" and (args.gameplay is not None or args.all):
        return True
    return None


def gameplay_scope_uses_behavior_seed(scope: str) -> bool:
    """Return whether one concrete gameplay sample scope varies actor policy."""

    return scope != "agency" and GAMEPLAY_SCOPE_SPECS[scope].uses_behavior_seed


def audit_variation_scope(variation: str) -> str:
    """Choose one replay-stable gameplay scope from the world root alone."""

    return GAMEPLAY_ROUTINE_VARIATION_SCOPES[
        int(variation, 0) % len(GAMEPLAY_ROUTINE_VARIATION_SCOPES)
    ]


def clear_gameplay_seed_environment(environ) -> None:
    """Clear ambient replay state before project-owned routine gameplay verification."""

    for key in GAMEPLAY_SEED_ENV_KEYS:
        environ.pop(key, None)


def configure_gameplay_verification_environment(
    args: argparse.Namespace,
    environ,
    *,
    randbits=secrets.randbits,
) -> tuple[str, str, str]:
    """Add one fresh replayable organic case without widening every gameplay scope."""

    assert gameplay_uses_behavior_seed(args) is not None
    environ.pop(GAMEPLAY_REPORT_MODE_ENV, None)
    clear_gameplay_seed_environment(environ)
    variation = args.variation_seed or f"0x{randbits(64):016X}"
    if args.preset == "gate":
        assert args.gameplay not in (None, "contracts", "all")
        scope = args.gameplay
    else:
        # The physical root owns broad-audit routing. Actor-policy entropy may change a choice
        # inside that sampled world, but must never reroute replay to a different subsystem.
        scope = audit_variation_scope(variation)
    use_behavior_seed = gameplay_scope_uses_behavior_seed(scope)
    behavior = "unused"
    if use_behavior_seed:
        behavior = args.behavior_seed or f"0x{randbits(64):016X}"
        environ[GAMEPLAY_BEHAVIOR_ENV] = behavior
    environ[GAMEPLAY_VARIATION_ENV] = variation
    environ[GAMEPLAY_VARIATION_SCOPE_ENV] = scope
    return scope, variation, behavior


REPORT_SCOPES = ("all", *GAMEPLAY_SCOPE_SPECS, "agency")
FAILED_TEST = re.compile(r"^    (?P<name>[A-Za-z0-9_:]+)$", re.MULTILINE)
FAILED_RERUN_TARGET = re.compile(r"to rerun pass `(?P<target>--lib|--test [A-Za-z0-9_-]+)`")
GAMEPLAY_REPLAY_ROOTS = re.compile(
    r"\bworld_root=(?P<world>\S+)\s+behavior_root=(?P<behavior>\S+)"
)
AGENCY_REPLAY_ROOT = re.compile(r"\bvariation_root=(?P<world>0x[0-9A-Fa-f]+)")
WORKSHOP_PLAN_SUMMARY = re.compile(
    r"\bplan=(?P<plan>\S+)\s+anchors=(?P<anchors>\d+)\s+variation=(?P<variation>\d+)\s+custom=(?P<custom>\d+)"
)


def cargo(alias: str) -> list[str]:
    return ["cargo", alias]


def lint_command() -> list[str]:
    """Lint the production library without compiling gameplay/report wrappers."""

    return cargo("lint-fast")


def rust_test_summary(stdout: str) -> str | None:
    """Return one compact count for successful Rust test output, if present."""

    counts = rust_test_result_counts(stdout)
    if not counts:
        return None
    passed = sum(result[0] for result in counts)
    ignored = sum(result[1] for result in counts)
    detail = f"{passed} test{'s' if passed != 1 else ''}"
    if ignored:
        detail += f", {ignored} ignored"
    return detail


def replay_root_detail(world: str, behavior: str) -> str | None:
    """Render only the replay roots a caller must actually supply."""

    if world in {"n/a", "maintained", "none", "None"}:
        return None
    if behavior in {"n/a", "maintained", "unused", "none", "None"}:
        return f"variation={world}"
    return f"roots={world}/{behavior}"


def gameplay_replay_summary(stdout: str) -> str | None:
    """Return one compact reproduction token from captured focused-gameplay output."""

    for line in stdout.splitlines():
        if line.startswith("PROBE INPUT ") and " replay=" in line:
            roots = GAMEPLAY_REPLAY_ROOTS.search(line)
            if roots is not None and roots.group("world") != "explicit":
                return replay_root_detail(roots.group("world"), roots.group("behavior"))
            replay = line.split(" replay=", 1)[1]
            if roots is not None:
                return f"roots={roots.group('world')}/{roots.group('behavior')}; replay={replay}"
            return f"replay={replay}"
        if line.startswith("HARNESS INPUT "):
            plan = WORKSHOP_PLAN_SUMMARY.search(line)
            if plan is not None and plan.group("plan") == "custom":
                return f"custom={plan.group('custom')}"
            match = GAMEPLAY_REPLAY_ROOTS.search(line)
            if match is not None:
                return replay_root_detail(match.group("world"), match.group("behavior"))
    return None


def report_repair_hint(label: str, output: str) -> str | None:
    """Return one replayable report command for a failed exploratory stage."""

    if not label.startswith("gameplay report"):
        return None
    scope = label.removeprefix("gameplay report").strip() or "all"
    command = ["python", "ci.py", "report"]
    if scope != "all":
        command.extend(("--scope", scope))
    if replay_flags := gameplay_replay_flags(output):
        command.extend(replay_flags.split())
    return " ".join(command)


def gameplay_environment_summary(label: str, environ) -> str | None:
    """Return replay roots plus the selected gameplay evidence authority when known."""

    if not label.startswith("gameplay") or label == "gameplay contracts":
        return None
    variation = environ.get(GAMEPLAY_VARIATION_ENV)
    if variation is None:
        return None
    replay = replay_root_detail(variation, environ.get(GAMEPLAY_BEHAVIOR_ENV, "n/a"))
    scope = environ.get(GAMEPLAY_VARIATION_SCOPE_ENV)
    sampled_scope = scope is not None
    if scope is None:
        for prefix in ("gameplay report ", "gameplay "):
            if label.startswith(prefix):
                candidate = label.removeprefix(prefix)
                if candidate in GAMEPLAY_SCOPE_SPECS or candidate == "agency":
                    scope = candidate
                break
    if scope is None:
        return replay
    evidence = gameplay_evidence_mode(scope)
    detail = f"evidence={evidence}"
    if sampled_scope:
        detail = f"sample={scope}; {detail}"
    return f"{detail}; {replay}" if replay is not None else detail


def quick_plan() -> list[tuple[str, list[str]]]:
    """Run the build-free edit-loop checks that are safe after every coherent text edit."""

    return [
        ("format changed Rust", [sys.executable, "tools/check_format.py"]),
        (
            "complexity ratchet",
            [sys.executable, "tools/check_bca.py", "check"],
        ),
        (
            "repository contracts",
            [sys.executable, "tools/check_authority_docs.py"],
        ),
        (
            "local CI contracts",
            [sys.executable, "-m", "unittest", "tools.test_ci", "-q"],
        ),
    ]


def repair_hint(command: list[str], stdout: str, stderr: str) -> str | None:
    """Return the narrow follow-up command for a failed broad executable lane when detectable."""

    combined = f"{stdout}\n{stderr}"
    if command == cargo("test-core"):
        failed = FAILED_TEST.findall(combined)
        if failed:
            return f"python tools/run_test.py {failed[-1]}"
    gameplay_targets = (
        GAMEPLAY_CONTRACTS_TARGET,
        GAMEPLAY_AUDIT_TARGET,
        GAMEPLAY_AGENCY_TARGET,
        *GAMEPLAY_TARGETS.values(),
    )
    if any(target in command for target in gameplay_targets):
        failed = FAILED_TEST.findall(combined)
        if failed:
            rerun_targets = FAILED_RERUN_TARGET.findall(combined)
            if rerun_targets and rerun_targets[-1] == "--lib":
                return command_with_gameplay_replay(
                    ["python", "tools/run_test.py"], combined, [failed[-1]]
                )
            if rerun_targets and rerun_targets[-1].startswith("--test "):
                target = rerun_targets[-1].removeprefix("--test ")
                return command_with_gameplay_replay(
                    ["python", "tools/run_test.py", "--target", target],
                    combined,
                    [failed[-1]],
                )
            return command_with_gameplay_replay(
                ["python", "ci.py", "audit", "--gameplay"], combined
            )
        for scope, target in GAMEPLAY_TARGETS.items():
            if target in command:
                return command_with_gameplay_replay(
                    ["python", "ci.py", "gate", "--gameplay", scope], combined
                )
        if GAMEPLAY_AUDIT_TARGET in command:
            return command_with_gameplay_replay(
                ["python", "ci.py", "audit", "--gameplay"], combined
            )
    return None


def command_with_gameplay_replay(
    prefix: list[str], output: str, suffix: list[str] | None = None
) -> str:
    """Render a gameplay repair command with captured replay roots when available."""

    command = list(prefix)
    if replay_flags := gameplay_replay_flags(output):
        command.extend(replay_flags.split())
    if suffix:
        command.extend(suffix)
    return " ".join(command)


def gameplay_replay_flags(output: str) -> str | None:
    """Return exact-test CLI replay flags from one captured failing gameplay test."""

    for line in reversed(output.splitlines()):
        if line.startswith("AGENCY INPUT "):
            match = AGENCY_REPLAY_ROOT.search(line)
            if match is not None:
                return f"--variation-seed {match.group('world')}"
        if line.startswith("PROBE INPUT ") or line.startswith("HARNESS INPUT "):
            match = GAMEPLAY_REPLAY_ROOTS.search(line)
            if match is None or match.group("world") in {
                "explicit",
                "maintained",
                "n/a",
                "none",
                "None",
            }:
                continue
            flags = ["--variation-seed", match.group("world")]
            behavior = match.group("behavior")
            if behavior not in ("n/a", "none", "None", "unused"):
                flags.extend(("--behavior-seed", behavior))
            return " ".join(flags)
    return None


def audit_plan(scope: str) -> list[tuple[str, list[str]]]:
    """Run an explicitly selected broad runtime audit surface."""

    if scope not in ("core", "gameplay", "all"):
        raise ValueError(f"unknown audit scope: {scope}")

    if scope == "all":
        # Keep the broad checkpoint as two explicit surfaces: feature-minimal owner tests and the
        # gameplay audit. Both use the same test profile, so compiled production dependencies remain
        # reusable without forcing gameplay fixture capability into the library-test artifact.
        return [
            ("core", cargo("test-core")),
            ("gameplay", gameplay_command("all")),
        ]
    if scope == "core":
        return [("core", cargo("test-core"))]
    if scope == "gameplay":
        return [("gameplay", gameplay_command("all"))]
    raise AssertionError("validated audit scope must return a plan")


def gameplay_target_command(
    target: str,
    *,
    test_filter: str | None = None,
    nocapture: bool = False,
    ignored: bool = False,
) -> list[str]:
    command = [
        "cargo",
        "test",
        "--quiet",
        "--locked",
        "--features",
        GAMEPLAY_FEATURE,
    ]
    command.extend(("--test", target))
    test_args: list[str] = []
    if test_filter is not None:
        command.append(test_filter)
        test_args.append("--exact")
    if ignored:
        test_args.append("--ignored")
    if nocapture:
        test_args.append("--nocapture")
    if test_args:
        command.append("--")
        command.extend(test_args)
    return command


def gameplay_command(scope: str) -> list[str]:
    if scope == "all":
        return gameplay_target_command(GAMEPLAY_AUDIT_TARGET)
    if scope == "contracts":
        return gameplay_target_command(GAMEPLAY_CONTRACTS_TARGET)
    return gameplay_target_command(
        GAMEPLAY_TARGETS[scope],
        test_filter=GAMEPLAY_TESTS.get(scope),
    )


def gameplay_plan(scope: str) -> list[tuple[str, list[str]]]:
    label = "gameplay" if scope == "all" else f"gameplay {scope}"
    return [(label, gameplay_command(scope))]


def gameplay_report_example_command(example: str) -> list[str]:
    """Build one report-only example command on the shared test-profile cache."""

    command = [
        "cargo",
        "run",
        "--quiet",
        "--locked",
        "--profile",
        "test",
        "--example",
        example,
        "--features",
        GAMEPLAY_FEATURE,
    ]
    return command


def scoped_report_target(scope: str) -> tuple[str, str]:
    """Return the focused test artifact and exact test reused by one scoped report."""

    if scope == "agency":
        return GAMEPLAY_AGENCY_TARGET, "gameplay_agency_counterfactuals"
    spec = GAMEPLAY_SCOPE_SPECS[scope]
    return spec.target, spec.test


def report_plan(scope: str = "all") -> list[tuple[str, list[str]]]:
    """Run the smallest faithful report surface for one gameplay scope."""

    if scope not in REPORT_SCOPES:
        raise ValueError(f"unknown gameplay report scope: {scope}")
    if scope == "all":
        command = gameplay_report_example_command(GAMEPLAY_REPORT_EXAMPLE)
    elif scope in SCOPED_REPORT_EXAMPLES:
        command = gameplay_report_example_command(SCOPED_REPORT_EXAMPLES[scope])
    else:
        target, test = scoped_report_target(scope)
        command = gameplay_target_command(
            target,
            test_filter=test,
            nocapture=True,
        )
    label = "gameplay report"
    if scope != "all":
        label = f"gameplay report {scope}"
    return [(label, command)]


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
            args.production,
            args.soak,
            args.gameplay,
            args.shaders,
            args.rustdoc,
            args.lint,
        )
    )
    if selected_lanes != 1:
        raise ValueError("gate requires exactly one build-producing lane")

    if args.production:
        return [("check", cargo("check-fast"))]
    if args.soak:
        return [("soak", cargo("test-soak"))]
    if args.gameplay:
        return gameplay_plan(args.gameplay)
    if args.shaders:
        return [("shaders", cargo("test-shaders"))]
    if args.rustdoc:
        return [("rustdoc", cargo("test-doc"))]
    if args.lint:
        return [("clippy", lint_command())]
    raise AssertionError("validated gate lane must return a plan")


def plan_for(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    if args.preset == "quick":
        return quick_plan()
    if args.preset == "gate":
        return gate_plan(args)
    if args.preset == "audit":
        return audit_plan_for_args(args)
    if args.preset == "report":
        return report_plan(args.scope)
    if args.preset == "bca":
        return bca_review_plan(
            args.since,
            args.path,
            changed_only=not args.hotspots,
        )
    raise ValueError(f"unknown local-CI preset: {args.preset}")


def execute_stage(command: list[str]) -> tuple[subprocess.CompletedProcess[str] | None, float, OSError | None]:
    started = time.perf_counter()
    environment = local_cargo_environment()
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
    show_replay: bool = False,
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
        output = ""
        if echo_success and result.stdout.strip():
            try:
                output = (
                    concise_gameplay_report(result.stdout)
                    if label.startswith("gameplay report")
                    else result.stdout.rstrip()
                )
            except ValueError as error:
                print(f"FAIL ({elapsed:.1f}s)")
                repair = report_repair_hint(label, result.stdout)
                if repair is None:
                    print(f"reproduce: {' '.join(command)}", file=sys.stderr)
                else:
                    print(f"repair: {repair}", file=sys.stderr)
                print(f"report summary: {error}", file=sys.stderr)
                print(bounded_failure_output(result.stdout), file=sys.stderr)
                return None
        detail = None if label.startswith("gameplay report") else rust_test_summary(result.stdout)
        details = [detail] if detail is not None else []
        if show_replay and label.startswith("gameplay") and label != "gameplay contracts":
            replay = gameplay_environment_summary(label, os.environ)
            if replay is None:
                replay = gameplay_replay_summary(result.stdout)
            if replay is not None:
                details.append(replay)
        suffix = f"; {'; '.join(details)}" if details else ""
        print(f"PASS ({elapsed:.1f}s{suffix})")
        if output:
            # Successful reports are evidence: keep all selected output, including replay inputs.
            # Transcript bounds belong only to the failure diagnostics below.
            print(output)
        return elapsed

    print(f"FAIL ({elapsed:.1f}s)")
    hint = report_repair_hint(label, result.stdout) or repair_hint(
        command, result.stdout, result.stderr
    )
    if hint is None:
        print(f"reproduce: {' '.join(command)}", file=sys.stderr)
    else:
        print(f"repair: {hint}", file=sys.stderr)
    for diagnostic in bounded_failure_streams(result.stdout, result.stderr):
        print(diagnostic, file=sys.stderr)
    return None


def run_stage(
    index: int,
    total: int,
    label: str,
    command: list[str],
    *,
    echo_success: bool = False,
    show_replay: bool = False,
) -> float | None:
    prefix = f"[{index}/{total}] {label}" if total > 1 else label
    print(f"{prefix} ... ", end="", flush=True)
    return report_stage(
        index,
        total,
        label,
        command,
        execute_stage(command),
        echo_success=echo_success,
        announced=True,
        show_replay=show_replay,
    )


def run_quick_stages(
    stages: list[tuple[str, list[str]]],
) -> list[tuple[str, float]] | None:
    """Run the independent build-free edit checks with one success line and precise failures."""

    with ThreadPoolExecutor(max_workers=len(stages)) as executor:
        executions = list(executor.map(lambda stage: execute_stage(stage[1]), stages))
    failed = [
        index
        for index, (result, _elapsed, start_error) in enumerate(executions)
        if start_error is not None or result is None or result.returncode != 0
    ]
    if not failed:
        return [
            (label, elapsed)
            for (label, _command), (_result, elapsed, _start_error) in zip(
                stages, executions, strict=True
            )
        ]

    print(f"quick ... FAIL ({len(failed)}/{len(stages)} checks)")
    for offset in failed:
        label, command = stages[offset]
        report_stage(offset + 1, len(stages), label, command, executions[offset])
    return None


def add_dry_run(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved stage(s) without executing them",
    )


def add_replay_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--variation-seed",
        type=parse_replay_seed,
        help="replay one physical-world variation root (decimal or 0x hex u64)",
    )
    parser.add_argument(
        "--behavior-seed",
        type=parse_replay_seed,
        help="replay one actor-policy root when the selected scope uses it",
    )


def build_parser() -> argparse.ArgumentParser:
    """Build a contextual local-CI command surface with no irrelevant preset flags."""

    parser = argparse.ArgumentParser(
        description="Run the smallest explicit local verification lane that proves the change."
    )
    presets = parser.add_subparsers(dest="preset", metavar="COMMAND")

    quick = presets.add_parser("quick", help="build-free edit-loop checks")
    add_dry_run(quick)

    gate = presets.add_parser("gate", help="one explicit compile, lint, or focused runtime proof")
    lane = gate.add_mutually_exclusive_group(required=True)
    lane.add_argument(
        "--production",
        action="store_true",
        help="type-check the production library when no executable proof fits",
    )
    lane.add_argument("--lint", action="store_true", help="lint the production library")
    lane.add_argument("--soak", action="store_true", help="run ignored long-horizon soak tests")
    lane.add_argument(
        "--gameplay",
        choices=("contracts", *GAMEPLAY_TARGETS),
        metavar="SCOPE",
        help="run shared contracts or one focused gameplay scope",
    )
    lane.add_argument("--shaders", action="store_true", help="validate built-in WGSL")
    lane.add_argument("--rustdoc", action="store_true", help="build Rust API documentation")
    add_replay_options(gate)
    add_dry_run(gate)

    audit = presets.add_parser("audit", help="explicit broad maintained checkpoint")
    scope = audit.add_mutually_exclusive_group(required=True)
    scope.add_argument("--core", action="store_true", help="run the complete core behavior suite")
    scope.add_argument(
        "--gameplay",
        action="store_const",
        const="all",
        dest="gameplay",
        help="run the consolidated gameplay audit",
    )
    scope.add_argument("--all", action="store_true", help="run core plus consolidated gameplay")
    add_replay_options(audit)
    add_dry_run(audit)

    report = presets.add_parser("report", help="exploratory replayable gameplay evidence")
    report.add_argument(
        "--scope",
        choices=REPORT_SCOPES,
        default="all",
        help="run one exploratory gameplay family (default: all)",
    )
    report.add_argument("--verbose", action="store_true", help="print the complete replay transcript")
    add_replay_options(report)
    add_dry_run(report)

    bca = presets.add_parser("bca", help="build-free changed-source complexity review")
    bca.add_argument("--since", default="HEAD", help="git comparison revision")
    bca.add_argument(
        "--path",
        action="append",
        default=[],
        help="restrict review to a source scope; repeat for multiple scopes",
    )
    bca.add_argument(
        "--hotspots",
        action="store_true",
        help="review current history-aware hotspots instead of changed source only",
    )
    add_dry_run(bca)
    return parser


def normalize_args(args: argparse.Namespace) -> argparse.Namespace:
    """Give plan/environment helpers stable fields without exposing irrelevant CLI options."""

    defaults = {
        "all": False,
        "core": False,
        "lint": False,
        "production": False,
        "soak": False,
        "gameplay": None,
        "shaders": False,
        "rustdoc": False,
        "dry_run": False,
        "since": "HEAD",
        "path": [],
        "hotspots": False,
        "verbose": False,
        "scope": "all",
        "variation_seed": None,
        "behavior_seed": None,
    }
    for field, value in defaults.items():
        if not hasattr(args, field):
            setattr(args, field, value)
    return args


def validate_preset_options(parser: argparse.ArgumentParser, args: argparse.Namespace) -> None:
    if args.preset == "report" and args.behavior_seed is not None and args.scope not in REPORT_BEHAVIOR_SCOPES:
        parser.error(
            f"report scope {args.scope!r} does not consume actor-policy variation; omit --behavior-seed"
        )
    if args.variation_seed is not None or args.behavior_seed is not None:
        variation_behavior = gameplay_uses_behavior_seed(args)
        if variation_behavior is None:
            parser.error("--variation-seed and --behavior-seed require a gameplay gate, audit, or report")
        if args.behavior_seed is not None and variation_behavior is False:
            parser.error("the selected gameplay scope does not use an actor-policy behavior seed")
        if args.preset in ("gate", "audit") and args.behavior_seed is not None and args.variation_seed is None:
            parser.error("routine gameplay --behavior-seed requires --variation-seed for complete replay")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = build_parser()
    arguments = list(sys.argv[1:] if argv is None else argv)
    if not arguments:
        arguments = ["quick"]
    elif arguments[0].startswith("-") and arguments[0] not in ("-h", "--help"):
        arguments.insert(0, "quick")
    args = normalize_args(parser.parse_args(arguments))
    validate_preset_options(parser, args)
    return args


def main() -> int:
    args = parse_args()
    if args.preset == "report":
        if args.verbose:
            os.environ["DEEP_HEARTH_GAMEPLAY_VERBOSE"] = "1"
    plan = plan_for(args)
    if args.dry_run:
        for label, command in plan:
            print(f"{label}: {' '.join(command)}")
        return 0
    if args.preset == "report":
        configure_report_mode_environment(args, os.environ)
        try:
            configure_report_replay_environment(args, os.environ)
        except ValueError as error:
            print(f"gameplay replay: {error}", file=sys.stderr)
            return 2
    elif gameplay_uses_behavior_seed(args) is not None:
        configure_gameplay_verification_environment(args, os.environ)
    elif args.preset == "gate" and args.gameplay == "contracts":
        os.environ.pop(GAMEPLAY_REPORT_MODE_ENV, None)
        clear_gameplay_seed_environment(os.environ)

    started = time.perf_counter()
    try:
        if args.preset == "quick":
            quick_timings = run_quick_stages(plan)
            if quick_timings is None:
                return 1
            total_elapsed = time.perf_counter() - started
            print(f"quick ... PASS ({total_elapsed:.1f}s; {len(quick_timings)} checks)")
            return 0

        timings: list[tuple[str, float]] = []
        if len(plan) > 1:
            print(f"local-ci {args.preset}: {len(plan)} stage(s)")
        for index, (label, command) in enumerate(plan, start=1):
            elapsed = run_stage(
                index,
                len(plan),
                label,
                command,
                echo_success=(
                    args.preset == "report"
                    or (args.preset == "bca" and args.hotspots)
                ),
                show_replay=(
                    (
                        args.preset == "report"
                        and os.environ.get("DEEP_HEARTH_GAMEPLAY_VERBOSE") is None
                        and os.environ.get("DEEP_HEARTH_GAMEPLAY_TRACE") is None
                    )
                    or args.preset in ("gate", "audit")
                ),
            )
            if elapsed is None:
                return 1
            timings.append((label, elapsed))
    except KeyboardInterrupt:
        print("\nINTERRUPTED", file=sys.stderr)
        return 130
    total_elapsed = time.perf_counter() - started
    if len(timings) > 1:
        slowest_label, slowest_elapsed = max(timings, key=lambda item: item[1])
        print(
            f"PASS total ({total_elapsed:.1f}s; slowest={slowest_label} {slowest_elapsed:.1f}s)"
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except BrokenPipeError:
        try:
            sys.stdout.close()
        finally:
            raise SystemExit(0) from None
