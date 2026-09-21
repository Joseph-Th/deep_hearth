#!/usr/bin/env python3
"""Bounded, repository-owned entry points for advisory Rust agent diagnostics."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
MUTANT_OUTPUT_ROOT = ROOT / "target" / "agent-output" / "rust-diagnostics" / "mutants"


def normalize_module_focus(focus: str) -> str:
    """Make the common owner-name form valid for cargo-modules."""

    focus = focus.strip()
    if focus.startswith(("crate::", "self::", "super::")):
        return focus
    return f"crate::{focus.lstrip(':')}"


def append_feature_args(command: list[str], args: argparse.Namespace) -> None:
    if args.all_features:
        command.append("--all-features")
    elif args.features:
        command.extend(("--features", ",".join(args.features)))


def modules_command(args: argparse.Namespace) -> list[str]:
    command = ["cargo", "modules", args.mode, "--lib"]
    append_feature_args(command, args)
    if args.tests:
        command.append("--cfg-test")
    depth = args.depth if args.depth is not None else (1 if args.mode == "dependencies" else 4)
    if args.mode == "structure":
        command.extend(("--no-fns", "--no-traits", "--no-types", "--max-depth", str(depth)))
    elif args.mode == "dependencies":
        command.extend(
            (
                "--no-externs",
                "--no-fns",
                "--no-sysroot",
                "--no-traits",
                "--no-types",
                "--no-owns",
                "--max-depth",
                str(depth),
            )
        )
    if args.focus:
        command.extend(("--focus-on", normalize_module_focus(args.focus)))
    return command


def mutants_command(args: argparse.Namespace, output_dir: Path | None = None) -> list[str]:
    command = ["cargo", "mutants", "--file", args.file]
    if args.regex:
        command.extend(("-F", args.regex))
    append_feature_args(command, args)
    if not args.run:
        command.append("--list")
        return command

    assert output_dir is not None
    command.extend(("-j", str(args.jobs), "-o", str(output_dir)))
    if args.timeout is not None:
        command.extend(("-t", str(args.timeout)))
    if args.skip_baseline:
        command.extend(("--baseline", "skip"))
    return command


def expand_command(args: argparse.Namespace) -> list[str]:
    command = ["cargo", "expand", "--quiet", "--locked", "--color", "never", "--lib"]
    append_feature_args(command, args)
    if args.tests:
        command.append("--tests")
    command.append(args.item)
    return command


def filtered_expansion(text: str, pattern: str, context: int) -> str:
    """Return merged, line-numbered windows around matching expanded source."""

    regex = re.compile(pattern)
    lines = text.splitlines()
    spans: list[tuple[int, int]] = []
    for index, line in enumerate(lines):
        if regex.search(line) is None:
            continue
        start = max(0, index - context)
        end = min(len(lines), index + context + 1)
        if spans and start <= spans[-1][1]:
            spans[-1] = (spans[-1][0], max(spans[-1][1], end))
        else:
            spans.append((start, end))
    rendered: list[str] = []
    for span_index, (start, end) in enumerate(spans):
        if span_index:
            rendered.append("...")
        rendered.extend(f"{index + 1:>6}: {lines[index]}" for index in range(start, end))
    return "\n".join(rendered)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="tool", required=True)

    modules = subcommands.add_parser(
        "modules",
        help="inspect module ownership, dependency shape, or unlinked source without changing code",
    )
    modules.add_argument(
        "mode",
        nargs="?",
        choices=("structure", "dependencies", "orphans"),
        default="structure",
    )
    modules.add_argument("--focus", help="owner/module path; bare paths are normalized to crate::<path>")
    modules.add_argument(
        "--depth",
        type=int,
        help="bounded depth override; defaults to 4 for structure and 1 for dependencies",
    )
    modules.add_argument("--tests", action="store_true", help="include #[cfg(test)] module linkage")
    modules.add_argument("--features", action="append", default=[], help="feature set; repeat as needed")
    modules.add_argument("--all-features", action="store_true")
    mutants = subcommands.add_parser(
        "mutants",
        help="list targeted mutants by default; execute only an explicitly selected regex with --run",
    )
    mutants.add_argument("file", help="owner Rust source file to mutate")
    mutants.add_argument("--re", dest="regex", help="mutation-name regex, usually an invariant-bearing function")
    mutants.add_argument("--run", action="store_true", help="execute selected mutants instead of listing them")
    mutants.add_argument("--jobs", type=int, default=2, help="bounded concurrent mutant jobs")
    mutants.add_argument("--timeout", type=int, help="optional per-command timeout in seconds")
    mutants.add_argument(
        "--skip-baseline",
        action="store_true",
        help="skip cargo-mutants' baseline only when the unchanged test surface was already proven",
    )
    mutants.add_argument("--features", action="append", default=[], help="feature set; repeat as needed")
    mutants.add_argument("--all-features", action="store_true")

    expand = subcommands.add_parser(
        "expand",
        help="inspect macro/derive-generated Rust for one named item or containing module",
    )
    expand.add_argument("item", help="module/item path accepted by cargo expand")
    expand.add_argument("--tests", action="store_true", help="include test expansion")
    expand.add_argument("--features", action="append", default=[], help="feature set; repeat as needed")
    expand.add_argument("--all-features", action="store_true")
    expand.add_argument(
        "--grep",
        help="show only expanded-source windows matching this regex; useful for sibling derive impls",
    )
    expand.add_argument("--context", type=int, default=4, help="lines around each --grep match")
    return parser


def validate_args(parser: argparse.ArgumentParser, args: argparse.Namespace) -> None:
    if getattr(args, "all_features", False) and getattr(args, "features", []):
        parser.error("--all-features cannot be combined with --features")
    if args.tool == "modules":
        if args.depth is not None and args.depth < 1:
            parser.error("--depth must be at least 1")
        if args.mode == "orphans" and args.focus:
            parser.error("orphans mode is crate-wide and does not accept --focus")
    elif args.tool == "mutants":
        if not args.file.endswith(".rs"):
            parser.error("mutants expects one Rust source file")
        if not (ROOT / args.file).is_file():
            parser.error(f"mutation source does not exist: {args.file}")
        if args.jobs < 1:
            parser.error("--jobs must be at least 1")
        if args.timeout is not None and args.timeout < 1:
            parser.error("--timeout must be at least 1 second")
        if args.run and not args.regex:
            parser.error("--run requires --re so mutation execution stays targeted")
        if not args.run and args.skip_baseline:
            parser.error("--skip-baseline is meaningful only with --run")
    elif args.tool == "expand" and args.context < 0:
        parser.error("--context cannot be negative")


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = build_parser()
    args = parser.parse_args(argv)
    validate_args(parser, args)
    return args


def diagnostic_environment() -> dict[str, str]:
    environment = os.environ.copy()
    environment["CARGO_TERM_COLOR"] = "never"
    environment["NO_COLOR"] = "1"
    return environment


def run_streaming(command: list[str]) -> int:
    print("+ " + " ".join(command), flush=True)
    return subprocess.run(command, cwd=ROOT, env=diagnostic_environment(), check=False).returncode


def run_expand(args: argparse.Namespace) -> int:
    command = expand_command(args)
    print("+ " + " ".join(command), file=sys.stderr, flush=True)
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=diagnostic_environment(),
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        if result.stdout:
            print(result.stdout, end="")
        if result.stderr:
            print(result.stderr, end="", file=sys.stderr)
        return result.returncode
    if args.grep is None:
        print(result.stdout, end="")
        return 0
    try:
        filtered = filtered_expansion(result.stdout, args.grep, args.context)
    except re.error as error:
        print(f"invalid --grep regex: {error}", file=sys.stderr)
        return 2
    if not filtered:
        print(f"no expanded source matched {args.grep!r}", file=sys.stderr)
        return 1
    print(filtered)
    return 0


def main() -> int:
    args = parse_args()
    if args.tool == "modules":
        return run_streaming(modules_command(args))
    if args.tool == "expand":
        return run_expand(args)

    output_dir = None
    if args.run:
        MUTANT_OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
        output_dir = Path(
            tempfile.mkdtemp(
                prefix=f"{Path(args.file).stem}-",
                dir=MUTANT_OUTPUT_ROOT,
            )
        )
        print(f"mutation logs: {output_dir.relative_to(ROOT)}")
    return run_streaming(mutants_command(args, output_dir))


if __name__ == "__main__":
    raise SystemExit(main())
