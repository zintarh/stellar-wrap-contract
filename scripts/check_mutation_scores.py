#!/usr/bin/env python3
"""Gate source-file mutation scores and publish a concise GitHub summary."""

import json
import os
import sys
import configparser
from collections import Counter, defaultdict
from pathlib import Path


REPORT_DIR = Path("mutants.out")
THRESHOLD_PATH = Path(".mutants.toml")
EXCLUDED_STEMS = {"test", "test_utils", "error_messages"}


def read_outcomes(report_dir: Path) -> tuple[dict[str, Counter[str]], list[str]]:
    counts: dict[str, Counter[str]] = defaultdict(Counter)
    uncaught: list[str] = []

    for status in ("caught", "missed", "timeout"):
        outcome_file = report_dir / f"{status}.txt"
        if not outcome_file.is_file():
            raise RuntimeError(f"cargo-mutants did not produce {outcome_file}")

        for line in outcome_file.read_text(encoding="utf-8").splitlines():
            line = line.strip()
            if not line:
                continue
            source_path = line.split(":", 1)[0]
            source = Path(source_path)
            if source.suffix != ".rs":
                raise RuntimeError(f"Unrecognized mutant source path: {line}")
            counts[source.stem][status] += 1
            if status in ("missed", "timeout"):
                uncaught.append(f"- `{status}`: `{line}`")

    outcomes_file = report_dir / "outcomes.json"
    if not outcomes_file.is_file():
        raise RuntimeError(f"cargo-mutants did not produce {outcomes_file}")
    outcomes = json.loads(outcomes_file.read_text(encoding="utf-8"))
    if outcomes["total_mutants"] != sum(
        outcomes[key] for key in ("caught", "missed", "timeout", "unviable", "success")
    ):
        raise RuntimeError("cargo-mutants outcome counts are incomplete")
    if outcomes["total_mutants"] != (
        sum(sum(counter.values()) for counter in counts.values())
        + outcomes["unviable"]
        + outcomes["success"]
    ):
        raise RuntimeError("cargo-mutants outcome files do not match outcomes.json")
    baselines = [
        outcome
        for outcome in outcomes["outcomes"]
        if outcome.get("scenario") == "Baseline"
    ]
    if not baselines or any(
        outcome.get("summary") != "Success"
        for outcome in baselines
    ):
        raise RuntimeError("cargo-mutants baseline build or tests failed")

    return counts, uncaught


def main() -> int:
    config = configparser.ConfigParser()
    with THRESHOLD_PATH.open(encoding="utf-8") as config_file:
        config.read_file(config_file)
    thresholds = {
        name: int(value) for name, value in config["thresholds"].items()
    }
    default_threshold = int(thresholds["default"])
    counts, uncaught = read_outcomes(REPORT_DIR)
    missing_gated_modules = [
        module
        for module in thresholds
        if module != "default"
        and (not Path("src", f"{module}.rs").is_file() or module not in counts)
    ]

    lines = [
        "## Per-module mutation scores",
        "",
        "Timed-out mutants count as uncaught. Modules with no generated mutants are not scoreable.",
        "",
        "| Module | Score | Required | Caught | Missed | Timed out |",
        "| --- | ---: | ---: | ---: | ---: | ---: |",
    ]
    failures: list[str] = []
    source_files = sorted(Path("src").rglob("*.rs"))

    for source_file in source_files:
        module = source_file.stem
        if module in EXCLUDED_STEMS or module.endswith("_test"):
            continue

        outcome = counts[module]
        caught = outcome["caught"]
        missed = outcome["missed"]
        timed_out = outcome["timeout"]
        total = caught + missed + timed_out
        if total == 0:
            continue

        score = caught * 100 / total
        threshold = int(thresholds.get(module, default_threshold))
        lines.append(
            f"| `{source_file}` | {score:.1f}% | {threshold}% | "
            f"{caught} | {missed} | {timed_out} |"
        )
        if score < threshold:
            failures.append(
                f"{source_file}: {score:.1f}% is below the required {threshold}%"
            )

    if not counts:
        raise RuntimeError("cargo-mutants produced no per-file outcomes")
    if missing_gated_modules:
        failures.extend(
            f"src/{module}.rs: no per-file mutation outcomes were generated"
            for module in missing_gated_modules
        )

    lines.extend(["", "### Missed and timed-out mutants", ""])
    lines.extend(uncaught or ["None."])
    if failures:
        lines.extend(["", "### Failed thresholds", ""])
        lines.extend(f"- {failure}" for failure in failures)

    summary = "\n".join(lines) + "\n"
    print(summary, end="")
    github_summary_path = os.getenv("GITHUB_STEP_SUMMARY")
    if github_summary_path:
        github_summary = Path(github_summary_path)
        with github_summary.open("a", encoding="utf-8") as summary_file:
            summary_file.write(summary)

    return 1 if failures else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(f"Mutation score check failed: {error}", file=sys.stderr)
        raise SystemExit(1)
