"""Shared environment for repository-owned local Rust verification."""

from __future__ import annotations

import os
from collections.abc import Mapping

def local_cargo_environment(
    base: Mapping[str, str] | None = None,
) -> dict[str, str]:
    """Normalize ambient overrides so every local lane reuses one Cargo artifact shape."""

    environment = dict(os.environ if base is None else base)
    environment["CARGO_TERM_COLOR"] = "never"
    for key in (
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_INCREMENTAL",
        "CARGO_BUILD_JOBS",
    ):
        environment.pop(key, None)
    for key in tuple(environment):
        if key.startswith(("CARGO_PROFILE_TEST_", "CARGO_PROFILE_UNIT_TEST_")):
            environment.pop(key)
    return environment
