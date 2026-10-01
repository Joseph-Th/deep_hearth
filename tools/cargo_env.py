"""Shared environment for repository-owned local Rust verification."""

from __future__ import annotations

import os
from collections.abc import Mapping

LIBRARY_TEST_CODEGEN_UNITS = "512"
LIBRARY_TEST_BUILD_JOBS = "4"


def local_cargo_environment(
    base: Mapping[str, str] | None = None,
    *,
    library_test_codegen: bool = False,
) -> dict[str, str]:
    """Normalize ambient overrides and apply the one measured lane-specific profile override."""

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
        if key.startswith("CARGO_PROFILE_TEST_"):
            environment.pop(key)
    if library_test_codegen:
        # The cfg(test) library is much larger than focused gameplay crates. A high CGU count
        # materially reduces measured post-edit codegen/link latency while focused targets retain
        # the lower-CGU profile and Cargo's wider default parallelism that benchmark better for
        # their smaller graphs.
        environment["CARGO_PROFILE_TEST_CODEGEN_UNITS"] = LIBRARY_TEST_CODEGEN_UNITS
        environment["CARGO_BUILD_JOBS"] = LIBRARY_TEST_BUILD_JOBS
    return environment
