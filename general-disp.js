const $ = (id) => document.getElementById(id);

const ROLES = [
  { name: 'Settler', color: '--settler' },
  { name: 'Helper', color: '--helper' },
  { name: 'Active leader', color: '--leader' },
  { name: 'Waiting leader', color: '--waiting' },
  { name: 'Weak zombie', color: '--weak' },
  { name: 'Strong zombie', color: '--strong' },
];

const PRESETS = {
  two: { family: 'path', nodes: 6, starts: [0, 0, 0, 0, 3, 3] },
  three: { family: 'cycle', nodes: 8, starts: [1, 5, 5, 4, 4, 1, 4, 1] },
};

const MASK = (1n << 64n) - 1n;

let wasm = null;
let starts = [];
let result = null;
let frame = 0;
let timer = null;
let points = [];

function css(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function groupColor(leader) {
  return `hsl(${(leader * 137.5) % 360} 70% 50%)`;
}

async function loadWasm() {
  const response = await fetch('./wasm/web/manifest.json', { cache: 'no-store' });
  const { buildId } = await response.json();
  const token = encodeURIComponent(buildId);
  const module = await import(`./wasm/web/ccm_wasm.js?v=${token}`);
  await module.default(`./wasm/web/ccm_wasm_bg.wasm?v=${token}`);
  return module;
}

function readGraph() {
  const family = $('family').value;
  const nodes = Number($('nodes').value);
  const param = Number($('param').value);
  return { family, nodes, param };
}

function graphLabel({ family, param }) {
  if (family === 'tree') return `tree:${param}`;
  if (family === 'grid') return `grid:${param}`;
  return family;
}

function graphError({ family, nodes, param }) {
  if (!Number.isInteger(nodes) || nodes < 2 || nodes > 64) return 'Nodes must be between 2 and 64.';
  if (family === 'cycle' && nodes < 3) return 'A cycle needs at least 3 nodes.';
  if ((family === 'tree' || family === 'grid') && (!Number.isInteger(param) || param < 1)) {
    return `${family === 'tree' ? 'Branching' : 'Columns'} must be at least 1.`;
  }
  return null;
}

function graphEdges({ family, nodes: n, param }) {
  const edges = [];
  if (family === 'path') for (let v = 0; v + 1 < n; v++) edges.push([v, v + 1]);
  if (family === 'cycle') for (let v = 0; v < n; v++) edges.push([v, (v + 1) % n]);
  if (family === 'star') for (let v = 1; v < n; v++) edges.push([0, v]);
  if (family === 'complete') {
    for (let a = 0; a < n; a++) for (let b = a + 1; b < n; b++) edges.push([a, b]);
  }
  if (family === 'tree') for (let c = 1; c < n; c++) edges.push([Math.floor((c - 1) / param), c]);
  if (family === 'grid') {
    for (let v = 0; v < n; v++) {
      if ((v % param) + 1 < param && v + 1 < n) edges.push([v, v + 1]);
      if (v + param < n) edges.push([v, v + param]);
    }
  }
  return edges;
}

function layout({ family, nodes: n, param }, width, height) {
  const pad = 36;
  const spanX = width - 2 * pad;
  const spanY = height - 2 * pad;
  const grid = (columns, order) => {
    const rows = Math.ceil(n / columns);
    return Array.from({ length: n }, (_, v) => {
      const row = Math.floor(v / columns);
      let column = v % columns;
      if (order === 'snake' && row % 2 === 1) column = columns - 1 - column;
      return {
        x: pad + (columns > 1 ? (spanX * column) / (columns - 1) : spanX / 2),
        y: pad + (rows > 1 ? (spanY * row) / (rows - 1) : spanY / 2),
      };
    });
  };
  if (family === 'path') return grid(Math.min(n, 12), 'snake');
  if (family === 'grid') return grid(param, 'rows');
  if (family === 'tree') {
    const depth = [0];
    for (let c = 1; c < n; c++) depth.push(depth[Math.floor((c - 1) / param)] + 1);
    const levels = Math.max(...depth) + 1;
    const layers = Array.from({ length: levels }, () => []);
    depth.forEach((d, v) => layers[d].push(v));
    const out = [];
    layers.forEach((layer, d) => layer.forEach((v, i) => {
      out[v] = {
        x: pad + (spanX * (i + 1)) / (layer.length + 1),
        y: pad + (levels > 1 ? (spanY * d) / (levels - 1) : spanY / 2),
      };
    }));
    return out;
  }
  const cx = width / 2;
  const cy = height / 2;
  const radius = Math.min(spanX, spanY) / 2;
  const ring = (v, count, offset) => ({
    x: cx + radius * Math.cos((2 * Math.PI * (v - offset)) / count - Math.PI / 2),
    y: cy + radius * Math.sin((2 * Math.PI * (v - offset)) / count - Math.PI / 2),
  });
  if (family === 'star') return Array.from({ length: n }, (_, v) => (v === 0 ? { x: cx, y: cy } : ring(v, n - 1, 1)));
  return Array.from({ length: n }, (_, v) => ring(v, n, 0));
}

function seeded(seed) {
  let state = BigInt.asUintN(64, seed);
  const next = () => {
    state = (state + 0x9E3779B97F4A7C15n) & MASK;
    let z = state;
    z = ((z ^ (z >> 30n)) * 0xBF58476D1CE4E5B9n) & MASK;
    z = ((z ^ (z >> 27n)) * 0x94D049BB133111EBn) & MASK;
    return z ^ (z >> 31n);
  };
  return (upper) => {
    const u = BigInt(upper);
    const threshold = ((MASK + 1n - u) & MASK) % u;
    for (;;) {
      const value = next();
      if (value >= threshold) return Number(value % u);
    }
  };
}

function placeUniform(n, agents, distinct, seed) {
  const index = seeded(seed);
  const candidates = [...Array(n).keys()];
  for (let end = n - 1; end >= 1; end--) {
    const selected = index(end + 1);
    [candidates[end], candidates[selected]] = [candidates[selected], candidates[end]];
  }
  candidates.length = distinct;
  return Array.from({ length: agents }, () => candidates[index(distinct)]);
}

function mode() {
  return document.querySelector('input[name="mode"]:checked').value;
}

function seedValue() {
  try {
    return BigInt($('seed').value || '0');
  } catch {
    return 0n;
  }
}

function updatePlacement() {
  const graph = readGraph();
  $('paramLabel').hidden = !(graph.family === 'tree' || graph.family === 'grid');
  $('paramName').textContent = graph.family === 'tree' ? 'Branching' : 'Columns';
  $('randomFields').hidden = mode() !== 'random';
  $('clickFields').hidden = mode() !== 'click';
  let error = graphError(graph);
  if (!error && mode() === 'random') {
    const agents = Number($('agents').value);
    const distinct = Number($('distinct').value);
    if (!Number.isInteger(agents) || agents < 1 || agents > graph.nodes) error = 'Agents must be between 1 and the node count.';
    else if (!Number.isInteger(distinct) || distinct < 1 || distinct > graph.nodes) error = 'Start nodes must be between 1 and the node count.';
    else starts = placeUniform(graph.nodes, agents, distinct, seedValue());
  }
  if (!error && mode() === 'click') starts = starts.filter((v) => v < graph.nodes);
  if (error) starts = [];
  $('message').textContent = error || '';
  const used = new Set(starts).size;
  $('summary').textContent = `${starts.length} agent${starts.length === 1 ? '' : 's'} on ${used} node${used === 1 ? '' : 's'}`;
  result = null;
  stop();
  render();
}

function roundLimit(k) {
  const log = Math.ceil(Math.log2(Math.max(k, 1))) + 1;
  return 480 * k * log * log;
}

function decodeFrames(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const count = view.getUint32(4, true);
  const optional = (at, width) => (bytes[at] === 0 ? 1 : 1 + width);
  const frames = [];
  let at = 8;
  for (let i = 0; i < count; i++) {
    const kind = bytes[at];
    const step = Number(view.getBigUint64(at + 1, true));
    at += 9;
    if (kind === 3) {
      const agents = view.getUint32(at, true);
      at += 4;
      const nodes = new Uint32Array(agents);
      const homes = new Int32Array(agents);
      for (let a = 0; a < agents; a++) {
        nodes[a] = view.getUint32(at, true);
        at += 5;
        homes[a] = bytes[at] === 0 ? -1 : view.getUint32(at + 1, true);
        at += optional(at, 4);
      }
      frames.push({ step, nodes, homes });
      continue;
    }
    const event = bytes[at];
    at += 1;
    if (event === 0) at += 1;
    else if (event === 1) { at += 14; at += optional(at, 2); }
    else if (event === 2) at += 4 + 4 * view.getUint32(at, true) + 10;
    else if (event === 3) at += 8;
    else if (event === 4) at += 6;
    else if (event === 5) { at += 16; at += optional(at, 2); at += optional(at, 2); }
    else if (event === 6) at += 5;
    else throw new Error(`Unknown trace event ${event}`);
  }
  return frames;
}

function decodeGroups(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const u32 = (at) => view.getUint32(at, true);
  const roles = [];
  const events = [];
  let at = 8;
  for (let i = 0, count = u32(4); i < count; i++) {
    const kind = bytes[at];
    const frame = Number(view.getBigUint64(at + 1, true));
    const slot = bytes[at + 9];
    at += 10;
    if (kind === 1) {
      roles.push({ frame, agent: u32(at), role: bytes[at + 4], level: u32(at + 5), leader: u32(at + 9) });
      at += 13;
    } else {
      events.push({
        frame, slot, code: bytes[at], agent: u32(at + 1), node: u32(at + 5),
        a: u32(at + 9), b: u32(at + 13), c: u32(at + 17), d: u32(at + 21),
      });
      at += 25;
    }
  }
  return { roles, events };
}

function snapshots(frames, roles, agents) {
  const role = new Uint8Array(agents);
  const level = new Uint32Array(agents);
  const leader = new Uint32Array(agents);
  let next = 0;
  return frames.map((f) => {
    while (next < roles.length && roles[next].frame <= f.step) {
      const change = roles[next++];
      role[change.agent] = change.role;
      level[change.agent] = change.level;
      leader[change.agent] = change.leader;
    }
    return { role: role.slice(), level: level.slice(), leader: leader.slice() };
  });
}

function run() {
  stop();
  const graph = readGraph();
  const error = graphError(graph) || (starts.length === 0 ? 'Place at least one agent.' : null);
  if (error) {
    $('message').textContent = error;
    return;
  }
  const edges = graphEdges(graph);
  const simulation = new wasm.WasmSimulation(
    graph.nodes,
    Uint32Array.from(edges.flat()),
    Uint32Array.from(starts),
    wasm.AlgorithmSelector.GeneralDisp,
    wasm.TraceMode.Full,
    BigInt(roundLimit(starts.length)),
    0,
    1,
  );
  try {
    const output = simulation.run();
    const frames = decodeFrames(output.render_bytes());
    const { roles, events } = decodeGroups(output.group_bytes());
    const termination = ['completed', 'round limit reached', 'cancelled', 'invalid input'][output.termination_code()];
    result = {
      graph, edges, frames, events, termination,
      starts: starts.slice(),
      steps: Number(output.rounds()),
      shown: snapshots(frames, roles, starts.length),
    };
    output.free();
    $('message').textContent = '';
  } catch (failure) {
    result = null;
    $('message').textContent = String(failure);
  } finally {
    simulation.free();
  }
  frame = 0;
  $('slider').max = result ? result.frames.length - 1 : 0;
  render();
}

function stop() {
  clearInterval(timer);
  timer = null;
  $('play').textContent = '▶';
}

function play() {
  if (!result) return;
  if (timer) {
    stop();
    return;
  }
  if (frame >= result.frames.length - 1) frame = 0;
  $('play').textContent = '⏸';
  timer = setInterval(() => {
    if (frame >= result.frames.length - 1) {
      stop();
      return;
    }
    frame += 1;
    render();
  }, Number($('speed').value));
}

function show(target) {
  if (!result) return;
  frame = Math.max(0, Math.min(result.frames.length - 1, target));
  render();
}

function slotText(step) {
  if (step === 0) return 'start';
  return `slot ${((step - 1) % 12) + 1}/12 · unit ${Math.floor((step - 1) / 12) + 1}`;
}

function describe(e) {
  const group = (level, leader) => `${level}:${leader}`;
  switch (e.code) {
    case 0: return `agent ${e.agent} became a zombie of ${group(e.a, e.b)} at node ${e.node}`;
    case 1: return `agent ${e.agent} settled at node ${e.node} in ${group(e.a, e.b)}`;
    case 2: return `merge: settler ${e.agent} at node ${e.node} joined ${group(e.c, e.d)} from ${group(e.a, e.b)}`;
    case 3: return `leader ${e.agent} rose to level ${e.a} with zombie ${e.b} at node ${e.node}`;
    case 4: return `leader ${e.agent} moved forward ${e.node} → ${e.a}`;
    case 5: return `leader ${e.agent} moved back ${e.node} → ${e.a}`;
    case 6: return `zombie ${e.agent}: location level ${e.a}, swarm level ${e.b}`;
    default: return `${e.b ? 'weak' : 'strong'} zombie ${e.agent} chased ${e.node} → ${e.a}`;
  }
}

function render() {
  drawCanvas();
  const current = result?.frames[frame];
  $('slider').value = frame;
  if (!result) {
    $('status').textContent = 'Not run yet';
  } else {
    const end = frame === result.frames.length - 1 ? ` · ${result.termination} after ${result.steps} steps` : '';
    $('status').textContent = `Step ${current.step} of ${result.steps} · ${slotText(current.step)}${end}`;
  }
  const shownStarts = result ? result.starts : starts;
  const graph = result ? result.graph : readGraph();
  $('cli').textContent = shownStarts.length
    ? `ccm transitions --algorithm general-disp --graph ${graphLabel(graph)} --nodes ${graph.nodes} --agents ${shownStarts.length} --ports canonical --placement explicit:${shownStarts.join(',')}`
    : '';
  renderEvents(current);
  renderAgents(current);
}

function renderEvents(current) {
  const list = $('events');
  list.replaceChildren();
  if (!result) return;
  const groupsOnly = $('groupsOnly').checked;
  const visible = result.events.filter((e) => e.frame <= current.step && (!groupsOnly || e.code <= 3));
  for (const e of visible.slice(-300).reverse()) {
    const item = document.createElement('li');
    if (e.frame === current.step) item.className = 'now';
    const when = document.createElement('span');
    when.className = 'when';
    when.textContent = `step ${e.frame - 1} · slot ${e.slot}`;
    item.append(when, describe(e));
    list.append(item);
  }
}

function renderAgents(current) {
  const body = $('agentRows');
  body.replaceChildren();
  if (!result) return;
  const shown = result.shown[frame];
  for (let a = 0; a < current.nodes.length; a++) {
    const row = document.createElement('tr');
    const home = current.homes[a];
    for (const text of [a, current.nodes[a], ROLES[shown.role[a]].name, `${shown.level[a]}:${shown.leader[a]}`, home < 0 ? '—' : home]) {
      const cell = document.createElement('td');
      cell.textContent = text;
      row.append(cell);
    }
    body.append(row);
  }
}

function agentsAt(node) {
  const nodes = result ? result.frames[frame].nodes : starts;
  const out = [];
  for (let a = 0; a < nodes.length; a++) if (nodes[a] === node) out.push(a);
  return out;
}

function drawCanvas() {
  const canvas = $('canvas');
  const ratio = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  canvas.width = Math.round(width * ratio);
  canvas.height = Math.round(height * ratio);
  const ctx = canvas.getContext('2d');
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const graph = result ? result.graph : readGraph();
  if (graphError(graph)) {
    points = [];
    return;
  }
  points = layout(graph, width, height);
  const edges = result ? result.edges : graphEdges(graph);
  ctx.strokeStyle = css('--edge');
  ctx.lineWidth = 1.5;
  for (const [a, b] of edges) {
    ctx.beginPath();
    ctx.moveTo(points[a].x, points[a].y);
    ctx.lineTo(points[b].x, points[b].y);
    ctx.stroke();
  }
  const shown = result ? result.shown[frame] : null;
  for (let v = 0; v < points.length; v++) {
    const { x, y } = points[v];
    ctx.beginPath();
    ctx.arc(x, y, 13, 0, 2 * Math.PI);
    ctx.fillStyle = css('--node');
    ctx.fill();
    ctx.strokeStyle = css('--edge');
    ctx.stroke();
    ctx.fillStyle = css('--muted');
    ctx.font = '11px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillText(String(v), x, y);
    const here = agentsAt(v);
    here.forEach((agent, i) => {
      const angle = (2 * Math.PI * i) / Math.max(here.length, 1) - Math.PI / 4;
      const distance = here.length === 1 ? 17 : 20;
      const ax = x + distance * Math.cos(angle);
      const ay = y + distance * Math.sin(angle);
      ctx.beginPath();
      ctx.arc(ax, ay, 5.5, 0, 2 * Math.PI);
      ctx.fillStyle = css(ROLES[shown ? shown.role[agent] : 2].color);
      ctx.fill();
      ctx.lineWidth = 2;
      ctx.strokeStyle = groupColor(shown ? shown.leader[agent] : agent);
      ctx.stroke();
      ctx.lineWidth = 1.5;
    });
  }
}

function nodeAt(event) {
  const rect = $('canvas').getBoundingClientRect();
  const x = event.clientX - rect.left;
  const y = event.clientY - rect.top;
  let best = -1;
  let bestDistance = 22;
  points.forEach((p, v) => {
    const distance = Math.hypot(p.x - x, p.y - y);
    if (distance < bestDistance) {
      best = v;
      bestDistance = distance;
    }
  });
  return { node: best, x, y };
}

function onCanvasClick(event) {
  if (mode() !== 'click') return;
  const { node } = nodeAt(event);
  if (node < 0) return;
  if (event.shiftKey) {
    const index = starts.lastIndexOf(node);
    if (index >= 0) starts.splice(index, 1);
  } else if (starts.length < readGraph().nodes) {
    starts.push(node);
  } else {
    $('message').textContent = 'There can be at most one agent per node in total.';
    return;
  }
  updatePlacement();
}

function onCanvasHover(event) {
  const tooltip = $('tooltip');
  const { node, x, y } = nodeAt(event);
  if (node < 0) {
    tooltip.hidden = true;
    return;
  }
  const shown = result ? result.shown[frame] : null;
  const lines = [`node ${node}`];
  for (const agent of agentsAt(node)) {
    lines.push(shown
      ? `agent ${agent} · ${ROLES[shown.role[agent]].name.toLowerCase()} · group ${shown.level[agent]}:${shown.leader[agent]}`
      : `agent ${agent}`);
  }
  tooltip.textContent = lines.join('\n');
  tooltip.style.left = `${x + 14}px`;
  tooltip.style.top = `${y + 14}px`;
  tooltip.hidden = false;
}

function applyPreset(name) {
  const preset = PRESETS[name];
  if (!preset) return;
  $('family').value = preset.family;
  $('nodes').value = preset.nodes;
  document.querySelector('input[name="mode"][value="click"]').checked = true;
  starts = preset.starts.slice();
  updatePlacement();
  run();
}

function applyQuery() {
  const query = new URLSearchParams(location.search);
  for (const key of ['family', 'nodes', 'param', 'agents', 'distinct', 'seed']) {
    if (query.has(key)) $(key).value = query.get(key);
  }
  if (query.get('mode') === 'click') document.querySelector('input[name="mode"][value="click"]').checked = true;
  if (query.has('starts')) {
    document.querySelector('input[name="mode"][value="click"]').checked = true;
    starts = query.get('starts').split(',').filter(Boolean).map(Number);
  }
  updatePlacement();
  if (query.has('preset')) applyPreset(query.get('preset'));
  else if (query.has('run')) run();
  if (result && query.get('at') === 'end') show(result.frames.length - 1);
}

function wire() {
  const legend = $('legend');
  for (const role of ROLES) {
    const item = document.createElement('li');
    const swatch = document.createElement('span');
    swatch.className = 'swatch';
    swatch.style.background = `var(${role.color})`;
    item.append(swatch, role.name);
    legend.append(item);
  }
  for (const id of ['family', 'nodes', 'param', 'agents', 'distinct', 'seed']) {
    $(id).addEventListener('input', updatePlacement);
  }
  document.querySelectorAll('input[name="mode"]').forEach((input) => input.addEventListener('change', () => {
    if (mode() === 'click') starts = [];
    updatePlacement();
  }));
  document.querySelectorAll('[data-preset]').forEach((button) => button.addEventListener('click', () => applyPreset(button.dataset.preset)));
  $('clear').addEventListener('click', () => {
    starts = [];
    updatePlacement();
  });
  $('run').addEventListener('click', run);
  $('play').addEventListener('click', play);
  $('first').addEventListener('click', () => show(0));
  $('back').addEventListener('click', () => show(frame - 1));
  $('forward').addEventListener('click', () => show(frame + 1));
  $('nextEvent').addEventListener('click', () => {
    if (!result) return;
    const now = result.frames[frame].step;
    const next = result.events.find((e) => e.code <= 3 && e.frame > now);
    if (next) show(result.frames.findIndex((f) => f.step >= next.frame));
  });
  $('slider').addEventListener('input', () => show(Number($('slider').value)));
  $('speed').addEventListener('change', () => {
    if (timer) {
      stop();
      play();
    }
  });
  $('groupsOnly').addEventListener('change', render);
  $('canvas').addEventListener('click', onCanvasClick);
  $('canvas').addEventListener('mousemove', onCanvasHover);
  $('canvas').addEventListener('mouseleave', () => { $('tooltip').hidden = true; });
  window.addEventListener('resize', drawCanvas);
}

async function main() {
  wire();
  try {
    wasm = await loadWasm();
  } catch {
    $('message').textContent = 'The Wasm package is missing. Run ./scripts/build-wasm.sh';
    $('run').disabled = true;
    return;
  }
  applyQuery();
}

main();
