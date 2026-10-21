const $ = (id) => document.getElementById(id);

const STATUS_COLORS = ['--r0', '--r2', '--r1'];
const GENERAL_ROLES = ['settler', 'helper', 'active leader', 'waiting leader', 'weak zombie', 'strong zombie'];
const GENERAL_COLORS = ['--r0', '--r1', '--r2', '--r3', '--r4', '--r5'];

const ALGORITHMS = {
  general: { label: 'GeneralDisp', selector: 'GeneralDisp', rooted: false, cli: 'general-disp', statuses: ['settled', 'unsettled', 'helper'] },
  rooted: { label: 'RootedDisp', selector: 'RootedDisp', rooted: true, cli: 'rooted-disp', statuses: ['settler', 'explorer', 'helper'] },
  p1tree: { label: 'P1Tree', selector: 'P1Tree', rooted: true, cli: 'p1tree', statuses: ['settled', 'unsettled', 'scout'] },
  help: { label: 'Help by Scouts', selector: 'HelpByScouts', rooted: true, cli: 'help-by-scouts', statuses: ['settled', 'unsettled', 'scout'] },
  drop: { label: 'Drop and Freeze', selector: 'DropAndFreeze', rooted: false, cli: 'drop-and-freeze', statuses: ['settled', 'unsettled', 'waiting'] },
};

const PHASES = ['macro-round', 'probe out', 'probe back', 'movement', 'scout', 'vacate', 'chase', 'follow', 'retrace', 'other', 'helpers home'];

const PRESETS = {
  two: { algorithm: 'general', family: 'path', nodes: 6, starts: [0, 0, 0, 0, 3, 3] },
  three: { algorithm: 'general', family: 'cycle', nodes: 8, starts: [1, 5, 5, 4, 4, 1, 4, 1] },
  star: { family: 'star', nodes: 9, agents: 9 },
};

const MAX_NODES = 300;
const MAX_AGENTS = 120;
const MASK = (1n << 64n) - 1n;

let wasm = null;
let starts = [];
let run = null;
let index = 0;
let timer = null;
let points = [];
let layoutKey = '';
let placed = null;
let picked = -1;
let anim = null;
let drag = null;
const view = { scale: 1, x: 0, y: 0 };

function css(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function groupColor(leader, alpha = 1) {
  return `hsl(${(leader * 137.5) % 360} 70% 50% / ${alpha})`;
}

// GeneralDisp territories: each node with a settler belongs to the settler's
// group, read from its home so a settler away helping still holds its node.
function territoryMap() {
  if (!run?.groups || !$('territories').checked) return null;
  const shown = run.groups.shown[index];
  const homes = run.frames[index].homes;
  const map = new Map();
  for (let a = 0; a < homes.length; a++) {
    if (homes[a] >= 0 && shown.role[a] <= 1) map.set(homes[a], shown.leader[a]);
  }
  return map;
}

async function loadWasm() {
  const response = await fetch('./wasm/web/manifest.json', { cache: 'no-store' });
  const { buildId } = await response.json();
  const token = encodeURIComponent(buildId);
  const module = await import(`./wasm/web/ccm_wasm.js?v=${token}`);
  await module.default(`./wasm/web/ccm_wasm_bg.wasm?v=${token}`);
  return module;
}

const RANDOM = new Set(['random', 'bounded']);
const PARAMS = { tree: ['Branching', 2], grid: ['Columns', 4], random: ['Extra edges', 10], bounded: ['Max degree', 3] };

function readGraph() {
  const family = $('family').value;
  return { family, nodes: Number($('nodes').value), param: Number($('param').value), seed: RANDOM.has(family) ? seedValue() : 0n };
}

function graphLabel({ family, param }) {
  if (family === 'tree') return `tree:${param}`;
  if (family === 'grid') return `grid:${param}`;
  if (family === 'random') return `random-connected:${param}`;
  if (family === 'bounded') return `random-bounded:${param}`;
  return family;
}

function graphError({ family, nodes, param }) {
  if (!Number.isInteger(nodes) || nodes < 2 || nodes > MAX_NODES) return `Nodes must be between 2 and ${MAX_NODES}.`;
  if (family === 'cycle' && nodes < 3) return 'A cycle needs at least 3 nodes.';
  if (family === 'complete' && nodes > 60) return 'Complete graphs are capped at 60 nodes here.';
  if ((family === 'tree' || family === 'grid') && (!Number.isInteger(param) || param < 1)) return 'Branching and columns must be at least 1.';
  if (family === 'random' && (!Number.isInteger(param) || param < 0)) return 'Extra edges must be 0 or more.';
  if (family === 'bounded' && (!Number.isInteger(param) || param < 1 || (param === 1 && nodes > 2))) return 'Max degree must be at least 2 (1 only fits 2 nodes).';
  return null;
}

function graphEdges({ family, nodes: n, param, seed }) {
  if (family === 'random') return randomConnectedEdges(n, param, seed);
  if (family === 'bounded') return randomBoundedEdges(n, param, seed);
  const edges = [];
  if (family === 'path') for (let v = 0; v + 1 < n; v++) edges.push([v, v + 1]);
  if (family === 'cycle') for (let v = 0; v < n; v++) edges.push([v, (v + 1) % n]);
  if (family === 'star') for (let v = 1; v < n; v++) edges.push([0, v]);
  if (family === 'complete') for (let a = 0; a < n; a++) for (let b = a + 1; b < n; b++) edges.push([a, b]);
  if (family === 'tree') for (let c = 1; c < n; c++) edges.push([Math.floor((c - 1) / param), c]);
  if (family === 'grid') {
    for (let v = 0; v < n; v++) {
      if ((v % param) + 1 < param && v + 1 < n) edges.push([v, v + 1]);
      if (v + param < n) edges.push([v, v + param]);
    }
  }
  return edges;
}

// The CLI's `random-connected:extra`: a random tree, then extra random edges.
function randomConnectedEdges(n, extra, seed) {
  const index = seeded(seed);
  const edges = new Set();
  const key = (a, b) => (a < b ? a * n + b : b * n + a);
  for (let node = 1; node < n; node++) edges.add(key(index(node), node));
  const maximum = (n * (n - 1)) / 2;
  const target = Math.min(edges.size + extra, maximum);
  const attemptLimit = Math.max(maximum * 8, 32);
  for (let attempts = 0; edges.size < target && attempts < attemptLimit; attempts++) {
    const left = index(n);
    const right = index(n);
    if (left !== right) edges.add(key(left, right));
  }
  for (let left = 0; left < n && edges.size < target; left++) {
    for (let right = left + 1; right < n && edges.size < target; right++) edges.add(key(left, right));
  }
  return [...edges].sort((p, q) => p - q).map((k) => [Math.floor(k / n), k % n]);
}

// The CLI's `random-bounded:max-degree`: a random tree under the degree cap,
// then random edges while both ends have room.
function randomBoundedEdges(n, maxDegree, seed) {
  const index = seeded(seed);
  const degrees = new Array(n).fill(0);
  const edges = new Set();
  const key = (a, b) => (a < b ? a * n + b : b * n + a);
  for (let node = 1; node < n; node++) {
    const candidates = [];
    for (let c = 0; c < node; c++) if (degrees[c] < maxDegree) candidates.push(c);
    const selected = candidates.length ? candidates[index(candidates.length)] : node - 1;
    edges.add(key(selected, node));
    degrees[selected] += 1;
    degrees[node] += 1;
  }
  const target = Math.max(Math.floor((n * maxDegree) / 2), edges.size);
  const attemptLimit = Math.max(n * maxDegree * 16, 32);
  for (let attempt = 0; attempt < attemptLimit && edges.size < target; attempt++) {
    const left = index(n);
    const right = index(n);
    if (left === right || degrees[left] >= maxDegree || degrees[right] >= maxDegree) continue;
    const k = key(left, right);
    if (!edges.has(k)) {
      edges.add(k);
      degrees[left] += 1;
      degrees[right] += 1;
    }
  }
  return [...edges].sort((p, q) => p - q).map((k) => [Math.floor(k / n), k % n]);
}

// A deterministic force-directed layout for graphs with no natural shape.
function springLayout(graph, width, height, pad) {
  const n = graph.nodes;
  const edges = graphEdges(graph);
  const pts = Array.from({ length: n }, (_, v) => ({
    x: Math.cos((2 * Math.PI * v) / n) * 0.5 + 0.5,
    y: Math.sin((2 * Math.PI * v) / n) * 0.5 + 0.5,
  }));
  const k = Math.sqrt(1 / n);
  let temperature = 0.1;
  for (let step = 0; step < 240; step++) {
    const force = pts.map(() => ({ x: 0, y: 0 }));
    for (let a = 0; a < n; a++) {
      for (let b = a + 1; b < n; b++) {
        const dx = pts[a].x - pts[b].x;
        const dy = pts[a].y - pts[b].y;
        const d2 = Math.max(dx * dx + dy * dy, 1e-6);
        const push = (k * k) / d2;
        force[a].x += dx * push;
        force[a].y += dy * push;
        force[b].x -= dx * push;
        force[b].y -= dy * push;
      }
    }
    for (const [a, b] of edges) {
      const dx = pts[a].x - pts[b].x;
      const dy = pts[a].y - pts[b].y;
      const d = Math.max(Math.hypot(dx, dy), 1e-6);
      const pull = d / k;
      force[a].x -= dx * pull;
      force[a].y -= dy * pull;
      force[b].x += dx * pull;
      force[b].y += dy * pull;
    }
    for (let v = 0; v < n; v++) {
      const f = Math.max(Math.hypot(force[v].x, force[v].y), 1e-9);
      const move = Math.min(f, temperature);
      pts[v].x += (force[v].x / f) * move;
      pts[v].y += (force[v].y / f) * move;
    }
    temperature *= 0.985;
  }
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  const [minX, maxX, minY, maxY] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
  const spanX = Math.max(width - 2 * pad, 1);
  const spanY = Math.max(height - 2 * pad, 1);
  return pts.map((p) => ({
    x: pad + (maxX > minX ? ((p.x - minX) / (maxX - minX)) * spanX : spanX / 2),
    y: pad + (maxY > minY ? ((p.y - minY) / (maxY - minY)) * spanY : spanY / 2),
  }));
}

function neighbors(graph, edges) {
  const out = Array.from({ length: graph.nodes }, () => []);
  for (const [a, b] of edges) {
    out[a].push(b);
    out[b].push(a);
  }
  for (const list of out) list.sort((p, q) => p - q);
  return out;
}

function layout(graph, width, height) {
  const { family, nodes: n, param } = graph;
  const pad = 44;
  if (RANDOM.has(family)) return springLayout(graph, width, height, pad);
  const spanX = Math.max(width - 2 * pad, 1);
  const spanY = Math.max(height - 2 * pad, 1);
  const grid = (columns, snake) => {
    const rows = Math.ceil(n / columns);
    return Array.from({ length: n }, (_, v) => {
      const row = Math.floor(v / columns);
      let column = v % columns;
      if (snake && row % 2 === 1) column = columns - 1 - column;
      return {
        x: pad + (columns > 1 ? (spanX * column) / (columns - 1) : spanX / 2),
        y: pad + (rows > 1 ? (spanY * row) / (rows - 1) : spanY / 2),
      };
    });
  };
  if (family === 'path') return grid(Math.min(n, Math.max(6, Math.floor(spanX / 60))), true);
  if (family === 'grid') return grid(param, false);
  if (family === 'tree') {
    const depth = [0];
    for (let c = 1; c < n; c++) depth.push(depth[Math.floor((c - 1) / param)] + 1);
    const levels = Math.max(...depth) + 1;
    const layers = Array.from({ length: levels }, () => []);
    depth.forEach((d, v) => layers[d].push(v));
    const out = [];
    layers.forEach((layer, d) => layer.forEach((v, i) => {
      out[v] = { x: pad + (spanX * (i + 1)) / (layer.length + 1), y: pad + (levels > 1 ? (spanY * d) / (levels - 1) : spanY / 2) };
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

function shuffled(n, index) {
  const candidates = [...Array(n).keys()];
  for (let end = n - 1; end >= 1; end--) {
    const selected = index(end + 1);
    [candidates[end], candidates[selected]] = [candidates[selected], candidates[end]];
  }
  return candidates;
}

function placeUniform(n, agents, distinct, seed) {
  const index = seeded(seed);
  const candidates = shuffled(n, index).slice(0, distinct);
  return Array.from({ length: agents }, () => candidates[index(distinct)]);
}

function placeSpread(n, agents, seed) {
  return shuffled(n, seeded(seed)).slice(0, agents);
}

function seedValue() {
  try {
    return BigInt($('seed').value || '0');
  } catch {
    return 0n;
  }
}

function defaultLimit(algorithm, k) {
  if (algorithm !== 'general') return 40 * k;
  const log = Math.ceil(Math.log2(Math.max(k, 1))) + 1;
  return 480 * k * log * log;
}

function syncControls() {
  const graph = readGraph();
  const spec = ALGORITHMS[$('algorithm').value];
  for (const option of $('placement').options) {
    const allowed = !spec.rooted || option.value === 'rooted';
    option.hidden = !allowed;
    option.disabled = !allowed;
  }
  if (spec.rooted && $('placement').value !== 'rooted') $('placement').value = 'rooted';
  $('placementNote').hidden = !spec.rooted;
  $('placementNote').textContent = `${spec.label} supports only a rooted start: every agent on one node. Pick the start node below, or click it on the graph.`;
  const placement = $('placement').value;
  $('paramWrap').hidden = !PARAMS[graph.family];
  if (PARAMS[graph.family]) $('paramName').textContent = PARAMS[graph.family][0];
  for (const button of document.querySelectorAll('[data-preset]')) {
    const only = PRESETS[button.dataset.preset].algorithm;
    button.hidden = Boolean(only) && only !== $('algorithm').value;
  }
  $('agentsWrap').hidden = placement === 'click';
  $('rootWrap').hidden = placement !== 'rooted';
  $('distinctWrap').hidden = placement !== 'groups';
  $('seedWrap').hidden = !(placement === 'groups' || placement === 'spread' || RANDOM.has(graph.family));
  $('clickHint').hidden = placement !== 'click';
  $('canvas').classList.toggle('placing', placement === 'click');
}

function updatePlacement() {
  syncControls();
  const graph = readGraph();
  const placement = $('placement').value;
  const agents = Number($('agents').value);
  let error = graphError(graph);
  if (!error && placement !== 'click' && (!Number.isInteger(agents) || agents < 1 || agents > Math.min(graph.nodes, MAX_AGENTS))) {
    error = `Agents must be between 1 and ${Math.min(graph.nodes, MAX_AGENTS)}.`;
  }
  if (!error) {
    if (placement === 'rooted') {
      const root = Number($('root').value);
      if (!Number.isInteger(root) || root < 0 || root >= graph.nodes) error = `Start node must be between 0 and ${graph.nodes - 1}.`;
      else starts = Array(agents).fill(root);
    }
    if (placement === 'spread') starts = placeSpread(graph.nodes, agents, seedValue());
    if (placement === 'groups') {
      const distinct = Number($('distinct').value);
      if (!Number.isInteger(distinct) || distinct < 1 || distinct > graph.nodes) error = 'Start nodes must be between 1 and the node count.';
      else starts = placeUniform(graph.nodes, agents, distinct, seedValue());
    }
    if (placement === 'click') starts = starts.filter((v) => v < graph.nodes);
  }
  if (error) starts = [];
  say(error || '');
  const used = new Set(starts).size;
  $('summary').textContent = `${starts.length} agent${starts.length === 1 ? '' : 's'} on ${used} node${used === 1 ? '' : 's'}`;
  run = null;
  picked = -1;
  anim = null;
  stop();
  renderMarkers();
  render();
}

function say(text, ok = false) {
  $('message').textContent = text;
  $('message').classList.toggle('ok', ok);
}

function buildFrames(bytes, initial, output, statusNames) {
  const k = initial.length;
  const nodes = Uint32Array.from(initial);
  const status = new Uint8Array(k).fill(1);
  const homes = new Int32Array(k).fill(-1);
  const frames = [];
  const events = [];
  const phases = new Map();
  const stepMoves = new Map();
  let current = 0;
  const flush = () => {
    if (frames.length && frames[frames.length - 1].step === current) frames.pop();
    frames.push({ step: current, nodes: nodes.slice(), status: status.slice(), homes: homes.slice() });
  };
  const data = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const u32 = (at) => data.getUint32(at, true);
  const count = bytes.length >= 8 ? u32(4) : 0;
  let at = 8;
  const optional = (width) => {
    const present = bytes[at] !== 0;
    const value = present ? (width === 2 ? data.getUint16(at + 1, true) : u32(at + 1)) : -1;
    at += present ? 1 + width : 1;
    return value;
  };
  for (let i = 0; i < count; i++) {
    const kind = bytes[at];
    const step = Number(data.getBigUint64(at + 1, true));
    at += 9;
    if (step !== current) {
      flush();
      current = step;
    }
    if (kind === 3) {
      const agents = u32(at);
      at += 4;
      for (let a = 0; a < agents; a++) {
        nodes[a] = u32(at);
        status[a] = bytes[at + 4];
        at += 5;
        homes[a] = optional(4);
      }
      continue;
    }
    const event = bytes[at];
    at += 1;
    if (event === 0) {
      phases.set(step, [...(phases.get(step) || []), bytes[at]]);
      at += 1;
    } else if (event === 1) {
      nodes[u32(at)] = u32(at + 8);
      at += 14;
      optional(2);
      stepMoves.set(step, (stepMoves.get(step) || 0) + 1);
    } else if (event === 2) {
      const size = u32(at);
      const from = u32(at + 4 + 4 * size);
      const to = u32(at + 8 + 4 * size);
      for (let j = 0; j < size; j++) nodes[u32(at + 4 + 4 * j)] = to;
      events.push({ step, important: true, text: `group of ${size} moved ${from} → ${to}` });
      stepMoves.set(step, (stepMoves.get(step) || 0) + size);
      at += 4 + 4 * size + 10;
    } else if (event === 3) {
      const agent = u32(at);
      const node = u32(at + 4);
      nodes[agent] = node;
      status[agent] = 0;
      homes[agent] = node;
      events.push({ step, important: true, text: `agent ${agent} settled at node ${node}` });
      at += 8;
    } else if (event === 4) {
      const agent = u32(at);
      status[agent] = bytes[at + 5];
      events.push({ step, important: false, text: `agent ${agent}: ${statusNames[bytes[at + 4]] ?? '?'} → ${statusNames[bytes[at + 5]] ?? '?'}` });
      at += 6;
    } else if (event === 5) {
      events.push({ step, important: true, text: `tree edge ${u32(at + 8)} → ${u32(at + 12)}` });
      at += 16;
      optional(2);
      optional(2);
    } else if (event === 6) {
      at += 5;
    } else {
      throw new Error(`Unknown trace event ${event}`);
    }
  }
  flush();
  if (frames[0].step !== 0) frames.unshift({ step: 0, nodes: Uint32Array.from(initial), status: new Uint8Array(k).fill(1), homes: new Int32Array(k).fill(-1) });
  const final = output.positions;
  const last = frames[frames.length - 1];
  if (last.nodes.some((node, a) => node !== final[a])) {
    frames.push({ step: Math.max(output.rounds, last.step + 1), nodes: Uint32Array.from(final), status: Uint8Array.from(output.statuses), homes: Int32Array.from(output.homes) });
  }
  return { frames, events: events.length ? events : derivedEvents(frames, statusNames), phases, stepMoves };
}

// Settles and status changes read from consecutive frames, for traces that
// record checkpoints but no events.
function derivedEvents(frames, statusNames) {
  const events = [];
  for (let i = 1; i < frames.length; i++) {
    const before = frames[i - 1];
    const after = frames[i];
    for (let a = 0; a < after.status.length; a++) {
      if (before.status[a] === after.status[a]) continue;
      const settled = after.status[a] === 0 && before.status[a] === 1;
      events.push({
        step: after.step,
        important: settled,
        text: settled
          ? `agent ${a} settled at node ${after.nodes[a]}`
          : `agent ${a}: ${statusNames[before.status[a]] ?? '?'} → ${statusNames[after.status[a]] ?? '?'}`,
      });
    }
  }
  return events;
}

function decodeGroups(bytes) {
  const data = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const u32 = (at) => data.getUint32(at, true);
  const roles = [];
  const events = [];
  let at = 8;
  for (let i = 0, count = bytes.length >= 8 ? u32(4) : 0; i < count; i++) {
    const kind = bytes[at];
    const frame = Number(data.getBigUint64(at + 1, true));
    at += 10;
    if (kind === 1) {
      roles.push({ frame, agent: u32(at), role: bytes[at + 4], level: u32(at + 5), leader: u32(at + 9) });
      at += 13;
    } else {
      events.push({ frame, code: bytes[at], agent: u32(at + 1), node: u32(at + 5), a: u32(at + 9), b: u32(at + 13), c: u32(at + 17), d: u32(at + 21) });
      at += 25;
    }
  }
  return { roles, events };
}

function describeGroupEvent(e) {
  switch (e.code) {
    case 0: return `agent ${e.agent} became a zombie of ${e.a}:${e.b} at node ${e.node}`;
    case 1: return `agent ${e.agent} settled at node ${e.node} in ${e.a}:${e.b}`;
    case 2: return `merge: settler ${e.agent} at node ${e.node} joined ${e.c}:${e.d} from ${e.a}:${e.b}`;
    case 3: return `leader ${e.agent} rose to level ${e.a} with zombie ${e.b} at node ${e.node}`;
    case 4: return `leader ${e.agent} moved forward ${e.node} → ${e.a}`;
    case 5: return `leader ${e.agent} moved back ${e.node} → ${e.a}`;
    case 6: return `zombie ${e.agent}: location level ${e.a}, swarm level ${e.b}`;
    default: return `${e.b ? 'weak' : 'strong'} zombie ${e.agent} chased ${e.node} → ${e.a}`;
  }
}

function roleSnapshots(frames, roles, agents) {
  const role = new Uint8Array(agents).fill(2);
  const level = new Uint32Array(agents).fill(1);
  const leader = Uint32Array.from({ length: agents }, (_, a) => a);
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

function newSimulation(algorithm, graph, edges, initial, mode, limit) {
  return new wasm.WasmSimulation(
    graph.nodes, Uint32Array.from(edges.flat()), Uint32Array.from(initial),
    wasm.AlgorithmSelector[ALGORITHMS[algorithm].selector], mode, BigInt(limit), 0, 1,
  );
}

const TERMINATIONS = ['completed', 'round limit reached', 'cancelled', 'invalid input'];

function simulate(algorithm, graph, initial, limit) {
  const spec = ALGORITHMS[algorithm];
  const edges = graphEdges(graph);
  const simulation = newSimulation(algorithm, graph, edges, initial, wasm.TraceMode.Full, limit);
  try {
    const output = simulation.run();
    const summary = {
      positions: output.positions(),
      statuses: output.statuses(),
      homes: output.homes(),
      rounds: Number(output.rounds()),
      totalMoves: Number(output.moves()),
      probes: Number(output.probes()),
      termination: TERMINATIONS[output.termination_code()],
    };
    const built = buildFrames(output.render_bytes(), initial, summary, spec.statuses);
    let groups = null;
    if (algorithm === 'general') {
      const decoded = decodeGroups(output.group_bytes());
      groups = { events: decoded.events, shown: roleSnapshots(built.frames, decoded.roles, initial.length) };
      built.events = decoded.events.map((e) => ({ step: e.frame, important: e.code <= 3, text: describeGroupEvent(e) }));
    }
    output.free();
    // Frames to play when still steps are collapsed: every frame with a move,
    // and the last frame of each run of frames without one.
    const frames = built.frames;
    const moved = frames.map((f, i) => i > 0 && f.nodes.some((v, a) => v !== frames[i - 1].nodes[a]));
    const compact = [0];
    const spans = new Map();
    for (let i = 1; i < frames.length; i++) {
      if (moved[i]) {
        compact.push(i);
        continue;
      }
      let j = i;
      while (j + 1 < frames.length && !moved[j + 1]) j++;
      compact.push(j);
      if (j > i) spans.set(j, [frames[i].step, frames[j].step]);
      i = j;
    }
    return { algorithm, graph, edges, starts: initial.slice(), limit, ...summary, ...built, groups, compact, spans };
  } finally {
    simulation.free();
  }
}

function scatteredStarts(list) {
  return list.some((v) => v !== list[0]);
}

function start() {
  stop();
  const algorithm = $('algorithm').value;
  const graph = readGraph();
  const error = graphError(graph) || (starts.length === 0 ? 'Place at least one agent.' : null);
  if (error) {
    say(error);
    return;
  }
  if (scatteredStarts(starts) && ALGORITHMS[algorithm].rooted) {
    say(`${ALGORITHMS[algorithm].label} needs every agent on one node. Use “All on node 0”, or GeneralDisp or Drop and Freeze for scattered starts.`);
    return;
  }
  const typed = Number($('roundLimit').value);
  const limit = $('roundLimit').value && typed > 0 ? typed : defaultLimit(algorithm, starts.length);
  const began = performance.now();
  try {
    run = simulate(algorithm, graph, starts, limit);
    say(`Ran in ${Math.round(performance.now() - began)} ms · ${run.frames.length} steps to play`, true);
  } catch (failure) {
    run = null;
    say(String(failure));
  }
  index = 0;
  picked = -1;
  anim = null;
  syncSlider();
  renderMarkers();
  render();
}

const SLOT_NAMES = ['election', 'settle', 'helpers go', 'check', 'finish', 'probe out', 'probe back', 'tally', 'chase', 'chase', 'move', 'parent'];

function playList() {
  return run && $('collapse').checked ? run.compact : null;
}

function spanAt(i) {
  return playList() ? run.spans.get(i) : undefined;
}

function stepAfter(i) {
  const list = playList();
  if (!list) return i + 1;
  return list.find((f) => f > i) ?? run.frames.length - 1;
}

function stepBefore(i) {
  const list = playList();
  if (!list) return i - 1;
  for (let j = list.length - 1; j >= 0; j--) if (list[j] < i) return list[j];
  return 0;
}

function sliderPosition(i) {
  const list = playList();
  if (!list) return i;
  let lo = 0;
  let hi = list.length - 1;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (list[mid] <= i) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

function syncSlider() {
  const list = playList();
  $('slider').max = run ? (list ? list.length - 1 : run.frames.length - 1) : 0;
  $('slider').value = run ? sliderPosition(index) : 0;
}

function stop() {
  clearInterval(timer);
  timer = null;
  $('play').textContent = '▶';
}

function togglePlay() {
  if (!run) return;
  if (timer) {
    stop();
    return;
  }
  if (index >= run.frames.length - 1) index = 0;
  $('play').textContent = '⏸';
  timer = setInterval(() => {
    if (index >= run.frames.length - 1) {
      stop();
      return;
    }
    show(stepAfter(index));
  }, Number($('speed').value));
}

function show(target) {
  if (!run) return;
  const next = Math.max(0, Math.min(run.frames.length - 1, target));
  const animate = $('animate').checked && next === index + 1;
  anim = animate ? { from: index, began: performance.now(), duration: Math.min(260, Number($('speed').value) * 0.8) } : null;
  index = next;
  render();
  if (anim) requestAnimationFrame(tick);
}

function tick(now) {
  if (!anim) return;
  const progress = (now - anim.began) / anim.duration;
  if (progress >= 1) {
    anim = null;
    drawCanvas();
    return;
  }
  drawCanvas(Math.max(0, progress));
  requestAnimationFrame(tick);
}

function jumpToNextEvent() {
  if (!run) return;
  const now = run.frames[index].step;
  const next = run.events.find((e) => e.important && e.step > now);
  if (next) jumpToStep(next.step);
}

function jumpToStep(step) {
  if (!run) return;
  const target = run.frames.findIndex((f) => f.step >= step);
  stop();
  anim = null;
  index = target < 0 ? run.frames.length - 1 : target;
  render();
}

const hiddenRoles = new Set();

function roleOf(agent) {
  if (run?.groups) {
    const shown = run.groups.shown[index];
    const role = shown.role[agent];
    return { name: GENERAL_ROLES[role], color: GENERAL_COLORS[role], order: role, settled: role === 0, group: `${shown.level[agent]}:${shown.leader[agent]}`, leader: shown.leader[agent] };
  }
  if (!run) return { name: 'agent', color: '--r2', order: 1, settled: false, group: null, leader: null };
  const status = run.frames[index].status[agent];
  return { name: ALGORITHMS[run.algorithm].statuses[status] ?? '?', color: STATUS_COLORS[status] ?? '--r2', order: status, settled: status === 0, group: null, leader: null };
}

function render() {
  drawCanvas(anim ? 0 : 1);
  renderHead();
  renderStep();
  renderGroups();
  renderEvents();
  renderAgents();
  $('slider').value = run ? sliderPosition(index) : 0;
  const span = spanAt(index);
  $('stepLabel').textContent = run ? `${span ? `Steps ${span[0]}–${span[1]}` : `Step ${run.frames[index].step}`} / ${run.frames[run.frames.length - 1].step}` : 'Step 0 / 0';
  for (const id of ['first', 'prev', 'play', 'next', 'jump', 'slider', 'share']) $(id).disabled = !run;
}

function renderHead() {
  const algorithm = run ? run.algorithm : $('algorithm').value;
  const graph = run ? run.graph : readGraph();
  $('title').textContent = run
    ? `${ALGORITHMS[algorithm].label} · ${graphLabel(graph)} ${graph.nodes} · ${run.starts.length} agents · ${run.termination}`
    : `${ALGORITHMS[algorithm].label} · ${graphLabel(graph)} ${graph.nodes} · ${starts.length} agents placed`;
  const legend = $('legend');
  legend.replaceChildren();
  const general = algorithm === 'general' && run;
  const names = general ? GENERAL_ROLES : ALGORITHMS[algorithm].statuses;
  const colors = general ? GENERAL_COLORS : STATUS_COLORS;
  const counts = new Map();
  if (run) {
    for (let a = 0; a < run.starts.length; a++) {
      const name = roleOf(a).name;
      counts.set(name, (counts.get(name) || 0) + 1);
    }
  }
  names.forEach((name, i) => {
    const item = document.createElement('button');
    item.type = 'button';
    item.className = `legendItem${hiddenRoles.has(name) ? ' off' : ''}`;
    item.dataset.role = name;
    item.title = hiddenRoles.has(name) ? `Show ${name}` : `Hide ${name}`;
    const dot = document.createElement('i');
    dot.style.background = `var(${colors[i]})`;
    item.append(dot, `${name}${run ? ` (${counts.get(name) || 0})` : ''}`);
    legend.append(item);
  });
  if (general) legend.append(Object.assign(document.createElement('span'), { textContent: $('territories').checked ? 'ring = group · tint = territory' : 'ring = group' }));
  $('territoriesWrap').hidden = !run?.groups;
  if (run && $('showTrails').checked) legend.append(Object.assign(document.createElement('span'), { textContent: 'arrows = moves this step' }));
  $('empty').hidden = Boolean(run) || starts.length > 0;
}

function renderMarkers() {
  const strip = $('markers');
  strip.replaceChildren();
  if (!run || run.frames.length < 2) return;
  const last = run.frames.length - 1;
  const span = playList() ? playList().length - 1 : last;
  const seen = new Set();
  let fi = 0;
  for (const e of run.events) {
    if (!e.important) continue;
    while (fi < last && run.frames[fi].step < e.step) fi++;
    const percent = span > 0 ? Math.round((sliderPosition(fi) / span) * 400) / 4 : 0;
    if (seen.has(percent)) continue;
    seen.add(percent);
    const tick = document.createElement('i');
    tick.style.left = `${percent}%`;
    strip.append(tick);
  }
}

function renderStep() {
  const card = $('stepCard');
  card.replaceChildren();
  const line = (text, className) => card.append(Object.assign(document.createElement('div'), { className, textContent: text }));
  if (!run) {
    line('No run yet', 'big');
    return;
  }
  const frame = run.frames[index];
  const total = run.frames[run.frames.length - 1].step;
  const span = spanAt(index);
  if (span) {
    line(`Steps ${span[0]}–${span[1]} of ${total}`, 'big');
    if (run.algorithm === 'general') {
      const slots = [];
      for (let s = span[0]; s <= span[1]; s++) {
        const name = SLOT_NAMES[(s - 1) % 12];
        if (!slots.includes(name)) slots.push(name);
      }
      line(`no movement · slots ${((span[0] - 1) % 12) + 1}–${((span[1] - 1) % 12) + 1} (${slots.join(', ')})`, 'sub');
    } else {
      line(`no movement for ${span[1] - span[0] + 1} steps`, 'sub');
    }
  } else {
    line(`Step ${frame.step} of ${total}`, 'big');
  }
  if (!span && run.algorithm === 'general') {
    line(frame.step === 0 ? 'start' : `slot ${((frame.step - 1) % 12) + 1}/12 · unit ${Math.floor((frame.step - 1) / 12) + 1}`, 'sub');
  } else if (!span) {
    const phases = run.phases.get(frame.step);
    line(phases ? phases.map((p) => PHASES[p] ?? 'phase').join(', ') : (frame.step === 0 ? 'start' : '—'), 'sub');
  }
  const settled = frame.status.filter((s) => s === 0).length;
  line(`${settled}/${frame.status.length} settled · ${run.stepMoves.get(frame.step) || 0} moves this step`, 'sub');
  if (index === run.frames.length - 1) line(`${run.termination} after ${run.rounds} rounds · ${run.totalMoves} moves · ${run.probes} probes`, 'sub');
  const code = document.createElement('code');
  code.className = 'sub';
  code.style.cssText = 'font-size:11px;word-break:break-all;margin-top:4px';
  code.textContent = `ccm ${run.algorithm === 'general' ? 'transitions' : 'run'} --algorithm ${ALGORITHMS[run.algorithm].cli} --graph ${graphLabel(run.graph)} --nodes ${run.graph.nodes} --agents ${run.starts.length} --ports canonical --placement explicit:${run.starts.join(',')}${RANDOM.has(run.graph.family) ? ` --seed ${run.graph.seed}` : ''}`;
  card.append(code);
}

function renderGroups() {
  const card = $('groupsCard');
  card.hidden = !run?.groups;
  if (card.hidden) return;
  const shown = run.groups.shown[index];
  const nodes = run.frames[index].nodes;
  const groups = new Map();
  const group = (leader) => {
    if (!groups.has(leader)) groups.set(leader, { leader, level: 0, node: -1, waiting: false, settlers: 0, zombies: 0 });
    return groups.get(leader);
  };
  for (let a = 0; a < shown.role.length; a++) {
    const role = shown.role[a];
    const g = group(shown.leader[a]);
    if (role === 2 || role === 3) {
      g.level = shown.level[a];
      g.node = nodes[a];
      g.waiting = role === 3;
    } else if (role <= 1) {
      g.settlers += 1;
    } else {
      g.zombies += 1;
    }
  }
  const body = $('groupRows');
  body.replaceChildren();
  const alive = [...groups.values()].filter((g) => g.node >= 0).sort((p, q) => q.level - p.level || q.leader - p.leader);
  for (const g of alive) {
    const row = document.createElement('tr');
    const label = document.createElement('td');
    const swatch = Object.assign(document.createElement('span'), { className: 'swatch' });
    swatch.style.background = groupColor(g.leader);
    label.append(swatch, `${g.level}:${g.leader}`);
    row.append(label);
    for (const text of [`node ${g.node}${g.waiting ? ' (waiting)' : ''}`, g.settlers, g.zombies]) {
      row.append(Object.assign(document.createElement('td'), { textContent: String(text) }));
    }
    body.append(row);
  }
}

function renderEvents() {
  const list = $('events');
  list.replaceChildren();
  if (!run) return;
  const now = run.frames[index].step;
  const [from] = spanAt(index) ?? [now];
  const importantOnly = $('importantOnly').checked;
  const visible = run.events.filter((e) => e.step <= now && (!importantOnly || e.important));
  for (const e of visible.slice(-250).reverse()) {
    const item = document.createElement('li');
    item.dataset.step = e.step;
    if (e.step >= from && e.step <= now) item.className = 'now';
    item.append(Object.assign(document.createElement('span'), { className: 'when', textContent: `step ${e.step}` }), e.text);
    list.append(item);
  }
  if (!visible.length) list.append(Object.assign(document.createElement('li'), { textContent: 'No events yet' }));
}

function renderAgents() {
  const body = $('agentRows');
  body.replaceChildren();
  $('extraHead').textContent = run?.groups ? 'Group' : 'Home';
  const nodes = run ? run.frames[index].nodes : starts;
  for (let a = 0; a < nodes.length; a++) {
    const role = roleOf(a);
    const row = document.createElement('tr');
    if (a === picked) row.className = 'picked';
    row.dataset.agent = a;
    const swatch = Object.assign(document.createElement('span'), { className: 'swatch' });
    swatch.style.background = `var(${role.color})`;
    const roleCell = document.createElement('td');
    roleCell.append(swatch, role.name);
    const home = run ? run.frames[index].homes[a] : -1;
    row.append(Object.assign(document.createElement('td'), { textContent: String(a) }));
    row.append(Object.assign(document.createElement('td'), { textContent: String(nodes[a]) }));
    row.append(roleCell);
    row.append(Object.assign(document.createElement('td'), { textContent: String(role.group ?? (home < 0 ? '—' : home)) }));
    body.append(row);
  }
  if (picked >= 0) body.querySelector('tr.picked')?.scrollIntoView({ block: 'nearest' });
}

function agentsAt(node) {
  const nodes = run ? run.frames[index].nodes : starts;
  const out = [];
  for (let a = 0; a < nodes.length; a++) if (nodes[a] === node) out.push(a);
  return out;
}

// Node and agent sizes from the shortest edge on screen, so the rings around
// neighbouring nodes do not run into each other.
function sizes(edges) {
  let spacing = Infinity;
  for (const [a, b] of edges) spacing = Math.min(spacing, Math.hypot(points[a].x - points[b].x, points[a].y - points[b].y));
  if (!Number.isFinite(spacing)) spacing = 120;
  const radius = Math.max(5, Math.min(14, spacing * 0.2));
  const dot = Math.max(3.5, Math.min(8, (spacing / 2 - radius) / 2.4));
  return { radius, dot, room: Math.max(spacing / 2 - 1, radius + 2 * dot + 4) };
}

function ringSlots(count, { radius, dot, room }, force) {
  const out = [];
  let r = radius + dot + 3;
  let left = count;
  let ring = 0;
  while (left > 0) {
    if (!force && r + dot > room) return null;
    const capacity = Math.max(1, Math.floor((2 * Math.PI * r) / (2 * dot + 2)));
    const here = force ? left : Math.min(left, capacity);
    for (let i = 0; i < here; i++) {
      const angle = (2 * Math.PI * i) / here - Math.PI / 2 + ring * 0.35;
      out.push({ x: r * Math.cos(angle), y: r * Math.sin(angle) });
    }
    left -= here;
    r += 2 * dot + 2;
    ring += 1;
  }
  return out;
}

function placeAgents(nodes, metrics) {
  const byNode = new Map();
  nodes.forEach((v, a) => {
    if (!byNode.has(v)) byNode.set(v, []);
    byNode.get(v).push(a);
  });
  const pos = new Array(nodes.length);
  const chips = [];
  const inside = new Map();
  for (const [v, list] of byNode) {
    const p = points[v];
    if (!p) continue;
    const around = [];
    for (const a of list) {
      const role = roleOf(a);
      if (hiddenRoles.has(role.name)) continue;
      if (role.settled && !inside.has(v)) {
        inside.set(v, a);
        pos[a] = { x: p.x, y: p.y };
      } else {
        around.push(a);
      }
    }
    around.sort((a, b) => roleOf(a).order - roleOf(b).order || a - b);
    const slots = ringSlots(around.length, metrics, false);
    if (slots) {
      around.forEach((a, i) => { pos[a] = { x: p.x + slots[i].x, y: p.y + slots[i].y }; });
      continue;
    }
    const byRole = new Map();
    for (const a of around) {
      const name = roleOf(a).name;
      if (!byRole.has(name)) byRole.set(name, []);
      byRole.get(name).push(a);
    }
    const groups = [...byRole.values()];
    const spots = ringSlots(groups.length, metrics, false) ?? ringSlots(groups.length, metrics, true);
    groups.forEach((agents, i) => {
      const at = { x: p.x + spots[i].x, y: p.y + spots[i].y };
      agents.forEach((a) => { pos[a] = at; });
      chips.push({ ...at, agents });
    });
  }
  return { pos, chips, inside, byNode };
}

function drawCanvas(progress = 1) {
  const canvas = $('canvas');
  const ratio = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  if (canvas.width !== Math.round(width * ratio)) canvas.width = Math.round(width * ratio);
  if (canvas.height !== Math.round(height * ratio)) canvas.height = Math.round(height * ratio);
  const ctx = canvas.getContext('2d');
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const graph = run ? run.graph : readGraph();
  if (graphError(graph)) {
    points = [];
    placed = null;
    return;
  }
  const area = freeArea(width, height);
  const key = `${graph.family}|${graph.nodes}|${graph.param}|${graph.seed}|${area.x}|${area.y}|${area.w}|${area.h}`;
  if (key !== layoutKey) {
    points = layout(graph, area.w, area.h).map((p) => ({ x: p.x + area.x, y: p.y + area.y }));
    layoutKey = key;
  }
  ctx.setTransform(ratio * view.scale, 0, 0, ratio * view.scale, ratio * view.x, ratio * view.y);
  const edges = run ? run.edges : graphEdges(graph);
  const metrics = sizes(edges);
  const { radius, dot } = metrics;
  const territory = territoryMap();
  if (territory) {
    for (const [node, leader] of territory) {
      ctx.beginPath();
      ctx.arc(points[node].x, points[node].y, radius * 2.2, 0, 2 * Math.PI);
      ctx.fillStyle = groupColor(leader, 0.14);
      ctx.fill();
    }
  }
  ctx.strokeStyle = css('--edge');
  ctx.lineWidth = 1.2;
  ctx.beginPath();
  for (const [a, b] of edges) {
    ctx.moveTo(points[a].x, points[a].y);
    ctx.lineTo(points[b].x, points[b].y);
  }
  ctx.stroke();
  if (territory) {
    ctx.lineWidth = 4;
    ctx.lineCap = 'round';
    for (const [a, b] of edges) {
      const leader = territory.get(a);
      if (leader === undefined || territory.get(b) !== leader) continue;
      ctx.beginPath();
      ctx.moveTo(points[a].x, points[a].y);
      ctx.lineTo(points[b].x, points[b].y);
      ctx.strokeStyle = groupColor(leader, 0.55);
      ctx.stroke();
    }
    ctx.lineCap = 'butt';
  }
  if (run && index > 0 && $('showTrails').checked) drawMoves(ctx, run.frames[index - 1].nodes, run.frames[index].nodes, radius);
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  const current = run ? run.frames[index].nodes : starts;
  const target = placeAgents(current, metrics);
  for (let v = 0; v < points.length; v++) {
    const { x, y } = points[v];
    const settler = target.inside.get(v);
    const owner = territory?.get(v);
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, 2 * Math.PI);
    ctx.fillStyle = css(target.byNode.has(v) ? '--node-busy' : '--node');
    ctx.fill();
    if (owner !== undefined) {
      ctx.fillStyle = groupColor(owner, 0.35);
      ctx.fill();
      ctx.strokeStyle = groupColor(owner);
      ctx.lineWidth = 3;
    } else if (settler !== undefined) {
      const color = css(roleOf(settler).color);
      ctx.globalAlpha = 0.3;
      ctx.fillStyle = color;
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.strokeStyle = color;
      ctx.lineWidth = 3;
    } else {
      ctx.strokeStyle = css('--edge');
      ctx.lineWidth = 1;
    }
    ctx.stroke();
    if (owner === undefined && settler !== undefined && roleOf(settler).leader !== null) {
      ctx.beginPath();
      ctx.arc(x, y, radius + 2.5, 0, 2 * Math.PI);
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = groupColor(roleOf(settler).leader);
      ctx.stroke();
    }
    if (settler === picked) drawPick(ctx, x, y, radius + 4);
    if (graph.nodes <= 80 || view.scale > 1.5) {
      ctx.fillStyle = css('--text');
      ctx.font = `${Math.max(9, radius - 3)}px system-ui, sans-serif`;
      ctx.fillText(String(v), x, y);
    }
  }
  if ($('showPorts').checked && graph.nodes <= 80) drawPorts(ctx, graph, edges, radius);
  const insideAgents = new Set(target.inside.values());
  if (progress < 1 && anim && run) {
    const from = placeAgents(run.frames[anim.from].nodes, metrics);
    const t = progress < 0.5 ? 2 * progress * progress : 1 - ((-2 * progress + 2) ** 2) / 2;
    for (let a = 0; a < current.length; a++) {
      const p = from.pos[a];
      const q = target.pos[a];
      if (!p || !q || (insideAgents.has(a) && from.inside.get(current[a]) === a)) continue;
      drawAgent(ctx, a, { x: p.x + (q.x - p.x) * t, y: p.y + (q.y - p.y) * t }, dot, false);
    }
  } else {
    const chipped = new Set(target.chips.flatMap((c) => c.agents));
    const labels = current.length <= 60 && view.scale * dot >= 5;
    for (let a = 0; a < current.length; a++) {
      if (!chipped.has(a) && !insideAgents.has(a) && target.pos[a]) drawAgent(ctx, a, target.pos[a], dot, labels);
    }
    for (const chip of target.chips) drawChip(ctx, chip, dot);
  }
  placed = { ...target, dot, radius };
}

function drawDot(ctx, x, y, size, color) {
  ctx.beginPath();
  ctx.arc(x, y, size, 0, 2 * Math.PI);
  ctx.fillStyle = color;
  ctx.fill();
}

function drawAgent(ctx, agent, p, dot, label) {
  const role = roleOf(agent);
  drawDot(ctx, p.x, p.y, dot, css(role.color));
  if (role.leader !== null) {
    ctx.lineWidth = 2;
    ctx.strokeStyle = groupColor(role.leader);
    ctx.stroke();
  }
  if (label) {
    ctx.fillStyle = '#fff';
    ctx.font = `bold ${Math.max(7, dot - 1)}px system-ui, sans-serif`;
    ctx.fillText(String(agent), p.x, p.y + 0.5);
  }
  if (agent === picked) drawPick(ctx, p.x, p.y, dot + 3);
}

function drawChip(ctx, chip, dot) {
  const size = dot + 3;
  drawDot(ctx, chip.x, chip.y, size, css(roleOf(chip.agents[0]).color));
  ctx.fillStyle = '#fff';
  ctx.font = `bold ${Math.max(8, size - 1)}px system-ui, sans-serif`;
  ctx.fillText(String(chip.agents.length), chip.x, chip.y + 0.5);
  if (chip.agents.includes(picked)) drawPick(ctx, chip.x, chip.y, size + 3);
}

function drawPick(ctx, x, y, r) {
  ctx.beginPath();
  ctx.arc(x, y, r, 0, 2 * Math.PI);
  ctx.lineWidth = 2.5;
  ctx.strokeStyle = css('--accent');
  ctx.stroke();
}

function drawMoves(ctx, before, after, radius) {
  const flows = new Map();
  for (let a = 0; a < after.length; a++) {
    if (before[a] === after[a]) continue;
    const key = `${before[a]}>${after[a]}`;
    flows.set(key, (flows.get(key) || 0) + 1);
  }
  ctx.save();
  ctx.strokeStyle = css('--accent');
  ctx.fillStyle = css('--accent');
  ctx.globalAlpha = 0.8;
  ctx.lineWidth = 2.5;
  for (const [key, count] of flows) {
    const [u, v] = key.split('>').map(Number);
    const p = points[u];
    const q = points[v];
    if (!p || !q) continue;
    const length = Math.hypot(q.x - p.x, q.y - p.y) || 1;
    const ux = (q.x - p.x) / length;
    const uy = (q.y - p.y) / length;
    const sx = p.x + ux * radius;
    const sy = p.y + uy * radius;
    const ex = q.x - ux * (radius + 3);
    const ey = q.y - uy * (radius + 3);
    ctx.beginPath();
    ctx.moveTo(sx, sy);
    ctx.lineTo(ex, ey);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(ex, ey);
    ctx.lineTo(ex - ux * 8 - uy * 4.5, ey - uy * 8 + ux * 4.5);
    ctx.lineTo(ex - ux * 8 + uy * 4.5, ey - uy * 8 - ux * 4.5);
    ctx.closePath();
    ctx.fill();
    if (count > 1) {
      ctx.globalAlpha = 1;
      ctx.font = 'bold 10px system-ui, sans-serif';
      ctx.fillText(`×${count}`, (sx + ex) / 2 - uy * 9, (sy + ey) / 2 + ux * 9);
      ctx.globalAlpha = 0.8;
    }
  }
  ctx.restore();
}

function drawPorts(ctx, graph, edges, radius) {
  const adjacency = neighbors(graph, edges);
  if (Math.max(...adjacency.map((list) => list.length)) > 12) return;
  ctx.save();
  ctx.font = '9px ui-monospace, Menlo, monospace';
  ctx.fillStyle = css('--muted');
  adjacency.forEach((list, v) => {
    const p = points[v];
    list.forEach((u, port) => {
      const q = points[u];
      const length = Math.hypot(q.x - p.x, q.y - p.y) || 1;
      ctx.fillText(String(port), p.x + ((q.x - p.x) / length) * (radius + 8), p.y + ((q.y - p.y) / length) * (radius + 8));
    });
  });
  ctx.restore();
}

function toWorld(event) {
  const rect = $('canvas').getBoundingClientRect();
  const mx = event.clientX - rect.left;
  const my = event.clientY - rect.top;
  return { mx, my, x: (mx - view.x) / view.scale, y: (my - view.y) / view.scale };
}

function nodeAt(world) {
  let best = -1;
  let bestDistance = 24 / view.scale;
  points.forEach((p, v) => {
    const d = Math.hypot(p.x - world.x, p.y - world.y);
    if (d < bestDistance) {
      best = v;
      bestDistance = d;
    }
  });
  return best;
}

function agentAt(world) {
  if (!placed) return -1;
  for (const chip of placed.chips) {
    if (Math.hypot(chip.x - world.x, chip.y - world.y) <= placed.dot * 1.6 + 3) return chip.agents[0];
  }
  const inside = new Set(placed.inside.values());
  for (let a = 0; a < placed.pos.length; a++) {
    const p = placed.pos[a];
    if (p && !inside.has(a) && Math.hypot(p.x - world.x, p.y - world.y) <= placed.dot * 1.3 + 2) return a;
  }
  for (const [node, a] of placed.inside) {
    const p = points[node];
    if (Math.hypot(p.x - world.x, p.y - world.y) <= placed.radius) return a;
  }
  return -1;
}

function zoomAt(mx, my, factor) {
  const scale = Math.max(0.3, Math.min(6, view.scale * factor));
  view.x = mx - ((mx - view.x) * scale) / view.scale;
  view.y = my - ((my - view.y) * scale) / view.scale;
  view.scale = scale;
  drawCanvas();
}

function fit() {
  view.scale = 1;
  view.x = 0;
  view.y = 0;
  drawCanvas();
}

function onCanvasClick(event) {
  const world = toWorld(event);
  if ($('placement').value === 'rooted' && !run) {
    const node = nodeAt(world);
    if (node >= 0) {
      $('root').value = node;
      updatePlacement();
    }
    return;
  }
  if ($('placement').value === 'click') {
    const node = nodeAt(world);
    if (node < 0) return;
    if (event.shiftKey) {
      const at = starts.lastIndexOf(node);
      if (at >= 0) starts.splice(at, 1);
    } else if (starts.length < Math.min(readGraph().nodes, MAX_AGENTS)) {
      starts.push(node);
    }
    updatePlacement();
    return;
  }
  const agent = agentAt(world);
  picked = agent === picked ? -1 : agent;
  render();
}

function onCanvasHover(event) {
  const tooltip = $('tooltip');
  const world = toWorld(event);
  const node = nodeAt(world);
  if (node < 0) {
    tooltip.hidden = true;
    return;
  }
  const graph = run ? run.graph : readGraph();
  const adjacency = neighbors(graph, run ? run.edges : graphEdges(graph));
  const lines = [`node ${node} · ports ${adjacency[node].map((u, port) => `${port}→${u}`).join(' ')}`];
  for (const agent of agentsAt(node)) {
    const role = roleOf(agent);
    const home = run ? run.frames[index].homes[agent] : -1;
    lines.push(`agent ${agent} · ${role.name}${role.group ? ` · group ${role.group}` : ''}${home >= 0 ? ` · home ${home}` : ''}`);
  }
  tooltip.textContent = lines.join('\n');
  tooltip.style.left = `${Math.max(4, Math.min(event.clientX + 14, window.innerWidth - 280))}px`;
  tooltip.style.top = `${Math.min(event.clientY + 14, window.innerHeight - 60)}px`;
  tooltip.hidden = false;
}

const WINDOWS = ['winSetup', 'winInspector'];
const narrow = () => matchMedia('(max-width: 760px)').matches;

// The graph fills the stage; the stage itself sits between the windows docked
// on the left and on the right, like the dashboard's results window.
function freeArea(width, height) {
  return { x: 0, y: 0, w: width, h: height };
}

function fitStage() {
  if (narrow()) return;
  let left = 16;
  let right = 16;
  for (const id of WINDOWS) {
    const win = $(id);
    if (win.classList.contains('collapsed')) continue;
    const box = win.getBoundingClientRect();
    if (box.left < window.innerWidth * 0.4) left = Math.max(left, box.right + 16);
    else if (box.right > window.innerWidth * 0.6) right = Math.max(right, window.innerWidth - box.left + 16);
  }
  if (window.innerWidth - left - right < 320) {
    left = 16;
    right = 16;
  }
  $('stage').style.setProperty('--content-left', `${Math.round(left)}px`);
  $('stage').style.setProperty('--content-right', `${Math.round(right)}px`);
}

function relayout() {
  fitStage();
  layoutKey = '';
  drawCanvas();
}

function placeWindow(win, left, top) {
  const x = Math.max(80 - win.offsetWidth, Math.min(left, window.innerWidth - 80));
  const y = Math.max(0, Math.min(top, window.innerHeight - 36));
  win.style.left = `${x}px`;
  win.style.top = `${y}px`;
  win.style.right = 'auto';
  win.style.bottom = 'auto';
  win.style.transform = 'none';
}

function setCollapsed(win, collapsed) {
  win.classList.toggle('collapsed', collapsed);
  win.querySelector('.collapse').textContent = collapsed ? '+' : '–';
}

function saveWindows() {
  const state = {};
  for (const id of WINDOWS) {
    const win = $(id);
    state[id] = { left: win.style.left, top: win.style.top, collapsed: win.classList.contains('collapsed') };
  }
  try {
    localStorage.setItem('ccm-windows-v2', JSON.stringify(state));
  } catch {
    // Storage may be unavailable; the layout then lasts for this visit.
  }
}

function restoreWindows() {
  let state = {};
  try {
    state = JSON.parse(localStorage.getItem('ccm-windows-v2') || '{}');
  } catch {
    return;
  }
  for (const id of WINDOWS) {
    const saved = state[id];
    if (!saved) continue;
    if (saved.left && saved.top) placeWindow($(id), parseFloat(saved.left), parseFloat(saved.top));
    setCollapsed($(id), Boolean(saved.collapsed));
  }
}

function resetWindows() {
  for (const id of WINDOWS) {
    $(id).style.cssText = '';
    setCollapsed($(id), false);
  }
  try {
    localStorage.removeItem('ccm-windows-v2');
  } catch {
    // Nothing saved to remove.
  }
  relayout();
}

function bringToFront(win) {
  for (const id of WINDOWS) $(id).classList.toggle('front', $(id) === win);
}

function wireWindows() {
  for (const id of WINDOWS) {
    const win = $(id);
    win.addEventListener('mousedown', () => bringToFront(win));
    win.querySelector('.winHead').addEventListener('mousedown', (event) => {
      if (event.button !== 0 || narrow() || event.target.closest('button, input, select, label')) return;
      event.preventDefault();
      const box = win.getBoundingClientRect();
      const grab = { x: event.clientX - box.left, y: event.clientY - box.top };
      const move = (e) => {
        placeWindow(win, e.clientX - grab.x, e.clientY - grab.y);
        fitStage();
      };
      const up = () => {
        window.removeEventListener('mousemove', move);
        window.removeEventListener('mouseup', up);
        saveWindows();
        relayout();
      };
      window.addEventListener('mousemove', move);
      window.addEventListener('mouseup', up);
    });
    win.querySelector('.collapse').addEventListener('click', () => {
      setCollapsed(win, !win.classList.contains('collapsed'));
      saveWindows();
      relayout();
    });
  }
  $('resetLayout').addEventListener('click', resetWindows);
  restoreWindows();
  fitStage();
  window.addEventListener('resize', fitStage);
}

function compareAll() {
  const graph = readGraph();
  const error = graphError(graph) || (starts.length === 0 ? 'Place at least one agent.' : null);
  const body = $('compareRows');
  body.replaceChildren();
  if (error) {
    say(error);
    return;
  }
  const edges = graphEdges(graph);
  const scattered = scatteredStarts(starts);
  for (const [key, spec] of Object.entries(ALGORITHMS)) {
    let cells;
    if (scattered && spec.rooted) {
      cells = ['refused: needs a rooted start', '—', '—', '—'];
    } else {
      const simulation = newSimulation(key, graph, edges, starts, wasm.TraceMode.Off, defaultLimit(key, starts.length));
      try {
        const output = simulation.run();
        cells = [TERMINATIONS[output.termination_code()], Number(output.rounds()), Number(output.moves()), Number(output.probes())];
        output.free();
      } catch (failure) {
        cells = [`error: ${failure}`, '—', '—', '—'];
      } finally {
        simulation.free();
      }
    }
    const row = document.createElement('tr');
    row.append(Object.assign(document.createElement('td'), { textContent: spec.label }));
    for (const text of cells) row.append(Object.assign(document.createElement('td'), { textContent: String(text) }));
    const action = document.createElement('td');
    if (cells[1] !== '—') {
      const load = Object.assign(document.createElement('button'), { type: 'button', textContent: 'Load' });
      load.dataset.algorithm = key;
      action.append(load);
    }
    row.append(action);
    body.append(row);
  }
  say(`Compared on ${graphLabel(graph)} ${graph.nodes} with ${starts.length} agents`, true);
}

function selectTab(name) {
  const inspect = name === 'inspect';
  $('tabInspect').setAttribute('aria-selected', String(inspect));
  $('tabCompare').setAttribute('aria-selected', String(!inspect));
  $('inspectPane').hidden = !inspect;
  $('comparePane').hidden = inspect;
}

async function share() {
  if (!run) return;
  const rooted = run.starts.every((v) => v === run.starts[0]);
  const query = new URLSearchParams({ alg: run.algorithm, family: run.graph.family, nodes: run.graph.nodes, run: '1' });
  if (rooted) {
    query.set('placement', 'rooted');
    query.set('agents', run.starts.length);
    query.set('root', run.starts[0]);
  } else {
    query.set('starts', run.starts.join(','));
  }
  if (PARAMS[run.graph.family]) query.set('param', run.graph.param);
  if (RANDOM.has(run.graph.family)) query.set('seed', run.graph.seed);
  const url = `${location.origin}${location.pathname}?${query}`;
  try {
    await navigator.clipboard.writeText(url);
    say('Link copied', true);
  } catch {
    say(url, true);
  }
}

function applyPreset(name) {
  const preset = PRESETS[name];
  if (!preset) return;
  if (preset.algorithm) $('algorithm').value = preset.algorithm;
  $('family').value = preset.family;
  $('nodes').value = preset.nodes;
  if (preset.starts) {
    $('placement').value = 'click';
    starts = preset.starts.slice();
  } else {
    $('placement').value = 'rooted';
    $('agents').value = preset.agents;
    $('root').value = 0;
  }
  updatePlacement();
  fit();
  start();
}

function applyQuery() {
  const query = new URLSearchParams(location.search);
  const fields = { alg: 'algorithm', family: 'family', nodes: 'nodes', param: 'param', placement: 'placement', agents: 'agents', root: 'root', distinct: 'distinct', seed: 'seed' };
  for (const [key, id] of Object.entries(fields)) if (query.has(key)) $(id).value = query.get(key);
  if (query.has('starts')) {
    $('placement').value = 'click';
    starts = query.get('starts').split(',').filter(Boolean).map(Number);
  }
  updatePlacement();
  if (query.has('preset')) applyPreset(query.get('preset'));
  else if (query.has('run')) start();
  if (run && query.get('at') === 'end') jumpToStep(Infinity);
}

function setTheme(theme) {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem('ccm-theme', theme);
  } catch {
    // Storage may be unavailable; the theme still applies to this page.
  }
  render();
}

function wire() {
  try {
    const saved = localStorage.getItem('ccm-theme');
    if (saved) document.documentElement.dataset.theme = saved;
  } catch {
    // Storage may be unavailable.
  }
  $('theme').addEventListener('click', () => {
    setTheme(document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark');
  });
  wireWindows();
  for (const id of ['family', 'nodes', 'param', 'agents', 'root', 'distinct', 'seed']) $(id).addEventListener('input', updatePlacement);
  $('family').addEventListener('change', () => {
    const param = PARAMS[$('family').value];
    if (param) $('param').value = param[1];
    updatePlacement();
  });
  $('placement').addEventListener('change', () => {
    if ($('placement').value === 'click') starts = [];
    updatePlacement();
  });
  $('algorithm').addEventListener('change', updatePlacement);
  document.querySelectorAll('[data-preset]').forEach((b) => b.addEventListener('click', () => applyPreset(b.dataset.preset)));
  $('run').addEventListener('click', start);
  $('play').addEventListener('click', togglePlay);
  $('first').addEventListener('click', () => jumpToStep(0));
  $('prev').addEventListener('click', () => show(stepBefore(index)));
  $('next').addEventListener('click', () => show(stepAfter(index)));
  $('territories').addEventListener('change', render);
  $('collapse').addEventListener('change', () => {
    syncSlider();
    renderMarkers();
    render();
  });
  $('jump').addEventListener('click', jumpToNextEvent);
  $('slider').addEventListener('input', () => {
    stop();
    anim = null;
    const list = playList();
    index = list ? list[Number($('slider').value)] : Number($('slider').value);
    render();
  });
  $('speed').addEventListener('change', () => {
    if (timer) {
      stop();
      togglePlay();
    }
  });
  for (const id of ['importantOnly', 'showTrails', 'showPorts']) $(id).addEventListener('change', render);
  $('fit').addEventListener('click', fit);
  $('legend').addEventListener('click', (event) => {
    const item = event.target.closest('[data-role]');
    if (!item) return;
    const role = item.dataset.role;
    if (hiddenRoles.has(role)) hiddenRoles.delete(role);
    else hiddenRoles.add(role);
    render();
  });
  $('share').addEventListener('click', share);
  $('tabInspect').addEventListener('click', () => selectTab('inspect'));
  $('tabCompare').addEventListener('click', () => selectTab('compare'));
  $('compare').addEventListener('click', compareAll);
  $('compareRows').addEventListener('click', (event) => {
    const button = event.target.closest('button[data-algorithm]');
    if (!button) return;
    $('algorithm').value = button.dataset.algorithm;
    selectTab('inspect');
    start();
  });
  $('events').addEventListener('click', (event) => {
    const item = event.target.closest('li[data-step]');
    if (item) jumpToStep(Number(item.dataset.step));
  });
  $('agentRows').addEventListener('click', (event) => {
    const row = event.target.closest('tr');
    if (!row) return;
    const agent = Number(row.dataset.agent);
    picked = picked === agent ? -1 : agent;
    render();
  });
  const canvas = $('canvas');
  canvas.addEventListener('mousedown', (event) => {
    drag = { mx: event.clientX, my: event.clientY, x: view.x, y: view.y, moved: false };
  });
  window.addEventListener('mousemove', (event) => {
    if (!drag) return;
    const dx = event.clientX - drag.mx;
    const dy = event.clientY - drag.my;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    drag.moved = true;
    canvas.classList.add('dragging');
    view.x = drag.x + dx;
    view.y = drag.y + dy;
    drawCanvas();
  });
  window.addEventListener('mouseup', (event) => {
    if (!drag) return;
    const clicked = !drag.moved && event.target === canvas;
    drag = null;
    canvas.classList.remove('dragging');
    if (clicked) onCanvasClick(event);
  });
  canvas.addEventListener('mousemove', (event) => {
    if (!drag?.moved) onCanvasHover(event);
  });
  canvas.addEventListener('mouseleave', () => { $('tooltip').hidden = true; });
  canvas.addEventListener('wheel', (event) => {
    event.preventDefault();
    const world = toWorld(event);
    const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 400 : 1;
    const rate = event.ctrlKey ? 0.01 : 0.0015;
    zoomAt(world.mx, world.my, Math.exp(-event.deltaY * unit * rate));
  }, { passive: false });
  document.addEventListener('keydown', (event) => {
    if (event.target.closest('input, select, textarea')) return;
    const center = () => ({ x: canvas.clientWidth / 2, y: canvas.clientHeight / 2 });
    const keys = {
      ArrowLeft: () => show(stepBefore(index)),
      ArrowRight: () => show(stepAfter(index)),
      Home: () => jumpToStep(0),
      End: () => jumpToStep(Infinity),
      ' ': togglePlay,
      n: jumpToNextEvent,
      '+': () => zoomAt(center().x, center().y, 1.25),
      '=': () => zoomAt(center().x, center().y, 1.25),
      '-': () => zoomAt(center().x, center().y, 0.8),
      0: fit,
    };
    const action = keys[event.key];
    if (action) {
      event.preventDefault();
      action();
    }
  });
  const resized = new ResizeObserver(() => drawCanvas());
  resized.observe(canvas);
  for (const id of WINDOWS) resized.observe($(id));
}

async function main() {
  wire();
  syncControls();
  try {
    wasm = await loadWasm();
    $('run').disabled = false;
  } catch {
    say('The Wasm package is missing. Run ./scripts/build-wasm.sh');
    $('run').disabled = true;
    return;
  }
  applyQuery();
}

main();
