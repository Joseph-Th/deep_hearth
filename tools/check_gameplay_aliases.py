#!/usr/bin/env python3
"""Fail-closed contract check for filtered gameplay-harness Cargo aliases."""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from pathlib import Path
import shlex
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / ".cargo" / "config.toml"


@dataclass(frozen=True)
class Selector:
    alias: str
    test: str
    ignored: bool = False


SELECTORS = (
    Selector("test-gameplay-scenarios", "gameplay_harness_gate"),
    Selector("test-gameplay-survival", "gameplay_survival_provisioning_probe"),
    Selector("test-gameplay-progression", "gameplay_primitive_progression_probe"),
    Selector("test-gameplay-ore", "gameplay_ore_preparation_probe"),
    Selector("test-gameplay-foundry", "gameplay_foundry_probe"),
    Selector("test-gameplay-report", "gameplay_harness_exploratory_report", ignored=True),
)


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


def alias_errors(aliases: dict[str, str], selectors: tuple[Selector, ...]) -> list[str]:
    errors: list[str] = []
    expected_aliases = {selector.alias for selector in selectors}
    actual_filtered = {
        name for name in aliases if name.startswith("test-gameplay-")
    }
    unexpected = sorted(actual_filtered - expected_aliases)
    missing = sorted(expected_aliases - actual_filtered)
    if unexpected:
        errors.append(f"uncontracted filtered gameplay aliases: {', '.join(unexpected)}")
    if missing:
        errors.append(f"missing filtered gameplay aliases: {', '.join(missing)}")

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
        if "--features" not in cargo_tokens or "test-gameplay" not in cargo_tokens:
            errors.append(f"{selector.alias}: command does not enable test-gameplay")
        if "--exact" not in libtest_tokens:
            errors.append(f"{selector.alias}: command is not an exact selector")
        has_ignored = "--ignored" in libtest_tokens
        if has_ignored != selector.ignored:
            expected = "with --ignored" if selector.ignored else "without --ignored"
            errors.append(f"{selector.alias}: command must run {expected}")
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


def cargo_listing(*extra_libtest: str) -> Counter[str]:
    command = [
        "cargo",
        "test",
        "--quiet",
        "--locked",
        "--test",
        "gameplay_harness",
        "--features",
        "test-gameplay",
        "--",
        "--list",
        "--format",
        "terse",
        *extra_libtest,
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
        raise RuntimeError(f"gameplay test inventory command failed with code {result.returncode}")
    return parse_listing(result.stdout)


def main() -> int:
    run_self_tests()
    with CONFIG.open("rb") as handle:
        config = tomllib.load(handle)
    aliases = config.get("alias", {})
    if not isinstance(aliases, dict):
        print(".cargo/config.toml has no [alias] table", file=sys.stderr)
        return 1

    errors = alias_errors(aliases, SELECTORS)
    try:
        all_tests = cargo_listing()
        ignored_tests = cargo_listing("--ignored")
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 1
    errors.extend(selector_errors(all_tests, ignored_tests, SELECTORS))
    if errors:
        for error in errors:
            print(f"gameplay alias contract: {error}", file=sys.stderr)
        return 1

    print(
        f"gameplay alias contract: PASS ({len(SELECTORS)} filtered aliases, {sum(all_tests.values())} harness tests)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
