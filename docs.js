// Rust owns the page; this is only the generated-module bootstrap.
const token = new URL(import.meta.url).searchParams.get('v') ?? '';
const suffix = token ? `?v=${encodeURIComponent(token)}` : '';
const module = await import(`./wasm/web/ccm_web.js${suffix}`);
await module.default(`./wasm/web/ccm_web_bg.wasm${suffix}`);
module.start_docs();
