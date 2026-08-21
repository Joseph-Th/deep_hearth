#!/usr/bin/env python3
"""Fast contract tests for the local CI plan; never invoke Cargo builds from this file."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import ci  # noqa: E402
from tools import check_authority_docs, run_test  # noqa: E402


def gate_args(**overrides: object) -> argparse.Namespace:
    values: dict[str, object] = {
        "preset": "gate",
        "core": False,
        "soak": False,
        "gameplay": None,
        "shaders": False,
        "rustdoc": False,
        "lint": False,
        "dry_run": False,
    }
    values.update(overrides)
    return argparse.Namespace(**values)


def cargo_build_commands(plan: list[tuple[str, list[str]]]) -> list[list[str]]:
    return [
        command
        for _label, command in plan
        if command[:2] != ["cargo", "fmt"] and command[:1] == ["cargo"]
    ]


class LocalCiPlanTests(unittest.TestCase):
    def test_quick_lane_is_build_free(self) -> None:
        self.assertEqual(cargo_build_commands(ci.quick_plan()), [])

    def test_standard_gate_compiles_production_once(self) -> None:
        builds = cargo_build_commands(ci.plan_for(gate_args()))
        self.assertEqual(builds, [["cargo", "check-fast"]])

    def test_focused_gameplay_does_not_precompile_production(self) -> None:
        builds = cargo_build_commands(ci.plan_for(gate_args(gameplay="survival")))
        self.assertEqual(len(builds), 1)
        self.assertNotIn("check-fast", builds[0])
        self.assertEqual(builds[0].count("--test"), 1)
        self.assertIn("gameplay_survival", builds[0])

    def test_audit_has_no_redundant_compile_only_stage(self) -> None:
        builds = cargo_build_commands(ci.audit_plan())
        self.assertFalse(any("check-fast" in command for command in builds))
        self.assertEqual(sum(command == ["cargo", "test-fast"] for command in builds), 1)
        self.assertEqual(sum("test-gameplay" in command for command in builds), 1)

    def test_report_reuses_ordinary_gameplay_feature_shape(self) -> None:
        commands = [command for _label, command in ci.report_plan()]
        flattened = "\n".join(" ".join(command) for command in commands)
        self.assertIn("test-gameplay", flattened)
        self.assertNotIn("test-gameplay-full", flattened)

    def test_git_wizard_validation_levels_match_iteration_policy(self) -> None:
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        validation = manifest["package"]["metadata"]["git-wizard"]["validation"]
        self.assertEqual(validation["quick"], "python ci.py quick")
        self.assertEqual(validation["standard"], "python ci.py gate")
        self.assertEqual(validation["full"], "python ci.py audit")

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
        error = check_authority_docs.ci_command_error("python ci.py gate --docs")
        self.assertIsNotNone(error)
        self.assertIn("invalid local CI command", error or "")


class ExactTestCommandTests(unittest.TestCase):
    def test_default_exact_command_reuses_default_library_shape(self) -> None:
        args = argparse.Namespace(
            target="lib",
            features=None,
            list=False,
            name="module::tests::case",
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

    def test_list_command_does_not_create_another_feature_shape(self) -> None:
        args = argparse.Namespace(
            target="lib",
            features=None,
            list=True,
            name="survival",
            ignored=False,
            nocapture=False,
        )
        self.assertEqual(
            run_test.cargo_command(args),
            ["cargo", "test", "--quiet", "--locked", "--lib", "--", "--list"],
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
