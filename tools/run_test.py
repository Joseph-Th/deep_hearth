#!/usr/bin/env python3
"""Discover, type-check, or run exact Rust tests without paying for selector mistakes."""

from __future__ import annotations

import argparse
import difflib
from functools import lru_cache
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import tomllib

import test_catalog


ROOT = Path(__file__).resolve().parents[1]
TEST_SUPPORT_FEATURE = "test-gameplay"
ZERO_TESTS = re.compile(r"\brunning 0 tests\b")
TEST_RESULT = re.compile(
    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored;"
)
FAILURE_HEAD_LINES = 16
FAILURE_TAIL_LINES = 64


def feature_set(raw: str | None) -> set[str]:
    if not raw:
        return set()
    return {feature for feature in re.split(r"[,\s]+", raw.strip()) if feature}


def expand_local_features(
    declared: dict[str, list[str]], requested: set[str], *, include_default: bool
) -> set[str]:
    """Expand Cargo-local feature groups without treating dependency features as local cfgs."""

    enabled = set(requested)
    if include_default and "default" in declared:
        enabled.add("default")
    pending = list(enabled)
    while pending:
        feature = pending.pop()
        for activated in declared.get(feature, []):
            if activated not in declared or activated in enabled:
                continue
            enabled.add(activated)
            pending.append(activated)
    return enabled


@lru_cache(maxsize=1)
def cargo_manifest() -> dict:
    return tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))


def cargo_test_target_definition(target: str) -> dict:
    for definition in cargo_manifest().get("test", []):
        if definition.get("name") == target:
            return definition
    raise ValueError(f"unknown Cargo test target: {target}")


def requested_target_features(target: str, raw: str | None) -> set[str]:
    """Return explicit features plus the target's Cargo-declared required features."""

    requested = feature_set(raw)
    if target == "lib":
        requested.add(TEST_SUPPORT_FEATURE)
    else:
        requested.update(cargo_test_target_definition(target).get("required-features", []))
    return requested


def cargo_feature_set(target: str, raw: str | None) -> set[str]:
    """Resolve the local cfg(feature) set Cargo enables for the exact-test command."""

    manifest = cargo_manifest()
    declared = manifest.get("features", {})
    return expand_local_features(
        declared, requested_target_features(target, raw), include_default=True
    )


def cargo_test_target_path(target: str) -> Path:
    return ROOT / cargo_test_target_definition(target)["path"]


attributes_enabled = test_catalog.attributes_enabled
root_sibling_imports = test_catalog.root_sibling_imports


def missing_root_modules(target: str, module_directory: Path) -> list[str]:
    """Find source-referenced root sibling modules omitted by one integration-test crate root."""

    features = cargo_feature_set(target, None)
    return test_catalog.missing_root_modules(
        ROOT,
        cargo_test_target_path(target),
        module_directory,
        features,
    )


def reachable_test_names(root: Path, features: set[str]) -> list[str]:
    return test_catalog.reachable_test_names(ROOT, root, features)


def integration_test_names(target: str, features: set[str]) -> list[str]:
    return reachable_test_names(cargo_test_target_path(target), features)


@lru_cache(maxsize=None)
def _source_test_catalog(target: str, raw_features: str | None) -> tuple[str, ...]:
    """Cache exact source test names for repeated build-free discovery in one process."""

    features = cargo_feature_set(target, raw_features)
    if target == "lib":
        names = reachable_test_names(ROOT / "src" / "lib.rs", features)
    else:
        names = integration_test_names(target, features)
    return tuple(sorted(set(names)))


def source_test_catalog(target: str, raw_features: str | None) -> list[str]:
    """Return exact test names from source without invoking Cargo or rustc."""

    return list(_source_test_catalog(target, raw_features))


def source_test_matches(selector: str, catalog: list[str]) -> list[str]:
    """Return source-catalog tests selected by an exact name or substring."""

    if selector in catalog:
        return [selector]
    return [name for name in catalog if selector in name]


def resolve_test_name(selector: str, catalog: list[str]) -> str:
    """Resolve one source selector without ever widening execution beyond one exact test."""

    matches = source_test_matches(selector, catalog)
    if len(matches) == 1:
        return matches[0]
    if matches:
        raise ValueError(f"test selector is ambiguous: {selector} ({len(matches)} matches)")
    raise ValueError(f"test selector not found: {selector}")


def cargo_command(args: argparse.Namespace) -> list[str]:
    if args.list:
        raise ValueError("source catalog listing does not invoke Cargo")
    command = ["cargo", "test", "--quiet", "--locked"]
    if args.target == "lib":
        command.append("--lib")
    else:
        command.extend(("--test", args.target))
    requested_features = requested_target_features(args.target, args.features)
    if requested_features:
        command.extend(("--features", ",".join(sorted(requested_features))))
    command.append(args.name)
    test_args: list[str] = []
    if not args.suite:
        test_args.append("--exact")
    if args.ignored:
        test_args.append("--ignored")
    if args.nocapture:
        test_args.append("--nocapture")
    if test_args:
        command.append("--")
        command.extend(test_args)
    return command


def executed_test_counts(stdout: str) -> tuple[int, int] | None:
    """Return executed and ignored counts from one selected Cargo test target."""

    matches = list(TEST_RESULT.finditer(stdout))
    if not matches:
        return None
    match = matches[-1]
    return int(match.group("passed")), int(match.group("ignored"))


def cargo_check_command(args: argparse.Namespace) -> list[str]:
    """Type-check one integration-test target without code generation or linking."""

    if args.list:
        raise ValueError("source catalog listing does not invoke Cargo")
    if args.target == "lib":
        raise ValueError(
            "lib --check is intentionally unsupported because Cargo's test check selects every "
            "integration target; run the exact unit test instead"
        )
    command = ["cargo", "check", "--quiet", "--locked"]
    command.extend(("--test", args.target))
    requested_features = requested_target_features(args.target, args.features)
    if requested_features:
        command.extend(("--features", ",".join(sorted(requested_features))))
    return command


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run one exact cached Rust test or one bounded source-catalog suite, type-check one "
            "integration target without linking, or inspect the build-free source catalog."
        )
    )
    parser.add_argument(
        "name",
        nargs="?",
        help="fully qualified test name or unique source-catalog substring",
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="list exact source test names without compiling or linking",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="type-check one integration test target without linking; lib unit tests execute exactly",
    )
    parser.add_argument(
        "--suite",
        action="store_true",
        help="run every source-catalog test matching NAME in one Cargo invocation",
    )
    parser.add_argument("--target", default="lib", help="Cargo test target name; defaults to lib")
    parser.add_argument(
        "--features",
        help="extra Cargo features; target required-features are inferred from Cargo.toml",
    )
    parser.add_argument("--ignored", action="store_true", help="select an ignored exact test")
    parser.add_argument("--nocapture", action="store_true", help="show selected-test output")
    parser.add_argument(
        "--variation-seed",
        help="replay DEEP_HEARTH_GAMEPLAY_VARIATION_SEED for this test execution",
    )
    parser.add_argument(
        "--behavior-seed",
        help="replay DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED for this test execution",
    )
    args = parser.parse_args(argv)
    if not args.list and not args.check and not args.name:
        parser.error("a test selector is required for test execution")
    if args.list and args.check:
        parser.error("--list and --check are mutually exclusive")
    if args.suite and (args.list or args.check):
        parser.error("--suite is an execution mode and cannot be combined with --list or --check")
    if args.check and args.target == "lib":
        parser.error(
            "--check cannot target lib without checking every integration target; run the exact "
            "unit test instead"
        )
    if args.check and args.name:
        parser.error("--check validates the whole integration target; omit the test selector")
    if args.suite and args.ignored:
        parser.error("--ignored requires exact execution; use an exact ignored-test selector")
    if (args.list or args.check) and (args.ignored or args.nocapture):
        parser.error("--ignored and --nocapture apply only to execution modes")
    if (args.list or args.check) and (args.variation_seed or args.behavior_seed):
        parser.error("gameplay replay seeds are execution-only options")
    return args


def load_source_catalog(args: argparse.Namespace) -> list[str] | None:
    try:
        return source_test_catalog(args.target, args.features)
    except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"FAIL source test catalog: {error}", file=sys.stderr)
        return None


def print_source_catalog(args: argparse.Namespace, catalog: list[str]) -> None:
    names = catalog if not args.name else [name for name in catalog if args.name in name]
    for name in names:
        print(name)
    print(f"{len(names)} test(s)")


def report_selection_error(selector: str, catalog: list[str], error: ValueError) -> None:
    candidates = source_test_matches(selector, catalog)
    if not candidates:
        candidates = difflib.get_close_matches(selector, catalog, n=5, cutoff=0.45)
    print(f"FAIL {error}", file=sys.stderr)
    print(f"catalog: python tools/run_test.py --list {selector}", file=sys.stderr)
    for candidate in candidates[:8]:
        print(f"candidate: {candidate}", file=sys.stderr)


def resolve_requested_selection(args: argparse.Namespace, catalog: list[str]) -> str | None:
    selector = args.name
    assert selector is not None

    try:
        if args.suite:
            matches = source_test_matches(selector, catalog)
            if not matches:
                raise ValueError(f"test suite selector not found: {selector}")
            args.name = selector
        else:
            args.name = resolve_test_name(selector, catalog)
    except ValueError as error:
        report_selection_error(selector, catalog, error)
        return None
    return selector


def gameplay_replay_environment(args: argparse.Namespace) -> dict[str, str]:
    replay: dict[str, str] = {}
    if args.variation_seed:
        replay["DEEP_HEARTH_GAMEPLAY_VARIATION_SEED"] = args.variation_seed
    if args.behavior_seed:
        replay["DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED"] = args.behavior_seed
    return replay


def execute_cargo_command(
    command: list[str],
    environment_overrides: dict[str, str] | None = None,
) -> tuple[subprocess.CompletedProcess[str], float]:
    environment = os.environ.copy()
    environment["CARGO_TERM_COLOR"] = "never"
    if environment_overrides:
        environment.update(environment_overrides)
    started = time.perf_counter()
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )
    return result, time.perf_counter() - started


def report_cargo_failure(
    command: list[str],
    result: subprocess.CompletedProcess[str],
    elapsed: float,
) -> None:
    print(f"FAIL ({elapsed:.1f}s)", file=sys.stderr)
    print(f"reproduce: {' '.join(command)}", file=sys.stderr)
    if result.stdout.strip():
        print(bounded_failure_output(result.stdout), file=sys.stderr)
    if result.stderr.strip():
        print(bounded_failure_output(result.stderr), file=sys.stderr)


def bounded_failure_output(output: str) -> str:
    """Retain useful compiler/test context without flooding a local repair loop."""

    lines = output.rstrip().splitlines()
    limit = FAILURE_HEAD_LINES + FAILURE_TAIL_LINES
    if len(lines) <= limit:
        return "\n".join(lines)
    omitted = len(lines) - limit
    return "\n".join(
        [
            *lines[:FAILURE_HEAD_LINES],
            f"... {omitted} line(s) omitted ...",
            *lines[-FAILURE_TAIL_LINES:],
        ]
    )


def suite_result_detail(stdout: str) -> str:
    counts = executed_test_counts(stdout)
    if counts is None:
        return "tests executed"
    passed, ignored = counts
    detail = f"{passed} tests"
    if ignored:
        detail += f", {ignored} ignored"
    return detail


def report_cargo_success(
    args: argparse.Namespace,
    selector: str | None,
    result: subprocess.CompletedProcess[str],
    elapsed: float,
) -> None:
    if args.check:
        print(f"PASS check {args.target} ({elapsed:.1f}s)")
        return
    if not args.check and args.nocapture and result.stdout.strip():
        print(result.stdout.rstrip())
    if args.suite:
        print(
            f"PASS suite {args.target}::{selector} "
            f"({suite_result_detail(result.stdout)}; {elapsed:.1f}s)"
        )
        return
    print(f"PASS {args.target}::{args.name} ({elapsed:.1f}s)")


def main() -> int:
    args = parse_args()
    if args.check:
        command = cargo_check_command(args)
        result, elapsed = execute_cargo_command(command)
        if result.returncode != 0:
            report_cargo_failure(command, result, elapsed)
            return result.returncode
        report_cargo_success(args, None, result, elapsed)
        return 0

    catalog = load_source_catalog(args)
    if catalog is None:
        return 2
    if args.list:
        print_source_catalog(args, catalog)
        return 0

    selector = resolve_requested_selection(args, catalog)
    if selector is None:
        return 2

    command = cargo_command(args)
    result, elapsed = execute_cargo_command(command, gameplay_replay_environment(args))
    if result.returncode != 0:
        report_cargo_failure(command, result, elapsed)
        return result.returncode

    if not args.check and ZERO_TESTS.search(result.stdout):
        mode = "suite" if args.suite else "exact test"
        print(f"FAIL Cargo did not execute cataloged {mode}: {args.name}", file=sys.stderr)
        print(f"catalog: python tools/run_test.py --list {args.name}", file=sys.stderr)
        return 2

    report_cargo_success(args, selector, result, elapsed)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
