#!/usr/bin/env python3
"""
Transpilers for .simf template formats.

Each transpiler reads a source file and writes equivalent SimplicityHL
to stdout.  Adding a new transpiler type only requires a new entry in
TRANSPILERS below.

Usage:
  python3 transpile.py <type> <input_file>

Supported types:
  template  — converts {{PARAM_NAME}} placeholders to param::PARAM_NAME
"""
import re
import sys


def transpile_template(source: str) -> str:
    """Convert {{PARAM_NAME}} to param::PARAM_NAME.
    If preceded by '0x', the '0x' prefix is also removed."""
    return re.sub(r'0x\{\{([A-Z0-9_]+)\}\}|\{\{([A-Z0-9_]+)\}\}',
                  lambda m: f'param::{m.group(1) or m.group(2)}', source)


TRANSPILERS = {
    "template": transpile_template,
}


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <type> <input_file>", file=sys.stderr)
        sys.exit(1)

    kind = sys.argv[1]
    path = sys.argv[2]

    if kind not in TRANSPILERS:
        known = ", ".join(TRANSPILERS)
        print(f"Unknown transpiler '{kind}'. Known: {known}", file=sys.stderr)
        sys.exit(1)

    with open(path, encoding="utf-8") as f:
        source = f.read()

    print(TRANSPILERS[kind](source), end="")
