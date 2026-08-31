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
    "DIRECTION.md",
    "GAMEPLAY_EVALUATION.md",
)

EXPECTED_BCA_POLICY = "**BCA policy:** ratchet"
EXPECTED_PROFILES = {
    "Universal",
    "Stateful Application",
    "Deterministic System",
    "Automated Behavior Evaluation",
}

IGNORED_DOCUMENTATION_ROOTS = {".git", "target"}
COLD_START_DOCUMENT_MAX_BYTES = {
    "AGENTS.md": 4_200,
    "README.md": 11_000,
    "STATUS.md": 12_500,
    "TESTING.md": 10_500,
}
# Preserve reserve below the tool-facing cold-start envelope. New orientation prose must earn
# context budget rather than consuming the entire envelope by default.
COLD_START_TOTAL_MAX_BYTES = 38_000

REQUIRED_AUTHORITY_SECTIONS = {
    "AGENTS.md": ("Cold start", "Operating protocol", "Guardrails", "Completion"),
    "README.md": (
        "Orientation",
        "Abstraction ladder",
        "Control coordinate",
        "Source role map",
        "Authorities",
        "Task map",
        "Change-impact map",
    ),
    "STATUS.md": (
        "Ordinary play",
        "Current integration frontier",
        "Implemented infrastructure",
        "Capability-only evaluation",
        "Absent scope",
    ),
    "TESTING.md": (
        "Fast path",
        "Evidence ladder",
        "Complexity review",
        "Unit tests",
        "Gameplay evaluation",
        "Completion",
    ),
    "ARCHITECTURE.md": (
        "Contract map",
        "State model",
        "Agent-legible control grammar",
        "Abstraction and dependency direction",
        "Ownership",
        "Mutation and failure",
        "Determinism",
        "Persistence and adapters",
        "Invariants",
        "Cross-owner flow discipline",
        "API and representation rules",
        "Naming",
        "Source and comment contracts",
    ),
    "TECHNICAL_DESIGN.md": (
        "Contract map",
        "System control model",
        "Subsystem contract card",
        "Global runtime facts",
        "Runtime owners",
        "Physical quantities",
        "Materials, inventory, and geology",
        "Production and processing",
        "Equipment, labor, survival, energy, and fluids",
        "Structures",
        "Spatial and presentation boundaries",
        "Trusted load",
    ),
    "GAME_DESIGN.md": (
        "Design map",
        "Core experience",
        "Design laws",
        "Control-oriented legibility",
        "Player loop",
        "System direction",
        "Progression",
        "Player information",
        "Mechanic acceptance",
        "Development direction",
        "Boundary",
    ),
    "DIRECTION.md": (
        "Planning map",
        "Accretion objective",
        "Control-surface program",
        "Default integration sequence",
        "Vertical-slice completion contract",
        "What not to accrete",
    ),
    "GAMEPLAY_EVALUATION.md": (
        "Evaluation map",
        "Actor contract",
        "Evidence modes",
        "Decision evidence",
        "Focused scopes",
        "Counterfactual and replay discipline",
    ),
}

REQUIRED_LINKS = {
    # README is the routing hub. Other authority pages link back to it instead of reproducing
    # the complete authority graph.
    "README.md": {
        "AGENTS.md",
        "ARCHITECTURE.md",
        "DIRECTION.md",
        "GAMEPLAY_EVALUATION.md",
        "GAME_DESIGN.md",
        "TECHNICAL_DESIGN.md",
        "STATUS.md",
        "TESTING.md",
    },
    "AGENTS.md": {"README.md"},
    "ARCHITECTURE.md": {"README.md"},
    "GAME_DESIGN.md": {"README.md"},
    "DIRECTION.md": {"README.md"},
    "GAMEPLAY_EVALUATION.md": {"README.md", "TESTING.md"},
    "TECHNICAL_DESIGN.md": {"README.md"},
    "STATUS.md": {"README.md"},
    "TESTING.md": {"README.md"},
}

MARKDOWN_LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
MARKDOWN_HEADING = re.compile(r"^#{1,6}\s+(.+?)\s*#*\s*$", re.MULTILINE)
CARGO_COMMAND = re.compile(r"\bcargo\s+([^\s`]+)")
INLINE_CODE = re.compile(r"`([^`\n]+)`")
CI_BRACED_CHOICE = re.compile(r"\{([^{}]+)\}")
ROUTE_REFERENCE = re.compile(
    r"(?<![A-Za-z0-9_.-])"
    r"(\.\./[A-Za-z0-9_./-]+|"
    r"(?:src|tests|tools|assets|\.cargo)/[A-Za-z0-9_./-]+|"
    r"(?:ci\.py|Cargo\.toml|README\.md|STATUS\.md|TESTING\.md|ARCHITECTURE\.md|"
    r"TECHNICAL_DESIGN\.md|GAME_DESIGN\.md|DIRECTION\.md|GAMEPLAY_EVALUATION\.md|AGENTS\.md|TASKS\.md))"
)
PUBLIC_TOP_LEVEL_MODULE = re.compile(r"^pub mod ([a-z_][a-z0-9_]*);$", re.MULTILINE)
SYSTEM_STATE_FIELD = re.compile(
    r"^\s*[a-z_][A-Za-z0-9_]*:\s*([A-Za-z][A-Za-z0-9_]*),\s*$", re.MULTILINE
)
DOCUMENTED_RUNTIME_OWNER = re.compile(
    r"^\|\s*`([A-Za-z][A-Za-z0-9_]*State)`\s*\|", re.MULTILINE
)
DOCUMENTED_SOURCE_MODULE = re.compile(r"`src/([a-z_][a-z0-9_]*)/`")
PUBLIC_MUT_SELF = re.compile(
    r"^\s*pub(?:\s+const)?\s+fn\s+([a-z_][a-z0-9_]*)\s*\(\s*&mut\s+self",
    re.MULTILINE,
)
LEVEL_TWO_HEADING = re.compile(r"^## ([^\r\n]+?)\s*$", re.MULTILINE)


def local_link_parts(raw: str) -> tuple[str, str | None] | None:
    target = raw.strip()
    if target.startswith("<") and ">" in target:
        target = target[1 : target.index(">")]
    else:
        target = target.split(maxsplit=1)[0]
    if not target or target.startswith(("http://", "https://", "mailto:")):
        return None
    path, separator, fragment = target.partition("#")
    return unquote(path), unquote(fragment) if separator else None


def link_target(raw: str) -> str | None:
    """Return only the local path portion retained for callers that do not need anchors."""

    parts = local_link_parts(raw)
    if parts is None or not parts[0]:
        return None
    return parts[0]


def markdown_heading_anchors(text: str) -> set[str]:
    """Return GitHub-style anchors for maintained Markdown headings, including duplicate suffixes."""

    counts: dict[str, int] = {}
    anchors: set[str] = set()
    for raw_heading in MARKDOWN_HEADING.findall(text):
        heading = re.sub(r"`([^`]*)`", r"\1", raw_heading)
        heading = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", heading)
        base = re.sub(r"[^\w\- ]", "", heading.lower(), flags=re.UNICODE)
        base = re.sub(r"\s+", "-", base.strip())
        if not base:
            continue
        duplicate_index = counts.get(base, 0)
        counts[base] = duplicate_index + 1
        anchors.add(base if duplicate_index == 0 else f"{base}-{duplicate_index}")
    return anchors


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


def markdown_section(text: str, heading: str) -> str:
    """Return one level-two Markdown section body without lower-level heading parsing."""

    marker = f"## {heading}"
    start = text.find(marker)
    if start < 0:
        return ""
    body_start = start + len(marker)
    next_heading = text.find("\n## ", body_start)
    if next_heading < 0:
        return text[body_start:]
    return text[body_start:next_heading]


def public_top_level_modules() -> list[str]:
    """Return the crate's public top-level modules in source declaration order."""

    text = (ROOT / "src" / "lib.rs").read_text(encoding="utf-8")
    return PUBLIC_TOP_LEVEL_MODULE.findall(text)


def documented_source_role_modules(readme: str) -> list[str]:
    """Return top-level modules classified by README's source-role map."""

    return DOCUMENTED_SOURCE_MODULE.findall(markdown_section(readme, "Source role map"))


def system_state_owner_types() -> list[str]:
    """Return durable root owner types from `SystemState` in persisted field order."""

    text = (ROOT / "src" / "core" / "state.rs").read_text(encoding="utf-8")
    match = re.search(r"struct SystemState\s*\{(?P<body>.*?)\n\}", text, re.DOTALL)
    if match is None:
        return []
    return SYSTEM_STATE_FIELD.findall(match.group("body"))


def documented_runtime_owner_types(technical_design: str) -> list[str]:
    """Return the runtime-owner atlas types in documented order."""

    return DOCUMENTED_RUNTIME_OWNER.findall(markdown_section(technical_design, "Runtime owners"))


def public_root_mutator_names(state_source: str) -> list[str]:
    """Return public mutable-self methods forbidden on the AppState/root-state source surface."""

    return PUBLIC_MUT_SELF.findall(state_source)


def check_cold_start_context_budget(documents: dict[str, str]) -> list[str]:
    """Bound automatically loaded orientation prose so cold-start context cannot grow invisibly."""

    errors: list[str] = []
    total = 0
    for relative, maximum in COLD_START_DOCUMENT_MAX_BYTES.items():
        size = len(documents.get(relative, "").encode("utf-8"))
        total += size
        if size > maximum:
            errors.append(
                f"{relative}: cold-start document is {size} bytes; budget is {maximum} bytes"
            )
    if total > COLD_START_TOTAL_MAX_BYTES:
        errors.append(
            "cold-start authority set is "
            f"{total} bytes; aggregate budget is {COLD_START_TOTAL_MAX_BYTES} bytes"
        )
    return errors


def cold_start_context_usage(documents: dict[str, str]) -> tuple[int, int]:
    """Return current cold-start bytes and reserved aggregate headroom."""

    used = sum(
        len(documents.get(relative, "").encode("utf-8"))
        for relative in COLD_START_DOCUMENT_MAX_BYTES
    )
    return used, max(0, COLD_START_TOTAL_MAX_BYTES - used)


def check_required_authority_sections(documents: dict[str, str]) -> list[str]:
    """Keep stable level-two routing landmarks present and unambiguous in authority pages."""

    errors: list[str] = []
    for relative, required in REQUIRED_AUTHORITY_SECTIONS.items():
        headings = LEVEL_TWO_HEADING.findall(documents.get(relative, ""))
        duplicate_headings = sorted(
            heading for heading in set(headings) if headings.count(heading) > 1
        )
        if duplicate_headings:
            errors.append(
                f"{relative}: duplicate level-two authority headings: "
                + ", ".join(duplicate_headings)
            )
        missing = [heading for heading in required if heading not in headings]
        if missing:
            errors.append(
                f"{relative}: missing required authority sections: " + ", ".join(missing)
            )
    return errors


def check_source_orientation_maps(documents: dict[str, str]) -> list[str]:
    """Keep agent routing maps synchronized with the live crate and persistent root state."""

    errors: list[str] = []
    readme_modules = documented_source_role_modules(documents.get("README.md", ""))
    source_modules = public_top_level_modules()
    duplicate_modules = sorted(
        module for module in set(readme_modules) if readme_modules.count(module) != 1
    )
    missing_modules = sorted(set(source_modules) - set(readme_modules))
    extra_modules = sorted(set(readme_modules) - set(source_modules))
    if duplicate_modules:
        errors.append(
            "README.md: source role map classifies modules more than once: "
            + ", ".join(duplicate_modules)
        )
    if missing_modules:
        errors.append(
            "README.md: source role map is missing public top-level modules: "
            + ", ".join(missing_modules)
        )
    if extra_modules:
        errors.append(
            "README.md: source role map names non-public top-level modules: "
            + ", ".join(extra_modules)
        )

    source_owners = system_state_owner_types()
    documented_owners = documented_runtime_owner_types(documents.get("TECHNICAL_DESIGN.md", ""))
    if not source_owners:
        errors.append("src/core/state.rs: could not discover SystemState runtime owners")
    elif documented_owners != source_owners:
        errors.append(
            "TECHNICAL_DESIGN.md: runtime owner atlas must match SystemState field types in order; "
            f"documented={documented_owners!r} source={source_owners!r}"
        )
    state_source = (ROOT / "src" / "core" / "state.rs").read_text(encoding="utf-8")
    public_mutators = public_root_mutator_names(state_source)
    if public_mutators:
        errors.append(
            "src/core/state.rs: root state exposes public mutable-self methods: "
            + ", ".join(public_mutators)
        )
    return errors


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
        parts = local_link_parts(match.group(1))
        if parts is None:
            continue
        target, fragment = parts
        checked += 1
        resolved = document.resolve() if not target else (document.parent / target).resolve()
        if not resolved.exists():
            errors.append(f"{relative}: broken local Markdown link: {target}")
            continue
        links.add(project_relative(resolved))
        if fragment and resolved.suffix.lower() == ".md":
            target_text = text if resolved == document.resolve() else resolved.read_text(encoding="utf-8")
            if fragment.lower() not in markdown_heading_anchors(target_text):
                display = f"{target}#{fragment}" if target else f"#{fragment}"
                errors.append(f"{relative}: broken local Markdown anchor: {display}")
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
    errors.extend(check_cold_start_context_budget(documents))
    errors.extend(check_required_authority_sections(documents))
    errors.extend(check_source_orientation_maps(documents))

    for command, sources in sorted(documented_ci_commands.items()):
        error = ci_command_error(command)
        if error is not None:
            errors.append(f"{', '.join(sorted(sources))}: {error}")

    errors.extend(check_required_authority_links(resolved_links))

    if errors:
        return errors

    cold_start_used, cold_start_reserve = cold_start_context_usage(documents)

    print(
        "documentation-contracts: PASS "
        f"({len(documents)} documents, {len(AUTHORITY_FILES)} authority pages, {checked_links} links, "
        f"{checked_routes} routes, {checked_aliases} Cargo aliases, "
        f"{len(documented_ci_commands)} local CI commands, "
        f"cold-start {cold_start_used}/{COLD_START_TOTAL_MAX_BYTES} bytes, "
        f"reserve {cold_start_reserve} bytes)"
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
