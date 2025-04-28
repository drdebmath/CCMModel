// simulation-runner.js

async function loadPyFile(py, fname) {
    const res = await fetch(fname);
    if (!res.ok) throw new Error(`Failed to load ${fname}`);
    const text = await res.text();
    py.FS.writeFile(fname, text);
  }
  
  export async function runSimulation(py, nodes, max_degree, agents, rounds, seed) {
    // Load all Python modules and helper script into Pyodide FS
    const pythonFiles = [
      'graph_utils.py',
      'agent.py',
      'simulation_wrapper.py'  // external script with core logic
    ];
    await Promise.all(pythonFiles.map(f => loadPyFile(py, f)));
  
    // Load and prepare the wrapper script
    let script = py.FS.readFile('simulation_wrapper.py', { encoding: 'utf8' });
  
    // Inject runtime parameters at the top
    const header = `nodes = ${nodes}\nagent_count = ${agents}\nrounds = ${rounds}\nseed = ${seed}\nmax_degree = ${max_degree}\n`;
    script = header + script;
  
    // Execute the Python script and parse JSON output
    const resultJson = await py.runPythonAsync(script);
    return JSON.parse(resultJson);
  }  