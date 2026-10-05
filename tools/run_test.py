#!/usr/bin/env python3
"""Discover, type-check, build, or run one targeted Rust test surface."""

from __future__ import annotations

import argparse
import difflib
from functools import lru_cache
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time
import tomllib

if __package__:
    from . import cargo_env, gameplay_targets, replay_seed, test_catalog
else:
    import cargo_env
    import gameplay_targets
    import replay_seed
    import test_catalog


ROOT = Path(__file__).resolve().parents[1]
ZERO_TESTS = re.compile(r"\brunning 0 tests\b")
TEST_RESULT = re.compile(
    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored;"
)
FAILURE_HEAD_LINES = 16
FAILURE_TAIL_LINES = 64
CATALOG_DISPLAY_LIMIT = 40
GAMEPLAY_VARIATION_ENV = gameplay_targets.GAMEPLAY_VARIATION_ENV
GAMEPLAY_BEHAVIOR_ENV = gameplay_targets.GAMEPLAY_BEHAVIOR_ENV
GAMEPLAY_REPORT_MODE_ENV = gameplay_targets.GAMEPLAY_REPORT_MODE_ENV
GAMEPLAY_VARIATION_SCOPE_ENV = gameplay_targets.GAMEPLAY_VARIATION_SCOPE_ENV
GAMEPLAY_FEATURE = gameplay_targets.GAMEPLAY_FEATURE
GAMEPLAY_PROBE_TESTS = gameplay_targets.GAMEPLAY_PROBE_TESTS
GAMEPLAY_PROBE_SCOPES = gameplay_targets.GAMEPLAY_PROBE_SCOPES
GAMEPLAY_BEHAVIOR_PROBE_TESTS = gameplay_targets.GAMEPLAY_BEHAVIOR_PROBE_TESTS


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
    """Return one cache-stable local test feature shape plus any explicit features."""

    requested = feature_set(raw)
    if target == "lib":
        # Exact/suite unit tests share the additive gameplay-test library artifact used by
        # `ci.py audit --all`. The library test catalog is identical with this feature enabled,
        # while switching between owner tests and gameplay checkpoints avoids a full lib rebuild.
        requested.add(GAMEPLAY_FEATURE)
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


def test_targets() -> tuple[str, ...]:
    """Return every explicit executable Rust test target, including the library test crate."""

    return (
        "lib",
        *(definition["name"] for definition in cargo_manifest().get("test", [])),
    )


@lru_cache(maxsize=None)
def _all_source_test_locations(raw_features: str | None) -> tuple[tuple[str, str], ...]:
    return tuple(
        (target, name)
        for target in test_targets()
        for name in source_test_catalog(target, raw_features)
    )


def all_source_test_locations(raw_features: str | None) -> list[tuple[str, str]]:
    """Return the build-free logical test catalog across every explicit Cargo test target."""

    return list(_all_source_test_locations(raw_features))


def all_source_test_names(raw_features: str | None) -> list[str]:
    return sorted({name for _target, name in all_source_test_locations(raw_features)})


@lru_cache(maxsize=None)
def target_source_paths(target: str, raw_features: str | None) -> frozenset[Path]:
    """Return one target's reachable Rust files for build-free topology checks."""

    features = cargo_feature_set(target, raw_features)
    root = ROOT / "src" / "lib.rs" if target == "lib" else cargo_test_target_path(target)
    return frozenset(
        path.resolve()
        for path, _prefix in test_catalog.reachable_modules(ROOT, root, features)
    )


def preferred_target(targets: set[str]) -> str:
    """Select the unique purpose-built owner; use the consolidated audit only as fallback."""

    audit = gameplay_targets.GAMEPLAY_AUDIT_TARGET
    purpose_built = targets - {audit}
    if len(purpose_built) == 1:
        return next(iter(purpose_built))
    if not purpose_built and targets == {audit}:
        return audit
    owners = ", ".join(sorted(purpose_built or targets))
    raise ValueError(f"logical test has multiple purpose-built targets: {owners}")


def resolve_automatic_exact_selection(
    selector: str, raw_features: str | None
) -> tuple[str, str]:
    """Resolve one logical test globally, then choose its purpose-built Cargo target."""

    if target := gameplay_targets.GAMEPLAY_PROBE_TARGETS.get(selector):
        return target, selector

    library_catalog = source_test_catalog("lib", raw_features)
    if selector_uses_library_owner(selector, library_catalog):
        library_matches = source_test_matches(selector, library_catalog)
        if len(library_matches) == 1:
            return "lib", library_matches[0]

    locations = all_source_test_locations(raw_features)
    exact = [(target, name) for target, name in locations if name == selector]
    matches = exact or [(target, name) for target, name in locations if selector in name]
    names = sorted({name for _target, name in matches})
    if len(names) > 1:
        raise ValueError(f"test selector is ambiguous: {selector} ({len(names)} matches)")
    if not names:
        raise ValueError(f"test selector not found: {selector}")
    name = names[0]
    targets = {target for target, candidate in matches if candidate == name}
    return preferred_target(targets), name


def resolve_automatic_suite_target(selector: str, raw_features: str | None) -> str:
    """Choose one purpose-built target containing the complete globally matched logical suite."""

    library_catalog = source_test_catalog("lib", raw_features)
    if selector_uses_library_owner(selector, library_catalog):
        library_matches = source_test_matches(selector, library_catalog)
        if library_matches:
            return "lib"

    matches_by_target = {
        target: source_test_matches(selector, source_test_catalog(target, raw_features))
        for target in test_targets()
    }
    logical_matches = {
        name for matches in matches_by_target.values() for name in matches
    }
    if not logical_matches:
        raise ValueError(f"test suite selector not found: {selector}")
    complete_targets = {
        target
        for target, matches in matches_by_target.items()
        if set(matches) == logical_matches
    }
    if not complete_targets:
        targets = ", ".join(
            target for target, matches in matches_by_target.items() if matches
        )
        raise ValueError(
            f"test suite selector spans different target catalogs: {selector} ({targets}); "
            "specify --target"
        )
    return preferred_target(complete_targets)


def source_test_matches(selector: str, catalog: list[str]) -> list[str]:
    """Return source-catalog tests selected by an exact name or substring."""

    if selector in catalog:
        return [selector]
    return [name for name in catalog if selector in name]


def selector_uses_library_owner(selector: str, library_catalog: list[str]) -> bool:
    """Return whether a qualified selector starts in a library-owned test namespace."""

    owner, separator, _remainder = selector.partition("::")
    if not separator:
        return False
    prefix = f"{owner}::"
    return any(name.startswith(prefix) for name in library_catalog)


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
    if args.target is None:
        raise ValueError("test target must be resolved before Cargo execution")
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
    if getattr(args, "verbose", False):
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


def cargo_build_command(args: argparse.Namespace) -> list[str]:
    """Build one selected test artifact without executing it, warming the eventual test cache."""

    if args.list:
        raise ValueError("source catalog listing does not invoke Cargo")
    if args.target is None:
        raise ValueError("--build requires an explicit test target")
    command = ["cargo", "test", "--quiet", "--locked", "--no-run"]
    if args.target == "lib":
        command.append("--lib")
    else:
        command.extend(("--test", args.target))
    requested_features = requested_target_features(args.target, args.features)
    if requested_features:
        command.extend(("--features", ",".join(sorted(requested_features))))
    return command


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run one exact cached Rust test or bounded suite, build one selected target, or "
            "inspect the build-free source catalog."
        )
    )
    parser.add_argument(
        "name",
        nargs="?",
        help="fully qualified test name or unique source-catalog substring",
    )
    compile_mode = parser.add_mutually_exclusive_group()
    compile_mode.add_argument(
        "--list",
        action="store_true",
        help="list exact source test names without compiling or linking",
    )
    compile_mode.add_argument(
        "--build",
        action="store_true",
        help=(
            "compile and link the smallest matching test target without executing it; "
            "the resulting artifact is reused by the later test run"
        ),
    )
    parser.add_argument(
        "--suite",
        action="store_true",
        help="run every source-catalog test matching NAME in one Cargo invocation",
    )
    parser.add_argument(
        "--target",
        help=(
            "explicit Cargo test target and cache shape; selector-based modes otherwise resolve "
            "the purpose-built matching source target automatically"
        ),
    )
    parser.add_argument(
        "--features",
        help="extra Cargo features; target required-features are inferred from Cargo.toml",
    )
    parser.add_argument("--ignored", action="store_true", help="select an ignored exact test")
    parser.add_argument(
        "--verbose",
        action="store_true",
        help=(
            "show selected-test output; full gameplay review/trace diagnostics belong to "
            "`python ci.py report`"
        ),
    )
    parser.add_argument(
        "--variation-seed",
        type=replay_seed.parse_replay_seed,
        help=(
            "add or replay one organic gameplay case for an exact probe; exact gameplay tests "
            "otherwise run maintained witnesses only"
        ),
    )
    parser.add_argument(
        "--behavior-seed",
        type=replay_seed.parse_replay_seed,
        help="replay DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED for this test execution",
    )
    args = parser.parse_args(argv)
    if not args.list and not args.build and not args.name:
        parser.error("a test selector is required for test execution")
    compile_only = args.build
    if args.suite and (args.list or compile_only):
        parser.error("--suite is an execution mode and cannot be combined with --list/--build")
    if compile_only and args.target is None and not args.name:
        parser.error("--build requires either a source selector or explicit --target")
    if compile_only and args.target is not None and args.name:
        parser.error(
            "with explicit --target, --build validates the whole target; omit NAME"
        )
    if args.suite and args.ignored:
        parser.error("--ignored requires exact execution; use an exact ignored-test selector")
    if (args.list or compile_only) and (args.ignored or args.verbose):
        parser.error("--ignored and --verbose apply only to test execution")
    if (args.list or compile_only) and (args.variation_seed or args.behavior_seed):
        parser.error("gameplay replay seeds apply only to test execution")
    if args.behavior_seed is not None and args.variation_seed is None:
        parser.error("--behavior-seed requires --variation-seed for a complete gameplay replay")
    return args


def load_source_catalog(args: argparse.Namespace) -> list[str] | None:
    if args.target is None:
        raise ValueError("source catalog target must be resolved before loading")
    try:
        return source_test_catalog(args.target, args.features)
    except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"FAIL source test catalog: {error}", file=sys.stderr)
        return None


def print_source_catalog(args: argparse.Namespace, catalog: list[str]) -> None:
    names = catalog if not args.name else [name for name in catalog if args.name in name]
    shown = names[:CATALOG_DISPLAY_LIMIT]
    for name in shown:
        print(name)
    if len(names) > len(shown):
        print(f"... {len(names) - len(shown)} more; refine the selector")
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


def resolve_automatic_selection(args: argparse.Namespace) -> tuple[str, list[str]] | None:
    """Resolve an omitted target without invoking Cargo, preserving one executable target."""

    selector = args.name
    assert selector is not None
    try:
        if args.suite:
            args.target = resolve_automatic_suite_target(selector, args.features)
        else:
            args.target, args.name = resolve_automatic_exact_selection(selector, args.features)
        return selector, source_test_catalog(args.target, args.features)
    except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
        report_selection_error(selector, all_source_test_names(args.features), error)
        return None


def resolve_automatic_build_target(args: argparse.Namespace) -> bool:
    """Resolve `--build` to the smallest matching source target."""

    selector = args.name
    assert selector is not None
    if target := gameplay_targets.GAMEPLAY_PROBE_TARGETS.get(selector):
        args.target = target
        return True
    try:
        library_catalog = source_test_catalog("lib", args.features)
        if selector_uses_library_owner(selector, library_catalog) and source_test_matches(
            selector, library_catalog
        ):
            args.target = "lib"
            return True
        args.target = resolve_automatic_suite_target(selector, args.features)
    except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
        report_selection_error(selector, all_source_test_names(args.features), error)
        return False
    return True


def gameplay_replay_environment(
    args: argparse.Namespace,
    *,
    randbits=secrets.randbits,
) -> dict[str, str]:
    """Add one organic case only when an exact gameplay probe explicitly requests it."""

    if args.suite or args.name not in GAMEPLAY_PROBE_TESTS:
        if args.variation_seed or args.behavior_seed:
            raise ValueError("gameplay replay seeds require one exact gameplay probe")
        return {}
    uses_behavior_seed = args.name in GAMEPLAY_BEHAVIOR_PROBE_TESTS
    if args.behavior_seed and not uses_behavior_seed:
        raise ValueError(f"{args.name} does not consume an actor-policy behavior seed")
    if args.variation_seed is None:
        return {}
    variation = args.variation_seed
    replay = {
        GAMEPLAY_VARIATION_ENV: variation,
        GAMEPLAY_VARIATION_SCOPE_ENV: GAMEPLAY_PROBE_SCOPES[args.name],
    }
    if uses_behavior_seed:
        replay[GAMEPLAY_BEHAVIOR_ENV] = (
            args.behavior_seed or f"0x{randbits(64):016X}"
        )
    return replay


def execute_cargo_command(
    command: list[str],
    environment_overrides: dict[str, str] | None = None,
) -> tuple[subprocess.CompletedProcess[str], float]:
    environment = cargo_env.local_cargo_environment()
    for key in (
        GAMEPLAY_REPORT_MODE_ENV,
        GAMEPLAY_VARIATION_ENV,
        GAMEPLAY_BEHAVIOR_ENV,
        GAMEPLAY_VARIATION_SCOPE_ENV,
    ):
        environment.pop(key, None)
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


def replay_command(args: argparse.Namespace, replay: dict[str, str]) -> str | None:
    """Return a copyable focused-test replay command when organic roots were supplied."""

    variation = replay.get(GAMEPLAY_VARIATION_ENV)
    if variation is None or args.target is None or args.name is None:
        return None
    command = ["python", "tools/run_test.py", "--target", args.target]
    if args.suite:
        command.append("--suite")
    if args.ignored:
        command.append("--ignored")
    command.extend(("--variation-seed", variation))
    if behavior := replay.get(GAMEPLAY_BEHAVIOR_ENV):
        command.extend(("--behavior-seed", behavior))
    command.append(args.name)
    return " ".join(command)


def report_cargo_failure(
    command: list[str],
    result: subprocess.CompletedProcess[str],
    elapsed: float,
    replay: str | None = None,
) -> None:
    print(f"FAIL ({elapsed:.1f}s)", file=sys.stderr)
    print(f"reproduce: {' '.join(command)}", file=sys.stderr)
    if replay is not None:
        print(f"replay: {replay}", file=sys.stderr)
    for diagnostic in bounded_failure_streams(result.stdout, result.stderr):
        print(diagnostic, file=sys.stderr)


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


def bounded_failure_streams(stdout: str, stderr: str) -> list[str]:
    """Return non-empty bounded diagnostics once, even when Cargo duplicates captured streams."""

    rendered: list[str] = []
    for stream in (stdout, stderr):
        if not stream.strip():
            continue
        bounded = bounded_failure_output(stream)
        if bounded not in rendered:
            rendered.append(bounded)
    return rendered


def suite_result_detail(stdout: str) -> str:
    counts = executed_test_counts(stdout)
    if counts is None:
        return "tests executed"
    passed, ignored = counts
    detail = f"{passed} tests"
    if ignored:
        detail += f", {ignored} ignored"
    return detail


def execution_error(
    args: argparse.Namespace,
    stdout: str,
    *,
    expected_suite_count: int | None = None,
) -> tuple[str, str] | None:
    """Return one precise failure and repair command when selected tests did not execute."""

    counts = executed_test_counts(stdout)
    if counts is not None:
        passed, ignored = counts
        if args.suite and expected_suite_count is not None:
            selected = passed + ignored
            if selected != expected_suite_count:
                return (
                    "Cargo/source catalog suite selection drifted: "
                    f"catalog={expected_suite_count} cargo={selected} for {args.name}",
                    f"python tools/run_test.py --list {args.name}",
                )
        if passed > 0:
            return None
        if not args.suite and ignored > 0 and not args.ignored:
            command = ["python", "tools/run_test.py", "--ignored", "--target", args.target]
            if args.variation_seed:
                command.extend(("--variation-seed", args.variation_seed))
            if args.behavior_seed:
                command.extend(("--behavior-seed", args.behavior_seed))
            command.append(args.name)
            return (
                f"cataloged exact test is ignored: {args.name}",
                " ".join(command),
            )
    if ZERO_TESTS.search(stdout) or counts is not None:
        mode = "suite" if args.suite else "exact test"
        return (
            f"Cargo did not execute cataloged {mode}: {args.name}",
            f"python tools/run_test.py --list {args.name}",
        )
    return None


def report_cargo_success(
    args: argparse.Namespace,
    selector: str | None,
    result: subprocess.CompletedProcess[str],
    elapsed: float,
    replay: dict[str, str] | None = None,
) -> None:
    if getattr(args, "verbose", False) and result.stdout.strip():
        print(result.stdout.rstrip())
    if args.suite:
        print(
            f"PASS suite {args.target}::{selector} "
            f"({suite_result_detail(result.stdout)}; {elapsed:.1f}s)"
        )
        return
    details = [f"{elapsed:.1f}s"]
    if replay and GAMEPLAY_VARIATION_ENV in replay:
        roots = replay[GAMEPLAY_VARIATION_ENV]
        if behavior := replay.get(GAMEPLAY_BEHAVIOR_ENV):
            roots = f"{roots}/{behavior}"
        details.append(f"roots={roots}")
    print(f"PASS {args.target}::{args.name} ({'; '.join(details)})")


def main() -> int:
    args = parse_args()
    if args.build and args.target is None:
        if not resolve_automatic_build_target(args):
            return 2
    if args.build:
        try:
            command = cargo_build_command(args)
        except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
            print(f"FAIL build selection: {error}", file=sys.stderr)
            return 2
        result, elapsed = execute_cargo_command(command)
        if result.returncode != 0:
            report_cargo_failure(command, result, elapsed)
            return result.returncode
        print(f"PASS build {args.target} ({elapsed:.1f}s)")
        return 0
    if args.list:
        if args.target is None:
            try:
                catalog = all_source_test_names(args.features)
            except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
                print(f"FAIL source test catalog: {error}", file=sys.stderr)
                return 2
        else:
            catalog = load_source_catalog(args)
            if catalog is None:
                return 2
        print_source_catalog(args, catalog)
        return 0

    selected_catalog: list[str]
    if args.target is None:
        resolved = resolve_automatic_selection(args)
        if resolved is None:
            return 2
        selector, selected_catalog = resolved
    else:
        catalog = load_source_catalog(args)
        if catalog is None:
            return 2
        selector = resolve_requested_selection(args, catalog)
        if selector is None:
            return 2
        selected_catalog = catalog

    command = cargo_command(args)
    try:
        replay = gameplay_replay_environment(args)
    except ValueError as error:
        print(f"FAIL gameplay replay: {error}", file=sys.stderr)
        return 2
    result, elapsed = execute_cargo_command(command, replay)
    if result.returncode != 0:
        report_cargo_failure(command, result, elapsed, replay_command(args, replay))
        return result.returncode

    expected_suite_count = (
        len(source_test_matches(selector, selected_catalog)) if args.suite else None
    )
    if mismatch := execution_error(
        args,
        result.stdout,
        expected_suite_count=expected_suite_count,
    ):
        failure, repair = mismatch
        print(f"FAIL {failure}", file=sys.stderr)
        print(f"repair: {repair}", file=sys.stderr)
        return 2

    report_cargo_success(args, selector, result, elapsed, replay)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
