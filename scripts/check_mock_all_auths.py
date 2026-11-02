#!/usr/bin/env python3
"""CI guard: ``mock_all_auths`` usage in Rust tests may only shrink.

``Env::mock_all_auths()`` accepts every ``require_auth`` call, so a test that
uses it still passes when an entrypoint authorises the wrong address. New
tests must scope auth with ``mock_auths(&[MockAuth { .. }])`` instead, or
assert on ``env.auths()`` after the call (see ``src/security_test.rs``).

Existing call sites are grandfathered in ``MAX_ALLOWED``. The count may go
down, never up. When a file is converted to exact auth mocks, lower the
constant to the new total in the same change.

Exit status:
  0 — usage is at or below the allowed count.
  1 — usage grew; new tests must use exact auth mocks.
"""

import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SCAN_DIRS = ["src", "tests", "contracts", "fuzz"]
NEEDLE = "mock_all_auths()"

# Current number of `mock_all_auths()` call sites across SCAN_DIRS.
MAX_ALLOWED = 254


def count_usages() -> dict[pathlib.Path, int]:
    counts: dict[pathlib.Path, int] = {}
    for scan_dir in SCAN_DIRS:
        for path in sorted((ROOT / scan_dir).rglob("*.rs")):
            if "target" in path.parts:
                continue
            hits = path.read_text(encoding="utf-8").count(NEEDLE)
            if hits:
                counts[path.relative_to(ROOT)] = hits
    return counts


def main() -> int:
    counts = count_usages()
    total = sum(counts.values())

    if total <= MAX_ALLOWED:
        print(f"mock_all_auths usage: {total} (allowed: {MAX_ALLOWED})")
        return 0

    print(
        f"mock_all_auths usage grew to {total} (allowed: {MAX_ALLOWED}).\n"
        "Write-path tests must use exact auth mocks, not mock_all_auths():\n"
        "  env.mock_auths(&[MockAuth { address, invoke }]) or check env.auths().\n"
        "Per-file counts:"
    )
    for path, hits in counts.items():
        print(f"  {hits:4d}  {path}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
