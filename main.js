// main.js

import { pyodideReady } from './pyodide-setup.js';
import { runSimulation } from './simulation-runner.js';
import { drawCytoscape } from './cytoscape-visualizer.js';

// Simplified input validation
function getVal(input, def, min) {
  let v = parseInt(input.value, 10);
  if (isNaN(v) || v < min) { input.value = def; return def; }
  return v;
}

const btn = document.getElementById('runBtn');
const out = document.getElementById('output');
const cyId = 'cy';

btn.onclick = async () => {
  btn.disabled = true;
  out.textContent = 'Loading…';
  try {
    const py = await pyodideReady;
    out.textContent = 'Running…';

    const n = getVal(document.getElementById('nodeCountInput'), 10, 1);
    const d = Math.min(getVal(document.getElementById('maxDegreeInput'), 4, 1), n - 1);
    const a = getVal(document.getElementById('agentCountInput'), 3, 1);
    const r = getVal(document.getElementById('roundCountInput'), 10, 1);

    out.textContent = `Gen graph: ${n} nodes, maxDeg ${d}, agents ${a}, rounds ${r}`;
    const edges = generateRandomGraphEdges(n, d);

    const data = await runSimulation(py, n, edges, a, r);
    if (!data.positions || !data.statuses) throw new Error('Bad data');

    drawCytoscape(cyId, data);
    out.textContent = `Displayed ${data.positions.length-1} rounds. Green = SETTLED.`;
  } catch (err) {
    console.error(err);
    out.textContent = `Error: ${err.message}`;
  } finally {
    btn.disabled = false;
  }
};

// same generateRandomGraphEdges as before…
function generateRandomGraphEdges(numNodes, maxDegree) {
  if (maxDegree >= numNodes) maxDegree = numNodes - 1;
  const pairs = [];
  for (let i = 0; i < numNodes; i++)
    for (let j = i+1; j < numNodes; j++)
      pairs.push([i,j]);
  for (let i = pairs.length-1; i>0; i--) {
    const j = Math.floor(Math.random()*(i+1));
    [pairs[i], pairs[j]] = [pairs[j], pairs[i]];
  }
  const deg = Array(numNodes).fill(0), edges = [];
  for (let [u,v] of pairs) {
    if (deg[u]<maxDegree && deg[v]<maxDegree) {
      edges.push([u,v]);
      deg[u]++; deg[v]++;
    }
  }
  return edges;
}
