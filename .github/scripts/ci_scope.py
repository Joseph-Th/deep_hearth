#!/usr/bin/env python3
"""Classify changed paths for Deep Hearth's pull-request CI lanes.

Unknown paths fail safe by enabling specialized validation. Only explicitly known unrelated domains
are skipped, so adding a new source area cannot silently remove coverage.
"""

from __future__ import annotations

import sys
from collections.abc import Iterable


BUILD_CONFIGURATION = frozenset({"Cargo.toml", "Cargo.lock", ".cargo/config.toml"})
DOCUMENTATION = frozenset(
    {
        "AGENTS.md",
        "GAME_DESIGN.md",
        "README.md",
        "STATUS.md",
        "TECHNICAL_DESIGN.md",
        "TESTING.md",
    }
)

SHADER_EXCLUDED_SOURCE_PATHS = frozenset({"src/content/gameplay_fixture.rs"})

PRODUCTION_LINT_EXCLUDED_SOURCE_PATHS = frozenset(
    {
        "src/content/gameplay_fixture.rs",
        "src/inventory/fixture.rs",
        "src/inventory/test_support.rs",
    }
)

CORE_EXCLUDED_SOURCE_PATHS = frozenset({"src/content/gameplay_fixture.rs"})

GAMEPLAY_UNRELATED_SOURCE_DOMAINS = frozenset(
    {
        "electrical",
        "fluid",
        "geology",
        "mechanical",
        "persistence",
        "shader",
        "texture",
    }
)

SHADER_UNRELATED_SOURCE_DOMAINS = frozenset(
    {
        "capability",
        "core",
        "electrical",
        "energy",
        "equipment",
        "fluid",
        "geology",
        "inventory",
        "maintenance",
        "material",
        "matter",
        "mechanical",
        "ore_processing",
        "persistence",
        "production",
        "registry",
        "simulation",
        "spatial",
        "structural",
        "thermal",
    }
)

SHADER_EXACT_SOURCE_PATHS = frozenset(
    {
        "src/bin/validate_shaders.rs",
        "src/content/mod.rs",
        "src/content/shaders.rs",
        "src/lib.rs",
    }
)


def _is_under(path: str, directory: str) -> bool:
    return path == directory or path.startswith(f"{directory}/")


def _source_domain(path: str) -> str | None:
    if not path.startswith("src/"):
        return None
    remainder = path.removeprefix("src/")
    return remainder.split("/", maxsplit=1)[0] if "/" in remainder else None


def _quality_path_is_relevant(path: str) -> bool:
    return (
        path in BUILD_CONFIGURATION
        or _is_under(path, "src")
        or _is_under(path, "tests")
    )


def _lint_path_is_relevant(path: str) -> bool:
    return path in BUILD_CONFIGURATION or (
        _is_under(path, "src") and path not in PRODUCTION_LINT_EXCLUDED_SOURCE_PATHS
    )


def _core_path_is_relevant(path: str) -> bool:
    return (
        path in BUILD_CONFIGURATION
        or (_is_under(path, "src") and path not in CORE_EXCLUDED_SOURCE_PATHS)
        or _is_under(path, "assets/shaders")
    )


def _gameplay_path_is_relevant(path: str) -> bool:
    if path in BUILD_CONFIGURATION or _is_under(path, "tests/gameplay_harness"):
        return True
    if path in DOCUMENTATION or _is_under(path, ".github") or _is_under(path, "assets/shaders"):
        return False
    if path == "src/bin/validate_shaders.rs":
        return False
    domain = _source_domain(path)
    if domain is not None:
        return domain not in GAMEPLAY_UNRELATED_SOURCE_DOMAINS
    return True


def _shader_path_is_relevant(path: str) -> bool:
    if path in BUILD_CONFIGURATION or path in SHADER_EXACT_SOURCE_PATHS:
        return True
    if path in SHADER_EXCLUDED_SOURCE_PATHS:
        return False
    if _is_under(path, "assets/shaders") or _is_under(path, "src/shader") or _is_under(
        path, "src/texture"
    ):
        return True
    if path in DOCUMENTATION or _is_under(path, ".github") or _is_under(path, "tests"):
        return False
    domain = _source_domain(path)
    if domain is not None:
        return domain not in SHADER_UNRELATED_SOURCE_DOMAINS
    return True


LANE_CLASSIFIERS = {
    "format": _quality_path_is_relevant,
    "lint": _lint_path_is_relevant,
    "core": _core_path_is_relevant,
    "gameplay": _gameplay_path_is_relevant,
    "shaders": _shader_path_is_relevant,
}


def should_run(lane: str, changed_paths: Iterable[str]) -> bool:
    try:
        classifier = LANE_CLASSIFIERS[lane]
    except KeyError as error:
        raise ValueError(f"unknown CI lane: {lane}") from error
    return any(classifier(path.strip()) for path in changed_paths if path.strip())


def build_plan(changed_paths: Iterable[str]) -> dict[str, bool]:
    paths = tuple(path.strip() for path in changed_paths if path.strip())
    return {lane: should_run(lane, paths) for lane in LANE_CLASSIFIERS}


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        lanes = ", ".join((*LANE_CLASSIFIERS, "plan"))
        print(f"usage: ci_scope.py <{lanes}>", file=sys.stderr)
        return 2
    command = argv[1]
    if command == "plan":
        for lane, enabled in build_plan(sys.stdin).items():
            print(f"{lane}={'true' if enabled else 'false'}")
        return 0
    if command not in LANE_CLASSIFIERS:
        lanes = ", ".join((*LANE_CLASSIFIERS, "plan"))
        print(f"usage: ci_scope.py <{lanes}>", file=sys.stderr)
        return 2
    print("true" if should_run(command, sys.stdin) else "false")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
