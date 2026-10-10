#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
matrix_tiktoken=$(ls -t target/debug/deps/libtiktoken_rs-*.rlib 2>/dev/null | head -n 1)
matrix_json=$(ls -t target/debug/deps/libserde_json-*.rlib 2>/dev/null | head -n 1)
if [ -z "$matrix_tiktoken" ] || [ -z "$matrix_json" ]; then
  echo 'Approved local tokenizer or JSON library is not built. Run the normal workspace checks first.' >&2
  exit 1
fi
matrix_probe_dir=$(mktemp -d /private/tmp/matrix-measure.XXXXXX)
rustc --edition=2021 matrix/tools/measure.rs \
  -L dependency=target/debug/deps \
  --extern "tiktoken_rs=$matrix_tiktoken" \
  --extern "serde_json=$matrix_json" \
  -o "$matrix_probe_dir/measure"
"$matrix_probe_dir/measure"
