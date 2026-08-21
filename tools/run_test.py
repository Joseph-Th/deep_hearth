#!/usr/bin/env python3
"""Discover or run exact Rust tests without paying build cost for catalog mistakes."""

from __future__ import annotations

import argparse
import difflib
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import tomllib


ROOT = Path(__file__).resolve().parents[1]
ZERO_TESTS = re.compile(r"\brunning 0 tests\b")
ATTRIBUTE = re.compile(r"^\s*#\[(?P<body>.+)\]\s*$")
FUNCTION = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(?P<name>[A-Za-z_]\w*)\s*\(")
INLINE_MODULE = re.compile(r"^mod\s+(?P<name>[A-Za-z_]\w*)\s*\{$")
EXTERNAL_MODULE = re.compile(
    r"^(?:pub(?:\([^)]*\))?\s+)?mod\s+(?P<name>[A-Za-z_]\w*)\s*;$"
)
PATH_ATTRIBUTE = re.compile(r'^#\[path\s*=\s*"(?P<path>[^"]+)"\]$')
FEATURE_NAME = re.compile(r'feature\s*=\s*"(?P<name>[^"]+)"')
BARE_TEST_CFG = re.compile(r"(?:^|[,(])\s*test\s*(?:[,)]|$)")


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
    if target != "lib":
        requested.update(cargo_test_target_definition(target).get("required-features", []))
    return requested


def cargo_feature_set(target: str, raw: str | None) -> set[str]:
    """Resolve the local cfg(feature) set Cargo enables for the exact-test command."""

    manifest = cargo_manifest()
    declared = manifest.get("features", {})
    return expand_local_features(
        declared, requested_target_features(target, raw), include_default=True
    )


def attributes_enabled(attributes: list[str], features: set[str]) -> bool:
    """Evaluate the simple feature cfgs used by this repository's test declarations."""

    for attribute in attributes:
        if not attribute.startswith("#[cfg("):
            continue
        if attribute == "#[cfg(test)]":
            continue
        required = FEATURE_NAME.findall(attribute)
        if not required:
            continue
        if "any(" in attribute:
            if BARE_TEST_CFG.search(attribute):
                continue
            if not any(feature in features for feature in required):
                return False
        elif not all(feature in features for feature in required):
            return False
    return True


def file_test_names(path: Path, prefix: tuple[str, ...], features: set[str]) -> list[str]:
    """Read direct #[test] declarations from one rustfmt-formatted source module."""

    names: list[str] = []
    pending_attributes: list[str] = []
    inline_test_module: str | None = None

    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if ATTRIBUTE.match(line):
            pending_attributes.append(stripped)
            continue

        module_match = INLINE_MODULE.match(stripped)
        if (
            line == stripped
            and module_match is not None
            and "#[cfg(test)]" in pending_attributes
        ):
            inline_test_module = module_match.group("name")
            pending_attributes.clear()
            continue

        if inline_test_module is not None and line == "}":
            inline_test_module = None
            pending_attributes.clear()
            continue

        function_match = FUNCTION.match(line)
        if function_match is not None and "#[test]" in pending_attributes:
            if attributes_enabled(pending_attributes, features):
                components = [*prefix]
                if inline_test_module is not None:
                    components.append(inline_test_module)
                components.append(function_match.group("name"))
                names.append("::".join(components))
            pending_attributes.clear()
            continue

        if stripped:
            pending_attributes.clear()

    return names


def cargo_test_target_path(target: str) -> Path:
    return ROOT / cargo_test_target_definition(target)["path"]


def external_modules(path: Path, features: set[str]) -> list[tuple[str, Path]]:
    modules: list[tuple[str, Path]] = []
    pending_attributes: list[str] = []
    explicit_path: str | None = None

    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if line == stripped and ATTRIBUTE.match(line):
            pending_attributes.append(stripped)
            path_match = PATH_ATTRIBUTE.match(stripped)
            if path_match is not None:
                explicit_path = path_match.group("path")
            continue

        module_match = EXTERNAL_MODULE.match(stripped) if line == stripped else None
        if module_match is not None:
            if attributes_enabled(pending_attributes, features):
                name = module_match.group("name")
                if explicit_path is not None:
                    module_path = path.parent / explicit_path
                else:
                    module_root = (
                        path.parent
                        if path.name in {"lib.rs", "main.rs", "mod.rs"}
                        else path.parent / path.stem
                    )
                    direct = module_root / f"{name}.rs"
                    nested = module_root / name / "mod.rs"
                    module_path = direct if direct.is_file() else nested
                if not module_path.is_file():
                    raise ValueError(
                        f"source catalog cannot resolve module {name!r} from {path.relative_to(ROOT)}"
                    )
                modules.append((name, module_path))
            pending_attributes.clear()
            explicit_path = None
            continue

        if stripped:
            pending_attributes.clear()
            explicit_path = None

    return modules


def reachable_test_names(root: Path, features: set[str]) -> list[str]:
    """Walk one Rust crate/module graph and return only tests reachable from its root."""

    names: list[str] = []
    pending: list[tuple[Path, tuple[str, ...]]] = [(root, ())]
    visited: set[tuple[Path, tuple[str, ...]]] = set()

    while pending:
        path, prefix = pending.pop()
        key = (path.resolve(), prefix)
        if key in visited:
            continue
        visited.add(key)
        names.extend(file_test_names(path, prefix, features))
        for module, module_path in external_modules(path, features):
            pending.append((module_path, (*prefix, module)))
    return names


def integration_test_names(target: str, features: set[str]) -> list[str]:
    return reachable_test_names(cargo_test_target_path(target), features)


def source_test_catalog(target: str, raw_features: str | None) -> list[str]:
    """Return exact test names from source without invoking Cargo or rustc."""

    features = cargo_feature_set(target, raw_features)
    if target == "lib":
        names = reachable_test_names(ROOT / "src" / "lib.rs", features)
    else:
        names = integration_test_names(target, features)
    return sorted(set(names))


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
    command.extend((args.name, "--", "--exact"))
    if args.ignored:
        command.append("--ignored")
    if args.nocapture:
        command.append("--nocapture")
    return command


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Run one exact cached Rust test from an exact or uniquely matching source selector, "
            "or inspect the build-free source catalog."
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
    parser.add_argument("--target", default="lib", help="Cargo test target name; defaults to lib")
    parser.add_argument(
        "--features",
        help="extra Cargo features; target required-features are inferred from Cargo.toml",
    )
    parser.add_argument("--ignored", action="store_true", help="select an ignored exact test")
    parser.add_argument("--nocapture", action="store_true", help="show exact-test output")
    args = parser.parse_args()
    if not args.list and not args.name:
        parser.error("an exact test name is required unless --list is used")
    if args.list and (args.ignored or args.nocapture):
        parser.error("--ignored and --nocapture apply only to exact execution")
    return args


def main() -> int:
    args = parse_args()
    try:
        catalog = source_test_catalog(args.target, args.features)
    except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"FAIL source test catalog: {error}", file=sys.stderr)
        return 2

    if args.list:
        names = catalog
        if args.name:
            names = [name for name in names if args.name in name]
        for name in names:
            print(name)
        print(f"{len(names)} test(s)")
        return 0

    selector = args.name
    try:
        args.name = resolve_test_name(selector, catalog)
    except ValueError as error:
        candidates = source_test_matches(selector, catalog)
        if not candidates:
            candidates = difflib.get_close_matches(selector, catalog, n=5, cutoff=0.45)
        print(f"FAIL {error}", file=sys.stderr)
        print(f"catalog: python tools/run_test.py --list {selector}", file=sys.stderr)
        for candidate in candidates[:8]:
            print(f"candidate: {candidate}", file=sys.stderr)
        return 2

    command = cargo_command(args)
    environment = os.environ.copy()
    environment["CARGO_TERM_COLOR"] = "never"
    started = time.perf_counter()
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )
    elapsed = time.perf_counter() - started
    if result.returncode != 0:
        print(f"FAIL ({elapsed:.1f}s)", file=sys.stderr)
        print(f"reproduce: {' '.join(command)}", file=sys.stderr)
        if result.stdout.strip():
            print(result.stdout.rstrip(), file=sys.stderr)
        if result.stderr.strip():
            print(result.stderr.rstrip(), file=sys.stderr)
        return result.returncode

    if ZERO_TESTS.search(result.stdout):
        print(f"FAIL Cargo did not execute cataloged exact test: {args.name}", file=sys.stderr)
        print(f"catalog: python tools/run_test.py --list {args.name}", file=sys.stderr)
        return 2

    if args.nocapture and result.stdout.strip():
        print(result.stdout.rstrip())
    print(f"PASS {args.target}::{args.name} ({elapsed:.1f}s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
