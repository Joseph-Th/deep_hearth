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
EXTERNAL_MODULE = re.compile(r"^mod\s+(?P<name>[A-Za-z_]\w*)\s*;$")
PATH_ATTRIBUTE = re.compile(r'^#\[path\s*=\s*"(?P<path>[^"]+)"\]$')
FEATURE_NAME = re.compile(r'feature\s*=\s*"(?P<name>[^"]+)"')


def feature_set(raw: str | None) -> set[str]:
    if not raw:
        return set()
    return {feature for feature in re.split(r"[,\s]+", raw.strip()) if feature}


def attributes_enabled(attributes: list[str], features: set[str]) -> bool:
    """Evaluate the simple feature cfgs used by this repository's test declarations."""

    for attribute in attributes:
        if not attribute.startswith("#[cfg("):
            continue
        required = FEATURE_NAME.findall(attribute)
        if not required:
            continue
        if "any(" in attribute:
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


def lib_module_prefix(path: Path) -> tuple[str, ...]:
    relative = path.relative_to(ROOT / "src")
    parts = list(relative.parts)
    if parts[-1] == "lib.rs":
        return ()
    if parts[-1] == "mod.rs":
        parts.pop()
    else:
        parts[-1] = Path(parts[-1]).stem
    return tuple(parts)


def cargo_test_target_path(target: str) -> Path:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    for definition in manifest.get("test", []):
        if definition.get("name") == target:
            return ROOT / definition["path"]
    raise ValueError(f"unknown Cargo test target: {target}")


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
                    direct = path.parent / f"{name}.rs"
                    nested = path.parent / name / "mod.rs"
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


def integration_test_names(target: str, features: set[str]) -> list[str]:
    root = cargo_test_target_path(target)
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


def source_test_catalog(target: str, raw_features: str | None) -> list[str]:
    """Return exact test names from source without invoking Cargo or rustc."""

    features = feature_set(raw_features)
    if target == "lib":
        names = [
            name
            for path in sorted((ROOT / "src").rglob("*.rs"))
            for name in file_test_names(path, lib_module_prefix(path), features)
        ]
    else:
        names = integration_test_names(target, features)
    return sorted(set(names))


def cargo_command(args: argparse.Namespace) -> list[str]:
    if args.list:
        raise ValueError("source catalog listing does not invoke Cargo")
    command = ["cargo", "test", "--quiet", "--locked"]
    if args.target == "lib":
        command.append("--lib")
    else:
        command.extend(("--test", args.target))
    if args.features:
        command.extend(("--features", args.features))
    command.extend((args.name, "--", "--exact"))
    if args.ignored:
        command.append("--ignored")
    if args.nocapture:
        command.append("--nocapture")
    return command


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run one exact cached Rust test, or inspect its build-free source catalog."
    )
    parser.add_argument("name", nargs="?", help="fully qualified exact test name")
    parser.add_argument(
        "--list",
        action="store_true",
        help="list exact source test names without compiling or linking",
    )
    parser.add_argument("--target", default="lib", help="Cargo test target name; defaults to lib")
    parser.add_argument("--features", help="Cargo feature set required by the selected target")
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

    if args.name not in catalog:
        candidates = [name for name in catalog if args.name in name or name.endswith(args.name)]
        if not candidates:
            candidates = difflib.get_close_matches(args.name, catalog, n=5, cutoff=0.45)
        print(f"FAIL exact test not found in source catalog: {args.name}", file=sys.stderr)
        print(f"catalog: python tools/run_test.py --list {args.name}", file=sys.stderr)
        for candidate in candidates[:8]:
            print(f"candidate: {candidate}", file=sys.stderr)
        return 2

    command = cargo_command(args)
    environment = os.environ.copy()
    environment.setdefault("CARGO_TERM_COLOR", "never")
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
