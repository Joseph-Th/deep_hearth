#!/usr/bin/env python3
"""Run the repository's pinned Big Code Analysis complexity ratchet."""

from __future__ import annotations

import subprocess
import sys


EXPECTED_VERSION = "bca 2.1.0"


def main() -> int:
    try:
        version = subprocess.run(
            ["bca", "--version"],
            text=True,
            capture_output=True,
            check=False,
        )
    except OSError as error:
        print(
            "Big Code Analysis is required for the complexity ratchet. "
            "Install it with `cargo install big-code-analysis-cli --version 2.1.0 --locked`.",
            file=sys.stderr,
        )
        print(error, file=sys.stderr)
        return 1

    observed = version.stdout.strip()
    if version.returncode != 0 or observed != EXPECTED_VERSION:
        print(
            f"BCA version mismatch: expected {EXPECTED_VERSION!r}, observed {observed!r}. "
            "Change the repository policy and baseline deliberately before changing metric semantics.",
            file=sys.stderr,
        )
        return 1

    return subprocess.run(
        ["bca", "check", "--no-suppress", "--no-remediation"],
        check=False,
    ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
