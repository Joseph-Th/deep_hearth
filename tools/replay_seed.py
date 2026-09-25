"""Shared replay-seed parsing for local gameplay tooling."""

from __future__ import annotations

import argparse
import re


def parse_replay_seed(raw: str) -> str:
    """Validate one harness-compatible u64 seed and normalize it as fixed-width hex."""

    text = raw.strip()
    if text.startswith(("0x", "0X")):
        digits = text[2:]
        if not digits or re.fullmatch(r"[0-9A-Fa-f]+", digits) is None:
            raise argparse.ArgumentTypeError(f"invalid hexadecimal u64 seed: {raw!r}")
        value = int(digits, 16)
    else:
        if not text or re.fullmatch(r"[0-9]+", text) is None:
            raise argparse.ArgumentTypeError(f"invalid decimal u64 seed: {raw!r}")
        value = int(text, 10)
    if value > 0xFFFF_FFFF_FFFF_FFFF:
        raise argparse.ArgumentTypeError(f"seed exceeds u64 range: {raw!r}")
    return f"0x{value:016X}"
