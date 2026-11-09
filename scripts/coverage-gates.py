#!/usr/bin/env python3
"""Per-crate line coverage gates.

Usage: coverage-gates.py <cargo llvm-cov --json --summary-only output> <crate>=<min percent>...
Sums line counts over the files in crates/<crate>/ and fails if any crate is below its minimum.
"""
import json
import sys
from collections import defaultdict


def main() -> int:
    report = json.load(open(sys.argv[1]))
    gates = {name: float(minimum) for name, minimum in (arg.split("=") for arg in sys.argv[2:])}
    lines = defaultdict(lambda: [0, 0])
    for data in report["data"]:
        for file in data["files"]:
            parts = file["filename"].split("/crates/", 1)
            if len(parts) != 2:
                continue
            crate = parts[1].split("/", 1)[0]
            summary = file["summary"]["lines"]
            lines[crate][0] += summary["covered"]
            lines[crate][1] += summary["count"]

    failed = False
    for crate, minimum in sorted(gates.items()):
        covered, count = lines.get(crate, (0, 0))
        percent = 100.0 * covered / count if count else 0.0
        ok = count > 0 and percent >= minimum
        failed |= not ok
        print(f"{'ok  ' if ok else 'FAIL'} {crate}: {percent:.1f}% lines (gate {minimum:.0f}%)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
