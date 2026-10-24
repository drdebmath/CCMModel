/* The worker is deliberately tiny: graph preparation is input plumbing, while
 * every state transition is executed by ccm-wasm (the native Rust adapter). */
let wasm = null;
let loadError = null;
let loadPromise = null;

async function load() {
  if (wasm || loadError) return wasm;
  if (!loadPromise) {
    loadPromise = (async () => {
      try {
        const manifestResponse = await fetch('./wasm/web/manifest.json', { cache: 'no-store' });
        if (!manifestResponse.ok) throw new Error(`WASM manifest returned ${manifestResponse.status}`);
        const { buildId } = await manifestResponse.json();
        if (!buildId) throw new Error('WASM manifest has no build ID');
        const token = encodeURIComponent(buildId);
        const module = await import(`./wasm/web/ccm_wasm.js?v=${token}`);
        await module.default(`./wasm/web/ccm_wasm_bg.wasm?v=${token}`);
        wasm = module;
        self.postMessage({ type: 'ready' });
        return wasm;
      } catch (error) {
        loadError = error;
        self.postMessage({ type: 'error', phase: 'load', message: 'WASM package is missing. Run: ./scripts/build-wasm.sh' });
        return null;
      }
    })();
  }
  return loadPromise;
}

load();

// The UI's algorithm values, mapped to the adapter's selector. Anything
// unrecognised falls back to Drop-and-Freeze, as it always has.
function algorithmSelector(name) {
  switch (name) {
    case 'help': return wasm.AlgorithmSelector.HelpByScouts;
    case 'p1tree': return wasm.AlgorithmSelector.P1Tree;
    case 'rooted': return wasm.AlgorithmSelector.RootedDisp;
    case 'general': return wasm.AlgorithmSelector.GeneralDisp;
    default: return wasm.AlgorithmSelector.DropAndFreeze;
  }
}

self.onmessage = async ({ data }) => {
  if (data.type === 'ping') {
    if (!wasm) await load();
    self.postMessage({ type: wasm ? 'ready' : 'error', message: loadError?.message });
    return;
  }
  if (data.type === 'sweep') {
    if (!wasm) await load();
    if (!wasm) return;
    // The dashboard asks for many small runs at once. Looping here keeps the
    // main thread free and avoids a postMessage round trip per data point.
    try {
      const algorithm = algorithmSelector(data.algorithm);
      const results = [];
      for (let i = 0; i < data.jobs.length; i++) {
        const job = data.jobs[i];
        const simulation = new wasm.WasmSimulation(
          job.nodeCount, new Uint32Array(job.edges), new Uint32Array(job.starts),
          algorithm, wasm.TraceMode.Off, BigInt(job.roundLimit), 0, 1,
        );
        const output = simulation.run();
        results.push({
          rounds: Number(output.rounds()),
          moves: Number(output.moves()),
          probes: Number(output.probes()),
          completed: output.termination_code() === 0,
        });
        output.free();
        simulation.free();
        if ((i + 1) % 8 === 0 || i + 1 === data.jobs.length) {
          self.postMessage({ type: 'sweep-progress', done: i + 1, total: data.jobs.length });
        }
      }
      self.postMessage({ type: 'sweep-result', results });
    } catch (error) {
      self.postMessage({ type: 'error', phase: 'sweep', message: error?.message || String(error) });
    }
    return;
  }
  if (data.type === 'compare') {
    if (!wasm) await load();
    if (!wasm) return;
    // Every algorithm runs on the same graph, port labels and starts, so the
    // rows differ only by algorithm. One failing algorithm is reported in its
    // own row rather than losing the others.
    const results = [];
    const transfer = [];
    for (let i = 0; i < data.algorithms.length; i++) {
      const name = data.algorithms[i];
      self.postMessage({ type: 'progress', message: `Comparing: ${i + 1} of ${data.algorithms.length}…` });
      try {
        const result = runOne(name, data);
        results.push({ algorithm: name, result });
        transfer.push(...transferables(result));
      } catch (error) {
        results.push({ algorithm: name, error: error?.message || String(error) });
      }
    }
    self.postMessage({ type: 'compare-result', results }, transfer);
    return;
  }
  if (data.type !== 'run') return;
  if (!wasm) await load();
  if (!wasm) return;
  try {
    self.postMessage({ type: 'progress', value: 0, message: 'Starting Rust simulation…' });
    const result = runOne(data.algorithm, data);
    self.postMessage({ type: 'progress', value: 1, message: 'Rust simulation complete.' });
    self.postMessage({ type: 'result', result }, transferables(result));
  } catch (error) {
    self.postMessage({ type: 'error', phase: 'run', message: error?.message || String(error) });
  }
};

// One simulation on the prepared inputs in `data`, as typed arrays the main
// thread reads without copying.
function runOne(name, data) {
  const traceMode = data.traceMode === 'full'
    ? wasm.TraceMode.Full
    : data.traceMode === 'bounded' ? wasm.TraceMode.Bounded : wasm.TraceMode.Off;
  const simulation = new wasm.WasmSimulation(
    data.nodeCount, new Uint32Array(data.edges), new Uint32Array(data.starts),
    algorithmSelector(name), traceMode, BigInt(data.roundLimit), data.maxTraceRecords, data.sampleEvery,
  );
  try {
    const output = simulation.run();
    const result = {
      positions: output.positions(),
      statuses: output.statuses(),
      homes: output.homes(),
      terminationCode: output.termination_code(),
      rounds: Number(output.rounds()), moves: Number(output.moves()), probes: Number(output.probes()),
      renderRecordCount: output.render_record_count(),
      renderTruncated: output.render_truncated(),
      renderBytes: output.render_bytes(),
      groupBytes: output.group_bytes(),
    };
    output.free();
    return result;
  } finally {
    simulation.free();
  }
}

function transferables(result) {
  return [result.positions.buffer, result.statuses.buffer, result.homes.buffer, result.renderBytes.buffer, result.groupBytes.buffer];
}
