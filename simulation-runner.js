// simulation-runner.js

async function loadPyFile(py, fname) {
  const res = await fetch(fname);
  if (!res.ok) throw new Error(`Failed to load ${fname}`);
  const text = await res.text();
  py.FS.writeFile(fname, text);
}

export async function runSimulation(py, nodes, max_degree, agents, rounds, seed, starting_positions, algorithm = "Help by Scouts") {
  // Load all Python modules and helper script into Pyodide FS
  const pythonFiles = [
    'graph_utils.py',
    'agent_drop_freeze.py',
    'agent_help_scouts.py',      // NEW: parallel greedy algorithm
    'simulation_wrapper.py'  // external script with core logic
  ];
  await Promise.all(pythonFiles.map(f => loadPyFile(py, f)));

  // Load and prepare the wrapper script
  let script = py.FS.readFile('simulation_wrapper.py', { encoding: 'utf8' });

  // Inject runtime parameters at the top
  const header =
    `nodes = ${nodes}\n` +
    `agent_count = ${agents}\n` +
    `rounds = ${rounds}\n` +
    `seed = ${seed}\n` +
    `max_degree = ${max_degree}\n` +
    `starting_positions = ${starting_positions}\n` +
    `algorithm = ${JSON.stringify(algorithm)}\n`;
  script = header + script;

  // Execute the Python script and parse JSON output
  const resultJson = await py.runPythonAsync(script);
  return JSON.parse(resultJson);
}