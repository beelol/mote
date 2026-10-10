#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
case "$*" in
  ''|--check) ;;
  *) echo 'Usage: sh matrix/tools/measure.sh [--check]' >&2; exit 2 ;;
esac
matrix_mode=${1-}
matrix_probe_dir=$(mktemp -d /private/tmp/matrix-measure.XXXXXX)
trap 'rm -f "$matrix_probe_dir/measure" "$matrix_probe_dir/output.tsv" "$matrix_probe_dir/artifacts.jsonl"; rmdir "$matrix_probe_dir"' EXIT HUP INT TERM
cargo build --workspace --offline --locked --message-format=json > "$matrix_probe_dir/artifacts.jsonl"
matrix_libs='tiktoken_rs serde_json matrix_lang mote_context mote_eventlog mote_ui serde rusqlite'
set --
for matrix_lib in $matrix_libs; do
  matrix_rlib=$(jq -ers --arg name "$matrix_lib" '
    [.[] | select(.reason == "compiler-artifact" and .target.name == $name)
      | .filenames[] | select(endswith(".rlib"))] | unique
    | if length == 1 then .[0] else error("expected one built library for " + $name) end
  ' "$matrix_probe_dir/artifacts.jsonl")
  set -- "$@" --extern "$matrix_lib=$matrix_rlib"
done
rustc --edition=2021 matrix/tools/measure.rs \
  -L dependency=target/debug/deps \
  "$@" \
  -o "$matrix_probe_dir/measure"
"$matrix_probe_dir/measure" > "$matrix_probe_dir/output.tsv"
if [ "$matrix_mode" = --check ]; then
  diff -u matrix/measurement-output.tsv "$matrix_probe_dir/output.tsv"
fi
cat "$matrix_probe_dir/output.tsv"
