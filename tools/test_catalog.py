#!/usr/bin/env python3
"""Build-free Rust test discovery for the exact local test runner."""

from __future__ import annotations

from pathlib import Path
import re


ATTRIBUTE = re.compile(r"^\s*#\[(?P<body>.+)\]\s*$")
FUNCTION = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(?P<name>[A-Za-z_]\w*)\s*\(")
INLINE_MODULE = re.compile(r"^mod\s+(?P<name>[A-Za-z_]\w*)\s*\{$")
EXTERNAL_MODULE = re.compile(
    r"^(?:pub(?:\([^)]*\))?\s+)?mod\s+(?P<name>[A-Za-z_]\w*)\s*;$"
)
PATH_ATTRIBUTE = re.compile(r'^#\[path\s*=\s*"(?P<path>[^"]+)"\]$')
FEATURE_PREDICATE = re.compile(r'^feature\s*=\s*"(?P<name>[^"]+)"$')
TOP_LEVEL_USE = re.compile(r"^use\s+(?P<body>.*?);$", re.DOTALL)


def split_cfg_arguments(expression: str) -> list[str]:
    """Split one cfg combinator argument list without guessing nested expression structure."""

    arguments: list[str] = []
    start = 0
    depth = 0
    for index, character in enumerate(expression):
        if character == "(":
            depth += 1
        elif character == ")":
            depth -= 1
            if depth < 0:
                raise ValueError(f"unbalanced cfg expression: {expression}")
        elif character == "," and depth == 0:
            arguments.append(expression[start:index].strip())
            start = index + 1
    if depth != 0:
        raise ValueError(f"unbalanced cfg expression: {expression}")
    arguments.append(expression[start:].strip())
    if any(not argument for argument in arguments):
        raise ValueError(f"empty cfg argument: {expression}")
    return arguments


def cfg_expression_enabled(expression: str, features: set[str]) -> bool:
    """Evaluate the repository's test/feature cfg vocabulary exactly and fail on unknown predicates."""

    expression = expression.strip()
    if expression == "test":
        return True
    feature = FEATURE_PREDICATE.fullmatch(expression)
    if feature is not None:
        return feature.group("name") in features
    for combinator in ("any", "all", "not"):
        prefix = f"{combinator}("
        if not expression.startswith(prefix) or not expression.endswith(")"):
            continue
        arguments = split_cfg_arguments(expression[len(prefix) : -1])
        values = [cfg_expression_enabled(argument, features) for argument in arguments]
        if combinator == "any":
            return any(values)
        if combinator == "all":
            return all(values)
        if len(values) != 1:
            raise ValueError(f"cfg not(...) requires one argument: {expression}")
        return not values[0]
    raise ValueError(f"source catalog does not understand cfg predicate: {expression}")


def attributes_enabled(attributes: list[str], features: set[str]) -> bool:
    """Evaluate cfg attributes as a Rust test build; unknown predicates fail before Cargo runs."""

    for attribute in attributes:
        if not attribute.startswith("#[cfg("):
            continue
        if not attribute.endswith(")]"):
            raise ValueError(f"malformed cfg attribute: {attribute}")
        if not cfg_expression_enabled(attribute[6:-2], features):
            return False
    return True


def file_test_names(path: Path, prefix: tuple[str, ...], features: set[str]) -> list[str]:
    """Read direct #[test] declarations from one rustfmt-formatted source module."""

    names: list[str] = []
    pending_attributes: list[str] = []
    inline_test_module: str | None = None

    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if ATTRIBUTE.match(line):
            pending_attributes.append(stripped)
            continue

        module_match = INLINE_MODULE.match(stripped)
        if line == stripped and module_match is not None and "#[cfg(test)]" in pending_attributes:
            inline_test_module = module_match.group("name")
            pending_attributes.clear()
            continue

        if inline_test_module is not None and line == "}":
            inline_test_module = None
            pending_attributes.clear()
            continue

        function_match = FUNCTION.match(line)
        if function_match is not None and "#[test]" in pending_attributes:
            if attributes_enabled(pending_attributes, features):
                components = [*prefix]
                if inline_test_module is not None:
                    components.append(inline_test_module)
                components.append(function_match.group("name"))
                names.append("::".join(components))
            pending_attributes.clear()
            continue

        if stripped:
            pending_attributes.clear()

    return names


def explicit_module_source(attributes: list[str]) -> str | None:
    for attribute in attributes:
        match = PATH_ATTRIBUTE.match(attribute)
        if match is not None:
            return match.group("path")
    return None


def resolve_external_module_path(
    project_root: Path,
    source: Path,
    name: str,
    attributes: list[str],
) -> Path:
    explicit = explicit_module_source(attributes)
    if explicit is not None:
        candidate = source.parent / explicit
    else:
        module_root = (
            source.parent
            if source.name in {"lib.rs", "main.rs", "mod.rs"}
            else source.parent / source.stem
        )
        direct = module_root / f"{name}.rs"
        nested = module_root / name / "mod.rs"
        candidate = direct if direct.is_file() else nested
    if not candidate.is_file():
        raise ValueError(
            f"source catalog cannot resolve module {name!r} from "
            f"{source.relative_to(project_root)}"
        )
    return candidate


def external_modules(
    project_root: Path,
    path: Path,
    features: set[str],
) -> list[tuple[str, Path]]:
    modules: list[tuple[str, Path]] = []
    pending_attributes: list[str] = []

    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if line == stripped and ATTRIBUTE.match(line):
            pending_attributes.append(stripped)
            continue

        module_match = EXTERNAL_MODULE.match(stripped) if line == stripped else None
        if module_match is not None:
            if attributes_enabled(pending_attributes, features):
                name = module_match.group("name")
                modules.append(
                    (
                        name,
                        resolve_external_module_path(project_root, path, name, pending_attributes),
                    )
                )
            pending_attributes.clear()
            continue

        if stripped:
            pending_attributes.clear()

    return modules


def reachable_modules(
    project_root: Path,
    root: Path,
    features: set[str],
) -> list[tuple[Path, tuple[str, ...]]]:
    """Return one Rust crate's external module closure with stable module prefixes."""

    modules: list[tuple[Path, tuple[str, ...]]] = []
    pending: list[tuple[Path, tuple[str, ...]]] = [(root, ())]
    visited: set[tuple[Path, tuple[str, ...]]] = set()
    while pending:
        path, prefix = pending.pop()
        key = (path.resolve(), prefix)
        if key in visited:
            continue
        visited.add(key)
        modules.append((path, prefix))
        for module, module_path in external_modules(project_root, path, features):
            pending.append((module_path, (*prefix, module)))
    return modules


def root_sibling_imports(
    source: str,
    module_depth: int,
    features: set[str] | None = None,
) -> set[str]:
    """Return enabled crate-root sibling modules referenced by ancestor-relative imports."""

    required: set[str] = set()
    prefix = "super::" * module_depth
    pending_attributes: list[str] = []
    lines = source.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if line == stripped and ATTRIBUTE.match(line):
            pending_attributes.append(stripped)
            index += 1
            continue
        if not line.startswith("use "):
            if stripped and not stripped.startswith(("//", "//!", "///")):
                pending_attributes.clear()
            index += 1
            continue

        parts = [line]
        while not parts[-1].rstrip().endswith(";"):
            index += 1
            if index >= len(lines):
                raise ValueError("unterminated top-level use statement")
            parts.append(lines[index])
        index += 1
        attributes = pending_attributes
        pending_attributes = []
        if not attributes_enabled(attributes, features or set()):
            continue
        statement = "\n".join(parts)
        match = TOP_LEVEL_USE.fullmatch(statement)
        if match is None:
            raise ValueError(f"source catalog cannot parse top-level use: {statement}")
        body = " ".join(match.group("body").split())
        if not body.startswith(prefix):
            continue
        body = body[len(prefix) :]
        if body.startswith("super::"):
            continue
        if body.startswith("{") and body.endswith("}"):
            for item in body[1:-1].split(","):
                name = item.strip().split("::", 1)[0].strip()
                if re.fullmatch(r"[A-Za-z_]\w*", name) and name != "self":
                    required.add(name)
            continue
        name = body.split("::", 1)[0].strip()
        if re.fullmatch(r"[A-Za-z_]\w*", name):
            required.add(name)
    return required


def missing_root_modules(
    project_root: Path,
    crate_root: Path,
    module_directory: Path,
    features: set[str],
) -> list[str]:
    """Find source-referenced root sibling modules omitted by one integration-test crate root."""

    declared = {name for name, _path in external_modules(project_root, crate_root, features)}
    available = {path.stem for path in module_directory.glob("*.rs")}
    required: set[str] = set()
    for path, prefix in reachable_modules(project_root, crate_root, features):
        if not prefix:
            continue
        required.update(
            root_sibling_imports(path.read_text(encoding="utf-8"), len(prefix), features) & available
        )
    return sorted(required - declared)


def reachable_test_names(
    project_root: Path,
    root: Path,
    features: set[str],
) -> list[str]:
    """Walk one Rust crate/module graph and return only tests reachable from its root."""

    names: list[str] = []
    for path, prefix in reachable_modules(project_root, root, features):
        names.extend(file_test_names(path, prefix, features))
    return names
