"""Unit tests for the pull-request CI changed-path classifier."""

import unittest

from ci_scope import should_run


class CiScopeTests(unittest.TestCase):
    def test_documentation_only_changes_skip_all_rust_builds(self) -> None:
        paths = ["README.md", "STATUS.md", "TECHNICAL_DESIGN.md"]
        for lane in ("quality", "lint", "core", "gameplay", "shaders"):
            with self.subTest(lane=lane):
                self.assertFalse(should_run(lane, paths))

    def test_quality_and_core_cover_their_complete_compile_inputs(self) -> None:
        self.assertTrue(should_run("quality", ["src/inventory/mod.rs"]))
        self.assertTrue(should_run("quality", ["tests/gameplay_harness/main.rs"]))
        self.assertFalse(should_run("quality", ["assets/shaders/surface.wgsl"]))
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


if __name__ == "__main__":
    unittest.main()
