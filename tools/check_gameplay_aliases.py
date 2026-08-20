#!/usr/bin/env python3
"""Static contract check for maintained gameplay Cargo aliases and targets."""

from __future__ import annotations

from collections import Counter
from pathlib import Path
import shlex
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / ".cargo" / "config.toml"
MANIFEST = ROOT / "Cargo.toml"
REPORT_SOURCE = ROOT / "tests" / "gameplay_harness" / "main.rs"

REPORT_ALIAS = "test-gameplay-report"
REPORT_TEST = "gameplay_harness_exploratory_report"

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


def focused_alias_errors(
    aliases: dict[str, str], suites: dict[str, str], command: str
) -> list[str]:
    errors: list[str] = []
    expected_aliases = set(suites)
    actual_aliases = {
        name
        for name in aliases
        if name.startswith(f"{command}-gameplay-") and name != REPORT_ALIAS
    }
    unexpected = sorted(actual_aliases - expected_aliases)
    missing = sorted(expected_aliases - actual_aliases)
    if unexpected:
        errors.append(f"uncontracted gameplay {command} aliases: {', '.join(unexpected)}")
    if missing:
        errors.append(f"missing gameplay {command} aliases: {', '.join(missing)}")

    for alias, target in suites.items():
        actual = aliases.get(alias)
        if actual is None:
            continue
        expected = [
            command,
            "--quiet",
            "--locked",
            "--test",
            target,
            "--features",
            "test-gameplay",
        ]
        if shlex.split(actual) != expected:
            errors.append(
                f"{alias}: must run the complete {target!r} target without a test-name filter"
            )
    return errors


def aggregate_alias_errors(
    aliases: dict[str, str], alias: str, command: str, suites: dict[str, str]
) -> list[str]:
    actual = aliases.get(alias)
    if actual is None:
        return [f"missing maintained gameplay alias: {alias}"]
    expected = Counter([command, "--quiet", "--locked", "--features", "test-gameplay"])
    for target in suites.values():
        expected.update(["--test", target])
    if Counter(shlex.split(actual)) != expected:
        return [
            f"{alias}: must include every focused gameplay target exactly once with only the "
            "test-gameplay feature"
        ]
    return []


def report_alias_errors(aliases: dict[str, str]) -> list[str]:
    actual = aliases.get(REPORT_ALIAS)
    if actual is None:
        return [f"missing gameplay report alias: {REPORT_ALIAS}"]
    expected = [
        "test",
        "--quiet",
        "--locked",
        "--test",
        "gameplay_harness",
        "--features",
        "test-gameplay-full",
        REPORT_TEST,
        "--",
        "--exact",
        "--ignored",
        "--nocapture",
    ]
    if shlex.split(actual) != expected:
        return [
            f"{REPORT_ALIAS}: must select the ignored {REPORT_TEST!r} test exactly in the "
            "gameplay_harness target"
        ]
    return []


def target_errors(manifest: dict[str, object]) -> list[str]:
    errors: list[str] = []
    test_entries = manifest.get("test", [])
    if not isinstance(test_entries, list):
        return ["Cargo.toml has no [[test]] target table"]

    by_name: dict[str, list[dict[str, object]]] = {}
    for entry in test_entries:
        if isinstance(entry, dict) and isinstance(entry.get("name"), str):
            by_name.setdefault(entry["name"], []).append(entry)

    expected_targets = {
        target: (TARGET_PATHS.get(target, f"tests/{target}.rs"), ["test-gameplay"])
        for target in SUITES.values()
    }
    expected_targets["gameplay_harness"] = (
        "tests/gameplay_harness/full_target.rs",
        ["test-gameplay-full"],
    )

    for target, (path, features) in expected_targets.items():
        entries = by_name.get(target, [])
        if len(entries) != 1:
            errors.append(
                f"gameplay target {target!r} has {len(entries)} Cargo.toml entries, expected exactly 1"
            )
            continue
        entry = entries[0]
        if entry.get("path") != path:
            errors.append(f"gameplay target {target!r} must use path {path!r}")
        if entry.get("required-features") != features:
            errors.append(
                f"gameplay target {target!r} must require exactly {features!r}"
            )
    return errors


def report_source_errors(source: str) -> list[str]:
    signature = f"fn {REPORT_TEST}()"
    if source.count(signature) != 1:
        return [
            f"{REPORT_ALIAS}: report source contains {source.count(signature)} {REPORT_TEST!r} tests, expected 1"
        ]
    index = source.index(signature)
    nearby = "\n".join(source[:index].splitlines()[-4:])
    if '#[ignore = "exploratory gameplay report"]' not in nearby:
        return [f"{REPORT_ALIAS}: {REPORT_TEST!r} must remain explicitly ignored"]
    return []


def run_self_tests() -> None:
    suites = {"test-gameplay-one": "one"}
    aliases = {
        "test-gameplay-one": "test --quiet --locked --test one --features test-gameplay",
        "test-gameplay": "test --quiet --locked --features test-gameplay --test one",
        REPORT_ALIAS: (
            "test --quiet --locked --test gameplay_harness --features test-gameplay-full "
            f"{REPORT_TEST} -- --exact --ignored --nocapture"
        ),
    }
    assert focused_alias_errors(aliases, suites, "test") == []
    assert aggregate_alias_errors(aliases, "test-gameplay", "test", suites) == []
    assert report_alias_errors(aliases) == []
    aliases["test-gameplay-one"] += " stale-filter"
    assert focused_alias_errors(aliases, suites, "test")


def main() -> int:
    run_self_tests()
    with CONFIG.open("rb") as handle:
        config = tomllib.load(handle)
    with MANIFEST.open("rb") as handle:
        manifest = tomllib.load(handle)
    aliases = config.get("alias", {})
    if not isinstance(aliases, dict):
        print(".cargo/config.toml has no [alias] table", file=sys.stderr)
        return 1

    errors = focused_alias_errors(aliases, SUITES, "test")
    errors.extend(focused_alias_errors(aliases, CHECK_SUITES, "check"))
    errors.extend(aggregate_alias_errors(aliases, "test-gameplay", "test", SUITES))
    errors.extend(aggregate_alias_errors(aliases, "check-gameplay", "check", CHECK_SUITES))
    errors.extend(report_alias_errors(aliases))
    errors.extend(target_errors(manifest))
    errors.extend(report_source_errors(REPORT_SOURCE.read_text(encoding="utf-8")))
    if errors:
        for error in errors:
            print(f"gameplay alias contract: {error}", file=sys.stderr)
        return 1

    print(
        "gameplay alias contract: PASS "
        f"(1 report selector, {len(SUITES)} focused test/check suites; static only)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
