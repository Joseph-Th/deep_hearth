#!/usr/bin/env python3
"""Validate Deep Hearth's documentation graph, references, and source-module orientation."""

from __future__ import annotations

import contextlib
import io
from pathlib import Path
import re
import shlex
import sys
import tomllib
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

import ci  # noqa: E402

AUTHORITY_FILES = (
    "AGENTS.md",
    "README.md",
    "STATUS.md",
    "TESTING.md",
    "ARCHITECTURE.md",
    "TECHNICAL_DESIGN.md",
    "GAME_DESIGN.md",
)

REQUIRED_LINKS = {
    "README.md": {
        "AGENTS.md",
        "ARCHITECTURE.md",
        "GAME_DESIGN.md",
        "TECHNICAL_DESIGN.md",
        "STATUS.md",
        "TESTING.md",
    },
    "STATUS.md": {
        "README.md",
        "ARCHITECTURE.md",
        "TECHNICAL_DESIGN.md",
        "GAME_DESIGN.md",
        "TESTING.md",
    },
    "TESTING.md": {"README.md", "STATUS.md"},
}

MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
CARGO_COMMAND = re.compile(r"\bcargo\s+([^\s`]+)")
INLINE_CODE = re.compile(r"`([^`\n]+)`")
CI_BRACED_CHOICE = re.compile(r"\{([^{}]+)\}")
ROUTE_REFERENCE = re.compile(
    r"(?<![A-Za-z0-9_.-])"
    r"(\.\./[A-Za-z0-9_./-]+|"
    r"(?:src|tests|tools|assets|\.cargo)/[A-Za-z0-9_./-]+|"
    r"(?:ci\.py|Cargo\.toml|README\.md|STATUS\.md|TESTING\.md|ARCHITECTURE\.md|"
    r"TECHNICAL_DESIGN\.md|GAME_DESIGN\.md|AGENTS\.md|TASKS\.md))"
)


def link_target(raw: str) -> str | None:
    target = raw.strip()
    if target.startswith("<") and ">" in target:
        target = target[1 : target.index(">")]
    else:
        target = target.split(maxsplit=1)[0]
    if not target or target.startswith(("#", "http://", "https://", "mailto:")):
        return None
    return unquote(target.split("#", maxsplit=1)[0])


def project_relative(path: Path) -> str:
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def load_aliases() -> set[str]:
    config = ROOT / ".cargo" / "config.toml"
    with config.open("rb") as handle:
        parsed = tomllib.load(handle)
    aliases = parsed.get("alias", {})
    if not isinstance(aliases, dict):
        return set()
    return set(aliases)


def normalize_ci_command(command: str) -> str | None:
    """Resolve documentation-only choice notation to one representative local CI command."""

    if not command.startswith("python ci.py"):
        return None
    normalized = CI_BRACED_CHOICE.sub(
        lambda match: match.group(1).split(",", maxsplit=1)[0], command
    )
    normalized = normalized.replace("[scope]", "workshop")
    if any(marker in normalized for marker in ("{", "}", "[", "]", "<", ">")):
        return None
    return normalized


def ci_command_error(command: str) -> str | None:
    """Return an error for one documented ci.py command without spawning or executing its plan."""

    normalized = normalize_ci_command(command)
    if normalized is None:
        return None
    try:
        parts = shlex.split(normalized)
    except ValueError as error:
        return f"invalid local CI command syntax: {command}: {error}"
    if parts[:2] != ["python", "ci.py"]:
        return None
    args = parts[2:]
    stderr = io.StringIO()
    try:
        with contextlib.redirect_stderr(stderr):
            parsed = ci.parse_args(args)
        ci.plan_for(parsed)
    except SystemExit:
        detail = stderr.getvalue().strip().splitlines()
        reason = detail[-1] if detail else "argument parsing failed"
        return f"invalid local CI command: {command} ({reason})"
    except ValueError as error:
        return f"invalid local CI command: {command} ({error})"
    else:
        return None


def check_authority_graph() -> list[str]:
    errors: list[str] = []
    documents: dict[str, str] = {}
    resolved_links: dict[str, set[str]] = {}

    for relative in AUTHORITY_FILES:
        path = ROOT / relative
        if not path.is_file():
            errors.append(f"missing required authority page: {relative}")
            continue
        documents[relative] = path.read_text(encoding="utf-8")

    aliases = load_aliases()
    checked_links = 0
    checked_routes = 0
    checked_aliases = 0
    documented_ci_commands: dict[str, set[str]] = {}

    for relative, text in documents.items():
        document = ROOT / relative
        links: set[str] = set()
        for match in MARKDOWN_LINK.finditer(text):
            target = link_target(match.group(1))
            if target is None:
                continue
            resolved = (document.parent / target).resolve()
            checked_links += 1
            if not resolved.exists():
                errors.append(f"{relative}: broken local Markdown link: {target}")
                continue
            links.add(project_relative(resolved))
        resolved_links[relative] = links

        for code in INLINE_CODE.findall(text):
            if code.startswith("python ci.py"):
                documented_ci_commands.setdefault(code, set()).add(relative)
            for match in ROUTE_REFERENCE.finditer(code):
                route = match.group(1).rstrip(".,;:")
                candidate = (ROOT / route).resolve()
                checked_routes += 1
                if not candidate.exists():
                    errors.append(f"{relative}: missing repository route: {route}")

        for match in CARGO_COMMAND.finditer(text):
            command = match.group(1).rstrip(".,;:")
            if "{" in command or "<" in command or "[" in command or "-" not in command:
                continue
            checked_aliases += 1
            if command not in aliases:
                errors.append(f"{relative}: unknown Cargo alias: cargo {command}")

    for command, sources in sorted(documented_ci_commands.items()):
        error = ci_command_error(command)
        if error is not None:
            errors.append(f"{', '.join(sorted(sources))}: {error}")

    for source, required in REQUIRED_LINKS.items():
        actual = resolved_links.get(source, set())
        for target in sorted(required - actual):
            errors.append(f"{source}: missing required authority link to {target}")

    if (ROOT / "TASKS.md").exists() and "README.md" in resolved_links:
        if "TASKS.md" not in resolved_links["README.md"]:
            errors.append("README.md: TASKS.md exists but is absent from the authority table")

    if errors:
        return errors

    print(
        "documentation-authority: PASS "
        f"({len(documents)} pages, {checked_links} links, "
        f"{checked_routes} routes, {checked_aliases} Cargo aliases, "
        f"{len(documented_ci_commands)} local CI commands)"
    )
    return []


def check_source_module_docs() -> tuple[list[str], int]:
    errors: list[str] = []
    sources = sorted((ROOT / "src").rglob("*.rs"))
    if not sources:
        return ["src/: no Rust source files found"], 0

    for path in sources:
        relative = project_relative(path)
        lines = path.read_text(encoding="utf-8").splitlines()
        first_nonblank = next((line.strip() for line in lines if line.strip()), "")
        if not first_nonblank.startswith("//!"):
            errors.append(f"{relative}: first nonblank line must be a //! module-purpose comment")
            continue
        if not first_nonblank.removeprefix("//!").strip():
            errors.append(f"{relative}: module-purpose comment must describe the module")

    return errors, len(sources)


def main() -> int:
    errors = check_authority_graph()
    source_errors, checked_sources = check_source_module_docs()
    errors.extend(source_errors)
    if not errors:
        print(f"source-module-docs: PASS ({checked_sources} Rust files)")
        return 0
    for error in sorted(set(errors)):
        print(f"documentation-contracts: {error}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
