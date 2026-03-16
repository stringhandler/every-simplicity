#!/usr/bin/env python3
"""
Regex-based parser for .simplicityhl / .simf source files.
Outputs one JSON object per file to stdout (NDJSON).

Adjust RESERVED_WORDS and BUILTIN_NAMESPACES to match the actual
SimplicityHL syntax as it evolves.
"""

import json
import re
import sys
from collections import Counter

# ---------------------------------------------------------------------------
# Known SimplicityHL reserved words.
# Add or remove as the language spec changes.
# ---------------------------------------------------------------------------
RESERVED_WORDS = [
    "match", "type", "struct", "enum",
    "if", "else", "witness", "for", "loop",
    "return", "true", "false", "pub", "use", "mod",
    "fold", "left", "right", "Left", "Right", "none", "some",
]

# ---------------------------------------------------------------------------
# Known built-in namespaces (everything that isn't jet::).
# Add new namespaces here as they are discovered.
# These are detected as `namespace::method` and stored as
# "namespace::method" keys in the builtins table.
# ---------------------------------------------------------------------------
BUILTIN_NAMESPACES = [
    "for_while",
    "list",
    "option",
    "either",
    "types",
]

# Methods that are recorded by their name alone (not "namespace::method")
# whenever they appear after ::, e.g. foo::into -> "into".
BARE_METHODS = {"into"}

# ---------------------------------------------------------------------------
# Patterns
# ---------------------------------------------------------------------------

# jet::sha256  or  jet :: add32
JET_PATTERN = re.compile(r'\bjet\s*::\s*(\w+)')

# witness::NAME  e.g.  witness::TRANSFER_OR_TIMEOUT
WITNESS_REF_PATTERN = re.compile(r'\bwitness\s*::\s*(\w+)')

# param::NAME  e.g.  param::ALICE_PUBLIC_KEY
PARAM_REF_PATTERN = re.compile(r'\bparam\s*::\s*(\w+)')

# Macros: identifiers followed by !  e.g.  assert!  panic!  dbg!
MACRO_PATTERN = re.compile(r'\b(\w+)!')

# Types: u1, u2, u4, u8, u16, u32, u64, u128, u256, bool, List, Option, Either
# u<N> are matched generically; named types are listed explicitly.
NAMED_TYPES = {"bool", "List", "Option", "Either", "Ctx8", "PubKey", "Signature", "Height"}
UINT_TYPE_PATTERN = re.compile(r'\bu(\d+)\b')
NAMED_TYPE_PATTERN = re.compile(
    r'\b(' + '|'.join(re.escape(t) for t in NAMED_TYPES) + r')\b'
)

# Matches either:
#   namespace::method      e.g.  for_while::fold
#   namespace::<...>       e.g.  array_fold::<sum, 7>  (turbofish / type params)
# Excludes "jet" which is handled separately.
ANY_NS_PATTERN = re.compile(r'\b(\w+)\s*::\s*(?:(\w+)|(?=<))')


def _strip_comments(source: str) -> str:
    source = re.sub(r'//[^\n]*', '', source)
    source = re.sub(r'/\*.*?\*/', '', source, flags=re.DOTALL)
    return source


def _extract_comments(source: str) -> list:
    """
    Extract human-readable comment text from source, sanitized for safe
    HTML embedding.  Strips //-line and /* block */ markers, trims whitespace,
    removes control characters, and deduplicates while preserving order.
    """
    comments = []

    # Line comments: everything after //
    for m in re.finditer(r'//([^\n]*)', source):
        text = m.group(1).strip()
        if text:
            comments.append(text)

    # Block comments: content between /* and */
    for m in re.finditer(r'/\*(.*?)\*/', source, flags=re.DOTALL):
        for line in m.group(1).splitlines():
            text = line.strip().lstrip('*').strip()
            if text:
                comments.append(text)

    # Sanitize: strip control characters, collapse internal whitespace.
    # HTML encoding is handled by the site renderer; we only need to ensure
    # the strings are clean text with no ASCII control chars (tabs preserved).
    cleaned = []
    seen = set()
    for c in comments:
        # Remove control characters except tab
        c = re.sub(r'[\x00-\x08\x0b-\x1f\x7f]', '', c)
        c = c.strip()
        if c and c not in seen:
            seen.add(c)
            cleaned.append(c)

    return cleaned


def parse_file(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        source = f.read()

    comments = _extract_comments(source)
    src = _strip_comments(source)

    # Extract witness references: witness::NAME
    # Use a deduplicated list to preserve first-seen order.
    witness_names = list(dict.fromkeys(WITNESS_REF_PATTERN.findall(src)))

    # Extract param references: param::NAME
    param_names = list(dict.fromkeys(PARAM_REF_PATTERN.findall(src)))

    # Strip witness::NAME and param::NAME before counting reserved words so that
    # "witness" / "param" keyword uses in e.g. `witness::FOO` don't inflate the count.
    src_no_ns = re.sub(r'\bwitness\s*::\s*\w+', '', src)
    src_no_ns = re.sub(r'\bparam\s*::\s*\w+', '', src_no_ns)
    src_no_witness_ns = src_no_ns

    # Reserved word counts
    reserved_counts = {}
    for word in RESERVED_WORDS:
        count = len(re.findall(rf'\b{re.escape(word)}\b', src_no_witness_ns))
        if count:
            reserved_counts[word] = count

    # Jet counts:  jet::name
    jet_counts = dict(Counter(JET_PATTERN.findall(src)))

    # Built-in counts: any namespace::method where namespace != "jet" or "witness"
    # Keyed as "namespace::method" for clarity in the TOML/site.
    builtin_calls = []
    for ns, method in ANY_NS_PATTERN.findall(src):
        if ns in ("jet", "witness", "param"):
            continue  # jet counted above; witness/param extracted separately
        if not method:
            # turbofish syntax e.g. array_fold::<...>
            builtin_calls.append(ns)
        elif method in BARE_METHODS:
            # record just the method name regardless of namespace
            builtin_calls.append(method)
        else:
            builtin_calls.append(f"{ns}::{method}")
    builtin_counts = dict(Counter(builtin_calls))

    # Macro counts: word!
    macro_counts: dict = {}
    for m in MACRO_PATTERN.finditer(src):
        name = m.group(1) + "!"
        macro_counts[name] = macro_counts.get(name, 0) + 1

    # Type counts
    type_counts: dict = {}
    for m in UINT_TYPE_PATTERN.finditer(src):
        t = m.group(0)
        type_counts[t] = type_counts.get(t, 0) + 1
    for t in NAMED_TYPE_PATTERN.findall(src):
        type_counts[t] = type_counts.get(t, 0) + 1

    return {
        "_file_path": path,
        "comments": comments,
        "reserved_words": reserved_counts,
        "jets": jet_counts,
        "builtins": builtin_counts,
        "witnesses": witness_names,
        "params": param_names,
        "types": type_counts,
        "macros": macro_counts,
    }


if __name__ == "__main__":
    for path in sys.argv[1:]:
        print(json.dumps(parse_file(path)))
