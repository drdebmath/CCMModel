#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

wasm_bindgen_bin="${WASM_BINDGEN:-wasm-bindgen}"
"$wasm_bindgen_bin" --version >/dev/null
mkdir -p wasm/web
cargo build -p ccm-wasm -p ccm-web --target wasm32-unknown-unknown --release
"$wasm_bindgen_bin" target/wasm32-unknown-unknown/release/ccm_wasm.wasm \
  --target web --out-dir wasm/web --no-typescript
"$wasm_bindgen_bin" target/wasm32-unknown-unknown/release/ccm_web.wasm \
  --target web --out-dir wasm/web --no-typescript

if command -v shasum >/dev/null 2>&1; then
  build_id="$(shasum -a 256 wasm/web/ccm_wasm_bg.wasm wasm/web/ccm_web_bg.wasm | shasum -a 256 | cut -c1-16)"
else
  build_id="$(cksum wasm/web/ccm_wasm_bg.wasm wasm/web/ccm_web_bg.wasm | cksum | awk '{print $1}')"
fi
printf '{"buildId":"%s"}\n' "$build_id" > wasm/web/manifest.json
echo "WASM browser packages written to wasm/web (build $build_id)"
