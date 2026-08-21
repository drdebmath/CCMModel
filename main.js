// Rust owns the application; this is only the generated-module bootstrap.
const manifestResponse = await fetch('./wasm/web/manifest.json', { cache: 'no-store' });
if (!manifestResponse.ok) throw new Error(`WASM manifest returned ${manifestResponse.status}`);
const { buildId } = await manifestResponse.json();
if (!buildId) throw new Error('WASM manifest has no build ID');
const token = encodeURIComponent(buildId);
const module = await import(`./wasm/web/ccm_web.js?v=${token}`);
await module.default(`./wasm/web/ccm_web_bg.wasm?v=${token}`);
module.start();
