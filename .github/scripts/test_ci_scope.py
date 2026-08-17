"""Unit tests for the pull-request CI changed-path classifier."""

import unittest

from ci_scope import build_plan, should_run


class CiScopeTests(unittest.TestCase):
    def test_documentation_only_changes_skip_all_rust_builds(self) -> None:
        paths = ["README.md", "STATUS.md", "TECHNICAL_DESIGN.md"]
        for lane in ("format", "lint", "core", "gameplay", "shaders"):
            with self.subTest(lane=lane):
                self.assertFalse(should_run(lane, paths))

    def test_quality_and_core_cover_their_complete_compile_inputs(self) -> None:
        self.assertTrue(should_run("format", ["src/inventory/mod.rs"]))
        self.assertTrue(should_run("format", ["tests/gameplay_harness/main.rs"]))
        self.assertFalse(should_run("format", ["assets/shaders/surface.wgsl"]))
        self.assertTrue(should_run("lint", ["src/inventory/mod.rs"]))
        self.assertFalse(should_run("lint", ["tests/gameplay_harness/main.rs"]))
        self.assertTrue(should_run("lint", [".cargo/config.toml"]))
        self.assertTrue(should_run("core", ["src/inventory/mod.rs"]))
        self.assertTrue(should_run("core", ["assets/shaders/surface.wgsl"]))
        self.assertFalse(should_run("core", ["tests/gameplay_harness/main.rs"]))

    def test_gameplay_skips_known_unrelated_domains_but_runs_unknown_source(self) -> None:
        self.assertTrue(should_run("gameplay", ["src/inventory/transactions.rs"]))
        self.assertTrue(should_run("gameplay", ["tests/gameplay_harness/report.rs"]))
        self.assertFalse(should_run("gameplay", ["src/fluid/storage_execution.rs"]))
        self.assertFalse(should_run("gameplay", ["assets/shaders/water.wgsl"]))
        self.assertTrue(should_run("gameplay", ["src/new_system/runtime.rs"]))
        self.assertTrue(should_run("gameplay", ["build.rs"]))

    def test_shader_scope_is_narrow_and_fails_safe_for_new_source_domains(self) -> None:
        self.assertTrue(should_run("shaders", ["assets/shaders/surface.wgsl"]))
        self.assertTrue(should_run("shaders", ["src/content/shaders.rs"]))
        self.assertTrue(should_run("shaders", ["src/texture/definitions.rs"]))
        self.assertFalse(should_run("shaders", ["src/inventory/transactions.rs"]))
        self.assertFalse(should_run("shaders", ["tests/gameplay_harness/main.rs"]))
        self.assertTrue(should_run("shaders", ["src/render_backend/device.rs"]))

    def test_any_relevant_path_enables_the_lane(self) -> None:
        self.assertTrue(should_run("gameplay", ["README.md", "src/production/state.rs"]))
        self.assertTrue(should_run("shaders", ["README.md", "Cargo.toml"]))

    def test_plan_classifies_one_path_set_for_every_lane(self) -> None:
        self.assertEqual(
            build_plan(["tests/gameplay_harness/report.rs"]),
            {
                "format": True,
                "lint": False,
                "core": False,
                "gameplay": True,
                "shaders": False,
            },
        )

    def test_gameplay_only_bootstrap_source_avoids_default_rust_builds(self) -> None:
        self.assertEqual(
            build_plan(["src/content/gameplay_fixture.rs"]),
            {
                "format": True,
                "lint": False,
                "core": False,
                "gameplay": True,
                "shaders": False,
            },
        )

    def test_shared_test_fixture_source_still_runs_core_and_gameplay(self) -> None:
        self.assertEqual(
            build_plan(["src/inventory/fixture.rs"]),
            {
                "format": True,
                "lint": False,
                "core": True,
                "gameplay": True,
                "shaders": False,
            },
        )


if __name__ == "__main__":
    unittest.main()
