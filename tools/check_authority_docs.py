#!/usr/bin/env python3
"""Validate Deep Hearth's documentation graph, references, and Rust-module orientation."""

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

EXPECTED_BCA_POLICY = "**BCA policy:** ratchet"
EXPECTED_PROFILES = {
    "Universal",
    "Stateful Application",
    "Deterministic System",
    "Automated Behavior Evaluation",
}

IGNORED_DOCUMENTATION_ROOTS = {".git", "target"}

REQUIRED_LINKS = {
    # README is the routing hub. Other authority pages link back to it instead of reproducing
    # the complete authority graph.
    "README.md": {
        "AGENTS.md",
        "ARCHITECTURE.md",
        "GAME_DESIGN.md",
        "TECHNICAL_DESIGN.md",
        "STATUS.md",
        "TESTING.md",
    },
    "AGENTS.md": {"README.md"},
    "ARCHITECTURE.md": {"README.md"},
    "GAME_DESIGN.md": {"README.md"},
    "TECHNICAL_DESIGN.md": {"README.md"},
    "STATUS.md": {"README.md"},
    "TESTING.md": {"README.md"},
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


def resolve_route(document: Path, route: str) -> Path:
    """Resolve a documented repository route from the location where it is written."""

    if route.startswith("../"):
        return (document.parent / route).resolve()
    return (ROOT / route).resolve()


def documentation_files() -> tuple[str, ...]:
    """Return maintained Markdown documents, excluding generated/build metadata trees."""

    return tuple(
        sorted(
            project_relative(path)
            for path in ROOT.rglob("*.md")
            if not any(part in IGNORED_DOCUMENTATION_ROOTS for part in path.relative_to(ROOT).parts)
        )
    )


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


def inspect_markdown_links(relative: str, text: str) -> tuple[list[str], set[str], int]:
    """Check local Markdown links in one maintained document."""

    errors: list[str] = []
    links: set[str] = set()
    checked = 0
    document = ROOT / relative
    for match in MARKDOWN_LINK.finditer(text):
        target = link_target(match.group(1))
        if target is None:
            continue
        checked += 1
        resolved = (document.parent / target).resolve()
        if not resolved.exists():
            errors.append(f"{relative}: broken local Markdown link: {target}")
            continue
        links.add(project_relative(resolved))
    return errors, links, checked


def inspect_repository_routes(relative: str, text: str) -> tuple[list[str], set[str], int]:
    """Check repository paths and collect local-CI commands named by one document."""

    errors: list[str] = []
    commands: set[str] = set()
    checked = 0
    document = ROOT / relative
    for code in INLINE_CODE.findall(text):
        if code.startswith("python ci.py"):
            commands.add(code)
        for match in ROUTE_REFERENCE.finditer(code):
            route = match.group(1).rstrip(".,;:")
            checked += 1
            if not resolve_route(document, route).exists():
                errors.append(f"{relative}: missing repository route: {route}")
    return errors, commands, checked


def inspect_cargo_aliases(relative: str, text: str, aliases: set[str]) -> tuple[list[str], int]:
    """Check concrete Cargo aliases named by one document."""

    errors: list[str] = []
    checked = 0
    for match in CARGO_COMMAND.finditer(text):
        command = match.group(1).rstrip(".,;:")
        if "{" in command or "<" in command or "[" in command or "-" not in command:
            continue
        checked += 1
        if command not in aliases:
            errors.append(f"{relative}: unknown Cargo alias: cargo {command}")
    return errors, checked


def check_execution_card(documents: dict[str, str]) -> list[str]:
    """Check machine-discoverable portfolio declarations in AGENTS.md."""

    errors: list[str] = []
    agents = documents.get("AGENTS.md", "")
    bca_declarations = [
        line.strip() for line in agents.splitlines() if line.startswith("**BCA policy:**")
    ]
    if bca_declarations != [EXPECTED_BCA_POLICY]:
        errors.append(
            "AGENTS.md: declare exactly one `**BCA policy:** ratchet` near the project entry point"
        )

    profile_prefix = "**Applicable profiles:**"
    profile_declarations = [
        line.strip() for line in agents.splitlines() if line.startswith(profile_prefix)
    ]
    if len(profile_declarations) != 1:
        errors.append("AGENTS.md: declare exactly one `**Applicable profiles:**` line")
        return errors

    declared = {
        profile.strip()
        for profile in profile_declarations[0].removeprefix(profile_prefix).split(";")
        if profile.strip()
    }
    missing = EXPECTED_PROFILES - declared
    if missing:
        errors.append(
            "AGENTS.md: missing applicable portfolio profiles: " + ", ".join(sorted(missing))
        )
    return errors


def check_required_authority_links(resolved_links: dict[str, set[str]]) -> list[str]:
    """Check the bounded authority graph and optional task routing."""

    errors: list[str] = []
    for source, required in REQUIRED_LINKS.items():
        actual = resolved_links.get(source, set())
        for target in sorted(required - actual):
            errors.append(f"{source}: missing required authority link to {target}")

    if (ROOT / "TASKS.md").exists() and "README.md" in resolved_links:
        if "TASKS.md" not in resolved_links["README.md"]:
            errors.append("README.md: TASKS.md exists but is absent from the authority table")
    return errors


def check_authority_graph() -> list[str]:
    errors: list[str] = []
    documents: dict[str, str] = {}
    resolved_links: dict[str, set[str]] = {}

    for relative in AUTHORITY_FILES:
        path = ROOT / relative
        if not path.is_file():
            errors.append(f"missing required authority page: {relative}")

    for relative in documentation_files():
        path = ROOT / relative
        documents[relative] = path.read_text(encoding="utf-8")

    aliases = load_aliases()
    checked_links = 0
    checked_routes = 0
    checked_aliases = 0
    documented_ci_commands: dict[str, set[str]] = {}

    for relative, text in documents.items():
        link_errors, links, link_count = inspect_markdown_links(relative, text)
        errors.extend(link_errors)
        checked_links += link_count
        resolved_links[relative] = links

        route_errors, commands, route_count = inspect_repository_routes(relative, text)
        errors.extend(route_errors)
        checked_routes += route_count
        for command in commands:
            documented_ci_commands.setdefault(command, set()).add(relative)

        alias_errors, alias_count = inspect_cargo_aliases(relative, text, aliases)
        errors.extend(alias_errors)
        checked_aliases += alias_count

    errors.extend(check_execution_card(documents))

    for command, sources in sorted(documented_ci_commands.items()):
        error = ci_command_error(command)
        if error is not None:
            errors.append(f"{', '.join(sorted(sources))}: {error}")

    errors.extend(check_required_authority_links(resolved_links))

    if errors:
        return errors

    print(
        "documentation-contracts: PASS "
        f"({len(documents)} documents, {len(AUTHORITY_FILES)} authority pages, {checked_links} links, "
        f"{checked_routes} routes, {checked_aliases} Cargo aliases, "
        f"{len(documented_ci_commands)} local CI commands)"
    )
    return []


def check_source_module_docs() -> tuple[list[str], int]:
    errors: list[str] = []
    sources = sorted(
        path
        for root in (ROOT / "src", ROOT / "tests")
        for path in root.rglob("*.rs")
    )
    if not sources:
        return ["src/ and tests/: no maintained Rust source files found"], 0

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
