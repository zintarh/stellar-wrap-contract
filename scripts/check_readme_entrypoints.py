#!/usr/bin/env python3
"""CI guard: every contract entrypoint in ``src/lib.rs`` must be documented.

Reads the entrypoints from the two ``#[contractimpl]`` blocks in ``src/lib.rs``
(the inherent ``impl StellarWrapContract`` and the ``TokenInterface`` impl) and
asserts that each appears in ``README.md``. This flags the pattern where a new
public function is added to the contract without a matching README entry.

Exit status:
  0 — all entrypoints are documented.
  1 — one or more entrypoints are missing from the README.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
LIB_PATH = ROOT / "src" / "lib.rs"
README_PATH = ROOT / "README.md"

# Matches `pub fn name(...)` (inherent impl) and `fn name(...)` (trait impl).
# Contract entrypoints are the only functions declared at this indentation in
# lib.rs, so this regex is unambiguous.
ENTRYPOINT_RE = re.compile(r"^\s*(?:pub\s+)?fn\s+(\w+)\s*\(")


def extract_entrypoints(lib_text: str) -> list[str]:
    names: list[str] = []
    for line in lib_text.splitlines():
        match = ENTRYPOINT_RE.match(line)
        if match:
            names.append(match.group(1))
    return names


def is_documented(name: str, readme: str) -> bool:
    # An entrypoint is considered documented if it appears either as an inline
    # code literal (`` `name` ``) or as a signature reference (``name(``).
    return f"`{name}`" in readme or f"{name}(" in readme


def main() -> int:
    lib_text = LIB_PATH.read_text(encoding="utf-8")
    readme_text = README_PATH.read_text(encoding="utf-8")

    entrypoints = extract_entrypoints(lib_text)
    missing = [name for name in entrypoints if not is_documented(name, readme_text)]

    if missing:
        print("ERROR: the following entrypoints are missing from README.md:")
        for name in missing:
            print(f"  - {name}")
        print(
            "Add each entrypoint to the README API reference "
            "(see the 'API reference' section)."
        )
        return 1

    print(f"OK: all {len(entrypoints)} contract entrypoints are documented in README.md.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
