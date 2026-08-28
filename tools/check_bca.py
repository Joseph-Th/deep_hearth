#!/usr/bin/env python3
"""Run pinned Big Code Analysis checks and advisory review commands."""

from __future__ import annotations

import argparse
from pathlib import PurePosixPath
import subprocess
import sys


EXPECTED_VERSION = "bca 2.1.0"
DEFAULT_REVIEW_METRICS = ("cognitive", "cyclomatic", "sloc")


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
    diff.add_argument(
        "--path",
        action="append",
        default=[],
        help="restrict the diff to a path; repeat for multiple paths",
    )

    review = modes.add_parser(
        "review",
        help="run the history-aware hotspot report followed by a focused changed-code diff",
    )
    review.add_argument("--top", type=int, default=30, help="rows per report table")
    review.add_argument("--since", default="HEAD", help="git revision used as the comparison base")
    review.add_argument(
        "--metric",
        action="append",
        default=[],
        help=(
            "restrict the diff to one metric; repeat for multiple metrics; "
            "defaults to cognitive, cyclomatic, and sloc"
        ),
    )
    review.add_argument(
        "--path",
        action="append",
        default=[],
        help="restrict both report and diff to a path; repeat for multiple paths",
    )
    return parser.parse_args(argv)


def report_command(top: int, paths: list[str]) -> list[str]:
    command = ["bca", "report", "--vcs", "--top", str(top)]
    for path in paths:
        command.extend(["--paths", path])
    return command


def compact_scopes(paths: list[str]) -> list[str]:
    """Drop duplicate or narrower scopes while preserving deterministic caller order."""

    compacted: list[str] = []
    for path in paths:
        if any(path == existing or path.startswith(f"{existing}/") for existing in compacted):
            continue
        compacted = [
            existing
            for existing in compacted
            if not existing.startswith(f"{path}/")
        ]
        compacted.append(path)
    return compacted


def resolve_review_diff_paths(
    paths: list[str],
    exists_at_revision,
) -> list[str]:
    """Widen new working-tree paths only as far as needed for a base-relative BCA diff."""

    resolved: list[str] = []
    for raw_path in paths:
        normalized = PurePosixPath(raw_path.replace("\\", "/")).as_posix()
        candidate = PurePosixPath(normalized)
        while candidate.as_posix() != "." and not exists_at_revision(candidate.as_posix()):
            candidate = candidate.parent
        resolved.append(candidate.as_posix())
    return compact_scopes(resolved)


def git_path_exists_at_revision(revision: str, path: str) -> bool:
    object_name = f"{revision}:{'' if path == '.' else path}"
    result = subprocess.run(
        ["git", "cat-file", "-e", object_name],
        text=True,
        capture_output=True,
        check=False,
    )
    return result.returncode == 0


def execution_commands_for(args: argparse.Namespace) -> list[list[str]]:
    """Resolve runtime-only review scope details without changing the public command contract."""

    if args.mode != "review" or not args.path:
        return commands_for(args)
    diff_paths = resolve_review_diff_paths(
        args.path,
        lambda path: git_path_exists_at_revision(args.since, path),
    )
    if diff_paths != compact_scopes(
        [PurePosixPath(path.replace("\\", "/")).as_posix() for path in args.path]
    ):
        print(
            "BCA review widened changed-code diff scope to "
            f"{', '.join(diff_paths)} because one or more requested paths do not exist at {args.since}."
        )
    metrics = args.metric or list(DEFAULT_REVIEW_METRICS)
    return [
        report_command(args.top, args.path),
        diff_command(args.since, metrics, diff_paths),
    ]


def diff_command(since: str, metrics: list[str], paths: list[str]) -> list[str]:
    command = ["bca", "diff", "--since", since, "--format", "markdown"]
    for metric in metrics:
        command.extend(["--metric", metric])
    for path in paths:
        command.extend(["--paths", path])
    return command


def commands_for(args: argparse.Namespace) -> list[list[str]]:
    if args.mode == "check":
        return [["bca", "check", "--no-suppress", "--no-remediation"]]
    if args.mode == "report":
        return [report_command(args.top, args.path)]
    if args.mode == "diff":
        return [diff_command(args.since, args.metric, args.path)]
    if args.mode == "review":
        metrics = args.metric or list(DEFAULT_REVIEW_METRICS)
        return [
            report_command(args.top, args.path),
            diff_command(args.since, metrics, args.path),
        ]
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
    for command in execution_commands_for(args):
        result = subprocess.run(command, check=False)
        if result.returncode != 0:
            return result.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
