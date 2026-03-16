#!/bin/bash
# Args: <mode> <clone_url> <branch> <file_path> [<file_path> ...]
#
# mode=parse   — regex parsing only (jets + reserved words), no simc
# mode=compile — full simc compilation (CMR, jets, compiled output, etc.)
#
# Encoding for compile mode:
#   file_path                         — base compile (no args, no witness)
#   file_path:::wit:::name:::value    — witness compile  (--wit <file>)
#   file_path:::args:::name:::value   — args compile     (--args <file>)
#
# Outputs one JSON object per line (NDJSON), one per encoded arg.
# Each object includes "_file_path", "_kind" ("" | "wit" | "args"),
# and "_item_name" (witness/args name, empty for base).
set -e

MODE="$1"
CLONE_URL="$2"
BRANCH="$3"
shift 3
FILE_PATHS=("$@")

if [[ -z "$MODE" || -z "$CLONE_URL" || -z "$BRANCH" || ${#FILE_PATHS[@]} -eq 0 ]]; then
  echo "Usage: entrypoint.sh <parse|compile> <clone_url> <branch> <file_path> [...]" >&2
  exit 1
fi

if [[ ! -d /workspace/repo/.git ]]; then
  git clone --depth 1 --branch "$BRANCH" "$CLONE_URL" /workspace/repo 2>/dev/null
fi
cd /workspace/repo

# Resolve a value that may be a URL to its raw content.
resolve_value() {
  local val="$1"
  if [[ "$val" =~ ^https?:// ]]; then
    local raw
    raw=$(echo "$val" | sed 's|github\.com/\([^/]*/[^/]*\)/blob/|raw.githubusercontent.com/\1/|')
    curl -sSf "$raw"
  else
    echo "$val"
  fi
}

case "$MODE" in
  parse)
    python3 /parse.py "${FILE_PATHS[@]}"
    ;;
  compile)
    # Cache parse.py output per unique file path
    declare -A PARSE_CACHE

    for ENCODED in "${FILE_PATHS[@]}"; do
      # Decode: file_path  |  file_path:::kind:::name:::value
      FILE_PATH="${ENCODED%%:::*}"
      REMAINDER="${ENCODED#*:::}"

      if [[ "$REMAINDER" == "$ENCODED" ]]; then
        KIND=""
        ITEM_NAME=""
        ITEM_VALUE=""
      else
        KIND="${REMAINDER%%:::*}"
        REST2="${REMAINDER#*:::}"
        ITEM_NAME="${REST2%%:::*}"
        AFTER_NAME="${REST2#*:::}"
        if [[ "$AFTER_NAME" == "$REST2" ]]; then
          ITEM_VALUE=""
        else
          ITEM_VALUE="$AFTER_NAME"
        fi
      fi

      # Preprocess with mcpp if the file contains #include directives
      COMPILE_FILE="$FILE_PATH"
      TMP_PREPROCESSED=""
      if grep -q '^[[:space:]]*#include' "$FILE_PATH" 2>/dev/null; then
        TMP_PREPROCESSED=$(mktemp --suffix=.simf)
        mcpp "$FILE_PATH" -o "$TMP_PREPROCESSED"
        COMPILE_FILE="$TMP_PREPROCESSED"
      fi

      # Build simc flags based on kind
      SIMC_EXTRA_ARGS=()
      if [[ "$KIND" == "wit" && -n "$ITEM_VALUE" ]]; then
        ITEM_CONTENT=$(resolve_value "$ITEM_VALUE")
        TMP_FILE=$(mktemp)
        echo "$ITEM_CONTENT" > "$TMP_FILE"
        SIMC_EXTRA_ARGS=(--wit "$TMP_FILE")
      elif [[ "$KIND" == "args" && -n "$ITEM_VALUE" ]]; then
        ITEM_CONTENT=$(resolve_value "$ITEM_VALUE")
        TMP_FILE=$(mktemp)
        echo "$ITEM_CONTENT" > "$TMP_FILE"
        SIMC_EXTRA_ARGS=(--args "$TMP_FILE")
      else
        TMP_FILE=""
      fi

      simc_out=$(simc "$COMPILE_FILE" "${SIMC_EXTRA_ARGS[@]}" --json 2>/tmp/simc_stderr) || {
        err=$(grep -m1 'error' /tmp/simc_stderr | tr '"\\' "''" || head -1 /tmp/simc_stderr | tr '"\\' "''")
        [[ -n "$TMP_FILE" ]] && rm -f "$TMP_FILE"

        # On simc failure: still run parse.py so jets/types/etc are captured,
        # then emit merged output with _error set.
        if [[ -n "${PARSE_CACHE[$FILE_PATH]+x}" ]]; then
          parse_out="${PARSE_CACHE[$FILE_PATH]}"
        else
          parse_out=$(python3 /parse.py "$COMPILE_FILE")
          PARSE_CACHE[$FILE_PATH]="$parse_out"
        fi
        [[ -n "$TMP_PREPROCESSED" ]] && rm -f "$TMP_PREPROCESSED"
        TMP_PARSE=$(mktemp)
        echo "$parse_out" > "$TMP_PARSE"
        python3 - "$TMP_PARSE" "$FILE_PATH" "$KIND" "$ITEM_NAME" "$err" <<'PYEOF'
import json, sys
parse = json.loads(open(sys.argv[1]).read())
parse['_file_path']  = sys.argv[2]
parse['_kind']       = sys.argv[3]
parse['_item_name']  = sys.argv[4]
parse['_error']      = sys.argv[5]
print(json.dumps(parse))
PYEOF
        rm -f "$TMP_PARSE"
        continue
      }
      [[ -n "$TMP_FILE" ]] && rm -f "$TMP_FILE"

      # Use cached parse.py result if available
      if [[ -n "${PARSE_CACHE[$FILE_PATH]+x}" ]]; then
        parse_out="${PARSE_CACHE[$FILE_PATH]}"
      else
        parse_out=$(python3 /parse.py "$COMPILE_FILE")
        PARSE_CACHE[$FILE_PATH]="$parse_out"
      fi
      [[ -n "$TMP_PREPROCESSED" ]] && rm -f "$TMP_PREPROCESSED"

      # Extract program from simc output
      program=$(python3 -c "import json,sys; d=json.loads(sys.argv[1]); print(d.get('program',''))" "$simc_out")

      # Prefer witness from simc output for hal invocation
      simc_witness=$(python3 -c "import json,sys; d=json.loads(sys.argv[1]); w=d.get('witness'); print(w if w is not None else '')" "$simc_out")

      # Run hal-simplicity to get CMR and other metadata
      if [[ -n "$program" ]]; then
        if [[ -n "$simc_witness" ]]; then
          hal_out=$(hal-simplicity simplicity info "$program" "$simc_witness" 2>/dev/null || echo '{}')
        else
          hal_out=$(hal-simplicity simplicity info "$program" 2>/dev/null || echo '{}')
        fi
        mermaid_out=$(hal-simplicity simplicity graph "$program" --format mermaid 2>/dev/null || echo '')
      else
        hal_out='{}'
        mermaid_out=''
      fi

      # Merge: parse < simc < hal, inject _file_path / _kind / _item_name / _mermaid.
      # Write large blobs to temp files to avoid "Argument list too long".
      # simc/hal emit 'jets' as a string (jet set name e.g. "core") which
      # would clobber parse.py's 'jets' count map — rename it to 'jet_set'.
      TMP_PARSE=$(mktemp); TMP_SIMC=$(mktemp); TMP_HAL=$(mktemp); TMP_MERMAID=$(mktemp)
      echo "$parse_out"   > "$TMP_PARSE"
      echo "$simc_out"    > "$TMP_SIMC"
      echo "$hal_out"     > "$TMP_HAL"
      echo "$mermaid_out" > "$TMP_MERMAID"
      python3 - "$TMP_PARSE" "$TMP_SIMC" "$TMP_HAL" "$FILE_PATH" "$KIND" "$ITEM_NAME" "$TMP_MERMAID" <<'PYEOF'
import json, sys
parse = json.loads(open(sys.argv[1]).read())
simc  = json.loads(open(sys.argv[2]).read())
hal   = json.loads(open(sys.argv[3]).read())
for d in (simc, hal):
    if 'jets' in d and isinstance(d['jets'], str):
        d['jet_set'] = d.pop('jets')
merged = {**parse, **simc, **hal}
merged['_file_path']  = sys.argv[4]
merged['_kind']       = sys.argv[5]
merged['_item_name']  = sys.argv[6]
merged['_mermaid']    = open(sys.argv[7]).read().strip()
print(json.dumps(merged))
PYEOF
      rm -f "$TMP_PARSE" "$TMP_SIMC" "$TMP_HAL" "$TMP_MERMAID"
    done
    ;;
  *)
    echo "Unknown mode: $MODE (expected parse or compile)" >&2
    exit 1
    ;;
esac
