#!/usr/bin/env python3
"""Fast contract tests for the local CI plan; never invoke Cargo builds from this file."""

from __future__ import annotations

import argparse
import contextlib
import io
from pathlib import Path
import re
import sys
import tempfile
import tomllib
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import ci  # noqa: E402
from tools import (  # noqa: E402
    check_authority_docs,
    check_bca,
    check_format,
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
        if re.search(r"\bDeserialize\b", attributes):
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

    def test_targeted_lint_infers_required_features_without_widening_targets(self) -> None:
        args = run_test.parse_args(
            ["--lint", "--target", ci.GAMEPLAY_TARGETS["fieldwork"]]
        )
        self.assertEqual(
            run_test.cargo_lint_command(args),
            [
                "cargo",
                "clippy",
                "--quiet",
                "--locked",
                "--profile",
                "test",
                "--test",
                ci.GAMEPLAY_TARGETS["fieldwork"],
                "--features",
                "test-gameplay",
                "--no-deps",
                "--",
                "-D",
                "warnings",
            ],
        )
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            run_test.parse_args(["--lint"])

    def test_targeted_lint_rejects_the_slow_library_test_graph(self) -> None:
        args = run_test.parse_args(["--lint", "--target", "lib"])
        with self.assertRaisesRegex(ValueError, "cargo lint-fast"):
            run_test.cargo_lint_command(args)

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
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            rust_diagnostics.parse_args(
                [
                    "mutants",
                    "src/survival/validation/direct_consumption.rs",
                    "--re",
                    "validate_pending_food_freshness",
                    "--run",
                    "--jobs",
                    "4",
                ]
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

    def test_rust_diagnostics_mutant_execution_fails_closed_when_lock_is_busy(self) -> None:
        lock_path = ROOT / "target" / "agent-output" / "rust-diagnostics" / "test-mutants.lock"
        try:
            with rust_diagnostics.mutation_execution_lock(lock_path):
                with self.assertRaises(rust_diagnostics.MutationRunBusyError):
                    with rust_diagnostics.mutation_execution_lock(lock_path):
                        self.fail("busy mutation lock must not enter the protected region")
        finally:
            lock_path.unlink(missing_ok=True)

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

    def test_quick_lane_formats_only_changed_rust(self) -> None:
        self.assertIn(
            ("format changed Rust", [sys.executable, "tools/check_format.py"]),
            ci.quick_plan(),
        )
        self.assertNotIn(("format", ["cargo", "fmt", "--check"]), ci.quick_plan())

    def test_changed_format_command_is_explicit_and_does_not_walk_child_modules(self) -> None:
        paths = [ROOT / "src" / "lib.rs", ROOT / "tests" / "gameplay_survival.rs"]
        command = check_format.changed_format_command(paths)
        self.assertEqual(command[:4], ["rustfmt", "--edition", "2024", "--check"])
        self.assertIn(["--color", "never"], [command[index:index + 2] for index in range(len(command) - 1)])
        self.assertIn("skip_children=true", command)
        self.assertEqual(
            command[-2:],
            [str(Path("src/lib.rs")), str(Path("tests/gameplay_survival.rs"))],
        )

    def test_formatter_policy_change_escalates_to_full_format_validation(self) -> None:
        self.assertTrue(check_format.formatting_policy_changed(["rustfmt.toml"]))
        self.assertTrue(check_format.formatting_policy_changed([".rustfmt.toml"]))
        self.assertFalse(
            check_format.formatting_policy_changed(["Cargo.toml", "src/lib.rs"])
        )

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
            and path.name != "mod_tests.rs"
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
            ROOT / "tests" / "gameplay_harness" / "fresh_seed.rs",
        }
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in maintained_rust_files(
                ROOT / "src", ROOT / "tests" / "gameplay_harness"
            )
            if path not in allowed
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
        structures = [
            (path, line, name, attributes)
            for path in maintained_rust_files(ROOT / "src")
            for line, name, attributes, _fields in deserialized_named_structs(path)
        ]
        self.assertGreater(
            len(structures),
            0,
            "Deserialize scanner must find maintained state before enforcing serde policy",
        )
        offenders = [
            f"{path.relative_to(ROOT).as_posix()}:{line}:{name}"
            for path, line, name, attributes in structures
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

    def test_local_test_profile_keeps_fast_relink_settings_explicit(self) -> None:
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        profile = manifest["profile"]["test"]
        self.assertEqual(profile.get("debug"), 0)
        self.assertGreaterEqual(profile.get("codegen-units", 0), 128)
        self.assertIs(profile.get("incremental"), True)

        cargo_config = tomllib.loads(
            (ROOT / ".cargo" / "config.toml").read_text(encoding="utf-8")
        )
        self.assertEqual(
            cargo_config["target"]["x86_64-pc-windows-msvc"].get("linker"),
            "lld-link.exe",
        )

    def test_gameplay_report_examples_are_executable_only(self) -> None:
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        examples = {
            definition["name"]: definition for definition in manifest.get("example", [])
        }
        report_examples = {ci.GAMEPLAY_REPORT_EXAMPLE, *ci.FOCUSED_REPORT_EXAMPLES.values()}
        self.assertTrue(report_examples)
        for name in report_examples:
            self.assertIs(examples[name].get("test"), False)

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

    def test_lint_gate_reuses_the_fast_production_alias(self) -> None:
        plan = ci.plan_for(gate_args(lint=True))
        self.assertEqual(plan, [("clippy", ["cargo", "lint-fast"])])
        command = plan[0][1]
        self.assertNotIn("--test", command)
        self.assertNotIn("--example", command)
        self.assertNotIn("test-gameplay", command)

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
            "woodworking": "woodworking_probe",
            "fieldwork": "fieldwork_probe",
            "power-provider": "power_provider_probe",
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
        fresh_seed = (ROOT / "tests" / "gameplay_harness" / "fresh_seed.rs").resolve()
        for target in (*ci.GAMEPLAY_TARGETS.values(), ci.GAMEPLAY_AUDIT_TARGET):
            root = run_test.cargo_test_target_path(target)
            features = run_test.cargo_feature_set(target, None)
            reachable = {
                path.resolve()
                for path, _prefix in run_test.test_catalog.reachable_modules(
                    ROOT, root, features
                )
            }
            self.assertNotIn(
                fresh_seed,
                reachable,
                f"routine gameplay target {target} must leave fresh organic sampling to the report",
            )
        report = ROOT / "tests" / "gameplay_report.rs"
        self.assertIn("gameplay_harness/fresh_seed.rs", report.read_text(encoding="utf-8"))

    def test_each_focused_gameplay_target_compiles_only_owner_local_tests(self) -> None:
        probe_modules = {
            "workshop": "workshop",
            "survival": "survival_probe",
            "progression": "progression_probe",
            "woodworking": "woodworking_probe",
            "fieldwork": "fieldwork_probe",
            "power-provider": "power_provider_probe",
            "ore": "ore_probe",
            "foundry": "foundry_probe",
        }
        for scope, target in ci.GAMEPLAY_TARGETS.items():
            tests = run_test.source_test_catalog(target, None)
            gate = ci.GAMEPLAY_TESTS[scope]
            self.assertIn(gate, tests)
            allowed_root_tests = {gate}
            if report_test := ci.FOCUSED_REPORT_TESTS.get(scope):
                allowed_root_tests.add(report_test)
            unrelated = [
                name
                for name in tests
                if name not in allowed_root_tests
                and not name.startswith(f"{probe_modules[scope]}::")
            ]
            self.assertEqual(
                unrelated,
                [],
                f"focused gameplay target {scope} must not compile unrelated tests",
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

    def test_all_audit_reuses_separate_core_and_gameplay_cache_shapes(self) -> None:
        plan = ci.audit_plan("all")
        builds = cargo_build_commands(plan)
        self.assertEqual(
            builds,
            [
                ["cargo", "test-core"],
                ci.gameplay_command("all"),
            ],
        )
        self.assertFalse(any("check-fast" in command for command in builds))
        self.assertFalse(any(stage in ci.quick_plan() for stage in plan))
        self.assertNotIn("test-gameplay", builds[0])
        self.assertIn("test-gameplay", builds[1])
        self.assertEqual(cargo_test_targets(builds[1]), [ci.GAMEPLAY_AUDIT_TARGET])

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

    def test_failed_stage_prints_one_narrow_action_when_repair_is_known(self) -> None:
        command = ci.gameplay_command("ore")
        output = "failures:\n    gameplay_ore_preparation_probe\n"
        error = "error: test failed, to rerun pass `--test gameplay_ore`"
        result = ci.subprocess.CompletedProcess(command, 1, output, error)
        with (
            contextlib.redirect_stdout(io.StringIO()) as stdout,
            contextlib.redirect_stderr(io.StringIO()) as stderr,
        ):
            self.assertIsNone(
                ci.report_stage(
                    1,
                    1,
                    "gameplay ore",
                    command,
                    (result, 0.25, None),
                    announced=True,
                )
            )
        self.assertEqual(stdout.getvalue(), "FAIL (0.2s)\n")
        self.assertIn(
            "repair: python tools/run_test.py --target gameplay_ore gameplay_ore_preparation_probe\n",
            stderr.getvalue(),
        )
        self.assertNotIn("reproduce:", stderr.getvalue())

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
                "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "0x000000000000AAAA",
                "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED": "0x000000000000BBBB",
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

    def test_library_check_type_checks_unit_test_code_without_linking(self) -> None:
        args = run_test.parse_args(["--check", "--target", "lib"])
        self.assertEqual(run_test.enabled_integration_test_targets(None), [])
        self.assertEqual(
            run_test.cargo_check_command(args),
            [
                "cargo",
                "check",
                "--quiet",
                "--locked",
                "--profile",
                "test",
                "--lib",
                "--tests",
            ],
        )

    def test_library_check_fails_closed_if_features_enable_integration_tests(self) -> None:
        args = run_test.parse_args(
            ["--check", "--target", "lib", "--features", "test-gameplay"]
        )
        with self.assertRaisesRegex(ValueError, ci.GAMEPLAY_CONTRACTS_TARGET):
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
                "--profile",
                "test",
                "--test",
                ci.GAMEPLAY_CONTRACTS_TARGET,
                "--features",
                "test-gameplay",
            ],
        )

    def test_focused_report_uses_the_smallest_declared_report_target(self) -> None:
        for scope in (*ci.GAMEPLAY_TESTS, "agency"):
            with self.subTest(scope=scope):
                plan = ci.report_plan(scope)
                self.assertEqual(plan[0][0], f"gameplay report {scope}")
                report_test = ci.FOCUSED_REPORT_TESTS.get(scope)
                if report_test is not None:
                    target = ci.GAMEPLAY_TARGETS[scope]
                    self.assertIn(report_test, run_test.source_test_catalog(target, None))
                    self.assertEqual(
                        plan[0][1],
                        ci.gameplay_targets_command(
                            (target,),
                            test_filter=report_test,
                            nocapture=True,
                            ignored=True,
                        ),
                    )
                else:
                    dedicated = ci.FOCUSED_REPORT_EXAMPLES.get(scope)
                    self.assertIsNotNone(dedicated)
                    self.assertEqual(
                        plan[0][1],
                        ci.gameplay_report_example_command(
                            dedicated,
                            ci.FOCUSED_REPORT_ARGUMENTS.get(scope, ()),
                        ),
                    )

        args = ci.parse_args(["report", "--scope", "fieldwork"])
        self.assertEqual(args.scope, "fieldwork")
        self.assertEqual(ci.plan_for(args), ci.report_plan("fieldwork"))
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            ci.parse_args(["quick", "--scope", "fieldwork"])

    def test_report_replay_seed_cli_validates_and_normalizes_before_building(self) -> None:
        args = ci.parse_args(
            [
                "report",
                "--scope",
                "woodworking",
                "--variation-seed",
                "42",
                "--behavior-seed",
                "0x2a",
            ]
        )
        self.assertEqual(args.variation_seed, "0x000000000000002A")
        self.assertEqual(args.behavior_seed, "0x000000000000002A")

        invalid = ("-1", "0x", "0xGG", "1_000", "18446744073709551616")
        for seed in invalid:
            with self.subTest(seed=seed):
                with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                    ci.parse_args(["report", "--variation-seed", seed])

    def test_report_rejects_behavior_seed_for_scope_that_does_not_use_policy_variation(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            ci.parse_args(
                ["report", "--scope", "fieldwork", "--behavior-seed", "0x1234"]
            )

    def test_report_seed_flags_are_report_only(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            ci.parse_args(["quick", "--variation-seed", "0x1234"])

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

    def test_check_mode_accepts_selector_for_build_free_target_resolution(self) -> None:
        args = run_test.parse_args(["--check", "gameplay_fieldwork_probe"])
        self.assertTrue(args.check)
        self.assertIsNone(args.target)
        self.assertTrue(run_test.resolve_automatic_validation_target(args))
        self.assertEqual(args.target, ci.GAMEPLAY_TARGETS["fieldwork"])

    def test_lint_mode_accepts_selector_for_build_free_target_resolution(self) -> None:
        args = run_test.parse_args(["--lint", "gameplay_fieldwork_probe"])
        self.assertTrue(args.lint)
        self.assertIsNone(args.target)
        self.assertTrue(run_test.resolve_automatic_validation_target(args))
        self.assertEqual(args.target, ci.GAMEPLAY_TARGETS["fieldwork"])

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

    def test_report_replay_environment_prefers_cli_roots_and_only_fills_missing_values(self) -> None:
        fresh = ci.parse_args(["report", "--scope", "woodworking"])
        generated: dict[str, str] = {}
        fresh_rolls = iter((0x1234, 0x5678))
        self.assertEqual(
            ci.configure_report_replay_environment(
                fresh, generated, randbits=lambda _bits: next(fresh_rolls)
            ),
            ("0x0000000000001234", "0x0000000000005678"),
        )

        args = ci.parse_args(
            ["report", "--scope", "woodworking", "--variation-seed", "0x2A"]
        )
        environment = {
            "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "0xOLD",
        }
        rolls = iter((0x55,))
        self.assertEqual(
            ci.configure_report_replay_environment(
                args, environment, randbits=lambda _bits: next(rolls)
            ),
            ("0x000000000000002A", "0x0000000000000055"),
        )
        self.assertEqual(
            environment["DEEP_HEARTH_GAMEPLAY_VARIATION_SEED"],
            "0x000000000000002A",
        )

        explicit = ci.parse_args(
            [
                "report",
                "--scope",
                "woodworking",
                "--variation-seed",
                "1",
                "--behavior-seed",
                "2",
            ]
        )
        explicit_environment = {
            "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "0xAAAA",
            "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED": "0xBBBB",
        }
        self.assertEqual(
            ci.configure_report_replay_environment(
                explicit,
                explicit_environment,
                randbits=lambda _bits: self.fail("explicit CLI roots must not consume entropy"),
            ),
            ("0x0000000000000001", "0x0000000000000002"),
        )

        ambient = ci.parse_args(["report", "--scope", "woodworking"])
        ambient_environment = {
            "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "42",
            "DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED": "0x2a",
        }
        self.assertEqual(
            ci.configure_report_replay_environment(
                ambient,
                ambient_environment,
                randbits=lambda _bits: self.fail("valid ambient roots must not consume entropy"),
            ),
            ("0x000000000000002A", "0x000000000000002A"),
        )

        invalid_environment = {"DEEP_HEARTH_GAMEPLAY_VARIATION_SEED": "not-a-seed"}
        with self.assertRaisesRegex(ValueError, "DEEP_HEARTH_GAMEPLAY_VARIATION_SEED"):
            ci.configure_report_replay_environment(
                ambient,
                invalid_environment,
                randbits=lambda _bits: self.fail("invalid ambient input must fail before entropy"),
            )

        fieldwork = ci.parse_args(["report", "--scope", "fieldwork"])
        fieldwork_environment = {"DEEP_HEARTH_GAMEPLAY_BEHAVIOR_SEED": "ignored-garbage"}
        self.assertEqual(
            ci.configure_report_replay_environment(
                fieldwork,
                fieldwork_environment,
                randbits=lambda _bits: 0x99,
            ),
            ("0x0000000000000099", "unused"),
        )

    def test_fresh_gameplay_variation_is_report_only(self) -> None:
        self.assertTrue(ci.uses_fresh_gameplay_variation(ci.parse_args(["report"])))
        for argv in (
            ["quick"],
            ["gate"],
            ["gate", "--gameplay", "contracts"],
            ["gate", "--gameplay", "survival"],
            ["gate", "--gameplay", "progression"],
            ["gate", "--gameplay", "workshop"],
            ["audit", "--core"],
            ["audit", "--gameplay"],
            ["audit", "--all"],
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
            ci.gameplay_environment_summary("gameplay", environment),
            "roots=0xAAAA/0xBBBB",
        )
        self.assertIsNone(ci.gameplay_environment_summary("core", environment))
        self.assertIsNone(ci.gameplay_environment_summary("gameplay contracts", environment))
        self.assertIsNone(ci.gameplay_environment_summary("compile", environment))

    def test_report_cli_preserves_large_success_evidence_but_bounds_failures(self) -> None:
        opening = "PLAYER FANTASY scope=current-ordinary fixture=opening"
        replay = (
            "PROBE INPUT name=transport-fixture mode=explore samples=2 organic=1 "
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
                        body = "\n".join(stdout.getvalue().splitlines()[1:]) + "\n"
                        if mode is not None:
                            self.assertEqual(body, transcript)
                        else:
                            self.assertEqual(body, "\n")
                            self.assertNotIn(opening, stdout.getvalue())
                            self.assertNotIn(ending, stdout.getvalue())
                        self.assertNotIn("local-ci report:", stdout.getvalue())
                        self.assertNotIn("PASS total", stdout.getvalue())
                    else:
                        self.assertNotIn(opening, stdout.getvalue())
                        bounded = "\n".join([
                            *lines[:ci.FAILURE_HEAD_LINES],
                            f"... {len(lines) - ci.FAILURE_HEAD_LINES - ci.FAILURE_TAIL_LINES} line(s) omitted ...",
                            *lines[-ci.FAILURE_TAIL_LINES:],
                        ])
                        self.assertEqual(
                            stderr.getvalue(),
                            "repair: python ci.py report --variation-seed 0x111 "
                            f"--behavior-seed 0x222\n{bounded}\n{bounded}\n",
                        )
                        self.assertNotIn("PASS total", stdout.getvalue())

    def test_survival_summary_counts_selected_preservation_policy_only(self) -> None:
        lines = [
            "SURVIVAL EXPERIENCE seed=0x1 pressure=hydration choice=[state:policy-sensitive diet:balanced-recovery meal:1000000mg drink:1250000uL] raw-opportunity=[origin:1 mode:scarce-timber] storage-policy:decline commitment:none commitment-reason:return-does-not-clear-threshold minimum-return:3000000ppm best-enclosure-counterfactual=[policy:enclosure-singleton candidates:1 build:150t] work-interlock=[integrated=[hydration-policy:task-floor drink:6000uL/1t prospect:24t opportunity-power:true reprovision:true:800uL/1t power:3t stored:100nJ final-reserve:900000ppmE/750000ppmH warning-safe:true]]",
            "SURVIVAL EXPERIENCE seed=0x2 pressure=energy choice=[state:policy-sensitive diet:compact-calories meal:500000mg drink:0uL] raw-opportunity=[origin:2 mode:choice-rich-timber] storage-policy:attention-efficient commitment:1 commitment-reason:return-clears-threshold minimum-return:1000000ppm best-enclosure-counterfactual=[policy:attention-efficient candidates:4 build:120t] work-interlock=[integrated=[hydration-policy:working-reserve drink:12000uL/1t prospect:48t opportunity-power:false reprovision:false:0uL/0t power:0t stored:0nJ final-reserve:950000ppmE/875000ppmH warning-safe:true]]",
            "SURVIVAL REVIEW seed=0x1 diet-evidence=[matched-counterfactual=[horizon:130t] tradeoff=[meal-mass-delta:+200000mg water-saved-delta:+0uL diet-quality-delta:+80000ppm recovery-delta:+1ppm/t] recovery-consequence=[choice:actionable deprivation:33000t provisioning-horizon:130t observe:1000t vitality:950000->[compact:953000 balanced:959000 delta:+6000ppm]]]",
        ]
        summary = "\n".join(gameplay_report_summary.ordinary_gameplay_summary(lines))
        self.assertIn("declined:1", summary)
        self.assertIn("efficient:1", summary)
        self.assertIn(
            "preservation-opportunity=[scarce:1 choice-rich:1 alternate:0 singleton:1 multi:1]",
            summary,
        )
        self.assertIn(
            "provisioning=[meal:500..1000g drink:0..1250mL] "
            "balanced-diet-counterfactual=[meal-extra:200..200g diet-quality-gain:80000..80000ppm vitality-gain:6000..6000ppm]",
            summary,
        )
        self.assertIn("commitment=[cleared:1 declined-return:1]", summary)
        self.assertIn(
            "work-interlock=[policy=[task-floor:1 working-reserve:1] opportunity-power:1 "
            "follow-up-needed:1 single-provision-sufficient:1/2 initial-drink:6..12mL "
            "follow-up-drink:0..0.8mL prospect:24..48t power:0..3t "
            "final-hydration:750000..875000ppm warning-safe:2/2]",
            summary,
        )

    def test_fieldwork_summary_separates_world_constraints_from_selected_tool(self) -> None:
        lines = [
            "FIELDWORK EXPERIENCE seed=0x1 sample=anchor outcome=completed order-horizon=short field-inspections=1 geology=quarry-soft full-order-tool=copper-reinforced-hard-pick tool=stone-quarry copper-opportunity=absent requested=100mg planned-local-work=100mg mining=100mg resource-knowledge-effect=same-tool",
            "FIELDWORK EXPERIENCE seed=0x2 sample=coverage outcome=known-target-supply order-horizon=project field-inspections=3 geology=quarry-reinforcement full-order-tool=stone-pick tool=copper-reinforced-quarry copper-opportunity=available requested=200mg planned-local-work=80mg mining=50mg resource-knowledge-effect=changed-tool",
            "FIELDWORK EXPERIENCE seed=0x3 sample=organic outcome=completed order-horizon=project field-inspections=2 geology=hard-pick-specialist full-order-tool=stone-quarry tool=copper-reinforced-hard-pick copper-opportunity=available requested=300mg planned-local-work=300mg mining=300mg resource-knowledge-effect=same-tool",
            "FIELDWORK INITIAL SHORTFALL RECOVERY seed=0x2 initial-supply-ended=true reroute-proved=true evidence=executed-multi-site-from-partial-extraction-state post-shortfall-execution=true mining-tool-reused=false survey-base-kit-reused=true strategy=indexed-channel survey-upgrade=40t projected-search=[point:234t indexed:202t] realized=[baseline-search:228t selected-search:162t upgrade:40t attention-delta:+26t total-attention-delta:+16t] adaptation=[hardness-tier-changes:2 tool-builds:1 tool-switches:1 blocked-sites:1 tool-preparation:10t ore-recovery-events:1 ore-recovery-required-access:0 ore-recovery-payback:1 ore-recovery:8t ore-feed:30mg native-recovered:20mg baseline-fulfilled:140mg fulfillment-delta:+10mg] sites-visited=3 search=162t/9.7m extraction=12t/43.2s initial-extracted=50mg additional-extracted=100mg fulfilled=150mg requested=200mg fulfillment=750000ppm remaining=50mg terminal=local-search-area-exhausted",
        ]
        summary = "\n".join(gameplay_report_summary.ordinary_gameplay_summary(lines))
        self.assertIn("sample-shape=[anchor:1 coverage:1 organic:1 replay:0]", summary)
        self.assertIn(
            "outcomes=[completed:2 local-supply-ended:1 fulfillment:250000..1000000ppm]",
            summary,
        )
        self.assertIn("organic-outcomes=[completed:1 local-supply-ended:0]", summary)
        self.assertIn(
            "reserve-knowledge=[workload-capped:1 tool-changed:1]",
            summary,
        )
        self.assertIn(
            "organic-reserve-knowledge=[workload-capped:0 tool-changed:0]",
            summary,
        )
        self.assertIn("orders=[short:1 project:2 bulk:0]", summary)
        self.assertIn(
            "initial-shortfall-campaign=[cases:1 strategy:point0/indexed1 survey-upgrade:40..40t "
            "realized-search=[positive:1 negative:0 flat:0 delta:+26..+26t] "
            "realized-total=[positive:1 negative:0 flat:0 delta:+16..+16t] "
            "adaptation=[geology-changed:1/1 retooled:1/1 salvaged:0/1 ore-funded:1/1(payback:1/access:0) "
            "blocked-sites:1..1 fulfillment-delta:+10..+10mg] "
            "completed:0 local-area-exhausted:1 sites:3..3 "
            "fulfillment:750000..750000ppm remaining:50..50mg]",
            summary,
        )
        self.assertIn(
            "geology=[soft:1 reinforcement:1 hard-specialist:1]", summary
        )
        self.assertIn("copper=[available:2 absent:1]", summary)
        self.assertIn(
            "tools=[stone-pick:0 soft-quarry:1 reinforced-quarry:1 hard-pick:1]",
            summary,
        )
        self.assertIn(
            "geology-tool=[soft:pick0/quarry1/reinforced0/hard0 "
            "reinforcement:pick0/quarry0/reinforced1/hard0 "
            "hard-specialist:pick0/quarry0/reinforced0/hard1]",
            summary,
        )

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
            "PROGRESSION EXPERIENCE seed=0x1 sample=anchor information=surface-resolved local-copper-sequence=pick-first scarcity=[direct-second-upgrade-blocked:true processed-output-playable:true converged-both-upgrades:true] processing-investment=[selected:mechanized preaction-manual:2470t conservative-machine-upper:1800t assembly:320t initial-charge:18t repeated-charge-upper:1400t choice-frozen-before-action:true] counterfactual=[crank-first-tradeoff hard-access-lead:478t] bridge-tradeoff=[manual-second:111t feed:41465mg recovery:650000ppm body:1000000000000nJ/1000uL; powered-line:421t feed:29947mg recovery:900000ppm body:2000000000000nJ/2000uL] disclosed-order-economics=[cycles:12 manual-player-attention:2470t mechanized-player-attention:429t saved:2041t] stockpiling-coverage-delegation=[feed-attention:28t maintenance-prep-overlap:40t productive-attention:68t returned-attention:315t returned:822454ppm overlap/setup:210526ppm unrecovered-setup:255t overlap-equivalent:unreached post-equivalent:0cycles stop:stockpile-order-complete economics:finite-stockpile-order-complete] selected-reinvestment=[completed]",
            "PROGRESSION FALLBACK seed=0x1 evidence=maintained-route-regression route=owned-ore->hand-break->hand-sort->cold-work captured:true input=[ore:88163mg copper:441278ppm] attention=[break:98t sort:49t cold-work:40t total:187t] matter=[native:25287mg residue:62876mg reinforcement:20000mg remainder:5287mg] recovery=[manual:650000ppm powered:900000ppm] survival-cost=[237021000000000nJ 66695uL] machinery=none stored-work=none matter=conserved",
            "PROGRESSION REVIEW seed=0x1 sample=anchor role=runtime-experience-after-disclosed-bootstrap continuity=single-state captured:true coverage-autonomy=[repeat-horizon:12/24cycles stop:stockpile-order-complete] selected-reinvestment=[completed copper-invested:60000mg next-stage=[sizing-plate-continuation:90t]] stored-work=[passive-loss:125000000000nJ reserve-recharge:1t]",
            "PROGRESSION GOAL seed=0x1 immediate=265t delayed=741t chosen=immediate",
            "LIBERATION COST seed=0x1 scavenger-marginal=[attention:17t native:6mg]",
            "LIBERATION KIT ACQUISITION seed=0x1 scope=raw-stone+logs->adze+reusable-base-processing-kit raw-origin=pre-admission-fixture pickup=same-voxel-runtime carried-custody=finite@voxel world-gathering-proved=false disclosed-campaign=8batches workload-known-before-build=true raw=[stone:8000000mg wood:15400000mg total:23400000mg] built=[adze:true crusher:true quern:true timber-riddle:true separator:true treadle:true paired-flywheel:true] attention:404t body=500000000000000nJ/100000uL copper-screen-upgrade=proved-by-progression-continuation matter=conserved",
            "LIBERATION ROUTE TRADEOFF seed=0x1 basis=matched-ore-mass feed=100mg manual=[attention:60t native:30mg recovery:650000ppm body:1nJ/1uL] powered=[elapsed:20t charge-attention:5t native:45mg] campaign=[planned:8batches executed:8 kit-payback:8batches attention:manual:480t/powered:444t body:manual:8nJ/8uL powered:500000000000008nJ/100008uL elapsed:160t final-condition=[crusher:970000 quern:850000 screen:981200 separator:971800 treadle:999040] justified:true] sizing=timber-riddle copper-input=none next-screen-upgrade=proved-by-progression-continuation base-kit=[executed attention:404t body:500000000000000nJ/100000uL] continuity=live-kit-used",
            "LIBERATION FRONTIER CAPABILITY seed=0x1 cleanup-executed=true reason=required-native-copper-conversion input=[100mg] concentrate=[first:70mg/700000ppm final:75mg/750000ppm] copper-in-concentrate=[first:49mg final:56mg scavenger-recovered:7mg] native-copper=50mg matter=conserved",
            "FIRST FOUNDRY EXPERIENCE seed=0x1 sample=anchor scope=ordinary-copper-recovery-coverage upstream=primitive-liberation-capability-proved state-continuity=separate-disclosed-opportunity raw-opportunity=[stone:12000000mg wood:12000000mg native:160000mg scrap:20000mg] build-choice=[order:20000mg direct-native:40t reinforcement:20000mg fulfillment:1000000ppm selection:direct-native foundry-deferred:true reason=current-order-does-not-repay-setup] scarcity-choice=[order:20000mg source:scrap-only cold-rework:18000mg/900000ppm shortfall:2000mg foundry:20000mg/1000000ppm selection:foundry reason:cold-rework-underfills-order] fabrication=800t/48.0m dynamo-path=treadle-additive-upgrade electrical-charge=[35t 12300000000000nJ body:100nJ/20uL] melt=[35t 2.1m feed:scrap] cast=[18t 1.1m heat:12300000000000nJ] downstream=[ingot:20000mg reinforcement:20000mg cold-work:45t] installed-recovery=[cold-rework:50t reinforcement:18000mg chips:2000mg fulfillment:900000ppm foundry-active:80t reinforcement:20000mg chips:0mg fulfillment:1000000ppm useful-gain:+2000mg attention-delta:+30t] total=898t/53.9m survival=[energy:1000nJ hydration:200uL] matter=conserved continuation=full-scrap-recovery",
            "LIBERATION FRONTIER seed=0x1 remaining-frontier=industrial-foundry-scale industrial-foundry-frontier=[assembly-edge=[furnace:false mold:false electrical-buffer:false thermal-sink:false] manual-electrical-generation:true support-required=[furnace:true mold:true] energy-scale=[manual-electrical-max:100000000uW industrial-furnace-transfer-ceiling:2000000000000uW ceiling-ratio:20000x melting-carrier:Electrical conversion-path:present]] reachability-authority=STATUS.md",
            "WOODWORKING EXPERIENCE seed=0x1 sample=anchor demand-horizon=immediate-only choice=bare-hands reason=bare-hands-avoids-investment-cost",
            "WOODWORKING FEEDBACK seed=0x1 basis=executed-lifecycle-versus-pre-action-policy-model attention=[setup-budget-met:false actual-payback:false] timber=[nominal:costlier actual:costlier] selected=bare-hands choice-revised-after-outcome=false",
            "FIELDWORK EXPERIENCE seed=0x1 sample=anchor outcome=completed order-horizon=short field-inspections=1 detailed-surveys=1 observed-hardness=1..2Pa observed-resource-mass=0..1mg planned-local-work=1mg geology=quarry-soft tool=stone-quarry adaptation=preparation-plus-order copper-opportunity=absent retained-native-copper=1mg requested=1mg mining=1mg resource-knowledge-effect=changed-tool",
            "FIELDWORK CONTINUATION seed=0x1 available=true reused-knowledge=true reused-tool=true requested=1mg extracted=1mg extraction=2t/7.2s avoided-search=10t/36.0s avoided-kit=50t/3.0m stop=order-complete scope=matched-repeat-order destination-capacity=diagnostic-only",
            "FIELDWORK DEPLETION seed=0x1 eligible=true repeat-orders=[complete:2 partial:1 horizon:12] extracted=5mg attention=6t/21.6s supply-ended=true terminal=short-claim condition-after=990000ppm body=[energy:1000000000000nJ hydration:1000uL] scope=matched-orders-on-known-site no-search=true no-new-tool=true diagnostic-only=true",
            "FIELDWORK DEPLETION RECOVERY seed=0x1 depletion-observed=true reroute-proved=true evidence=executed-from-depleted-state post-depletion-execution=true mining-tool-reused=false selected-tool=copper-reinforced-hard-pick retool=10t ore-recovery=[reason:payback ticks:8 feed:30mg native:20mg] survey-base-kit-reused=true strategy=point-search survey-upgrade=0t search=10t/36.0s extraction=3t/10.8s extracted=1mg stop=order-complete",
            "FIELDWORK SITE REUSE seed=0x1 available=true kit-reused=true knowledge-reused=false strategy=indexed-channel search=10t/36.0s first-expedition-kit=50t/3.0m scope=new-site-search-only extraction-evaluated-by-lived-reroute=true upgrade-cost-reported-separately=true",
            "FIELDWORK SURVEY CAMPAIGN seed=0x1 planned-sites=3 upgrade-available=true selected=indexed-channel policy=min-expected-search-attention-with-minimum-return minimum-return=100000ppm projected=[point:30t indexed:26t] realized=[baseline-search:34t selected-search:22t upgrade:4t attention-delta:+8t] execution=search-only extraction-owned-by-lived-reroute=true choice-frozen-before-branch=true",
            "FIELDWORK TOOL MARKET phase=acquired-evidence selected=stone-pick selected-total=24t heavy-best=stone-quarry heavy-total=37t heavy-preparation-extra=+20t heavy-order-saving=+7t heavy-total-delta=+13t heavy-investment=deferred",
            "FIELDWORK BULK CROSSOVER seed=0x1 available=true tool=stone-quarry order=16000000mg base-batches=32 current-order=1000000mg scope=diagnostic-visible-state no-hidden-reserve=true",
            "FIELDWORK PACING seed=0x1 search=10t/36.0s sampling-tool=20t/72.0s extraction-tool=30t/108.0s extraction=4t/14.4s batches=1 first-ore=64t/3.8m episode-end=64t/3.8m output=1mg outcome=completed requested=1mg scope=raw-tools-and-preowned-copper-to-first-ore repeat-extraction-excludes-discovery=true output-grade=500000ppm",
            "POWER PROJECT EXPERIENCE seed=0x1 sample=anchor era=primitive selected=crank declared=[work:1000000000000nJ pristine-charge-events:1 consumer-projected-charge-events:1 consumer-projected-services:1 project-cache=[food:8000000mg preservation:4000000ppm water:256000000uL]] executed=[charge-events:2 survival-limited-batches:1 active-attention:20t provider-attention:5t consumer-runtime:9t maintenance=[services:1 preparation:4t service:3t replacement:1000mg] provisioning=[stops:1 attention:8t drinks:1 volume:10000uL meals:0 mass:0mg] elapsed:29t reserves=[start:101nJ/101uL end:1nJ/1uL]] condition=[provider:990000ppm consumer:900000ppm] full-counterfactual=[crank-active-attention:20t treadle-active-attention:21t attention-best:crank selected-agrees:true] evidence=complete-selected-project-canonical",
            "POWER PROVIDER EXPERIENCE seed=0x1 sample=anchor workload-source=declared-consumer-project project=[consumer:stone-crusher feed:1000000mg work:1000000000000nJ buffer-lower-bound-charges:1 consumer-projected-charges:1 projected-services:1] buffer:1000000000000nJ decision=[selected:crank policy=minimize-workload-attention-then-metabolic-then-hydration-then-material] crank=[first-charge:2t second-charge:3t] treadle=[first-charge:1t second-charge:2t] productive-cycle=[consumer:stone-crusher crank:9t treadle:9t] projected-provider-lifecycle=[crank:body:10000000000000nJ/20000uL condition:990000ppm treadle:body:8000000000000nJ/18000uL condition:995000ppm] comparison=[charge-attention-reduction:1ppm metabolic-crank:2nJ metabolic-treadle:1nJ pristine-rate-break-even:2 wear-aware-decision-crossover:3 provider-lifecycle=condition-carried-no-service] evidence=[build+charge+productive-discharge+recharge:executed selected-project:executed comparator-lifecycle:projected-canonical consumer:stone-crusher]",
            "POWER PROJECT EXPERIENCE seed=0x1 sample=anchor era=settlement selected=walking-wheel declared=[work:400000000000000nJ pristine-charge-events:80 project-cache=[food:8000000mg preservation:4000000ppm water:256000000uL]] executed=[charge-events:80 survival-limited-batches:0 active-attention:2500t provider-attention:2200t consumer-runtime:5600t maintenance=[services:4 preparation:240t service:12t replacement:216000mg] provisioning=[stops:2 attention:48t drinks:2 volume:200000uL meals:0 mass:0mg] elapsed:8100t reserves=[start:1001nJ/1001uL end:1nJ/1uL]] condition=[provider:900000ppm consumer:800000ppm] full-counterfactual=[treadle-active-attention:2600t walking-active-attention:2500t attention-best:walking-wheel selected-agrees:true] evidence=complete-selected-project-canonical",
            "POWER SETTLEMENT seed=0x1 sample=anchor workload-source=declared-consumer-project project=[consumer:powered-saw feed:1600000000mg work:400000000000000nJ charge-events:80] buffer:5000000000000nJ decision=[selected:walking-wheel policy:minimize-workload-attention-then-metabolic-then-hydration-then-material projected-attention-treadle:2290t projected-attention-walking:2210t] treadle=[first-charge:14t second-charge:15t] walking-wheel=[first-charge:10t second-charge:11t] productive-cycle=[consumer:powered-saw treadle:56t walking:56t] projected-provider-lifecycle=[treadle:body:100000000000000nJ/200000uL condition:800000ppm walking-wheel:body:80000000000000nJ/150000uL condition:900000ppm] comparison=[charge-saving:4t metabolic-saving:1nJ pristine-rate-break-even:60charges wear-aware-decision-crossover:56charges provider-lifecycle=condition-carried-no-service] evidence=[build+charge+productive-discharge+recharge:executed selected-project:executed comparator-lifecycle:projected-canonical consumer:powered-saw]",
            "WORKSHOP CAPABILITY mode=exploratory scenarios=1 orders=[complete:1 partial:0 productive:1/1] adaptive=[total:0 condition:0 stored-work:0] stops=[structural:0 maintenance-required:1 energy:0 declined-manual:0 survival-limited-manual:0] maintenance-blockers=[replacement-supply:1 service-labor:0]",
            "WORKSHOP EXPERIENCE REVIEW fantasy=operate+adapt pressure-shape=[clean:1 single:0 multi-system:10] interlocks=[stored-work+throughput:11 body+power:5 wear+maintenance:6 structure+production:9] recovery=[suspensions:3 resumed:3 stranded:0]",
            "AGENCY SUMMARY worlds=1",
            "CAPABILITY ORE_PREP seed=0x1 outcome=completed feed=[copper:400000ppm]",
            "ORE REVIEW seed=0x2 sample=coverage role=capability-only outcome=stopped stage=grind blocker=finite-energy available=1nJ requested=2nJ tick=3 retry=stage-input-retained retry-input=10mg matter=conserved",
            "CAPABILITY FOUNDRY seed=0x1 outcome=full-order-complete offered=10mg melted=10mg unmelted=0mg feed-retained=true melt-limit=offered-batch first-cast=10mg cast-limit=offered-batch molten-after-first=0mg recovery-cast=0mg molten-final=0mg heating=[runtime-route:direct-melt same-source-preheat:counterfactual-only direct:10mg/2t preheated:10mg/3t]",
            "POWER BUILD BILL seed=0x1 noisy-detail",
            "FIELDWORK PACING seed=0x1 noisy-detail",
        ]
        output = "\n".join(lines)
        concise = gameplay_report_summary.concise_gameplay_report(output, {})
        concise_lines = concise.splitlines()
        self.assertLessEqual(
            len(concise_lines),
            16,
            "default gameplay digest must stay reviewable without pinning its exact section count",
        )
        self.assertLessEqual(max(map(len, concise_lines)), 900)
        self.assertLess(len(concise.encode()), 6_000)
        for prefix in (
            "SIMULATION TIME ",
            "GAMEPLAY probe=primitive-progression ",
            "GAMEPLAY probe=primitive-liberation ",
            "GAMEPLAY probe=woodworking ",
            "GAMEPLAY probe=fieldwork ",
            "GAMEPLAY probe=power-provider ",
            "GAMEPLAY probe=survival ",
            "GAMEPLAY loop ",
            "CAPABILITY probe=workshop ",
            "CAPABILITY probe=agency ",
            "CAPABILITY probe=ore ",
            "CAPABILITY probe=foundry ",
        ):
            self.assertTrue(
                any(line.startswith(prefix) for line in concise_lines),
                f"missing digest line {prefix!r}",
            )
        for redundant in (
            "PLAYER FANTASY ",
            "EVALUATION SCOPE ",
            "PROBE INPUT ",
            "SURVIVAL EXPERIENCE ",
            "PROGRESSION EXPERIENCE ",
            "POWER PROJECT EXPERIENCE ",
            "WORKSHOP EXPERIENCE REVIEW ",
        ):
            self.assertNotIn(redundant, concise)
        self.assertIn(
            "disclosed-order-attention=[manual:2470..2470t mechanized:429..429t saved:2041..2041t]",
            concise,
        )
        self.assertIn("remaining-frontier=industrial-foundry-scale", concise)
        self.assertIn("cleanup-executed=1/1", concise)
        self.assertIn(
            "first-foundry=[defer:1/1 scarcity-select:1/1 scarcity-shortfall:2..2g native:40..40t/100..100% setup:800..800t upgrade:1/1 recovery:50..50t/90..90%->80..80t/100..100% gain:2..2g/+30..+30t]",
            concise,
        )
        self.assertNotIn("industrial-foundry-frontier=", concise)
        self.assertIn(
            "kit-decision=[attention-payback:8..8jobs disclosed-horizon:8..8batches selected:kit1/manual0 policy=manual-below-payback;kit-at-or-above evaluated:1/1 preassembled:0]",
            concise,
        )
        self.assertIn(
            "kit-acquisition=[executed:1 live-routes:1 preassembled:0",
            concise,
        )
        self.assertIn(
            "source=[fixture:1/1 pickup-runtime:1/1 world-gathering:0/1]",
            concise,
        )
        self.assertIn("choice=[saw:0 adze:0 bare:1]", concise)
        self.assertIn(
            "heavy-tool-market=[selected:0 deferred:1 unavailable:0",
            concise,
        )
        self.assertIn(
            "project-experience=[charges:2..2 services:1..1 feed:1..1kg provisioning-stops:1..1",
            concise,
        )
        self.assertNotIn("evidence-scope=", concise)
        self.assertIn("pacing-physical=[first-expedition:3.8..3.8m", concise)
        self.assertIn("reuse-physical=[repeat-complete:7.2..7.2s", concise)
        self.assertIn("integrated-campaign=[single-state:1/1 fantasy-captured:1/1]", concise)
        self.assertIn(
            "work-interlock=[policy=[task-floor:0 working-reserve:0]",
            concise,
        )
        self.assertIn(
            "delegate=[mechanized-processing:1/1 attention-saved:2041..2041t",
            concise,
        )
        self.assertIn(
            "CAPABILITY probe=ore samples=2 completed=1 stopped=1 finite-energy-stops=1 retryable-energy-stops=1 variable-feed=1",
            concise,
        )
        self.assertEqual(
            gameplay_report_summary.concise_gameplay_report(
                output, {"DEEP_HEARTH_GAMEPLAY_VERBOSE": "1"}
            ),
            output,
        )

    def test_scoped_gameplay_report_omits_cross_system_loop_digest(self) -> None:
        with (
            mock.patch.object(
                gameplay_report_summary,
                "ordinary_gameplay_summary",
                return_value=["ORDINARY SUMMARY probe=fieldwork samples=1"],
            ),
            mock.patch.object(
                gameplay_report_summary,
                "player_loop_evidence",
                return_value="PLAYER LOOP EVIDENCE observe-infer=[partial:true]",
            ) as loop_evidence,
        ):
            concise = gameplay_report_summary.concise_gameplay_report(
                "SIMULATION TIME physical-tick-us=3600000",
                {},
            )

        self.assertIn("GAMEPLAY probe=fieldwork samples=1", concise)
        self.assertNotIn("GAMEPLAY loop ", concise)
        loop_evidence.assert_not_called()

    def test_concise_report_rejects_missing_executed_probe_summary(self) -> None:
        transcript = (
            "PROBE INPUT name=fieldwork mode=explore samples=1 organic=1 "
            "world_root=0x1 behavior_root=unused replay=organic:0x2\n"
        )
        with self.assertRaisesRegex(ValueError, "ordinary:fieldwork"):
            gameplay_report_summary.concise_gameplay_report(transcript, {})

    def test_report_summary_contract_failure_is_a_clean_ci_failure(self) -> None:
        command = ci.report_plan("fieldwork")[0][1]
        transcript = (
            "PROBE INPUT name=fieldwork mode=explore samples=1 organic=1 "
            "world_root=0x1 behavior_root=unused replay=organic:0x2\n"
        )
        result = ci.subprocess.CompletedProcess(command, 0, transcript, "")
        with (
            contextlib.redirect_stdout(io.StringIO()) as stdout,
            contextlib.redirect_stderr(io.StringIO()) as stderr,
        ):
            self.assertIsNone(
                ci.report_stage(
                    1,
                    1,
                    "gameplay report fieldwork",
                    command,
                    (result, 0.25, None),
                    echo_success=True,
                    announced=True,
                )
            )
        self.assertEqual(stdout.getvalue(), "FAIL (0.2s)\n")
        self.assertIn(
            "repair: python ci.py report --scope fieldwork --variation-seed 0x1\n",
            stderr.getvalue(),
        )
        self.assertIn("report summary: concise gameplay summary lost executed probe evidence", stderr.getvalue())
        self.assertNotIn("PASS", stdout.getvalue())

    def test_liberation_summary_marks_preassembled_routes_as_controlled_evidence(self) -> None:
        lines = [
            "LIBERATION ROUTE TRADEOFF seed=0x2 basis=matched-ore-mass feed=100mg manual=[attention:60t native:30mg recovery:650000ppm body:1nJ/1uL] powered=[elapsed:20t charge-attention:5t native:45mg] campaign=[planned:8batches kit-payback:not-applicable economics:not-applicable justified:not-applicable] base-kit=[not-executed-this-sample] continuity=controlled-preassembled-kit",
            "LIBERATION FRONTIER CAPABILITY seed=0x2 cleanup-executed=true reason=required-native-copper-conversion concentrate=[first:70mg/700000ppm final:75mg/750000ppm] copper-in-concentrate=[first:49mg final:56mg scavenger-recovered:7mg] native-copper=50mg matter=conserved",
            "LIBERATION FRONTIER seed=0x2 remaining-frontier=industrial-foundry-scale industrial-foundry-frontier=[assembly-edge=[furnace:false mold:false electrical-buffer:false thermal-sink:false] manual-electrical-generation:true support-required=[furnace:true mold:true] energy-scale=[manual-electrical-max:100000000uW industrial-furnace-transfer-ceiling:2000000000000uW ceiling-ratio:20000x melting-carrier:Electrical conversion-path:present]]",
        ]
        summary = "\n".join(gameplay_report_summary.ordinary_gameplay_summary(lines))
        self.assertIn(
            "kit-acquisition=[executed:0 live-routes:0 preassembled:1",
            summary,
        )
        self.assertIn(
            "kit-decision=[attention-payback:n/a disclosed-horizon:n/a selected:kit0/manual0 policy=manual-below-payback;kit-at-or-above evaluated:0/1 preassembled:1]",
            summary,
        )

    def test_progression_summary_preserves_stockpiling_delay_and_supply_blocking(self) -> None:
        lines = [
            "PROGRESSION EXPERIENCE seed=0x1 local-copper-sequence=pick-first selected-reinvestment=[completed]",
            "PROGRESSION EXPERIENCE seed=0x2 local-copper-sequence=crank-first selected-reinvestment=[blocked:known-target-supply]",
            "PROGRESSION EXPERIENCE seed=0x3 local-copper-sequence=pick-first selected-reinvestment=[blocked:crushed-storage available:10mg requires-more-than:20mg]",
            "PROGRESSION GOAL seed=0x1 immediate=264t delayed=747t chosen=immediate",
            "PROGRESSION GOAL seed=0x2 immediate=267t delayed=blocked:target-supply chosen=immediate",
        ]
        summary = "\n".join(gameplay_report_summary.ordinary_gameplay_summary(lines))
        self.assertIn(
            "reinvestment=[completed:1 target-supply:1 storage-capacity:1]",
            summary,
        )
        self.assertIn(
            "reinvestment-timing=[selected-immediate:2/0 stockpile-first-counterfactual:1/1 delay-avoided:483..483t]",
            summary,
        )

    def test_player_loop_summary_distinguishes_survival_and_selected_maintenance(self) -> None:
        output = "\n".join(
            (
                "SURVIVAL EXPERIENCE seed=0x1 work-interlock=[integrated=[hydration-policy:task-floor drink:10uL/1t prospect:48t opportunity-power:true reprovision:true:10uL/1t power:3t stored:500000000000nJ final-reserve:250000ppmE/250000ppmH warning-safe:true]]",
                "SURVIVAL EXPERIENCE seed=0x2 work-interlock=[integrated=[hydration-policy:working-reserve drink:20uL/2t prospect:12t opportunity-power:false reprovision:false:0uL/0t power:0t stored:0nJ final-reserve:500000ppmE/500000ppmH warning-safe:true]]",
                "WOODWORKING EXPERIENCE seed=0x1 choice=stone-adze routes=[adze:10logs timber:100mg attention:100t production:20t maintenance:80t/1services final-condition:900000ppm; saw-assisted:min-saw-logs:0 fundable:false actual=[saw:0 adze-fallback:0 fallback-copper:false saw-services:0 adze-services:0]]",
                "WOODWORKING EXPERIENCE seed=0x2 choice=frame-saw routes=[adze:10logs timber:100mg attention:100t production:20t maintenance:80t/1services final-condition:900000ppm; saw-assisted:min-saw-logs:9 fundable:true actual=[saw:9 adze-fallback:1 fallback-copper:false saw-services:1 adze-services:1]]",
            )
        )
        detailed = gameplay_report_summary.player_loop_evidence(output.splitlines())
        self.assertIsNotNone(detailed)
        self.assertIn(
            "survive-adapt=[reprovisioned-after-work:1/2 hydration-policy:task-floor1/working-reserve1 opportunistic-power:1/1 mechanized-project-breaks:0/0 break-count:0 warning-safe:2/2]",
            detailed,
        )
        self.assertIn(
            "maintain-recover=[woodworking-service-worlds:2/2 woodworking-service-events:3 mechanized-projects-with-service:0/0 mechanized-service-events:0]",
            detailed,
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

    def test_woodworking_summary_classifies_conservative_setup_budget_without_calling_it_model_error(self) -> None:
        summary = gameplay_report_summary.concise_gameplay_report(
            "\n".join(
                (
                    "WOODWORKING EXPERIENCE seed=0x1 demand-horizon=project choice=stone-adze reason=pipeline-timber-cost-not-recovered",
                    "WOODWORKING FEEDBACK seed=0x1 basis=executed-lifecycle-versus-pre-action-policy-model attention=[setup-budget-met:false actual-payback:true] timber=[nominal:costlier actual:costlier] selected=stone-adze choice-revised-after-outcome=false",
                )
            ),
            {},
        )
        self.assertIn(
            "lifecycle-feedback=[samples:1/1 setup-budget-met:0/1 realized-payback:1/1 budget-vs-payback=[conservative:1 optimistic:0] timber-model-agrees:1/1 choice-revised:0/1]",
            summary,
        )

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
        self.assertEqual(
            examples,
            {ci.GAMEPLAY_REPORT_EXAMPLE, *ci.FOCUSED_REPORT_EXAMPLES.values()},
        )
        self.assertTrue(
            all(definition.get("test") is False for definition in manifest.get("example", [])),
            "gameplay reports are executable tools, not cargo-test targets",
        )
        tests_by_name = {
            definition["name"]: definition for definition in manifest.get("test", [])
        }
        examples_by_name = {
            definition["name"]: definition for definition in manifest.get("example", [])
        }
        for scope, example in ci.FOCUSED_REPORT_EXAMPLES.items():
            owner_scope = "workshop" if scope == "agency" else scope
            focused = tests_by_name[ci.GAMEPLAY_TARGETS[owner_scope]]
            report = examples_by_name[example]
            self.assertEqual(report.get("required-features"), focused.get("required-features"))
            report_root = ROOT / report["path"]
            focused_name = Path(focused["path"]).name
            self.assertIn(
                f'#[path = "{focused_name}"]',
                report_root.read_text(encoding="utf-8"),
            )

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
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative in check_authority_docs.AUTHORITY_FILES:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("# maintained\n", encoding="utf-8")
            specialized = root / "assets" / "shaders" / "README.md"
            specialized.parent.mkdir(parents=True, exist_ok=True)
            specialized.write_text("# shader notes\n", encoding="utf-8")
            generated = root / "target" / "generated.md"
            generated.parent.mkdir(parents=True, exist_ok=True)
            generated.write_text("# generated\n", encoding="utf-8")

            with mock.patch.object(check_authority_docs, "ROOT", root):
                documents = set(check_authority_docs.documentation_files())

        self.assertTrue(set(check_authority_docs.AUTHORITY_FILES).issubset(documents))
        self.assertIn("assets/shaders/README.md", documents)
        self.assertNotIn("target/generated.md", documents)

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
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "src" / "owner.rs"
            integration = root / "tests" / "boundary.rs"
            source.parent.mkdir(parents=True)
            integration.parent.mkdir(parents=True)
            source.write_text("//! production owner\n", encoding="utf-8")
            integration.write_text("//! integration boundary\n", encoding="utf-8")

            with mock.patch.object(check_authority_docs, "ROOT", root):
                errors, checked = check_authority_docs.check_source_module_docs()
                integration.write_text("fn missing_module_doc() {}\n", encoding="utf-8")
                broken, broken_checked = check_authority_docs.check_source_module_docs()

        self.assertEqual(errors, [])
        self.assertEqual(checked, 2)
        self.assertEqual(broken_checked, 2)
        self.assertTrue(any("tests/boundary.rs" in error for error in broken))

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

    def test_automatic_selection_prefers_owner_focused_gameplay_targets(self) -> None:
        cases = {
            "batch_capped_mining_finishes_the_requested_order": "fieldwork",
            "woodworking_keeps_pre_action_setup_budget_choice_when_realized_saw_is_cheaper": "woodworking",
            "primitive_treadle_requires_meaningful_attention_return": "power-provider",
        }
        for selector, scope in cases.items():
            target, _name = run_test.resolve_automatic_exact_selection(selector, None)
            self.assertEqual(target, ci.GAMEPLAY_TARGETS[scope])
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

    def test_check_and_lint_modes_require_a_selector_or_explicit_target(self) -> None:
        for mode in ("--check", "--lint"):
            with self.subTest(mode=mode):
                with contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit):
                        run_test.parse_args([mode])

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

    def test_unknown_unit_owner_falls_back_to_the_normal_library_shape(self) -> None:
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

    def test_exact_unit_command_reuses_shared_library_test_artifact(self) -> None:
        args = argparse.Namespace(
            target="lib",
            features=None,
            list=False,
            name="core::time::tests::absolute_tick_and_relative_span_add_without_wraparound",
            suite=False,
            ignored=False,
            nocapture=False,
        )
        command = run_test.cargo_command(args)
        self.assertNotIn("--features", command)
        self.assertIn("--exact", command)

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

    def test_exact_ignored_test_requires_explicit_ignored_execution(self) -> None:
        args = argparse.Namespace(
            target="gameplay_fieldwork",
            name="gameplay_fieldwork_report",
            suite=False,
            ignored=False,
            variation_seed="0x1234",
            behavior_seed=None,
        )
        output = (
            "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; "
            "27 filtered out; finished in 0.00s"
        )
        self.assertEqual(
            run_test.execution_error(args, output),
            (
                "cataloged exact test is ignored: gameplay_fieldwork_report",
                "python tools/run_test.py --ignored --target gameplay_fieldwork "
                "--variation-seed 0x1234 gameplay_fieldwork_report",
            ),
        )

    def test_exact_execution_requires_a_passed_test(self) -> None:
        args = argparse.Namespace(
            target="lib",
            name="missing::test",
            suite=False,
            ignored=False,
            variation_seed=None,
            behavior_seed=None,
        )
        output = "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out"
        self.assertEqual(
            run_test.execution_error(args, output),
            (
                "Cargo did not execute cataloged exact test: missing::test",
                "python tools/run_test.py --list missing::test",
            ),
        )

    def test_suite_execution_requires_at_least_one_passed_test(self) -> None:
        args = argparse.Namespace(
            target="lib",
            name="owner::tests::",
            suite=True,
            ignored=False,
            variation_seed=None,
            behavior_seed=None,
        )
        output = "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out"
        self.assertEqual(
            run_test.execution_error(args, output),
            (
                "Cargo did not execute cataloged suite: owner::tests::",
                "python tools/run_test.py --list owner::tests::",
            ),
        )


if __name__ == "__main__":
    unittest.main(verbosity=1)
