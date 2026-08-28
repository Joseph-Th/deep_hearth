#!/usr/bin/env python3
"""Fast contract tests for the local CI plan; never invoke Cargo builds from this file."""

from __future__ import annotations

import argparse
import contextlib
import io
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
        "all": False,
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


def cargo_test_targets(command: list[str]) -> list[str]:
    return [command[index + 1] for index, value in enumerate(command[:-1]) if value == "--test"]


class LocalCiPlanTests(unittest.TestCase):
    def test_quick_lane_is_build_free(self) -> None:
        self.assertEqual(cargo_build_commands(ci.quick_plan()), [])

    def test_quick_lane_includes_bca_complexity_ratchet(self) -> None:
        self.assertIn(
            (
                "complexity ratchet",
                [sys.executable, "tools/check_bca.py"],
            ),
            ci.quick_plan(),
        )

    def test_rust_test_summary_is_concise_and_aggregates_multiple_results(self) -> None:
        output = (
            "test result: ok. 18 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out\n"
            "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        )
        self.assertEqual(ci.rust_test_summary(output), "20 tests, 1 ignored")
        self.assertIsNone(ci.rust_test_summary("Finished test profile"))

    def test_unit_test_bodies_stay_out_of_production_source_files(self) -> None:
        inline_marker = "#[cfg(test)]\nmod tests {"
        offenders = [
            path.relative_to(ROOT).as_posix()
            for path in (ROOT / "src").rglob("*.rs")
            if inline_marker in path.read_text(encoding="utf-8")
        ]
        self.assertEqual(offenders, [])

    def test_standard_gate_compiles_production_once(self) -> None:
        builds = cargo_build_commands(ci.plan_for(gate_args()))
        self.assertEqual(builds, [["cargo", "check-fast"]])

    def test_soak_gate_does_not_repeat_ordinary_core_tests(self) -> None:
        builds = cargo_build_commands(ci.plan_for(gate_args(soak=True)))
        self.assertEqual(builds, [["cargo", "test-soak"]])

    def test_focused_gameplay_does_not_precompile_production(self) -> None:
        builds = cargo_build_commands(ci.plan_for(gate_args(gameplay="survival")))
        self.assertEqual(len(builds), 1)
        self.assertNotIn("check-fast", builds[0])
        self.assertEqual(builds[0].count("--test"), 1)
        self.assertIn("gameplay_survival", builds[0])

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
        self.assertEqual(sum(command == ["cargo", "test-fast"] for command in builds), 1)
        self.assertEqual(sum("test-gameplay" in command for command in builds), 1)

    def test_scoped_audits_do_not_build_the_other_broad_surface(self) -> None:
        core_builds = cargo_build_commands(ci.audit_plan("core"))
        gameplay_builds = cargo_build_commands(ci.audit_plan("gameplay"))
        self.assertEqual(core_builds, [["cargo", "test-fast"]])
        self.assertEqual(len(gameplay_builds), 1)
        self.assertIn("test-gameplay", gameplay_builds[0])
        self.assertNotIn(["cargo", "test-fast"], gameplay_builds)

    def test_broad_gameplay_audit_links_one_consolidated_target(self) -> None:
        command = ci.gameplay_command("all")
        self.assertEqual(
            cargo_test_targets(command),
            [ci.GAMEPLAY_AUDIT_TARGET],
        )

    def test_broad_core_failure_points_to_one_exact_repair(self) -> None:
        output = "failures:\n    mining::execution::tests::missing_capability\n"
        self.assertEqual(
            ci.repair_hint(["cargo", "test-fast"], output, ""),
            "python tools/run_test.py mining::execution::tests::missing_capability",
        )

    def test_gameplay_failure_points_to_one_exact_repair(self) -> None:
        output = "failures:\n    configuration::tests::broken_contract\n"
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_workshop configuration::tests::broken_contract",
        )

    def test_gameplay_failure_without_test_name_falls_back_to_focused_target(self) -> None:
        error = "error: test failed, to rerun pass `--test gameplay_workshop`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), "", error),
            "python ci.py gate --gameplay workshop",
        )

    def test_consolidated_gameplay_failure_points_back_to_narrow_focused_target(self) -> None:
        output = "failures:\n    focused::gameplay_ore_preparation_probe\n"
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_ore gameplay_ore_preparation_probe",
        )

    def test_agency_failure_reuses_the_aggregate_target_instead_of_linking_another_binary(self) -> None:
        output = "failures:\n    agency::gameplay_maintained_agency_counterfactuals\n"
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit agency::gameplay_maintained_agency_counterfactuals",
        )

    def test_global_catalog_failure_reuses_the_aggregate_target(self) -> None:
        output = "failures:\n    catalog_contract_tests::gameplay_machine_process_catalog_has_evidence\n"
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit catalog_contract_tests::gameplay_machine_process_catalog_has_evidence",
        )

    def test_unknown_aggregate_only_failure_stays_on_the_aggregate_target(self) -> None:
        output = "failures:\n    future_contracts::new_global_check\n"
        error = "error: test failed, to rerun pass `--test gameplay_audit`"
        self.assertEqual(
            ci.repair_hint(ci.gameplay_command("all"), output, error),
            "python tools/run_test.py --target gameplay_audit future_contracts::new_global_check",
        )

    def test_integration_exact_command_infers_target_required_features(self) -> None:
        args = argparse.Namespace(
            target="gameplay_ore",
            features=None,
            list=False,
            name="gameplay_ore_preparation_probe",
            ignored=False,
            nocapture=False,
        )
        command = run_test.cargo_command(args)
        self.assertEqual(command.count("--features"), 1)
        self.assertIn("test-gameplay", command)
        self.assertIn("gameplay_ore", command)

    def test_report_reuses_ordinary_gameplay_feature_shape(self) -> None:
        plan = ci.report_plan()
        commands = [command for _label, command in plan]
        flattened = "\n".join(" ".join(command) for command in commands)
        self.assertEqual(len(plan), 1)
        self.assertNotIn("test-gameplay-full", flattened)
        self.assertEqual(
            run_test.requested_target_features(ci.GAMEPLAY_AUDIT_TARGET, None),
            {"test-gameplay"},
        )
        report = plan[0][1]
        self.assertIn("tools/run_test.py", report)
        self.assertIn(ci.GAMEPLAY_AUDIT_TARGET, report)
        self.assertIn("gameplay_report", report)
        self.assertIn("--ignored", report)
        self.assertIn("--nocapture", report)
        self.assertNotIn("gameplay_workshop", report)

    def test_report_replay_environment_is_fresh_by_default_and_preserves_explicit_roots(self) -> None:
        generated: dict[str, str] = {}
        rolls = iter((0x1234, 0x5678))
        variation, behavior = ci.configure_report_replay_environment(
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
            ci.configure_report_replay_environment(
                explicit, randbits=lambda _bits: self.fail("explicit replay roots must not consume entropy")
            ),
            ("0xAA", "0xBB"),
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
            set(ci.GAMEPLAY_TARGETS.values()) | {ci.GAMEPLAY_AUDIT_TARGET},
        )
        binaries = {definition["name"] for definition in manifest.get("bin", [])}
        self.assertEqual(binaries, {"validate-shaders"})

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
        self.assertIsNone(check_authority_docs.ci_command_error("python ci.py audit --core"))
        self.assertIsNone(check_authority_docs.ci_command_error("python ci.py audit --gameplay"))
        self.assertIsNone(check_authority_docs.ci_command_error("python ci.py audit --all"))
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


class ExactTestCommandTests(unittest.TestCase):
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

    def test_source_catalog_matches_default_library_test_names_without_building(self) -> None:
        catalog = run_test.source_test_catalog("lib", None)
        self.assertIn(
            "core::time::tests::absolute_tick_and_relative_span_add_without_wraparound",
            catalog,
        )
        self.assertIn(
            "core::time::calendar_tests::calendar_exposes_exact_physical_world_time_per_tick",
            catalog,
        )
        self.assertIn(
            "content::equipment::tests::primitive_copper_upgrades_improve_their_intended_nominal_capability",
            catalog,
        )
        self.assertIn(
            "thermal::processes::tests::sensible_heating_rejects_heater_after_mounted_support_fails",
            catalog,
        )
        self.assertNotIn(
            "content::shaders::tests::built_in_programs_assemble_and_validate_as_portable_wgsl",
            catalog,
        )

    def test_source_catalog_honors_explicit_test_features(self) -> None:
        catalog = run_test.source_test_catalog("lib", "test-shader-validation")
        self.assertIn(
            "content::shaders::tests::built_in_programs_assemble_and_validate_as_portable_wgsl",
            catalog,
        )

    def test_source_catalog_resolves_gameplay_target_modules(self) -> None:
        workshop = run_test.source_test_catalog("gameplay_workshop", None)
        ore = run_test.source_test_catalog("gameplay_ore", None)
        audit = run_test.source_test_catalog("gameplay_audit", None)
        self.assertIn("workshop_contract_tests::gameplay_harness_gate", workshop)
        self.assertNotIn("agency::gameplay_maintained_agency_counterfactuals", workshop)
        self.assertNotIn(
            "catalog_contract_tests::gameplay_machine_process_catalog_has_evidence", workshop
        )
        self.assertIn(
            "configuration::tests::default_gate_keeps_maintained_anchors_and_adds_a_bounded_variation_sample",
            workshop,
        )
        self.assertEqual(ore, ["gameplay_ore_preparation_probe"])
        self.assertTrue(
            {
                "focused::gameplay_survival_provisioning_probe",
                "focused::gameplay_primitive_progression_probe",
                "focused::gameplay_ore_preparation_probe",
                "focused::gameplay_foundry_probe",
                "workshop_contract_tests::gameplay_harness_gate",
                "agency::gameplay_maintained_agency_counterfactuals",
                "gameplay_report",
            }.issubset(set(audit))
        )
        self.assertTrue(set(workshop).issubset(set(audit)))
        self.assertEqual(
            set(audit) - set(workshop),
            {
                "agency::gameplay_maintained_agency_counterfactuals",
                "catalog_contract_tests::gameplay_machine_process_catalog_has_evidence",
                "focused::gameplay_survival_provisioning_probe",
                "focused::gameplay_primitive_progression_probe",
                "focused::gameplay_ore_preparation_probe",
                "focused::gameplay_foundry_probe",
                "gameplay_report",
            },
            "every broad-audit-only maintained test must be an explicit counterfactual, focused probe, or report",
        )

    def test_source_catalog_listing_never_builds_through_cargo_command(self) -> None:
        args = argparse.Namespace(
            target="lib",
            features=None,
            list=True,
            name="survival",
            ignored=False,
            nocapture=False,
        )
        with self.assertRaisesRegex(ValueError, "does not invoke Cargo"):
            run_test.cargo_command(args)


if __name__ == "__main__":
    unittest.main(verbosity=1)
