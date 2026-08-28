#!/usr/bin/env python3
"""Run pinned Big Code Analysis checks and advisory review commands."""

from __future__ import annotations

import argparse
import subprocess
import sys


EXPECTED_VERSION = "bca 2.1.0"


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run the repository-owned Big Code Analysis workflow with the pinned CLI version."
    )
    modes = parser.add_subparsers(dest="mode", required=True)
    modes.add_parser("check", help="run the mandatory cognitive-complexity ratchet")

    report = modes.add_parser(
        "report",
        help="show advisory complexity hotspots joined to VCS change history",
    )
    report.add_argument("--top", type=int, default=30, help="rows per report table")
    report.add_argument(
        "--path",
        action="append",
        default=[],
        help="restrict analysis to a path; repeat for multiple paths",
    )

    diff = modes.add_parser(
        "diff",
        help="show advisory metric changes relative to a git revision",
    )
    diff.add_argument("--since", default="HEAD", help="git revision used as the comparison base")
    diff.add_argument(
        "--metric",
        action="append",
        default=[],
        help="restrict the diff to one metric; repeat for multiple metrics",
    )
    return parser.parse_args(argv)


def command_for(args: argparse.Namespace) -> list[str]:
    if args.mode == "check":
        return ["bca", "check", "--no-suppress", "--no-remediation"]
    if args.mode == "report":
        command = ["bca", "report", "--vcs", "--top", str(args.top)]
        for path in args.path:
            command.extend(["--paths", path])
        return command
    if args.mode == "diff":
        command = ["bca", "diff", "--since", args.since, "--format", "markdown"]
        for metric in args.metric:
            command.extend(["--metric", metric])
        return command
    raise ValueError(f"unknown BCA mode: {args.mode}")


def verify_version() -> int:
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

    return 0


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    version_result = verify_version()
    if version_result != 0:
        return version_result
    return subprocess.run(command_for(args), check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
