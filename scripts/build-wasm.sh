#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

sha256() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256; else sha256sum; fi
}

hash_files() {
  sha256 < <(cat "$@") | cut -c1-16
}

# Every input the browser package is built from. Sorted under a fixed locale so
# the result does not depend on the machine's collation.
source_hash() {
  {
    LC_ALL=C find crates -type f \( -name '*.rs' -o -name 'Cargo.toml' \) | LC_ALL=C sort
    printf '%s\n' Cargo.toml Cargo.lock rust-toolchain.toml scripts/build-wasm.sh
  } | while IFS= read -r file; do
    printf '%s\n' "$file"
    cat "$file"
  done | sha256 | cut -c1-16
}

# Lets a check compare the committed package against the current sources
# without installing a toolchain or compiling anything.
if [ "${1:-}" = "--source-id" ]; then
  source_hash
  exit 0
fi

wasm_bindgen_bin="${WASM_BINDGEN:-wasm-bindgen}"
"$wasm_bindgen_bin" --version >/dev/null
mkdir -p wasm/web

# Panic and error paths bake the source location into the binary, which
# otherwise ships the building machine's home directory to every visitor and
# makes the output differ between machines. Remap both roots to fixed names.
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$repo_dir=/ccm --remap-path-prefix=$cargo_home=/cargo"

cargo build -p ccm-wasm -p ccm-web --target wasm32-unknown-unknown --release
"$wasm_bindgen_bin" target/wasm32-unknown-unknown/release/ccm_wasm.wasm \
  --target web --out-dir wasm/web --no-typescript
"$wasm_bindgen_bin" target/wasm32-unknown-unknown/release/ccm_web.wasm \
  --target web --out-dir wasm/web --no-typescript

# `buildId` is derived from the emitted binaries and is what the page and the
# worker use to bust caches. `sourceId` is derived from the inputs instead, so
# a check can tell that the committed package is stale without reproducing the
# compilation: identical bytes across machines would require bit-for-bit
# reproducible codegen, which is a stronger promise than this needs.
build_id="$(hash_files wasm/web/ccm_wasm_bg.wasm wasm/web/ccm_web_bg.wasm)"
source_id="$(source_hash)"
printf '{"buildId":"%s","sourceId":"%s"}\n' "$build_id" "$source_id" > wasm/web/manifest.json
echo "WASM browser packages written to wasm/web (build $build_id, source $source_id)"
