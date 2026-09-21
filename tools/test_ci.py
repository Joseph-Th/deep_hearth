#!/usr/bin/env python3
"""Fast contract tests for the local CI plan; never invoke Cargo builds from this file."""

from __future__ import annotations

import argparse
import contextlib
import io
from pathlib import Path
import re
import sys
import tomllib
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import ci  # noqa: E402
from tools import (  # noqa: E402
    check_authority_docs,
    check_bca,
    gameplay_report_summary,
    run_test,
    rust_diagnostics,
)


_source_text_cache: dict[Path, str] = {}
_maintained_files_cache: dict[tuple[Path, ...], list[Path]] = {}


def read_maintained_text(path: Path) -> str:
    """Return cached source text; the working tree is static during one contract run."""

    if path not in _source_text_cache:
        _source_text_cache[path] = path.read_text(encoding="utf-8")
    return _source_text_cache[path]


def maintained_rust_files(*roots: Path) -> list[Path]:
    """Return maintained Rust files in stable order without repeated directory walks."""

    if roots not in _maintained_files_cache:
        files = [path for root in roots for path in root.rglob("*.rs")]
        _maintained_files_cache[roots] = sorted(files)
    return _maintained_files_cache[roots]


def _harness_root_declaration(harness: Path, module: str) -> str:
    """Render the exact root lines that wire one harness module into a focused target."""

    assert (harness / f"{module}.rs").is_file(), module
    return f'#[path = "gameplay_harness/{module}.rs"]\nmod {module};'


def gate_args(**overrides: object) -> argparse.Namespace:
    values: dict[str, object] = {
        "preset": "gate",
        "all": False,
        "core": False,
        "soak": False,
        "gameplay": None,
        "shaders": False,
        "rustdoc": False,
        "lint": False,
        "dry_run": False,
        "since": "HEAD",
        "path": [],
        "hotspots": False,
        "verbose": False,
    }
    values.update(overrides)
    return argparse.Namespace(**values)


def cargo_build_commands(plan: list[tuple[str, list[str]]]) -> list[list[str]]:
    return [
        command
        for _label, command in plan
        if command[:2] != ["cargo", "fmt"] and command[:1] == ["cargo"]
    ]


def cargo_test_targets(command: list[str]) -> list[str]:
    return [command[index + 1] for index, value in enumerate(command[:-1]) if value == "--test"]


def brace_delta(line: str) -> int:
    return line.count("{") - line.count("}")


def read_multiline_attribute(lines: list[str], index: int) -> tuple[str, int]:
    parts = [lines[index].strip()]
    balance = parts[0].count("[") - parts[0].count("]")
    while balance > 0:
        index += 1
        part = lines[index].strip()
        parts.append(part)
        balance += part.count("[") - part.count("]")
    return " ".join(parts), index + 1


def named_struct_fields(
    lines: list[str],
    struct_index: int,
) -> tuple[list[tuple[int, str, str]], int]:
    fields: list[tuple[int, str, str]] = []
    attributes: list[str] = []
    field_parts: list[str] = []
    body_depth = brace_delta(lines[struct_index])
    index = struct_index + 1
    while index < len(lines) and body_depth > 0:
        current = lines[index]
        stripped = current.strip()
        depth_before = body_depth
        body_depth += brace_delta(current)
        if depth_before != 1 or stripped == "}":
            index += 1
            continue
        if stripped.startswith("#[") and not field_parts:
            attribute, index = read_multiline_attribute(lines, index)
            attributes.append(attribute)
            continue
        if not stripped or stripped.startswith(("///", "//")):
            index += 1
            continue
        field_parts.append(stripped)
        if stripped.endswith(","):
            fields.append((index + 1, " ".join(field_parts), " ".join(attributes)))
            attributes.clear()
            field_parts.clear()
        index += 1
    return fields, index


def deserialized_named_structs(
    path: Path,
) -> list[tuple[int, str, str, list[tuple[int, str, str]]]]:
    lines = read_maintained_text(path).splitlines()
    structures: list[tuple[int, str, str, list[tuple[int, str, str]]]] = []
    pending_attributes: list[str] = []
    depth = 0
    index = 0

    while index < len(lines):
        stripped = lines[index].strip()
        if depth != 0:
            depth += brace_delta(lines[index])
            index += 1
            continue
        if stripped.startswith("#["):
            attribute, index = read_multiline_attribute(lines, index)
            pending_attributes.append(attribute)
            continue
        if not stripped or stripped.startswith("///") or stripped.startswith("//!"):
            index += 1
            continue

        match = re.match(
            r"(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z0-9_]+)\s*\{",
            stripped,
        )
        attributes = " ".join(pending_attributes)
        pending_attributes.clear()
        if match is None:
            depth += brace_delta(lines[index])
            index += 1
            continue

        fields, next_index = named_struct_fields(lines, index)
        if re.search(r"Deserialize", attributes):
            structures.append((index + 1, match.group(1), attributes, fields))
        index = next_index

    return structures


class LocalCiPlanTests(unittest.TestCase):
    def test_rust_diagnostics_normalizes_module_owner_focus(self) -> None:
        args = rust_diagnostics.parse_args(["modules", "--focus", "survival"])
        self.assertEqual(
            rust_diagnostics.modules_command(args),
            [
                "cargo",
                "modules",
                "structure",
                "--lib",
                "--no-fns",
                "--no-traits",
                "--no-types",
                "--max-depth",
                "4",
                "--focus-on",
                "crate::survival",
            ],
        )

    def test_rust_diagnostics_orphans_include_test_linkage_only_when_requested(self) -> None:
        args = rust_diagnostics.parse_args(["modules", "orphans", "--tests"])
        self.assertEqual(
            rust_diagnostics.modules_command(args),
            ["cargo", "modules", "orphans", "--lib", "--cfg-test"],
        )

    def test_rust_diagnostics_dependency_view_is_focused_and_bounded(self) -> None:
        args = rust_diagnostics.parse_args(
            ["modules", "dependencies", "--focus", "survival"]
        )
        self.assertEqual(
            rust_diagnostics.modules_command(args),
            [
                "cargo",
                "modules",
                "dependencies",
                "--lib",
                "--no-externs",
                "--no-fns",
                "--no-sysroot",
                "--no-traits",
                "--no-types",
                "--no-owns",
                "--max-depth",
                "1",
                "--focus-on",
                "crate::survival",
            ],
        )
        overridden = rust_diagnostics.parse_args(
            ["modules", "dependencies", "--focus", "survival", "--depth", "2"]
        )
        self.assertIn("2", rust_diagnostics.modules_command(overridden))

    def test_rust_diagnostics_mutants_list_before_targeted_execution(self) -> None:
        args = rust_diagnostics.parse_args(
            [
                "mutants",
                "src/survival/validation/direct_consumption.rs",
                "--re",
                "validate_pending_food_freshness",
            ]
        )
        self.assertEqual(
            rust_diagnostics.mutants_command(args),
            [
                "cargo",
                "mutants",
                "--file",
                "src/survival/validation/direct_consumption.rs",
                "-F",
                "validate_pending_food_freshness",
                "--list",
            ],
        )

    def test_rust_diagnostics_mutant_execution_is_targeted_and_bounded(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            rust_diagnostics.parse_args(
                ["mutants", "src/survival/validation/direct_consumption.rs", "--run"]
            )
        args = rust_diagnostics.parse_args(
            [
                "mutants",
                "src/survival/validation/direct_consumption.rs",
                "--re",
                "validate_pending_food_freshness",
                "--run",
            ]
        )
        output = Path("target/agent-output/rust-diagnostics/mutants/example")
        self.assertEqual(
            rust_diagnostics.mutants_command(args, output),
            [
                "cargo",
                "mutants",
                "--file",
                "src/survival/validation/direct_consumption.rs",
                "-F",
                "validate_pending_food_freshness",
                "-j",
                "2",
                "-o",
                str(output),
            ],
        )

    def test_rust_diagnostics_expand_is_locked_and_item_scoped(self) -> None:
        args = rust_diagnostics.parse_args(
            ["expand", "survival::state::direct_consumption", "--grep", "PendingEating"]
        )
        self.assertEqual(
            rust_diagnostics.expand_command(args),
            [
                "cargo",
                "expand",
                "--quiet",
                "--locked",
                "--color",
                "never",
                "--lib",
                "survival::state::direct_consumption",
            ],
        )
        filtered = rust_diagnostics.filtered_expansion(
            "zero\nimpl Serialize for PendingEating {\none\ntwo\n",
            "PendingEating",
            1,
        )
        self.assertIn("impl Serialize for PendingEating", filtered)
        self.assertNotIn("two", filtered)

    def test_quick_lane_is_build_free(self) -> None:
        self.assertEqual(cargo_build_commands(ci.quick_plan()), [])

    def test_quick_lane_runs_python_contracts_as_an_importable_module(self) -> None:
        self.assertIn(
            (
                "local CI contracts",
                [sys.executable, "-m", "unittest", "tools.test_ci", "-q"],
            ),
            ci.quick_plan(),
        )

    def test_quick_lane_includes_bca_complexity_ratchet(self) -> None:
        self.assertIn(
            (
                "complexity ratchet",
                [sys.executable, "tools/check_bca.py", "check"],
            ),
            ci.quick_plan(),
        )

    def test_focused_gameplay_roots_are_closed_over_harness_dependencies(self) -> None:
        harness = ROOT / "tests" / "gameplay_harness"
        for scope, target in ci.GAMEPLAY_TARGETS.items():
            missing = run_test.missing_root_modules(target, harness)
            if missing:
                snippet = "\n".join(
                    _harness_root_declaration(harness, module) for module in missing
                )
                self.fail(
                    f"focused gameplay target {scope!r} is missing root-level harness "
                    f"modules {missing}; add to tests/{target}.rs:\n{snippet}"
                )

    def test_focused_gameplay_targets_exclude_report_only_catalog_code(self) -> None:
        report_only = ROOT / "tests" / "gameplay_harness" / "catalog.rs"
        for scope, target in ci.GAMEPLAY_TARGETS.items():
            features = run_test.cargo_feature_set(target, None)
            root = run_test.cargo_test_target_path(target)
            reachable = {
                path
                for path, _prefix in run_test.test_catalog.reachable_modules(ROOT, root, features)
            }
            self.assertNotIn(
                report_only,
                reachable,
                f"focused gameplay target {scope!r} must not compile report-only catalog code",
            )

    def test_root_sibling_import_parser_handles_nested_and_grouped_modules(self) -> None:
        self.assertEqual(
            run_test.root_sibling_imports("use super::super::{seed, temporal};", 2),
            {"seed", "temporal"},
        )
        self.assertEqual(
            run_test.root_sibling_imports(
                "use super::focused_seeds::FocusedProbeCase;", 1
            ),
            {"focused_seeds"},
        )
        self.assertEqual(run_test.root_sibling_imports("use super::local_item;", 2), set())
        self.assertEqual(
            run_test.root_sibling_imports(
                '#[cfg(not(test))]\nuse super::catalog::process_catalog_entries;', 1
            ),
            set(),
        )
        self.assertEqual(
            run_test.root_sibling_imports(
                '#[cfg(feature = "test-gameplay")]\nuse super::catalog::process_catalog_entries;',
                1,
                {"test-gameplay"},
            ),
            {"catalog"},
        )

    def test_bca_preset_reuses_the_pinned_changed_source_review(self) -> None:
        plan = ci.bca_review_plan("HEAD~1", ["src/inventory", "src/production"])
        self.assertEqual(
            plan,
            [
                (
                    "BCA changed-source review",
                    [
                        sys.executable,
                        "tools/check_bca.py",
                        "review",
                        "--changed",
                        "--since",
                        "HEAD~1",
                        "--path",
                        "src/inventory",
                        "--path",
                        "src/production",
                    ],
                )
            ],
        )
        self.assertEqual(cargo_build_commands(plan), [])
        self.assertEqual(
            ci.plan_for(
                gate_args(
                    preset="bca",
                    since="HEAD~1",
                    path=["src/inventory", "src/production"],
                )
            ),
            plan,
        )

    def test_bca_changed_review_includes_gameplay_harness_source(self) -> None:
        self.assertEqual(
            check_bca.select_changed_review_paths(
                [
                    "README.md",
                    "tests/gameplay_survival.rs",
                    "tests/gameplay_harness/survival_probe.rs",
                    "tests/notes.txt",
                ],
                ["tests"],
            ),
            [
                "tests/gameplay_harness/survival_probe.rs",
                "tests/gameplay_survival.rs",
            ],
        )

    def test_bca_hotspot_preset_reuses_the_same_history_aware_review_without_change_filtering(self) -> None:
        plan = ci.bca_review_plan(
            "HEAD~2",
            ["src/inventory"],
            changed_only=False,
        )
        self.assertEqual(
            plan,
            [
                (
                    "BCA hotspot review",
                    [
                        sys.executable,
                        "tools/check_bca.py",
                        "review",
                        "--since",
                        "HEAD~2",
                        "--path",
                        "src/inventory",
                    ],
                )
            ],
        )
        self.assertEqual(cargo_build_commands(plan), [])
        self.assertEqual(
            ci.plan_for(
                gate_args(
                    preset="bca",
                    since="HEAD~2",
                    path=["src/inventory"],
                    hotspots=True,
                )
            ),
            plan,
        )

    def test_bca_review_widens_new_paths_to_the_nearest_base_scope(self) -> None:
        base_paths = {
            "src",
            "src/production",
            "src/production/state",
            "src/production/state/validation.rs",
        }
        self.assertEqual(
            check_bca.resolve_review_diff_paths(
                [
                    "src/production/state/validation.rs",
                    "src/production/state/validation/job.rs",
                    "src/production/state/validation/indexes.rs",
                ],
                base_paths.__contains__,
            ),
            ["src/production/state"],
        )
        self.assertEqual(
            check_bca.resolve_review_diff_paths(
                ["src/production/state/validation.rs"],
                base_paths.__contains__,
            ),
            ["src/production/state/validation.rs"],
        )

    def test_bca_changed_review_selects_maintained_source_inside_requested_scope(self) -> None:
        self.assertEqual(
            check_bca.select_changed_review_paths(
                [
                    "README.md",
                    "src/labor/power_execution.rs",
                    "src/labor/power_execution/start.rs",
                    "src/production/state.rs",
                    "tests/gameplay_harness/workshop.rs",
                ],
                ["src/labor/power_execution"],
            ),
            [
                "src/labor/power_execution.rs",
                "src/labor/power_execution/start.rs",
            ],
        )

    def test_bca_changed_review_builds_exact_report_and_base_compatible_diff_scope(self) -> None:
        args = check_bca.parse_args(
            ["review", "--changed", "--since", "HEAD", "--path", "src/production/state"]
        )
        # The review helper narrates scope widening to stdout; keep the
        # contract output concise by discarding that narration here.
        with contextlib.redirect_stdout(io.StringIO()):
            commands = check_bca.execution_commands_for(
                args,
                changed_paths=[
                    "src/production/state.rs",
                    "src/production/state/indexes.rs",
                    "src/labor/power_execution.rs",
                ],
                exists_at_revision={
                    "src/production/state.rs",
                    "src/production/state",
                }.__contains__,
            )
        self.assertEqual(
            commands,
            [
                [
                    "bca",
                    "report",
                    "--vcs",
                    "--top",
                    "30",
                    "--paths",
                    "src/production/state.rs",
                    "--paths",
                    "src/production/state/indexes.rs",
                ],
                [
                    "bca",
                    "diff",
                    "--since",
                    "HEAD",
                    "--format",
                    "markdown",
                    "--metric",
                    "cognitive",
                    "--metric",
                    "cyclomatic",
                    "--metric",
                    "sloc",
                    "--paths",
                    "src/production/state.rs",
                    "--paths",
                    "src/production/state",
                ],
            ],
        )

    def test_bca_changed_review_is_a_clean_noop_without_changed_source(self) -> None:
        args = check_bca.parse_args(["review", "--changed", "--path", "src/production"])
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(
                check_bca.execution_commands_for(args, changed_paths=["README.md"]),
                [],
            )
        self.assertIn("no changed maintained Rust source", output.getvalue())

    def test_bca_workflow_keeps_gate_and_advisory_modes_distinct(self) -> None:
        self.assertEqual(
            check_bca.commands_for(check_bca.parse_args(["check"])),
            [["bca", "check", "--no-suppress", "--no-remediation"]],
        )
        self.assertEqual(
            check_bca.commands_for(
                check_bca.parse_args(
                    [
                        "report",
                        "--top",
                        "12",
                        "--path",
                        "src/production",
                        "--path",
                        "src/inventory",
                    ]
                )
            ),
            [
                [
                    "bca",
                    "report",
                    "--vcs",
                    "--top",
                    "12",
                    "--paths",
                    "src/production",
                    "--paths",
                    "src/inventory",
                ]
            ],
        )
        self.assertEqual(
            check_bca.commands_for(
                check_bca.parse_args(
                    [
                        "diff",
                        "--since",
                        "HEAD~1",
                        "--metric",
                        "cognitive",
                        "--metric",
                        "cyclomatic",
                        "--path",
                        "src/mining/execution.rs",
                    ]
                )
            ),
            [
                [
                    "bca",
                    "diff",
                    "--since",
                    "HEAD~1",
                    "--format",
                    "markdown",
                    "--metric",
                    "cognitive",
                    "--metric",
                    "cyclomatic",
                    "--paths",
                    "src/mining/execution.rs",
                ]
            ],
        )
        self.assertEqual(
            check_bca.commands_for(
                check_bca.parse_args(
                    [
                        "review",
                        "--since",
                        "HEAD~2",
                        "--top",
                        "15",
                        "--path",
                        "src/structural/analysis.rs",
                    ]
                )
            ),
            [
                [
                    "bca",
                    "report",
                    "--vcs",
                    "--top",
                    "15",
                    "--paths",
                    "src/structural/analysis.rs",
                ],
                [
                    "bca",
                    "diff",
                    "--since",
                    "HEAD~2",
                    "--format",
                    "markdown",
                    "--metric",
                    "cognitive",
                    "--metric",
                    "cyclomatic",
                    "--metric",
                    "sloc",
                    "--paths",
                    "src/structural/analysis.rs",
                ],
            ],
        )

    def test_rust_test_summary_is_concise_and_aggregates_multiple_results(self) -> None:
        output = (
            "test result: ok. 18 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n"
            "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        )
        self.assertEqual(ci.rust_test_summary(output), "20 tests, 1 ignored")
        self.assertIsNone(ci.rust_test_summary("Finished test profile"))

    def test_unit_test_bodies_stay_out_of_production_source_files(self) -> None:
        inline_module = re.compile(r"#\[cfg\(test\)\]\s*mod\s+[A-Za-z0-9_]+\s*\{")
        maintained_support = maintained_rust_files(
            ROOT / "src", ROOT / "tests" / "gameplay_harness"
        )
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in maintained_support
            if not path.name.endswith("_tests.rs")
            and path.name not in {"tests.rs", "mod_tests.rs"}
            if inline_module.search(read_maintained_text(path))
        ]
        self.assertEqual(offenders, [])

    def test_gameplay_harness_does_not_enumerate_unexpected_variants_only_to_panic(self) -> None:
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in maintained_rust_files(ROOT / "tests" / "gameplay_harness")
            if "other @ (" in read_maintained_text(path)
        ]
        self.assertEqual(offenders, [])

    def test_gameplay_harness_does_not_bind_assertions_to_panic_prose(self) -> None:
        forbidden = re.compile(r"#\[should_panic\s*\([^]]*\bexpected\s*=", re.DOTALL)
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in maintained_rust_files(ROOT / "tests" / "gameplay_harness")
            if forbidden.search(read_maintained_text(path))
        ]
        self.assertEqual(offenders, [])

    def test_unit_tests_do_not_bind_assertions_to_panic_prose(self) -> None:
        forbidden = re.compile(r"#\[should_panic\s*\([^]]*\bexpected\s*=", re.DOTALL)
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in maintained_rust_files(ROOT / "src")
            if forbidden.search(read_maintained_text(path))
        ]
        self.assertEqual(offenders, [])

    def test_gate_compiled_sources_avoid_wall_clock_nondeterminism(self) -> None:
        forbidden = re.compile(r"\bSystemTime\b|Instant::now|thread::sleep")
        # Fresh organic sampling is report-only and prints its root before
        # execution, so it stays replayable; routine gates never compile it.
        allowed = {
            (ROOT / "tests" / "gameplay_harness" / "fresh_seed.rs").resolve(),
        }
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in maintained_rust_files(
                ROOT / "src", ROOT / "tests" / "gameplay_harness"
            )
            if path.resolve() not in allowed
            if forbidden.search(read_maintained_text(path))
        ]
        self.assertEqual(offenders, [])

    def test_gameplay_harness_never_discards_tick_outcomes(self) -> None:
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in maintained_rust_files(ROOT / "tests" / "gameplay_harness")
            if "let _ = advance_tick" in read_maintained_text(path)
        ]
        self.assertEqual(offenders, [])

    def test_gameplay_feature_public_surface_is_explicitly_bounded(self) -> None:
        exposed: set[tuple[str, str]] = set()
        for path in maintained_rust_files(ROOT / "src"):
            lines = read_maintained_text(path).splitlines()
            attributes: list[str] = []
            index = 0
            while index < len(lines):
                stripped = lines[index].strip()
                if stripped.startswith("#["):
                    attribute = [stripped]
                    while sum(part.count("[") - part.count("]") for part in attribute) > 0:
                        index += 1
                        attribute.append(lines[index].strip())
                    attributes.append(" ".join(attribute))
                    index += 1
                    continue
                if not stripped or stripped.startswith("///") or stripped.startswith("//!"):
                    index += 1
                    continue
                gameplay_gated = any(
                    attribute.startswith("#[cfg(") and "test-gameplay" in attribute
                    for attribute in attributes
                )
                if gameplay_gated and stripped.startswith("pub "):
                    exposed.add((path.relative_to(ROOT).as_posix(), stripped))
                attributes.clear()
                index += 1

        self.assertEqual(
            exposed,
            {
                ("src/content/mod.rs", "pub mod gameplay_fixture;"),
            },
        )
        content = read_maintained_text(ROOT / "src" / "content" / "mod.rs")
        self.assertRegex(
            content,
            r'#\[cfg\(feature = "test-gameplay"\)\]\s*#\[doc\(hidden\)\]\s*pub mod gameplay_fixture;',
        )

    def test_test_only_source_items_do_not_use_external_public_visibility(self) -> None:
        offenders: list[str] = []
        for path in maintained_rust_files(ROOT / "src"):
            lines = read_maintained_text(path).splitlines()
            attributes: list[str] = []
            index = 0
            while index < len(lines):
                stripped = lines[index].strip()
                if stripped.startswith("#["):
                    attribute = [stripped]
                    while sum(part.count("[") - part.count("]") for part in attribute) > 0:
                        index += 1
                        attribute.append(lines[index].strip())
                    attributes.append(" ".join(attribute))
                    index += 1
                    continue
                if not stripped or stripped.startswith("///") or stripped.startswith("//!"):
                    index += 1
                    continue
                if "cfg(test)" in " ".join(attributes) and stripped.startswith("pub "):
                    relative = path.relative_to(ROOT).as_posix()
                    offenders.append(f"{relative}:{index + 1}:{stripped}")
                attributes.clear()
                index += 1
        self.assertEqual(offenders, [])

    def test_validated_token_types_are_must_use(self) -> None:
        validated_type = re.compile(
            r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+(Validated[A-Za-z0-9_]*)"
        )
        offenders: list[str] = []
        for path in maintained_rust_files(ROOT / "src"):
            lines = read_maintained_text(path).splitlines()
            for index, line in enumerate(lines):
                match = validated_type.match(line)
                if match is None:
                    continue
                attributes = lines[max(0, index - 5) : index]
                if not any(attribute.strip().startswith("#[must_use") for attribute in attributes):
                    relative = path.relative_to(ROOT).as_posix()
                    offenders.append(f"{relative}:{index + 1}:{match.group(1)}")
        self.assertEqual(offenders, [])

    def test_outcome_types_are_must_use(self) -> None:
        outcome_type = re.compile(
            r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+([A-Za-z0-9_]*Outcome[A-Za-z0-9_]*)"
        )
        offenders: list[str] = []
        for path in maintained_rust_files(ROOT / "src"):
            lines = read_maintained_text(path).splitlines()
            for index, line in enumerate(lines):
                match = outcome_type.match(line)
                if match is None:
                    continue
                attributes = lines[max(0, index - 5) : index]
                if not any(attribute.strip().startswith("#[must_use") for attribute in attributes):
                    relative = path.relative_to(ROOT).as_posix()
                    offenders.append(f"{relative}:{index + 1}:{match.group(1)}")
        self.assertEqual(offenders, [])

    def test_deserialized_structs_deny_unknown_fields(self) -> None:
        offenders = [
            f"{path.relative_to(ROOT).as_posix()}:{line}:{name}"
            for path in maintained_rust_files(ROOT / "src")
            for line, name, attributes, _fields in deserialized_named_structs(path)
            if "serde(deny_unknown_fields)" not in attributes
        ]
        self.assertEqual(offenders, [])

    def test_deserialized_ordered_collections_are_duplicate_strict(self) -> None:
        strict_markers = (
            "deserialize_btree_map_no_duplicates",
            "deserialize_btree_map_of_sets_no_duplicates",
        )
        offenders = [
            f"{path.relative_to(ROOT).as_posix()}:{line}:{field}"
            for path in maintained_rust_files(ROOT / "src")
            for _struct_line, _name, _attributes, fields in deserialized_named_structs(path)
            for line, field, attributes in fields
            if ":" in field
            and ("BTreeMap<" in field or "BTreeSet<" in field)
            and "serde(skip" not in attributes
            and not any(marker in attributes for marker in strict_markers)
        ]
        self.assertEqual(offenders, [])

    def test_persistent_serde_does_not_silently_accept_compatibility_shortcuts(self) -> None:
        forbidden = re.compile(
            r"#\[serde\([^]]*\b(default|flatten|alias|skip_deserializing|other)\b"
        )
        offenders = [
            f"{path.relative_to(ROOT).as_posix()}:{index + 1}:{line.strip()}"
            for path in maintained_rust_files(ROOT / "src")
            for index, line in enumerate(read_maintained_text(path).splitlines())
            if forbidden.search(line)
        ]
        self.assertEqual(offenders, [])

    def test_app_state_deserialization_is_owned_by_trusted_load(self) -> None:
        state_source = read_maintained_text(ROOT / "src" / "core" / "state.rs")
        app_state = re.search(
            r"((?:#\[[^\n]+\]\s*)*)pub struct AppState\s*\{",
            state_source,
        )
        self.assertIsNotNone(app_state)
        assert app_state is not None
        self.assertNotIn("Deserialize", app_state.group(1))

        persistence_source = read_maintained_text(ROOT / "src" / "persistence" / "mod.rs")
        self.assertRegex(
            persistence_source,
            r'#\[serde\(deserialize_with = "crate::core::state::deserialize_unvalidated_app_state"\)\]\s*state: AppState,',
        )

    def test_app_state_public_surface_does_not_expose_world_seed(self) -> None:
        state_source = read_maintained_text(ROOT / "src" / "core" / "state.rs")
        self.assertIsNone(
            re.search(r"\bpub\s+(?:const\s+)?fn\s+world_seed\s*\(", state_source)
        )

    def test_app_state_hidden_snapshot_traits_are_evaluation_only(self) -> None:
        state_source = read_maintained_text(ROOT / "src" / "core" / "state.rs")
        self.assertIn(
            '#[cfg_attr(any(test, feature = "test-gameplay"), derive(Clone, PartialEq, Eq))]',
            state_source,
        )
        self.assertNotRegex(
            state_source,
            r"#\[derive\([^)]*\b(?:Clone|PartialEq|Eq)\b[^)]*\)\]\s*pub struct AppState",
        )

    def test_mining_hidden_snapshot_traits_are_evaluation_only(self) -> None:
        state_source = read_maintained_text(ROOT / "src" / "mining" / "state.rs")
        job_source = read_maintained_text(ROOT / "src" / "mining" / "state" / "job.rs")
        expected = '#[cfg_attr(any(test, feature = "test-gameplay"), derive(PartialEq, Eq))]'
        self.assertIn(expected, state_source)
        self.assertIn(expected, job_source)
        self.assertNotRegex(
            state_source,
            r"#\[derive\([^)]*\b(?:PartialEq|Eq)\b[^)]*\)\]\s*#\[serde\(deny_unknown_fields\)\]\s*pub struct MiningState",
        )
        self.assertNotRegex(
            job_source,
            r"#\[derive\([^)]*\b(?:PartialEq|Eq)\b[^)]*\)\]\s*#\[serde\(deny_unknown_fields\)\]\s*pub struct MiningJobRecord",
        )

    def test_gameplay_harness_cannot_read_authoritative_geology(self) -> None:
        forbidden = re.compile(r"\.geology\(\)|\bGeologicalDepositId\b|\bget_deposit\(")
        offenders = [
            f"{path.relative_to(ROOT).as_posix()}:{index + 1}:{line.strip()}"
            for path in maintained_rust_files(ROOT / "tests" / "gameplay_harness")
            for index, line in enumerate(read_maintained_text(path).splitlines())
            if forbidden.search(line)
        ]
        self.assertEqual(offenders, [])

    def test_standard_gate_compiles_production_once(self) -> None:
        self.assertEqual(
            ci.plan_for(gate_args()),
            [("compile", ["cargo", "check-fast"])],
        )

    def test_gate_does_not_repeat_build_free_quick_checks(self) -> None:
        for args in (
            gate_args(),
            gate_args(gameplay="survival"),
            gate_args(soak=True),
            gate_args(lint=True),
        ):
            plan = ci.plan_for(args)
            self.assertEqual(len(plan), 1)
            self.assertFalse(any(stage in ci.quick_plan() for stage in plan))

    def test_lint_gate_uses_representative_surfaces_without_rebuilding_wrappers(self) -> None:
        plan = ci.plan_for(gate_args(lint=True))
        self.assertEqual(plan, [("clippy", ci.lint_command())])
        command = plan[0][1]
        self.assertEqual(command[:2], ["cargo", "clippy"])
        self.assertIn("--locked", command)
        self.assertIn("--lib", command)
        self.assertEqual(cargo_test_targets(command), [ci.GAMEPLAY_AUDIT_TARGET])
        self.assertIn("--example", command)
        self.assertIn(ci.GAMEPLAY_REPORT_EXAMPLE, command)
        self.assertIn("test-gameplay", command)
        self.assertNotIn("--all-targets", command)
        self.assertNotIn("--all-features", command)
        self.assertNotIn(ci.GAMEPLAY_CONTRACTS_TARGET, command)
        for target in ci.GAMEPLAY_TARGETS.values():
            self.assertNotIn(target, command)
        self.assertNotIn("-j", command)
        self.assertNotIn("--jobs", command)
        self.assertEqual(command[-2:], ["-D", "warnings"])

    def test_lint_representatives_cover_focused_harness_module_closures(self) -> None:
        audit_root = run_test.cargo_test_target_path(ci.GAMEPLAY_AUDIT_TARGET)
        audit_features = run_test.cargo_feature_set(ci.GAMEPLAY_AUDIT_TARGET, None)
        representative_modules = {
            path.resolve()
            for path, _prefix in run_test.test_catalog.reachable_modules(
                ROOT, audit_root, audit_features
            )
        }
        representative_modules.update(
            path.resolve()
            for path, _prefix in run_test.test_catalog.reachable_modules(
                ROOT, ROOT / "tests" / "gameplay_report.rs", {"test-gameplay"}
            )
        )

        for target in (ci.GAMEPLAY_CONTRACTS_TARGET, *ci.GAMEPLAY_TARGETS.values()):
            root = run_test.cargo_test_target_path(target)
            features = run_test.cargo_feature_set(target, None)
            shared_modules = {
                path.resolve()
                for path, _prefix in run_test.test_catalog.reachable_modules(
                    ROOT, root, features
                )
                if path.resolve() != root.resolve()
            }
            self.assertTrue(
                shared_modules <= representative_modules,
                f"{target} contains harness modules outside the representative lint surfaces",
            )

    def test_soak_gate_does_not_repeat_ordinary_core_tests(self) -> None:
        builds = cargo_build_commands(ci.plan_for(gate_args(soak=True)))
        self.assertEqual(builds, [["cargo", "test-soak"]])

    def test_focused_gameplay_does_not_precompile_production(self) -> None:
        plan = ci.plan_for(gate_args(gameplay="survival"))
        self.assertEqual(plan, ci.gameplay_plan("survival"))
        builds = cargo_build_commands(plan)
        self.assertEqual(len(builds), 1)
        self.assertNotIn("check-fast", builds[0])
        self.assertEqual(builds[0].count("--test"), 1)
        self.assertIn(ci.GAMEPLAY_TARGETS["survival"], builds[0])
        self.assertNotIn(ci.GAMEPLAY_CONTRACTS_TARGET, builds[0])
        for scope, target in ci.GAMEPLAY_TARGETS.items():
            if scope != "survival":
                self.assertNotIn(target, builds[0])
        self.assertIn(ci.GAMEPLAY_TESTS["survival"], builds[0])
        self.assertIn("--exact", builds[0])

    def test_gameplay_contract_gate_uses_only_the_lightweight_contract_target(self) -> None:
        command = ci.gameplay_command("contracts")
        self.assertEqual(cargo_test_targets(command), [ci.GAMEPLAY_CONTRACTS_TARGET])
        self.assertIn("test-gameplay", command)
        self.assertNotIn("--exact", command)
        self.assertNotIn("--nocapture", command)
        self.assertEqual(
            ci.plan_for(gate_args(gameplay="contracts")),
            [("gameplay contracts", command)],
        )

    def test_focused_gameplay_scopes_use_separate_targets_with_one_library_feature_shape(self) -> None:
        self.assertEqual(set(ci.GAMEPLAY_TARGETS), set(ci.GAMEPLAY_TESTS))
        self.assertEqual(len(set(ci.GAMEPLAY_TARGETS.values())), len(ci.GAMEPLAY_TARGETS))
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        definitions = {definition["name"]: definition for definition in manifest.get("test", [])}
        for scope, target in ci.GAMEPLAY_TARGETS.items():
            command = ci.gameplay_command(scope)
            self.assertEqual(cargo_test_targets(command), [target])
            self.assertIn("test-gameplay", command)
            self.assertIn(ci.GAMEPLAY_TESTS[scope], command)
            self.assertIn("--nocapture", command)
            self.assertEqual(definitions[target].get("required-features"), ["test-gameplay"])
        self.assertEqual(
            definitions[ci.GAMEPLAY_CONTRACTS_TARGET].get("required-features"),
            ["test-gameplay"],
        )
        self.assertEqual(
            definitions[ci.GAMEPLAY_AUDIT_TARGET].get("required-features"),
            ["test-gameplay"],
        )
        self.assertNotIn("--nocapture", ci.gameplay_command("all"))

    def test_focused_gameplay_roots_do_not_import_unrelated_probe_families(self) -> None:
        probe_modules = {
            "workshop": "workshop",
            "survival": "survival_probe",
            "progression": "progression_probe",
            "ore": "ore_probe",
            "foundry": "foundry_probe",
        }
        contracts_only = {"process_catalog_contract_tests", "seed_contract_tests"}
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        target_paths = {
            definition["name"]: ROOT / definition["path"]
            for definition in manifest.get("test", [])
        }
        for scope, target in ci.GAMEPLAY_TARGETS.items():
            source = target_paths[target].read_text(encoding="utf-8")
            forbidden = contracts_only | {
                module for owner, module in probe_modules.items() if owner != scope
            }
            if scope != "workshop":
                forbidden.add("agency")
            for module in forbidden:
                self.assertNotIn(
                    f"mod {module};",
                    source,
                    f"focused gameplay target {scope} must not pull unrelated harness family {module}",
                )

    def test_routine_gameplay_targets_do_not_compile_fresh_seed_generation(self) -> None:
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        target_paths = {
            definition["name"]: ROOT / definition["path"]
            for definition in manifest.get("test", [])
        }
        for target in (*ci.GAMEPLAY_TARGETS.values(), ci.GAMEPLAY_AUDIT_TARGET):
            source = target_paths[target].read_text(encoding="utf-8")
            self.assertNotIn(
                "gameplay_harness/fresh_seed.rs",
                source,
                f"routine gameplay target {target} must leave fresh organic sampling to the report",
            )
        report = ROOT / "tests" / "gameplay_report.rs"
        self.assertIn("gameplay_harness/fresh_seed.rs", report.read_text(encoding="utf-8"))

    def test_each_focused_gameplay_target_compiles_only_its_gate(self) -> None:
        for scope, target in ci.GAMEPLAY_TARGETS.items():
            self.assertEqual(
                run_test.source_test_catalog(target, None),
                [ci.GAMEPLAY_TESTS[scope]],
                f"focused gameplay target {scope} must not code-generate unrelated tests",
            )

    def test_gameplay_replay_summary_is_compact_for_focused_and_workshop_runs(self) -> None:
        self.assertEqual(
            ci.gameplay_replay_summary(
                "PROBE INPUT name=survival-provisioning mode=gate samples=2 organic=1 "
                "world_root=0x111 behavior_root=0x222 "
                "replay=anchor:0xA@0x1,organic:0xC@0x3\n"
            ),
            "roots=0x111/0x222",
        )
        self.assertEqual(
            ci.gameplay_replay_summary(
                "PROBE INPUT name=survival-provisioning mode=gate samples=3 organic=0 "
                "world_root=0xE7A10A7E5EED2026 behavior_root=0xE7A10A7E5EED2026 "
                "replay=anchor:0xA@0x1,coverage:0xB@0x2,coverage:0xC@0x3\n"
            ),
            "maintained=3",
        )
        self.assertEqual(
            ci.gameplay_replay_summary(
                "PROBE INPUT name=survival-provisioning mode=gate samples=2 organic=0 "
                "world_root=explicit behavior_root=0x222 "
                "replay=replay:0xA@0x1,replay:0xC@0x3\n"
            ),
            "roots=explicit/0x222; replay=replay:0xA@0x1,replay:0xC@0x3",
        )
        self.assertEqual(
            ci.gameplay_replay_summary(
                "HARNESS INPUT plan=anchor+variation anchors=3 variation=1 custom=0 "
                "world_root=0x1234 behavior_root=0x5678 replay=ignored\n"
            ),
            "roots=0x1234/0x5678",
        )
        self.assertEqual(
            ci.gameplay_replay_summary(
                "HARNESS INPUT plan=maintained anchors=7 variation=0 custom=0 "
                "world_root=n/a behavior_root=0x1 replay=ignored\n"
            ),
            "maintained=7",
        )
        self.assertEqual(
            ci.gameplay_replay_summary(
                "HARNESS INPUT plan=custom anchors=0 variation=0 custom=2 "
                "world_root=n/a behavior_root=0x1 replay=ignored\n"
            ),
            "custom=2",
        )
        self.assertIsNone(ci.gameplay_replay_summary("test result: ok. 1 passed"))

    def test_gate_rejects_complete_core_suite_as_a_repair_loop(self) -> None:
        with self.assertRaisesRegex(ValueError, "audit-only"):
            ci.plan_for(gate_args(core=True))

    def test_gate_rejects_broad_all_scope_as_a_repair_loop(self) -> None:
        with self.assertRaisesRegex(ValueError, "audit-only"):
            ci.plan_for(gate_args(all=True))

    def test_gate_rejects_all_gameplay_as_a_repair_loop(self) -> None:
        with self.assertRaisesRegex(ValueError, "all-gameplay.*audit-only"):
            ci.plan_for(gate_args(gameplay="all"))

    def test_gate_rejects_multiple_build_lanes(self) -> None:
        with self.assertRaisesRegex(ValueError, "exactly one build-producing lane"):
            ci.plan_for(gate_args(soak=True, gameplay="ore"))

    def test_audit_has_no_redundant_compile_only_stage(self) -> None:
        builds = cargo_build_commands(ci.audit_plan("all"))
        self.assertFalse(any("check-fast" in command for command in builds))
        self.assertEqual(builds, [ci.combined_test_command()])
        self.assertIn("--lib", builds[0])
        self.assertEqual(
            cargo_test_targets(builds[0]),
            [ci.GAMEPLAY_AUDIT_TARGET],
        )

    def test_combined_audit_summary_keeps_core_and_gameplay_counts_legible(self) -> None:
        output = "\n".join(
            [
                "test result: ok. 559 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out",
                "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out",
                "test result: ok. 14 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out",
            ]
        )
        self.assertEqual(
            ci.combined_test_summary(output),
            "559 core + 26 gameplay, 1 ignored",
        )

    def test_core_repair_loop_stays_feature_minimal_while_gameplay_is_explicit(self) -> None:
        config = tomllib.loads((ROOT / ".cargo" / "config.toml").read_text(encoding="utf-8"))
        core_alias = config["alias"]["test-core"]
        gameplay = " ".join(ci.gameplay_command("all"))
        self.assertNotIn("--features", core_alias)
        self.assertIn("--features test-gameplay", gameplay)

    def test_scoped_audits_do_not_build_the_other_broad_surface(self) -> None:
        core_builds = cargo_build_commands(ci.audit_plan("core"))
        gameplay_builds = cargo_build_commands(ci.audit_plan("gameplay"))
        self.assertEqual(core_builds, [["cargo", "test-core"]])
        self.assertEqual(len(gameplay_builds), 1)
        self.assertIn("test-gameplay", gameplay_builds[0])
        self.assertNotIn(["cargo", "test-core"], gameplay_builds)

    def test_broad_gameplay_audit_uses_one_consolidated_target(self) -> None:
        command = ci.gameplay_command("all")
        self.assertEqual(cargo_test_targets(command), [ci.GAMEPLAY_AUDIT_TARGET])
        catalog = run_test.source_test_catalog(ci.GAMEPLAY_AUDIT_TARGET, None)
        for test_name in ci.GAMEPLAY_TESTS.values():
            self.assertIn(test_name, catalog)
        self.assertIn(
            "process_catalog_contract_tests::every_authored_process_has_legible_physical_execution_topology",
            catalog,
        )

    def test_broad_core_failure_points_to_one_exact_repair(self) -> None:
        output = "failures:\n    mining::execution::tests::missing_capability\n"
        self.assertEqual(
            ci.repair_hint(["cargo", "test-core"], output, ""),
            "python tools/run_test.py mining::execution::tests::missing_capability",
        )

    def test_gameplay_contract_failure_reuses_the_already_built_contract_target(self) -> None:
        output = "failures:\n    configuration_tests::broken_contract\n"
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit configuration_tests::broken_contract",
        )

    def test_combined_audit_core_failure_points_to_exact_unit_test(self) -> None:
        output = "failures:\n    mining::execution::tests::missing_capability\n"
        error = "error: test failed, to rerun pass `--lib`"
        self.assertEqual(
            ci.repair_hint(ci.combined_test_command(), output, error),
            "python tools/run_test.py mining::execution::tests::missing_capability",
        )

    def test_gameplay_failure_without_test_name_reuses_the_semantic_scope(self) -> None:
        error = "error: test failed, to rerun pass `--test gameplay_workshop`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("workshop"), "", error),
            "python ci.py gate --gameplay workshop",
        )

    def test_focused_gameplay_failure_points_to_exact_small_target(self) -> None:
        output = "failures:\n    gameplay_ore_preparation_probe\n"
        error = "error: test failed, to rerun pass `--test gameplay_ore`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("ore"), output, error),
            "python tools/run_test.py --target gameplay_ore gameplay_ore_preparation_probe",
        )

    def test_broad_focused_failure_stays_on_the_warm_audit_target(self) -> None:
        output = (
            "PROBE INPUT name=ore-preparation mode=gate samples=3 organic=1 "
            "world_root=0x1234 behavior_root=n/a replay=anchor:0x1,organic:0x2\n"
            "failures:\n    gameplay_ore_preparation_probe\n"
        )
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit --variation-seed 0x1234 gameplay_ore_preparation_probe",
        )

    def test_agency_failure_reuses_the_warm_audit_target(self) -> None:
        output = (
            "AGENCY INPUT mode=gate organic=1 variation_root=0x24311DCEB06D58AE\n"
            "failures:\n    agency::gameplay_agency_counterfactuals\n"
        )
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit --variation-seed 0x24311DCEB06D58AE agency::gameplay_agency_counterfactuals",
        )

    def test_workshop_failure_preserves_both_replay_roots(self) -> None:
        output = (
            "HARNESS INPUT plan=anchor+variation anchors=7 variation=1 custom=0 "
            "world_root=0xAAAA behavior_root=0xBBBB replay=0x1@0x2\n"
            "failures:\n    gameplay_harness_gate\n"
        )
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit --variation-seed 0xAAAA --behavior-seed 0xBBBB gameplay_harness_gate",
        )

    def test_process_catalog_failure_reuses_the_warm_audit_target(self) -> None:
        output = "failures:\n    process_catalog_contract_tests::every_authored_process_has_legible_physical_execution_topology\n"
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit process_catalog_contract_tests::every_authored_process_has_legible_physical_execution_topology",
        )

    def test_survival_generator_failure_reuses_the_warm_audit_target(self) -> None:
        output = "failures:\n    survival_contract_tests::survival_generation_covers_authored_options_without_policy_leakage\n"
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit survival_contract_tests::survival_generation_covers_authored_options_without_policy_leakage",
        )

    def test_failure_output_keeps_context_and_tail_without_unbounded_transcripts(self) -> None:
        lines = [f"line-{index}" for index in range(100)]
        bounded = ci.bounded_failure_output("\n".join(lines))
        self.assertIn("line-0", bounded)
        self.assertIn("line-99", bounded)
        self.assertIn("20 line(s) omitted", bounded)
        self.assertNotIn("line-20\n", bounded)

    def test_run_test_replay_flags_map_to_existing_harness_environment(self) -> None:
        args = run_test.parse_args(
            [
                "--target",
                ci.GAMEPLAY_AUDIT_TARGET,
                "--variation-seed",
                "0xAAAA",
                "--behavior-seed",
                "0xBBBB",
                "gameplay_harness_gate",
            ]
        )
        self.assertEqual(
            run_test.gameplay_replay_environment(args),
            {
                "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "0xAAAA",
                "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED": "0xBBBB",
            },
        )

    def test_run_test_verbose_only_controls_selected_test_output(self) -> None:
        args = run_test.parse_args(
            [
                "--target",
                ci.GAMEPLAY_TARGETS["foundry"],
                "--verbose",
                ci.GAMEPLAY_TESTS["foundry"],
            ]
        )
        self.assertIn("--nocapture", run_test.cargo_command(args))
        self.assertEqual(run_test.gameplay_replay_environment(args), {})

    def test_unknown_gameplay_failure_falls_back_to_the_broad_gameplay_audit(self) -> None:
        output = "failures:\n    future_contracts::new_global_check\n"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, ""),
            "python ci.py audit --gameplay",
        )

    def test_integration_exact_command_infers_target_required_features(self) -> None:
        args = argparse.Namespace(
            target=ci.GAMEPLAY_TARGETS["ore"],
            features=None,
            list=False,
            name=ci.GAMEPLAY_TESTS["ore"],
            suite=False,
            ignored=False,
            nocapture=False,
        )
        command = run_test.cargo_command(args)
        self.assertEqual(command.count("--features"), 1)
        self.assertIn("test-gameplay", command)
        self.assertIn(ci.GAMEPLAY_TARGETS["ore"], command)

    def test_library_check_refuses_cargos_broad_all_test_graph(self) -> None:
        args = argparse.Namespace(
            target="lib",
            features=None,
            list=False,
            suite=False,
        )
        with self.assertRaisesRegex(ValueError, "every integration target"):
            run_test.cargo_check_command(args)

    def test_integration_check_command_infers_required_features_without_linking(self) -> None:
        args = argparse.Namespace(
            target=ci.GAMEPLAY_CONTRACTS_TARGET,
            features=None,
            list=False,
            suite=False,
        )
        self.assertEqual(
            run_test.cargo_check_command(args),
            [
                "cargo",
                "check",
                "--quiet",
                "--locked",
                "--test",
                ci.GAMEPLAY_CONTRACTS_TARGET,
                "--features",
                "test-gameplay",
            ],
        )

    def test_check_mode_is_target_only_and_needs_no_test_selector(self) -> None:
        args = run_test.parse_args(["--check", "--target", ci.GAMEPLAY_AUDIT_TARGET])
        self.assertTrue(args.check)
        self.assertIsNone(args.name)
        self.assertEqual(args.target, ci.GAMEPLAY_AUDIT_TARGET)

    def test_check_mode_rejects_a_misleading_test_selector(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                run_test.parse_args(
                    [
                        "--check",
                        "--target",
                        ci.GAMEPLAY_AUDIT_TARGET,
                        "gameplay_ore_preparation_probe",
                    ]
                )

    def test_run_test_failure_output_is_bounded(self) -> None:
        lines = [f"line-{index}" for index in range(100)]
        bounded = run_test.bounded_failure_output("\n".join(lines))
        self.assertIn("line-0", bounded)
        self.assertIn("line-99", bounded)
        self.assertIn("20 line(s) omitted", bounded)
        self.assertNotIn("line-20\n", bounded)

    def test_report_reuses_ordinary_gameplay_feature_shape(self) -> None:
        plan = ci.report_plan()
        commands = [command for _label, command in plan]
        flattened = "\n".join(" ".join(command) for command in commands)
        self.assertEqual(len(plan), 1)
        self.assertNotIn("test-gameplay-full", flattened)
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        report_definition = next(
            definition
            for definition in manifest.get("example", [])
            if definition["name"] == ci.GAMEPLAY_REPORT_EXAMPLE
        )
        self.assertEqual(report_definition.get("required-features"), ["test-gameplay"])
        report = plan[0][1]
        self.assertEqual(
            report,
            [
                "cargo",
                "run",
                "--quiet",
                "--locked",
                "--profile",
                "test",
                "--example",
                ci.GAMEPLAY_REPORT_EXAMPLE,
                "--features",
                "test-gameplay",
            ],
        )

    def test_gameplay_replay_environment_is_fresh_by_default_and_preserves_explicit_roots(self) -> None:
        generated: dict[str, str] = {}
        rolls = iter((0x1234, 0x5678))
        variation, behavior = ci.configure_gameplay_replay_environment(
            generated, randbits=lambda _bits: next(rolls)
        )
        self.assertEqual(variation, "0x0000000000001234")
        self.assertEqual(behavior, "0x0000000000005678")
        self.assertEqual(generated["DEEP_HEARTH_GAMEPLAY_VARIATION_SEED"], variation)
        self.assertEqual(generated["DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED"], behavior)

        explicit = {
            "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "0xAA",
            "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED": "0xBB",
        }
        self.assertEqual(
            ci.configure_gameplay_replay_environment(
                explicit, randbits=lambda _bits: self.fail("explicit replay roots must not consume entropy")
            ),
            ("0xAA", "0xBB"),
        )

    def test_supported_gameplay_commands_use_fresh_bounded_variation(self) -> None:
        for argv in (
            ["gate", "--gameplay", "survival"],
            ["gate", "--gameplay", "progression"],
            ["gate", "--gameplay", "workshop"],
            ["audit", "--gameplay"],
            ["audit", "--all"],
            ["report"],
        ):
            with self.subTest(argv=argv):
                self.assertTrue(ci.uses_fresh_gameplay_variation(ci.parse_args(argv)))
        for argv in (
            ["quick"],
            ["gate"],
            ["gate", "--gameplay", "contracts"],
            ["audit", "--core"],
            ["gate", "--lint"],
        ):
            with self.subTest(argv=argv):
                self.assertFalse(ci.uses_fresh_gameplay_variation(ci.parse_args(argv)))

    def test_successful_gameplay_stage_can_report_environment_replay_roots(self) -> None:
        environment = {
            "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "0xAAAA",
            "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED": "0xBBBB",
        }
        self.assertEqual(
            ci.gameplay_environment_summary("gameplay progression", environment),
            "roots=0xAAAA/0xBBBB",
        )
        self.assertEqual(
            ci.gameplay_environment_summary("core + gameplay", environment),
            "roots=0xAAAA/0xBBBB",
        )
        self.assertIsNone(ci.gameplay_environment_summary("gameplay contracts", environment))
        self.assertIsNone(ci.gameplay_environment_summary("compile", environment))

    def test_report_cli_preserves_large_success_evidence_but_bounds_failures(self) -> None:
        opening = "PLAYER FANTASY scope=current-ordinary fixture=opening"
        replay = (
            "PROBE INPUT name=survival-provisioning mode=explore samples=2 organic=1 "
            "world_root=0x111 behavior_root=0x222 replay=anchor:0xA@0x1,organic:0xB@0x2"
        )
        ending = "EVIDENCE CONTRACT fixture=report-end"
        lines = [
            opening,
            replay,
            *[f"SURVIVAL REVIEW fixture-row={index} {'x' * 200}" for index in range(1000)],
            ending,
        ]
        transcript = "\n".join(lines) + "\n"
        self.assertGreater(len(lines), ci.FAILURE_HEAD_LINES + ci.FAILURE_TAIL_LINES)
        self.assertGreater(len(transcript), 200_000)
        command = ci.report_plan()[0][1]
        for mode in (None, "DEEP_HEARTH_GAMEPLAY_VERBOSE", "DEEP_HEARTH_GAMEPLAY_TRACE"):
            for returncode in (0, 1):
                with self.subTest(mode=mode, returncode=returncode):
                    environment = {
                        "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "0x111",
                        "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED": "0x222",
                    }
                    if mode is not None:
                        environment[mode] = "1"
                    result = ci.subprocess.CompletedProcess(
                        command, returncode, transcript, transcript if returncode else ""
                    )
                    with (
                        mock.patch.dict(ci.os.environ, environment, clear=True),
                        mock.patch.object(sys, "argv", ["ci.py", "report"]),
                        mock.patch.object(ci.subprocess, "run", return_value=result) as run,
                        contextlib.redirect_stdout(io.StringIO()) as stdout,
                        contextlib.redirect_stderr(io.StringIO()) as stderr,
                    ):
                        self.assertEqual(ci.main(), returncode)
                    run.assert_called_once()
                    self.assertEqual(run.call_args.args[0], command)
                    self.assertTrue(run.call_args.kwargs["capture_output"])
                    if returncode == 0:
                        self.assertEqual(stderr.getvalue(), "")
                        self.assertIn("roots=0x111/0x222", stdout.getvalue())
                        # Compare the entire body, not only markers that a head/tail limiter keeps.
                        body = "\n".join(stdout.getvalue().splitlines()[2:-1]) + "\n"
                        if mode is not None:
                            self.assertEqual(body, transcript)
                        else:
                            self.assertEqual(body, f"{opening}\n")
                            self.assertIn(opening, stdout.getvalue())
                            self.assertNotIn(ending, stdout.getvalue())
                        self.assertIn("PASS total", stdout.getvalue())
                    else:
                        self.assertNotIn(opening, stdout.getvalue())
                        bounded = "\n".join([
                            *lines[:ci.FAILURE_HEAD_LINES],
                            f"... {len(lines) - ci.FAILURE_HEAD_LINES - ci.FAILURE_TAIL_LINES} line(s) omitted ...",
                            *lines[-ci.FAILURE_TAIL_LINES:],
                        ])
                        self.assertEqual(
                            stderr.getvalue(),
                            f"reproduce: {' '.join(command)}\n{bounded}\n{bounded}\n",
                        )
                        self.assertNotIn("PASS total", stdout.getvalue())

    def test_survival_summary_counts_selected_preservation_policy_only(self) -> None:
        lines = [
            "SURVIVAL EXPERIENCE seed=0x1 pressure=hydration raw-opportunity=[origin:1 mode:scarce-timber] storage-policy:decline commitment:none best-enclosure-counterfactual=[policy:enclosure-singleton candidates:1 build:150t]",
            "SURVIVAL EXPERIENCE seed=0x2 pressure=energy raw-opportunity=[origin:2 mode:choice-rich-timber] storage-policy:attention-efficient commitment:1 best-enclosure-counterfactual=[policy:attention-efficient candidates:4 build:120t]",
        ]
        summary = "\n".join(gameplay_report_summary.ordinary_gameplay_summary(lines))
        self.assertIn("declined:1", summary)
        self.assertIn("efficient:1", summary)
        self.assertIn(
            "preservation-opportunity=[scarce:1 choice-rich:1 alternate:0 finite:0 singleton:1 multi:1]",
            summary,
        )

    def test_fieldwork_summary_separates_world_constraints_from_selected_tool(self) -> None:
        lines = [
            "FIELDWORK EXPERIENCE seed=0x1 sample=anchor outcome=completed order-horizon=short field-inspections=1 geology=quarry-soft tool=stone-quarry copper-opportunity=absent requested=100mg planned-local-work=100mg mining=100mg resource-knowledge-effect=same-tool",
            "FIELDWORK EXPERIENCE seed=0x2 sample=coverage outcome=known-target-supply order-horizon=long field-inspections=3 geology=quarry-reinforcement tool=copper-reinforced-quarry copper-opportunity=available requested=200mg planned-local-work=80mg mining=50mg resource-knowledge-effect=changed-tool",
            "FIELDWORK EXPERIENCE seed=0x3 sample=organic outcome=completed order-horizon=long field-inspections=2 geology=hard-pick-specialist tool=copper-reinforced-hard-pick copper-opportunity=available requested=300mg planned-local-work=300mg mining=300mg resource-knowledge-effect=same-tool",
        ]
        summary = "\n".join(gameplay_report_summary.ordinary_gameplay_summary(lines))
        self.assertIn("sample-shape=[anchor:1 coverage:1 organic:1 replay:0]", summary)
        self.assertIn(
            "outcomes=[completed:2 local-supply-ended:1 fulfillment:250000..1000000ppm]",
            summary,
        )
        self.assertIn("organic-outcomes=[completed:1 local-supply-ended:0]", summary)
        self.assertIn(
            "reserve-knowledge=[workload-capped:1 tool-changed:1 feasibility-changed:0]",
            summary,
        )
        self.assertIn(
            "organic-reserve-knowledge=[workload-capped:0 tool-changed:0 feasibility-changed:0]",
            summary,
        )
        self.assertIn("orders=[short:1 long:2]", summary)
        self.assertIn(
            "geology=[soft:1 reinforcement:1 hard-specialist:1]", summary
        )
        self.assertIn("copper=[available:2 absent:1]", summary)

    def test_default_gameplay_report_keeps_compact_semantic_summaries_only(self) -> None:
        lines = [
            "SIMULATION TIME physical-tick-us=3600000",
            "PLAYER FANTASY scope=current-ordinary",
            "EVALUATION SCOPE kind=ordinary-play evidence=runtime-actions-after-disclosed-bootstrap",
            "CONTENT registry_schema=64 equipment=[authored:12]",
            "CONTENT ACQUISITION EDGES equipment=[authored-edge:8 no-authored-edge:4]",
            "EVIDENCE CONTRACT runtime-experience-after-disclosed-bootstrap=[survival,primitive-progression]",
            "EVALUATION SCOPE kind=controlled-capability evidence=isolated-system-behavior",
            "PROBE INPUT name=survival-provisioning mode=explore samples=1 organic=0",
            "SURVIVAL EXPERIENCE seed=0x1 sample=anchor pressure=hydration choice=[state:policy-sensitive diet:balanced-recovery] storage-policy:decline",
            "PROGRESSION EXPERIENCE seed=0x1 sample=anchor information=surface-resolved local-copper-sequence=pick-first scarcity=[direct-second-upgrade-blocked:true processed-output-playable:true converged-both-upgrades:true] processing-investment=[selected:mechanized preaction-manual:2470t conservative-machine-upper:1800t assembly:320t initial-charge:18t repeated-charge-upper:1400t choice-frozen-before-action:true] counterfactual=[crank-first-tradeoff hard-access-lead:478t] bridge-tradeoff=[manual-second:111t feed:41465mg recovery:650000ppm body:1nJ/1uL; powered-line:421t feed:29947mg recovery:900000ppm body:2nJ/2uL] disclosed-order-economics=[cycles:12 manual-player-attention:2470t mechanized-player-attention:429t saved:2041t] stockpiling-coverage-delegation=[feed-attention:28t maintenance-prep-overlap:40t productive-attention:68t returned-attention:315t returned:822454ppm overlap/setup:210526ppm unrecovered-setup:255t overlap-equivalent:unreached post-equivalent:0cycles stop:stockpile-order-complete economics:finite-stockpile-order-complete] selected-reinvestment=[completed]",
            "PROGRESSION GOAL seed=0x1 immediate=265t delayed=741t chosen=immediate",
            "LIBERATION FRONTIER CAPABILITY seed=0x1 selected-by-current-player=false reason=no-ordinary-concentrate-sink input=[100mg] concentrate=[first:70mg/700000ppm final:75mg/750000ppm] copper-in-concentrate=[first:49mg final:56mg scavenger-recovered:7mg] matter=conserved",
            "LIBERATION FRONTIER seed=0x1 smelting-frontier=prepared-ore-concentrate->pure-metal",
            "WOODWORKING EXPERIENCE seed=0x1 sample=anchor choice=bare-hands reason=bare-hands-avoids-investment-cost",
            "FIELDWORK EXPERIENCE seed=0x1 sample=anchor outcome=completed order-horizon=short field-inspections=1 detailed-surveys=1 observed-hardness=1..2Pa geology=quarry-soft tool=stone-quarry adaptation=preparation-plus-order copper-opportunity=absent retained-native-copper=1mg requested=1mg mining=1mg",
            "POWER PROVIDER EXPERIENCE seed=0x1 sample=anchor job=[flywheel:1nJ planned-charges:1] decision=[selected:crank] comparison=[charge-attention-reduction:1ppm metabolic-crank:2nJ metabolic-treadle:1nJ break-even-charges:2]",
            "POWER SETTLEMENT seed=0x1 sample=anchor buffer:5000nJ planned-charges=80 decision=[selected:walking-wheel projected-attention-treadle:2290t projected-attention-walking:2210t choice-frozen-before-action:true] comparison=[charge-saving:4t metabolic-saving:1nJ break-even:60charges]",
            "WORKSHOP CAPABILITY mode=exploratory scenarios=1",
            "WORKSHOP EXPERIENCE REVIEW fantasy=operate+adapt dynamic-scenarios:10/11 interlocks=[stored-work+throughput:11 body+power:5 wear+maintenance:6 structure+production:9] recovery=[suspensions:3 resumed:3 stranded:0]",
            "AGENCY SUMMARY worlds=1",
            "CAPABILITY ORE_PREP seed=0x1 outcome=completed feed=[copper:400000ppm]",
            "CAPABILITY FOUNDRY seed=0x1 outcome=full-order-complete melt-limit=offered-batch cast-limit=offered-batch recovery-cast=0mg",
            "POWER BUILD BILL seed=0x1 noisy-detail",
            "FIELDWORK PACING seed=0x1 noisy-detail",
        ]
        output = "\n".join(lines)
        concise = gameplay_report_summary.concise_gameplay_report(output, {})
        for prefix in (
            "SIMULATION TIME ",
            "PLAYER FANTASY ",
            "EVALUATION SCOPE kind=ordinary-play ",
            "CONTENT registry_schema=",
            "CONTENT ACQUISITION EDGES ",
            "EVALUATION SCOPE kind=controlled-capability ",
            "ORDINARY SUMMARY probe=primitive-progression ",
            "FRONTIER SUMMARY probe=primitive-liberation ",
            "ORDINARY SUMMARY probe=woodworking ",
            "ORDINARY SUMMARY probe=fieldwork ",
            "ORDINARY SUMMARY probe=power-provider ",
            "ORDINARY SUMMARY probe=survival ",
            "CONTROLLED SUMMARY probe=workshop ",
            "CONTROLLED SUMMARY probe=agency ",
            "ORE CAPABILITY SUMMARY ",
            "FOUNDRY CAPABILITY SUMMARY ",
        ):
            self.assertIn(prefix, concise)
        for noisy in (
            "PROBE INPUT ",
            "SURVIVAL EXPERIENCE ",
            "PROGRESSION EXPERIENCE ",
            "LIBERATION FRONTIER CAPABILITY ",
            "WOODWORKING EXPERIENCE ",
            "FIELDWORK EXPERIENCE ",
            "POWER PROVIDER EXPERIENCE ",
            "CAPABILITY ORE_PREP ",
            "CAPABILITY FOUNDRY ",
            "EVIDENCE CONTRACT ",
            "WORKSHOP CAPABILITY ",
            "WORKSHOP EXPERIENCE REVIEW ",
            "AGENCY SUMMARY ",
            "POWER BUILD BILL ",
            "FIELDWORK PACING ",
        ):
            self.assertNotIn(noisy, concise)
        self.assertIn(
            "stockpiling-counterfactual=[returned-attention:315..315t returned-share:822454..822454ppm maintenance-prep-overlap:40..40t useful-overlap/setup:210526..210526ppm post-overlap-equivalent-cycles:0..0]",
            concise,
        )
        self.assertIn(
            "processing-recovery=[manual:650000..650000ppm powered:900000..900000ppm]",
            concise,
        )
        self.assertIn(
            "scarcity-bridge=[direct-second-blocked:1 processed-output-playable:1 converged:1]",
            concise,
        )
        self.assertIn(
            "preaction-processing-investment=[selected:mechanized manual:2470..2470t conservative-machine-upper:1800..1800t frozen:1/1]",
            concise,
        )
        self.assertIn(
            "disclosed-order-attention=[manual:2470..2470t mechanized:429..429t saved:2041..2041t]",
            concise,
        )
        self.assertIn(
            "settlement-choice=[treadle:0 walking:1] organic-settlement-choice=[treadle:0 walking:0] settlement-planned-charges=80..80 settlement-break-even-charges=60..60",
            concise,
        )
        self.assertIn(
            "continuation=[immediate:1/0 stockpile-first:1/0 delay-avoided:476..476t]",
            concise,
        )
        self.assertIn("dynamic=10/11", concise)
        self.assertIn(
            "interlocks=[stored-work:11 body-power:5 wear-maintenance:6 structure-production:9]",
            concise,
        )
        self.assertIn("recovery=[suspended:3 resumed:3 stranded:0]", concise)
        self.assertIn("final-concentrate-grade=750000..750000ppm", concise)
        self.assertIn("current-player-selected=false", concise)
        self.assertIn("scavenger-copper=7..7mg", concise)
        self.assertIn(
            "smelting-frontier=prepared-ore-concentrate->pure-metal",
            concise,
        )
        self.assertEqual(
            gameplay_report_summary.concise_gameplay_report(
                output, {"DEEP_HEARTH_GAMEPLAY_VERBOSE": "1"}
            ),
            output,
        )

    def test_progression_summary_preserves_stockpiling_delay_and_supply_blocking(self) -> None:
        lines = [
            "PROGRESSION EXPERIENCE seed=0x1 local-copper-sequence=pick-first selected-reinvestment=[completed]",
            "PROGRESSION EXPERIENCE seed=0x2 local-copper-sequence=crank-first selected-reinvestment=[completed]",
            "PROGRESSION GOAL seed=0x1 immediate=264t delayed=747t chosen=immediate",
            "PROGRESSION GOAL seed=0x2 immediate=267t delayed=blocked:target-supply chosen=immediate",
        ]
        summary = "\n".join(gameplay_report_summary.ordinary_gameplay_summary(lines))
        self.assertIn(
            "continuation=[immediate:2/0 stockpile-first:1/1 delay-avoided:483..483t]",
            summary,
        )

    def test_report_verbose_flag_is_explicit_and_report_only(self) -> None:
        self.assertTrue(ci.parse_args(["report", "--verbose"]).verbose)
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                ci.parse_args(["gate", "--verbose"])

    def test_woodworking_summary_preserves_no_tool_choice(self) -> None:
        summary = gameplay_report_summary.concise_gameplay_report(
            "WOODWORKING EXPERIENCE seed=0xFA choice=bare-hands reason=bare-hands-avoids-investment-cost",
            {},
        )
        self.assertIn("choice=[saw:0 adze:0 bare:1]", summary)

    def test_git_wizard_validation_levels_match_iteration_policy(self) -> None:
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        validation = manifest["package"]["metadata"]["git-wizard"]["validation"]
        self.assertEqual(validation["quick"], "python ci.py quick")
        self.assertEqual(validation["standard"], "python ci.py gate")
        self.assertNotIn("full", validation)

    def test_build_producing_cargo_targets_are_explicit_not_auto_discovered(self) -> None:
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        package = manifest["package"]
        for key in ("autobins", "autoexamples", "autotests", "autobenches"):
            self.assertFalse(package[key])
        targets = {definition["name"] for definition in manifest.get("test", [])}
        self.assertEqual(
            targets,
            {
                ci.GAMEPLAY_AUDIT_TARGET,
                ci.GAMEPLAY_CONTRACTS_TARGET,
                *ci.GAMEPLAY_TARGETS.values(),
            },
        )
        binaries = {definition["name"] for definition in manifest.get("bin", [])}
        self.assertEqual(binaries, {"validate-shaders"})
        examples = {definition["name"] for definition in manifest.get("example", [])}
        self.assertEqual(examples, {ci.GAMEPLAY_REPORT_EXAMPLE})

    def test_shader_validation_reuses_existing_test_profile(self) -> None:
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        config = tomllib.loads((ROOT / ".cargo" / "config.toml").read_text(encoding="utf-8"))
        shader_alias = config["alias"]["test-shaders"]
        self.assertNotIn("test-all", config["alias"])
        self.assertNotIn("validation", manifest.get("profile", {}))
        self.assertIn("--profile test", shader_alias)
        self.assertNotIn("--profile validation", shader_alias)

    def test_audit_requires_explicit_scope(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                ci.parse_args(["audit"])
        self.assertTrue(ci.parse_args(["audit", "--all"]).all)

    def test_documented_ci_command_checker_rejects_removed_flags(self) -> None:
        self.assertIsNone(check_authority_docs.ci_command_error("python ci.py gate --rustdoc"))
        self.assertIsNone(
            check_authority_docs.ci_command_error("python ci.py gate --gameplay [scope]")
        )
        self.assertIsNone(
            check_authority_docs.ci_command_error(
                "python ci.py gate --gameplay {workshop,survival,progression,ore,foundry}"
            )
        )
        self.assertIsNone(
            check_authority_docs.ci_command_error("python ci.py gate --gameplay contracts")
        )
        self.assertIsNone(check_authority_docs.ci_command_error("python ci.py audit --core"))
        self.assertIsNone(check_authority_docs.ci_command_error("python ci.py audit --gameplay"))
        self.assertIsNone(check_authority_docs.ci_command_error("python ci.py audit --all"))
        self.assertIsNone(
            check_authority_docs.ci_command_error(
                "python ci.py bca --hotspots --path src/inventory"
            )
        )
        audit_error = check_authority_docs.ci_command_error("python ci.py audit")
        self.assertIsNotNone(audit_error)
        self.assertIn("invalid local CI command", audit_error or "")
        broad_gate_error = check_authority_docs.ci_command_error("python ci.py gate --core")
        self.assertIsNotNone(broad_gate_error)
        self.assertIn("invalid local CI command", broad_gate_error or "")
        error = check_authority_docs.ci_command_error("python ci.py gate --docs")
        self.assertIsNotNone(error)
        self.assertIn("invalid local CI command", error or "")

    def test_documentation_checker_covers_specialized_docs_not_generated_output(self) -> None:
        documents = set(check_authority_docs.documentation_files())
        self.assertTrue(set(check_authority_docs.AUTHORITY_FILES).issubset(documents))
        self.assertIn("assets/shaders/README.md", documents)
        self.assertFalse(any(path.startswith("target/") for path in documents))

    def test_agent_orientation_maps_track_live_module_and_owner_topology(self) -> None:
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        technical_design = (ROOT / "TECHNICAL_DESIGN.md").read_text(encoding="utf-8")

        documented_modules = check_authority_docs.documented_source_role_modules(readme)
        source_modules = check_authority_docs.public_top_level_modules()
        self.assertEqual(len(documented_modules), len(set(documented_modules)))
        self.assertEqual(set(documented_modules), set(source_modules))
        self.assertEqual(
            check_authority_docs.documented_runtime_owner_types(technical_design),
            check_authority_docs.system_state_owner_types(),
        )
        state_source = (ROOT / "src" / "core" / "state.rs").read_text(encoding="utf-8")
        self.assertEqual(check_authority_docs.public_root_mutator_names(state_source), [])

    def test_cold_start_authority_context_stays_bounded(self) -> None:
        documents = {
            relative: (ROOT / relative).read_text(encoding="utf-8")
            for relative in check_authority_docs.COLD_START_DOCUMENT_MAX_BYTES
        }
        self.assertEqual(check_authority_docs.check_cold_start_context_budget(documents), [])

        per_file = dict(documents)
        per_file["AGENTS.md"] = "x" * (
            check_authority_docs.COLD_START_DOCUMENT_MAX_BYTES["AGENTS.md"] + 1
        )
        self.assertTrue(
            any(
                "AGENTS.md: cold-start document" in error
                for error in check_authority_docs.check_cold_start_context_budget(per_file)
            )
        )

        aggregate = {
            relative: "x" * maximum
            for relative, maximum in check_authority_docs.COLD_START_DOCUMENT_MAX_BYTES.items()
        }
        self.assertTrue(
            any(
                "aggregate budget" in error
                for error in check_authority_docs.check_cold_start_context_budget(aggregate)
            )
        )

    def test_cold_start_usage_reports_current_cost_and_reserved_headroom(self) -> None:
        documents = {
            relative: "x" * (index + 1)
            for index, relative in enumerate(
                check_authority_docs.COLD_START_DOCUMENT_MAX_BYTES
            )
        }
        expected_used = sum(range(1, len(documents) + 1))
        used, reserve = check_authority_docs.cold_start_context_usage(documents)
        self.assertEqual(used, expected_used)
        self.assertEqual(
            reserve,
            check_authority_docs.COLD_START_TOTAL_MAX_BYTES - expected_used,
        )

    def test_authority_routing_sections_stay_present_and_unambiguous(self) -> None:
        documents = {
            relative: (ROOT / relative).read_text(encoding="utf-8")
            for relative in check_authority_docs.AUTHORITY_FILES
        }
        self.assertEqual(check_authority_docs.check_required_authority_sections(documents), [])

        missing = dict(documents)
        missing["README.md"] = missing["README.md"].replace(
            "## Control coordinate", "## Renamed coordinate", 1
        )
        errors = check_authority_docs.check_required_authority_sections(missing)
        self.assertTrue(
            any(
                "README.md: missing required authority sections: Control coordinate" in error
                for error in errors
            )
        )

        duplicate = dict(documents)
        duplicate["DIRECTION.md"] += "\n## Accretion objective\n"
        errors = check_authority_docs.check_required_authority_sections(duplicate)
        self.assertTrue(
            any(
                "DIRECTION.md: duplicate level-two authority headings: Accretion objective" in error
                for error in errors
            )
        )

    def test_public_root_mutator_guard_rejects_a_second_command_surface(self) -> None:
        fixture = "impl AppState {\n    pub fn inventory_mut(&mut self) {}\n}"
        self.assertEqual(
            check_authority_docs.public_root_mutator_names(fixture),
            ["inventory_mut"],
        )

    def test_agent_orientation_checker_rejects_missing_and_duplicate_module_roles(self) -> None:
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        technical_design = (ROOT / "TECHNICAL_DESIGN.md").read_text(encoding="utf-8")

        missing = readme.replace("`src/thermal/`", "`src/not_real/`", 1)
        errors = check_authority_docs.check_source_orientation_maps(
            {"README.md": missing, "TECHNICAL_DESIGN.md": technical_design}
        )
        self.assertTrue(any("missing public top-level modules: thermal" in error for error in errors))
        self.assertTrue(any("non-public top-level modules: not_real" in error for error in errors))

        duplicate = readme.replace(
            "`src/core/`, `src/capability/`",
            "`src/core/`, `src/core/`, `src/capability/`",
            1,
        )
        errors = check_authority_docs.check_source_orientation_maps(
            {"README.md": duplicate, "TECHNICAL_DESIGN.md": technical_design}
        )
        self.assertTrue(any("classifies modules more than once: core" in error for error in errors))

    def test_agent_orientation_checker_rejects_runtime_owner_atlas_drift(self) -> None:
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        technical_design = (ROOT / "TECHNICAL_DESIGN.md").read_text(encoding="utf-8")
        drifted = technical_design.replace(
            "| `EnergyState` | `AppState::energy()`",
            "| `MissingEnergyState` | `AppState::energy()`",
            1,
        )

        errors = check_authority_docs.check_source_orientation_maps(
            {"README.md": readme, "TECHNICAL_DESIGN.md": drifted}
        )
        self.assertTrue(any("runtime owner atlas must match SystemState" in error for error in errors))

    def test_execution_card_checker_requires_portfolio_profiles_and_bca_policy(self) -> None:
        valid = {
            "AGENTS.md": (
                "**Applicable profiles:** Universal; Stateful Application; Deterministic System; "
                "Automated Behavior Evaluation\n**BCA policy:** ratchet\n"
            )
        }
        self.assertEqual(check_authority_docs.check_execution_card(valid), [])

        missing_profile = {
            "AGENTS.md": "**Applicable profiles:** Universal\n**BCA policy:** ratchet\n"
        }
        errors = check_authority_docs.check_execution_card(missing_profile)
        self.assertTrue(any("missing applicable portfolio profiles" in error for error in errors))

        missing_bca = {
            "AGENTS.md": (
                "**Applicable profiles:** Universal; Stateful Application; Deterministic System; "
                "Automated Behavior Evaluation\n"
            )
        }
        errors = check_authority_docs.check_execution_card(missing_bca)
        self.assertTrue(any("BCA policy" in error for error in errors))

    def test_module_doc_checker_covers_production_and_integration_rust(self) -> None:
        errors, checked = check_authority_docs.check_source_module_docs()
        expected = sum(1 for root in (ROOT / "src", ROOT / "tests") for _ in root.rglob("*.rs"))

        self.assertEqual(errors, [])
        self.assertEqual(checked, expected)
        self.assertGreater(sum(1 for _ in (ROOT / "tests").rglob("*.rs")), 0)

    def test_documentation_routes_resolve_from_nested_document_location(self) -> None:
        nested = ROOT / "assets" / "shaders" / "README.md"
        self.assertEqual(
            check_authority_docs.resolve_route(nested, "../../TESTING.md"),
            ROOT / "TESTING.md",
        )
        self.assertEqual(
            check_authority_docs.resolve_route(nested, "src/shader/"),
            ROOT / "src" / "shader",
        )

    def test_documentation_checker_validates_semantic_markdown_anchors(self) -> None:
        valid_errors, _, valid_checked = check_authority_docs.inspect_markdown_links(
            "README.md",
            "[trusted load](TECHNICAL_DESIGN.md#trusted-load)\n## Task map\n[task map](#task-map)\n",
        )
        self.assertEqual(valid_errors, [])
        self.assertEqual(valid_checked, 2)

        broken_errors, _, broken_checked = check_authority_docs.inspect_markdown_links(
            "README.md",
            "[missing](TECHNICAL_DESIGN.md#not-a-real-contract)\n[local](#not-a-real-section)\n",
        )
        self.assertEqual(broken_checked, 2)
        self.assertTrue(
            any(
                "TECHNICAL_DESIGN.md#not-a-real-contract" in error
                for error in broken_errors
            )
        )
        self.assertTrue(any("#not-a-real-section" in error for error in broken_errors))


class ExactTestCommandTests(unittest.TestCase):
    def test_omitted_target_is_resolved_build_free_at_execution_time(self) -> None:
        args = run_test.parse_args([ci.GAMEPLAY_TESTS["ore"]])
        self.assertIsNone(args.target)
        target, name = run_test.resolve_automatic_exact_selection(args.name, args.features)
        self.assertEqual(target, ci.GAMEPLAY_TARGETS["ore"])
        self.assertEqual(name, ci.GAMEPLAY_TESTS["ore"])

    def test_automatic_selection_prefers_the_smallest_duplicate_test_target(self) -> None:
        target, name = run_test.resolve_automatic_exact_selection(
            "process_catalog_contract_tests::every_authored_process_has_legible_physical_execution_topology",
            None,
        )
        self.assertEqual(target, ci.GAMEPLAY_CONTRACTS_TARGET)
        self.assertEqual(
            name,
            "process_catalog_contract_tests::every_authored_process_has_legible_physical_execution_topology",
        )
        self.assertLess(
            run_test.target_source_weight(target, None),
            run_test.target_source_weight(ci.GAMEPLAY_AUDIT_TARGET, None),
        )

    def test_automatic_selection_keeps_unit_tests_on_the_library_target(self) -> None:
        target, name = run_test.resolve_automatic_exact_selection(
            "absolute_tick_and_relative_span_add_without_wraparound",
            None,
        )
        self.assertEqual(target, "lib")
        self.assertEqual(
            name,
            "core::time::tests::absolute_tick_and_relative_span_add_without_wraparound",
        )

    def test_global_source_catalog_contains_focused_gameplay_without_target_hint(self) -> None:
        self.assertIn(ci.GAMEPLAY_TESTS["ore"], run_test.all_source_test_names(None))

    def test_automatic_suite_resolution_stays_on_one_complete_target(self) -> None:
        self.assertEqual(
            run_test.resolve_automatic_suite_target(
                "ore_processing::separation_execution::tests::",
                None,
            ),
            "lib",
        )

    def test_check_mode_requires_an_explicit_target(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                run_test.parse_args(["--check"])

    def test_source_cfg_evaluation_treats_test_as_enabled_and_expands_local_features(self) -> None:
        declared = {
            "default": ["base"],
            "base": [],
            "group": ["leaf", "dep:external", "external/feature"],
            "leaf": [],
        }
        features = run_test.expand_local_features(
            declared, {"group"}, include_default=True
        )
        self.assertEqual(features, {"default", "base", "group", "leaf"})
        self.assertTrue(
            run_test.attributes_enabled(
                ['#[cfg(any(test, feature = "missing"))]'], features
            )
        )
        self.assertFalse(
            run_test.attributes_enabled(
                ['#[cfg(any(feature = "test-soak", feature = "missing"))]'], set()
            )
        )
        self.assertFalse(
            run_test.attributes_enabled(['#[cfg(feature = "missing")]'], features)
        )
        self.assertTrue(
            run_test.attributes_enabled(
                ['#[cfg(all(test, feature = "leaf"))]'], features
            )
        )
        self.assertFalse(run_test.attributes_enabled(['#[cfg(not(test))]'], features))
        self.assertFalse(
            run_test.attributes_enabled(
                ['#[cfg(all(not(test), feature = "leaf"))]'], features
            )
        )
        with self.assertRaisesRegex(ValueError, "does not understand cfg predicate"):
            run_test.attributes_enabled(['#[cfg(target_os = "windows")]'], features)

    def test_unique_test_selector_resolves_to_one_exact_catalog_name(self) -> None:
        catalog = [
            "inventory::tests::transfer_preserves_mass",
            "mining::tests::mining_preserves_mass",
        ]
        self.assertEqual(
            run_test.resolve_test_name("transfer_preserves_mass", catalog),
            "inventory::tests::transfer_preserves_mass",
        )
        self.assertEqual(
            run_test.resolve_test_name("mining::tests::mining_preserves_mass", catalog),
            "mining::tests::mining_preserves_mass",
        )

    def test_ambiguous_test_selector_is_rejected_before_cargo(self) -> None:
        catalog = [
            "inventory::tests::preserves_mass",
            "mining::tests::preserves_mass",
        ]
        with self.assertRaisesRegex(ValueError, "ambiguous.*2 matches"):
            run_test.resolve_test_name("preserves_mass", catalog)

    def test_default_exact_command_reuses_shared_test_support_shape(self) -> None:
        args = argparse.Namespace(
            target="lib",
            features=None,
            list=False,
            name="module::tests::case",
            suite=False,
            ignored=False,
            nocapture=False,
        )
        self.assertEqual(
            run_test.cargo_command(args),
            [
                "cargo",
                "test",
                "--quiet",
                "--locked",
                "--lib",
                "module::tests::case",
                "--",
                "--exact",
            ],
        )

    def test_source_catalog_matches_default_library_test_names_without_building(self) -> None:
        catalog = run_test.source_test_catalog("lib", None)
        self.assertIn(
            "core::time::tests::absolute_tick_and_relative_span_add_without_wraparound",
            catalog,
        )
        self.assertIn(
            "core::time::tests::calendar_exposes_exact_physical_world_time_per_tick",
            catalog,
        )
        self.assertIn(
            "content::equipment::tests::primitive_copper_upgrades_improve_their_intended_nominal_capability",
            catalog,
        )
        self.assertIn(
            "thermal::processes::tests::validation::sensible_heating_rejects_heater_after_mounted_support_fails",
            catalog,
        )
        self.assertNotIn(
            "content::shaders::tests::built_in_programs_assemble_and_validate_as_portable_wgsl",
            catalog,
        )

    def test_source_catalog_does_not_resurrect_shader_binary_validation_as_a_unit_test(self) -> None:
        catalog = run_test.source_test_catalog("lib", "test-shader-validation")
        self.assertNotIn(
            "content::shaders::tests::built_in_programs_assemble_and_validate_as_portable_wgsl",
            catalog,
        )

    def test_source_catalog_resolves_gameplay_target_modules(self) -> None:
        contracts = run_test.source_test_catalog(ci.GAMEPLAY_CONTRACTS_TARGET, None)
        self.assertIn(
            "process_catalog_contract_tests::every_authored_process_has_legible_physical_execution_topology",
            contracts,
        )
        for scope, target in ci.GAMEPLAY_TARGETS.items():
            focused = run_test.source_test_catalog(target, None)
            self.assertIn(
                ci.GAMEPLAY_TESTS[scope],
                focused,
                f"focused gameplay scope {scope} must resolve in its dedicated target",
            )
        workshop = run_test.source_test_catalog(ci.GAMEPLAY_TARGETS["workshop"], None)
        self.assertNotIn("agency::gameplay_agency_counterfactuals", workshop)
        self.assertNotIn("scenario_tests::world_seed_never_changes_player_policy", workshop)
        audit = run_test.source_test_catalog(ci.GAMEPLAY_AUDIT_TARGET, None)
        self.assertIn("agency::gameplay_agency_counterfactuals", audit)
        self.assertIn("scenario_tests::world_seed_never_changes_player_policy", audit)
        self.assertTrue(
            all("gameplay_report" not in run_test.source_test_catalog(target, None) for target in ci.GAMEPLAY_AUDIT_TARGETS)
        )

    def test_source_catalog_listing_never_builds_through_cargo_command(self) -> None:
        args = argparse.Namespace(
            target="lib",
            features=None,
            list=True,
            name="survival",
            suite=False,
            ignored=False,
            nocapture=False,
        )
        with self.assertRaisesRegex(ValueError, "does not invoke Cargo"):
            run_test.cargo_command(args)

    def test_suite_command_runs_one_catalog_group_without_exact_filtering(self) -> None:
        args = argparse.Namespace(
            target="lib",
            features=None,
            list=False,
            name="ore_processing::separation_execution::tests::",
            suite=True,
            ignored=False,
            nocapture=False,
        )
        command = run_test.cargo_command(args)
        self.assertNotIn("--features", command)
        self.assertIn(args.name, command)
        self.assertNotIn("--exact", command)

    def test_suite_result_counts_come_from_cargo_execution_not_source_matches(self) -> None:
        output = "test result: ok. 19 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out"
        self.assertEqual(run_test.executed_test_counts(output), (19, 2))


if __name__ == "__main__":
    unittest.main(verbosity=1)
