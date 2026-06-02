#!/usr/bin/env python3
"""Report storage-next type and facade inventory.

This is a cleanup planning tool, not a Rust parser. It intentionally uses
conservative text matching so that the same command can be run before and after
cleanup PRs to show trend, hotspots, and review scope.
"""

from __future__ import annotations

import argparse
import contextlib
import io
import re
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path


DEFAULT_ROOT = Path("crates/storage-next/src")
TYPE_RE = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.MULTILINE,
)
PUB_USE_RE = re.compile(r"^\s*pub(?:\([^)]*\))?\s+use\s+", re.MULTILINE)
WORD_RE_TEMPLATE = r"\b{}\b"
TYPE_SUFFIXES = (
    "Request",
    "Plan",
    "Outcome",
    "Recovery",
    "Candidate",
    "PreparedOutput",
    "Proof",
    "Attestation",
    "Safety",
    "Policy",
    "Reason",
    "Invalidity",
    "Report",
    "Stats",
)


@dataclass(frozen=True)
class RustFile:
    path: Path
    rel: Path
    text: str

    @property
    def loc(self) -> int:
        return len(self.text.splitlines())

    @property
    def is_test(self) -> bool:
        parts = set(self.rel.parts)
        return (
            "tests" in parts
            or "test_support" in parts
            or self.rel.name == "tests.rs"
        )

    @property
    def is_testkit(self) -> bool:
        return "testkit" in self.rel.parts

    @property
    def is_public_api(self) -> bool:
        return self.rel.parts[:1] == ("api",)

    @property
    def is_durable_format(self) -> bool:
        return self.rel.parts[:1] == ("format",)

    @property
    def is_production(self) -> bool:
        return not self.is_test and not self.is_testkit

    @property
    def is_cleanup_target(self) -> bool:
        return (
            self.is_production
            and not self.is_public_api
            and not self.is_durable_format
        )


@dataclass(frozen=True)
class TypeDef:
    name: str
    file: RustFile


def rust_files(root: Path) -> list[RustFile]:
    files = []
    for path in sorted(root.rglob("*.rs")):
        files.append(RustFile(path=path, rel=path.relative_to(root), text=path.read_text()))
    return files


def type_defs(files: list[RustFile]) -> list[TypeDef]:
    defs: list[TypeDef] = []
    for file in files:
        for match in TYPE_RE.finditer(file.text):
            defs.append(TypeDef(name=match.group(1), file=file))
    return defs


def statements_matching(text: str, pattern: re.Pattern[str]) -> list[str]:
    statements = []
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index]
        if pattern.match(line):
            statement = [line]
            while ";" not in lines[index] and index + 1 < len(lines):
                index += 1
                statement.append(lines[index])
            statements.append("\n".join(statement))
        index += 1
    return statements


def exported_names(statement: str) -> list[str]:
    after_use = re.sub(r"(?s)^.*?\buse\s+", "", statement).rstrip(";").strip()
    if "{" not in after_use:
        return [terminal_name(after_use)]

    names: list[str] = []
    for group in re.findall(r"\{([^{}]*)\}", after_use, flags=re.DOTALL):
        for item in group.split(","):
            name = terminal_name(item.strip())
            if name and name not in {"self", "crate", "super"}:
                names.append(name)
    return names


def terminal_name(item: str) -> str:
    if not item:
        return ""
    item = item.split(" as ", maxsplit=1)[-1].strip()
    item = item.split("::")[-1].strip()
    return re.sub(r"[^A-Za-z0-9_].*$", "", item)


def reexport_counts(files: list[RustFile]) -> list[tuple[RustFile, int, int]]:
    rows = []
    for file in files:
        if file.rel.name not in {"mod.rs", "lib.rs"}:
            continue
        statements = statements_matching(file.text, PUB_USE_RE)
        names = [name for stmt in statements for name in exported_names(stmt) if name]
        if statements or names:
            rows.append((file, len(statements), len(names)))
    return sorted(rows, key=lambda row: (row[2], row[1], str(row[0].rel)), reverse=True)


def suffix_counts(defs: list[TypeDef]) -> Counter[str]:
    counter: Counter[str] = Counter()
    for type_def in defs:
        for suffix in TYPE_SUFFIXES:
            if type_def.name.endswith(suffix):
                counter[suffix] += 1
    return counter


def type_occurrences(files: list[RustFile], defs: list[TypeDef]) -> list[tuple[TypeDef, int]]:
    production_text = "\n".join(file.text for file in files if file.is_production)
    rows = []
    for type_def in defs:
        if not type_def.file.is_cleanup_target:
            continue
        count = len(re.findall(WORD_RE_TEMPLATE.format(re.escape(type_def.name)), production_text))
        rows.append((type_def, count))
    return sorted(rows, key=lambda row: (row[1], str(row[0].file.rel), row[0].name))


def scaffold_counts(files: list[RustFile]) -> list[tuple[RustFile, int, int]]:
    rows = []
    for file in files:
        unused = file.text.count("unused_imports")
        dead = file.text.count("dead_code")
        if unused or dead:
            rows.append((file, unused, dead))
    return sorted(rows, key=lambda row: (row[1] + row[2], str(row[0].rel)), reverse=True)


def print_table(headers: tuple[str, ...], rows: list[tuple[object, ...]]) -> None:
    print("| " + " | ".join(headers) + " |")
    print("|" + "|".join("---" for _ in headers) + "|")
    for row in rows:
        print("| " + " | ".join(str(cell) for cell in row) + " |")


def parse_name_limit(values: list[str], option: str) -> dict[str, int]:
    limits: dict[str, int] = {}
    for value in values:
        name, separator, raw_limit = value.partition("=")
        if not separator or not name or not raw_limit:
            raise SystemExit(f"{option} expects NAME=LIMIT, got {value!r}")
        try:
            limits[name] = int(raw_limit)
        except ValueError as error:
            raise SystemExit(f"{option} limit must be an integer: {value!r}") from error
    return limits


def parse_scaffold_limit(value: str) -> tuple[str, int, int]:
    name, separator, raw_limits = value.partition("=")
    raw_unused, limit_separator, raw_dead = raw_limits.partition(":")
    if not separator or not limit_separator or not name or not raw_unused or not raw_dead:
        raise SystemExit(
            f"--max-scaffold-markers expects FILE=UNUSED:DEAD, got {value!r}"
        )
    try:
        return name, int(raw_unused), int(raw_dead)
    except ValueError as error:
        raise SystemExit(
            f"--max-scaffold-markers limits must be integers: {value!r}"
        ) from error


def render_report(
    files: list[RustFile],
    defs: list[TypeDef],
    production_defs: list[TypeDef],
    cleanup_defs: list[TypeDef],
    public_api_defs: list[TypeDef],
    format_defs: list[TypeDef],
    top: int,
) -> str:
    buffer = io.StringIO()
    with contextlib.redirect_stdout(buffer):
        print("# Storage-Next Type Inventory Baseline")
        print()
        print("Generated by `scripts/architecture/storage_next_type_inventory.py`.")
        print("Counts are conservative text matches, not a Rust AST parse.")
        print()

        print("## Summary")
        print()
        print_table(
            ("Metric", "Count"),
            [
                ("Rust files", len(files)),
                ("Production Rust files", sum(file.is_production for file in files)),
                ("All struct/enum definitions", len(defs)),
                ("Production struct/enum definitions", len(production_defs)),
                ("Cleanup-target production definitions", len(cleanup_defs)),
                ("Public API definitions", len(public_api_defs)),
                ("Durable format definitions", len(format_defs)),
            ],
        )
        print()

        by_file: defaultdict[Path, int] = defaultdict(int)
        for type_def in defs:
            by_file[type_def.file.rel] += 1
        print("## Top Files By Type Count")
        print()
        print_table(
            ("File", "Types", "LOC"),
            [
                (path, count, next(file.loc for file in files if file.rel == path))
                for path, count in sorted(
                    by_file.items(), key=lambda item: item[1], reverse=True
                )[:top]
            ],
        )
        print()

        print("## Files Over 1,500 LOC")
        print()
        large_files = [(file.rel, file.loc) for file in files if file.loc > 1500]
        print_table(("File", "LOC"), sorted(large_files, key=lambda row: row[1], reverse=True))
        print()

        print("## Parent Module Re-Exports")
        print()
        print_table(
            ("File", "pub use statements", "Approx exported names"),
            [
                (file.rel, statements, names)
                for file, statements, names in reexport_counts(files)[:top]
            ],
        )
        print()

        print("## Operation-Family Suffix Counts")
        print()
        suffix_rows = sorted(
            suffix_counts(cleanup_defs).items(), key=lambda item: item[1], reverse=True
        )
        print_table(("Suffix", "Cleanup-target types"), suffix_rows)
        print()

        print("## Existing Scaffold Allowance Markers")
        print()
        print_table(
            ("File", "unused_imports markers", "dead_code markers"),
            [
                (file.rel, unused, dead)
                for file, unused, dead in scaffold_counts(files)[:top]
            ],
        )
        print()

        print("## Low-Reference Cleanup Candidates")
        print()
        low_ref_rows = [
            (type_def.file.rel, type_def.name, count)
            for type_def, count in type_occurrences(files, cleanup_defs)
            if count <= 3
        ][:top]
        print_table(("File", "Type", "Production references"), low_ref_rows)
        print()
    return buffer.getvalue()


def guard_failures(
    args: argparse.Namespace,
    report: str,
    files: list[RustFile],
    defs: list[TypeDef],
    cleanup_defs: list[TypeDef],
) -> list[str]:
    failures: list[str] = []

    if args.require_inventory is not None:
        expected = args.require_inventory.read_text()
        if report != expected:
            failures.append(
                f"inventory output differs from {args.require_inventory}; "
                "rerun the inventory script and review the count diff"
            )

    if args.max_all_types is not None and len(defs) > args.max_all_types:
        failures.append(
            f"all struct/enum definitions grew to {len(defs)} "
            f"(limit {args.max_all_types})"
        )

    if (
        args.max_cleanup_target_types is not None
        and len(cleanup_defs) > args.max_cleanup_target_types
    ):
        failures.append(
            f"cleanup-target production definitions grew to {len(cleanup_defs)} "
            f"(limit {args.max_cleanup_target_types})"
        )

    reexport_by_file = {
        str(file.rel): names for file, _statements, names in reexport_counts(files)
    }
    for path, limit in parse_name_limit(args.max_reexport_names, "--max-reexport-names").items():
        count = reexport_by_file.get(path, 0)
        if count > limit:
            failures.append(f"{path} re-exported names grew to {count} (limit {limit})")

    suffix_by_name = suffix_counts(cleanup_defs)
    for suffix, limit in parse_name_limit(args.max_suffix_count, "--max-suffix-count").items():
        count = suffix_by_name.get(suffix, 0)
        if count > limit:
            failures.append(f"{suffix} cleanup-target suffix count grew to {count} (limit {limit})")

    scaffold_by_file = {
        str(file.rel): (unused, dead) for file, unused, dead in scaffold_counts(files)
    }
    for raw_limit in args.max_scaffold_markers:
        path, unused_limit, dead_limit = parse_scaffold_limit(raw_limit)
        unused, dead = scaffold_by_file.get(path, (0, 0))
        if unused > unused_limit or dead > dead_limit:
            failures.append(
                f"{path} scaffold markers grew to unused_imports={unused}, "
                f"dead_code={dead} (limits {unused_limit}:{dead_limit})"
            )

    return failures


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--top", type=int, default=20)
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--require-inventory", type=Path)
    parser.add_argument("--max-all-types", type=int)
    parser.add_argument("--max-cleanup-target-types", type=int)
    parser.add_argument("--max-reexport-names", action="append", default=[])
    parser.add_argument("--max-suffix-count", action="append", default=[])
    parser.add_argument("--max-scaffold-markers", action="append", default=[])
    args = parser.parse_args()

    files = rust_files(args.root)
    defs = type_defs(files)
    production_defs = [type_def for type_def in defs if type_def.file.is_production]
    cleanup_defs = [type_def for type_def in defs if type_def.file.is_cleanup_target]
    public_api_defs = [
        type_def
        for type_def in defs
        if type_def.file.is_production and type_def.file.is_public_api
    ]
    format_defs = [
        type_def
        for type_def in defs
        if type_def.file.is_production and type_def.file.is_durable_format
    ]

    report = render_report(
        files,
        defs,
        production_defs,
        cleanup_defs,
        public_api_defs,
        format_defs,
        args.top,
    )
    if not args.quiet:
        print(report, end="")

    failures = guard_failures(args, report, files, defs, cleanup_defs)
    if failures:
        for failure in failures:
            print(f"error: {failure}", file=sys.stderr)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
