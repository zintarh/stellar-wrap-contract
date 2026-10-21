#!/usr/bin/env python3
"""Keep `src/error_messages.json` the single source for contract error codes.

`src/error_messages.json` is canonical: every `ContractError` variant maps to
one entry with `variant`, `code`, `actionable`, and `message`. This script

  * regenerates the table in `ERRORS.md` from the JSON (default mode), and
  * in `--check` mode verifies that `src/errors.rs` and the generated section
    of `ERRORS.md` both agree with the JSON, exiting non-zero when they do not.

CI runs `python3 scripts/generate_errors_md.py --check` so the copies cannot
drift apart again. The Rust-side mirror of this check lives in
`src/error_messages.rs` (`every_variant_has_a_mapping`).
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JSON_PATH = ROOT / "src" / "error_messages.json"
ERRORS_RS_PATH = ROOT / "src" / "errors.rs"
ERRORS_MD_PATH = ROOT / "ERRORS.md"

BEGIN_MARKER = "<!-- BEGIN GENERATED TABLE: do not edit by hand; run scripts/generate_errors_md.py -->"
END_MARKER = "<!-- END GENERATED TABLE -->"


def load_catalog() -> list[dict]:
    entries = json.loads(JSON_PATH.read_text())
    seen_variants: set[str] = set()
    seen_codes: set[int] = set()
    for entry in entries:
        for key in ("variant", "code", "actionable", "message"):
            if key not in entry:
                sys.exit(f"{JSON_PATH.name}: entry missing '{key}': {entry}")
        if entry["variant"] in seen_variants:
            sys.exit(f"{JSON_PATH.name}: duplicate variant {entry['variant']}")
        if entry["code"] in seen_codes:
            sys.exit(f"{JSON_PATH.name}: duplicate code {entry['code']}")
        seen_variants.add(entry["variant"])
        seen_codes.add(entry["code"])
    return sorted(entries, key=lambda entry: entry["code"])


def parse_errors_rs() -> dict[str, int]:
    source = ERRORS_RS_PATH.read_text()
    start = source.find("pub enum ContractError")
    if start == -1:
        sys.exit(f"{ERRORS_RS_PATH.name}: pub enum ContractError not found")
    body = source[start:]
    variants = {}
    for name, code in re.findall(
        r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(\d+)\s*,", body, re.MULTILINE
    ):
        variants[name] = int(code)
    if not variants:
        sys.exit(f"{ERRORS_RS_PATH.name}: no variants parsed from enum body")
    return variants


def render_table(entries: list[dict]) -> str:
    lines = [
        BEGIN_MARKER,
        "",
        "| Code | Variant name | User can act? | Message |",
        "|---:|---|:---:|---|",
    ]
    for entry in entries:
        actionable = "yes" if entry["actionable"] else "no"
        message = entry["message"].replace("|", "\\|").replace("\n", " ")
        lines.append(
            f"| {entry['code']} | `{entry['variant']}` | {actionable} | {message} |"
        )
    lines += [
        "",
        f"Generated from `{JSON_PATH.relative_to(ROOT)}` "
        f"({len(entries)} variants). Do not edit this table by hand: run "
        "`python3 scripts/generate_errors_md.py` and commit the result.",
        "",
        END_MARKER,
    ]
    return "\n".join(lines)


def check_errors_rs(entries: list[dict]) -> list[str]:
    problems = []
    enum = parse_errors_rs()
    catalog = {entry["variant"]: entry["code"] for entry in entries}
    for name, code in enum.items():
        if name not in catalog:
            problems.append(
                f"errors.rs: variant {name} = {code} has no entry in "
                f"{JSON_PATH.name}"
            )
        elif catalog[name] != code:
            problems.append(
                f"errors.rs: {name} = {code} but {JSON_PATH.name} says "
                f"{catalog[name]}"
            )
    for name, code in catalog.items():
        if name not in enum:
            problems.append(
                f"{JSON_PATH.name}: entry {name} = {code} is not a variant of "
                f"ContractError in {ERRORS_RS_PATH.name}"
            )
    return problems


def check_errors_md(rendered: str) -> list[str]:
    content = ERRORS_MD_PATH.read_text()
    begin = content.find(BEGIN_MARKER)
    end = content.find(END_MARKER)
    if begin == -1 or end == -1 or end < begin:
        return [
            f"{ERRORS_MD_PATH.name}: generated-table markers not found; run "
            "python3 scripts/generate_errors_md.py"
        ]
    current = content[begin : end + len(END_MARKER)]
    if current != rendered:
        return [
            f"{ERRORS_MD_PATH.name}: generated table is out of date; run "
            "python3 scripts/generate_errors_md.py"
        ]
    return []


def regenerate(errors_md: str) -> None:
    rendered = render_table(load_catalog())
    begin = errors_md.find(BEGIN_MARKER)
    end = errors_md.find(END_MARKER)
    if begin == -1 or end == -1 or end < begin:
        sys.exit(
            f"{ERRORS_MD_PATH.name}: generated-table markers not found; add "
            f"{BEGIN_MARKER} and {END_MARKER} around the generated section"
        )
    updated = errors_md[:begin] + rendered + errors_md[end + len(END_MARKER) :]
    ERRORS_MD_PATH.write_text(updated)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify errors.rs and ERRORS.md match error_messages.json",
    )
    args = parser.parse_args()

    entries = load_catalog()
    if args.check:
        problems = check_errors_rs(entries)
        problems += check_errors_md(render_table(entries))
        if problems:
            print("error catalog is out of sync:", file=sys.stderr)
            for problem in problems:
                print(f"  - {problem}", file=sys.stderr)
            return 1
        print(
            f"OK: {ERRORS_RS_PATH.name} and {ERRORS_MD_PATH.name} match "
            f"{JSON_PATH.name} ({len(entries)} variants)"
        )
        return 0

    regenerate(ERRORS_MD_PATH.read_text())
    problems = check_errors_rs(entries)
    if problems:
        print("errors.rs does not match the catalog:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print(f"regenerated {ERRORS_MD_PATH.name} from {JSON_PATH.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
