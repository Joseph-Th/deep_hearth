"""Shared compact rendering for local Cargo/test command output."""

from __future__ import annotations

import re


FAILURE_HEAD_LINES = 16
FAILURE_TAIL_LINES = 64
RUST_TEST_RESULT = re.compile(
    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored;"
)


def rust_test_result_counts(output: str) -> list[tuple[int, int]]:
    """Return `(passed, ignored)` for every successful Rust test-result line."""

    return [
        (int(match.group("passed")), int(match.group("ignored")))
        for match in RUST_TEST_RESULT.finditer(output)
    ]


def bounded_failure_output(output: str) -> str:
    """Retain useful failure context without flooding a local repair loop."""

    lines = output.rstrip().splitlines()
    limit = FAILURE_HEAD_LINES + FAILURE_TAIL_LINES
    if len(lines) <= limit:
        return "\n".join(lines)
    omitted = len(lines) - limit
    return "\n".join(
        [
            *lines[:FAILURE_HEAD_LINES],
            f"... {omitted} line(s) omitted ...",
            *lines[-FAILURE_TAIL_LINES:],
        ]
    )


def bounded_failure_streams(stdout: str, stderr: str) -> list[str]:
    """Return non-empty bounded diagnostics once when captured streams duplicate them."""

    rendered: list[str] = []
    for stream in (stdout, stderr):
        if not stream.strip():
            continue
        bounded = bounded_failure_output(stream)
        if bounded not in rendered:
            rendered.append(bounded)
    return rendered
