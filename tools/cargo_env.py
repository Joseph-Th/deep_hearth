"""Shared environment for repository-owned local Rust verification."""

from __future__ import annotations

import os
from collections.abc import Mapping


def local_cargo_environment(base: Mapping[str, str] | None = None) -> dict[str, str]:
    """Return the repository-owned verification environment and cache shape."""

    environment = dict(os.environ if base is None else base)
    environment["CARGO_TERM_COLOR"] = "never"
    environment.pop("RUSTFLAGS", None)
    environment.pop("CARGO_ENCODED_RUSTFLAGS", None)
    return environment
