#!/usr/bin/env python3
"""Fail-closed contract check for aggregate selectors and focused gameplay targets."""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
import argparse
import json
from pathlib import Path
import shlex
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / ".cargo" / "config.toml"
MANIFEST = ROOT / "Cargo.toml"


@dataclass(frozen=True)
class Selector:
    alias: str
    test: str
    ignored: bool = False


SELECTORS = (
    Selector("test-gameplay-report", "gameplay_harness_exploratory_report", ignored=True),
)

SUITES = {
    "test-gameplay-workshop": "gameplay_workshop",
    "test-gameplay-survival": "gameplay_survival",
    "test-gameplay-progression": "gameplay_progression",
    "test-gameplay-ore": "gameplay_ore",
    "test-gameplay-foundry": "gameplay_foundry",
}

CHECK_SUITES = {
    "check-gameplay-workshop": "gameplay_workshop",
    "check-gameplay-survival": "gameplay_survival",
    "check-gameplay-progression": "gameplay_progression",
    "check-gameplay-ore": "gameplay_ore",
    "check-gameplay-foundry": "gameplay_foundry",
}

TARGET_PATHS = {
    "gameplay_workshop": "tests/gameplay_harness/main.rs",
}


def parse_listing(text: str) -> Counter[str]:
    tests: Counter[str] = Counter()
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if line.endswith(": test"):
            tests[line.removesuffix(": test")] += 1
    return tests


def selector_errors(
    all_tests: Counter[str], ignored_tests: Counter[str], selectors: tuple[Selector, ...]
) -> list[str]:
    errors: list[str] = []
    for selector in selectors:
        count = all_tests[selector.test]
        if count != 1:
            errors.append(
                f"{selector.alias}: selector {selector.test!r} resolves to {count} tests, expected exactly 1"
            )
            continue
        ignored_count = ignored_tests[selector.test]
        expected_ignored_count = 1 if selector.ignored else 0
        if ignored_count != expected_ignored_count:
            state = "ignored" if selector.ignored else "active"
            errors.append(
                f"{selector.alias}: selector {selector.test!r} is not {state} as configured"
            )
    return errors


def alias_errors(
    aliases: dict[str, str],
    selectors: tuple[Selector, ...],
    suites: dict[str, str],
) -> list[str]:
    errors: list[str] = []
    expected_aliases = {selector.alias for selector in selectors} | suites.keys()
    actual_aliases = {name for name in aliases if name.startswith("test-gameplay-")}
    unexpected = sorted(actual_aliases - expected_aliases)
    missing = sorted(expected_aliases - actual_aliases)
    if unexpected:
        errors.append(f"uncontracted gameplay aliases: {', '.join(unexpected)}")
    if missing:
        errors.append(f"missing gameplay aliases: {', '.join(missing)}")

    for alias, target in suites.items():
        command = aliases.get(alias)
        if command is None:
            continue
        expected = [
            "test",
            "--quiet",
            "--locked",
            "--test",
            target,
            "--features",
            "test-gameplay",
        ]
        if shlex.split(command) != expected:
            errors.append(
                f"{alias}: focused suite must run the complete {target!r} target without a test-name filter"
            )

    for selector in selectors:
        command = aliases.get(selector.alias)
        if command is None:
            continue
        tokens = shlex.split(command)
        if "--" not in tokens:
            errors.append(f"{selector.alias}: command has no libtest argument boundary")
            continue
        boundary = tokens.index("--")
        cargo_tokens = tokens[:boundary]
        libtest_tokens = tokens[boundary + 1 :]
        if selector.test not in cargo_tokens:
            errors.append(
                f"{selector.alias}: command does not select {selector.test!r}"
            )
        if "--test" not in cargo_tokens or "gameplay_harness" not in cargo_tokens:
            errors.append(f"{selector.alias}: command does not target gameplay_harness")
        if "--features" not in cargo_tokens or "test-gameplay-full" not in cargo_tokens:
            errors.append(f"{selector.alias}: command does not enable test-gameplay-full")
        if "--exact" not in libtest_tokens:
            errors.append(f"{selector.alias}: command is not an exact selector")
        has_ignored = "--ignored" in libtest_tokens
        if has_ignored != selector.ignored:
            expected = "with --ignored" if selector.ignored else "without --ignored"
            errors.append(f"{selector.alias}: command must run {expected}")
    return errors


def maintained_gate_alias_errors(
    aliases: dict[str, str], suites: dict[str, str]
) -> list[str]:
    command = aliases.get("test-gameplay")
    if command is None:
        return ["missing maintained gameplay gate alias: test-gameplay"]

    tokens = shlex.split(command)
    expected = Counter(["test", "--quiet", "--locked", "--features", "test-gameplay"])
    for target in suites.values():
        expected.update(["--test", target])
    if Counter(tokens) != expected:
        return [
            "test-gameplay: maintained gate must run each focused gameplay target exactly once "
            "with only the test-gameplay feature and no test-name filter"
        ]
    return []


def check_alias_errors(aliases: dict[str, str], suites: dict[str, str]) -> list[str]:
    errors: list[str] = []
    expected_aliases = set(suites)
    actual_aliases = {name for name in aliases if name.startswith("check-gameplay-")}
    unexpected = sorted(actual_aliases - expected_aliases)
    missing = sorted(expected_aliases - actual_aliases)
    if unexpected:
        errors.append(f"uncontracted gameplay check aliases: {', '.join(unexpected)}")
    if missing:
        errors.append(f"missing gameplay check aliases: {', '.join(missing)}")

    for alias, target in suites.items():
        command = aliases.get(alias)
        if command is None:
            continue
        expected = [
            "check",
            "--quiet",
            "--locked",
            "--test",
            target,
            "--features",
            "test-gameplay",
        ]
        if shlex.split(command) != expected:
            errors.append(
                f"{alias}: focused check must type-check only the complete {target!r} target"
            )
    return errors


def maintained_check_alias_errors(
    aliases: dict[str, str], suites: dict[str, str]
) -> list[str]:
    command = aliases.get("check-gameplay")
    if command is None:
        return ["missing maintained gameplay check alias: check-gameplay"]

    tokens = shlex.split(command)
    expected = Counter(["check", "--quiet", "--locked", "--features", "test-gameplay"])
    for target in suites.values():
        expected.update(["--test", target])
    if Counter(tokens) != expected:
        return [
            "check-gameplay: maintained check must include each focused gameplay target exactly once "
            "with only the test-gameplay feature"
        ]
    return []


def focused_target_errors(manifest: dict[str, object], suites: dict[str, str]) -> list[str]:
    errors: list[str] = []
    test_entries = manifest.get("test", [])
    if not isinstance(test_entries, list):
        return ["Cargo.toml has no [[test]] target table"]

    by_name: dict[str, list[dict[str, object]]] = {}
    for entry in test_entries:
        if not isinstance(entry, dict):
            continue
        name = entry.get("name")
        if isinstance(name, str):
            by_name.setdefault(name, []).append(entry)

    for target in sorted(set(suites.values())):
        entries = by_name.get(target, [])
        if len(entries) != 1:
            errors.append(
                f"focused target {target!r} has {len(entries)} Cargo.toml entries, expected exactly 1"
            )
            continue
        entry = entries[0]
        expected_path = TARGET_PATHS.get(target, f"tests/{target}.rs")
        if entry.get("path") != expected_path:
            errors.append(
                f"focused target {target!r} must use path {expected_path!r}"
            )
        if entry.get("required-features") != ["test-gameplay"]:
            errors.append(
                f"focused target {target!r} must require only the test-gameplay feature"
            )
    return errors


def run_self_tests() -> None:
    selectors = (
        Selector("active", "active_case"),
        Selector("ignored", "ignored_case", ignored=True),
    )
    valid_all = parse_listing("active_case: test\nignored_case: test\n")
    valid_ignored = parse_listing("ignored_case: test\n")
    assert selector_errors(valid_all, valid_ignored, selectors) == []

    missing = selector_errors(parse_listing("ignored_case: test\n"), valid_ignored, selectors)
    assert any("resolves to 0 tests" in error for error in missing)

    ignored_drift = selector_errors(
        valid_all,
        parse_listing("active_case: test\nignored_case: test\n"),
        selectors,
    )
    assert any("not active" in error for error in ignored_drift)

    aliases = {
        "test-gameplay-suite": "test --quiet --locked --test focused --features test-gameplay",
        "test-gameplay": "test --quiet --locked --features test-gameplay --test focused",
        "check-gameplay-suite": "check --quiet --locked --test focused --features test-gameplay",
        "check-gameplay": "check --quiet --locked --features test-gameplay --test focused",
    }
    suites = {"test-gameplay-suite": "focused"}
    check_suites = {"check-gameplay-suite": "focused"}
    assert alias_errors(aliases, (), suites) == []
    assert maintained_gate_alias_errors(aliases, suites) == []
    assert check_alias_errors(aliases, check_suites) == []
    assert maintained_check_alias_errors(aliases, check_suites) == []
    aliases["test-gameplay-suite"] += " stale_filter"
    assert any(
        "complete 'focused' target" in error
        for error in alias_errors(aliases, (), suites)
    )
    aliases["test-gameplay"] += " stale_filter"
    assert any(
        "no test-name filter" in error
        for error in maintained_gate_alias_errors(aliases, suites)
    )
    aliases["check-gameplay-suite"] += " stale_filter"
    assert any(
        "complete 'focused' target" in error
        for error in check_alias_errors(aliases, check_suites)
    )
    aliases["check-gameplay"] += " stale_filter"
    assert any(
        "each focused gameplay target exactly once" in error
        for error in maintained_check_alias_errors(aliases, check_suites)
    )

    valid_manifest = {
        "test": [
            {
                "name": "focused",
                "path": "tests/focused.rs",
                "required-features": ["test-gameplay"],
            }
        ]
    }
    assert focused_target_errors(valid_manifest, suites) == []
    assert any(
        "expected exactly 1" in error
        for error in focused_target_errors({"test": []}, suites)
    )

    executable_fixture = json.dumps(
        {
            "reason": "compiler-artifact",
            "target": {"name": "gameplay_harness"},
            "profile": {"test": True},
            "executable": "target/test/gameplay_harness.exe",
        }
    )
    assert parse_test_executable(executable_fixture) == Path(
        "target/test/gameplay_harness.exe"
    )
    try:
        parse_test_executable("")
    except RuntimeError as error:
        assert "exactly one test executable" in str(error)
    else:
        raise AssertionError("missing gameplay executable must fail closed")


def parse_test_executable(cargo_output: str) -> Path:
    executables: list[Path] = []
    for line in cargo_output.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (
            message.get("reason") == "compiler-artifact"
            and message.get("target", {}).get("name") == "gameplay_harness"
            and message.get("profile", {}).get("test")
            and message.get("executable")
        ):
            executables.append(Path(message["executable"]))
    unique = list(dict.fromkeys(executables))
    if len(unique) != 1:
        raise RuntimeError(
            "gameplay harness build did not report exactly one test executable "
            f"(found {len(unique)})"
        )
    return unique[0]


def cargo_test_executable() -> Path:
    command = [
        "cargo",
        "test",
        "--locked",
        "--test",
        "gameplay_harness",
        "--features",
        "test-gameplay-full",
        "--no-run",
        "--message-format=json",
    ]
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        if result.stdout.strip():
            print(result.stdout.rstrip(), file=sys.stderr)
        if result.stderr.strip():
            print(result.stderr.rstrip(), file=sys.stderr)
        raise RuntimeError(f"gameplay test discovery build failed with code {result.returncode}")
    return parse_test_executable(result.stdout)


def binary_listing(executable: Path, *extra_libtest: str) -> Counter[str]:
    command = [
        str(executable),
        "--list",
        "--format",
        "terse",
        *extra_libtest,
    ]
    result = subprocess.run(
        command,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        if result.stdout.strip():
            print(result.stdout.rstrip(), file=sys.stderr)
        if result.stderr.strip():
            print(result.stderr.rstrip(), file=sys.stderr)
        raise RuntimeError(f"gameplay test inventory command failed with code {result.returncode}")
    return parse_listing(result.stdout)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate gameplay Cargo aliases and focused-target wiring."
    )
    parser.add_argument(
        "--static",
        action="store_true",
        help="validate command/manifest wiring only; skip live full-harness test discovery",
    )
    args = parser.parse_args()

    run_self_tests()
    with CONFIG.open("rb") as handle:
        config = tomllib.load(handle)
    with MANIFEST.open("rb") as handle:
        manifest = tomllib.load(handle)
    aliases = config.get("alias", {})
    if not isinstance(aliases, dict):
        print(".cargo/config.toml has no [alias] table", file=sys.stderr)
        return 1

    errors = alias_errors(aliases, SELECTORS, SUITES)
    errors.extend(maintained_gate_alias_errors(aliases, SUITES))
    errors.extend(check_alias_errors(aliases, CHECK_SUITES))
    errors.extend(maintained_check_alias_errors(aliases, CHECK_SUITES))
    errors.extend(focused_target_errors(manifest, SUITES))
    if args.static:
        if errors:
            for error in errors:
                print(f"gameplay alias contract: {error}", file=sys.stderr)
            return 1
        print(
            "gameplay alias contract: PASS static "
            f"({len(SELECTORS)} filtered aliases, {len(SUITES)} focused test/check suites)"
        )
        return 0

    try:
        executable = cargo_test_executable()
        all_tests = binary_listing(executable)
        ignored_tests = binary_listing(executable, "--ignored")
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 1
    errors.extend(selector_errors(all_tests, ignored_tests, SELECTORS))
    if errors:
        for error in errors:
            print(f"gameplay alias contract: {error}", file=sys.stderr)
        return 1

    print(
        "gameplay alias contract: PASS "
        f"({len(SELECTORS)} filtered aliases, {len(SUITES)} focused suites, "
        f"{sum(all_tests.values())} full harness tests)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
