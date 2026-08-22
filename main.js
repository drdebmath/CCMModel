// Rust owns the application; this is only the generated-module bootstrap.
// The build id arrives in this module's own URL, set by the page, so the
// manifest is read once per load rather than once here and once there.
const token = new URL(import.meta.url).searchParams.get('v') ?? '';
const suffix = token ? `?v=${encodeURIComponent(token)}` : '';
const module = await import(`./wasm/web/ccm_web.js${suffix}`);
await module.default(`./wasm/web/ccm_web_bg.wasm${suffix}`);
module.start();
