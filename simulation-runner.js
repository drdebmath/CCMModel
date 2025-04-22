/**
 * Fetches Python files and writes them to Pyodide's virtual filesystem.
 * @param {object} pyodide - The initialized Pyodide instance.
 * @param {string[]} files - Array of Python filenames to fetch.
 */
async function loadPythonFiles(pyodide, files) {
    console.log("Loading Python files into Pyodide FS...");
    await Promise.all(files.map(async (fname) => {
        try {
            console.log(`Fetching ${fname}...`);
            const response = await fetch(fname);
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status} for ${fname}`);
            }
            const data = await response.text();
            pyodide.FS.writeFile(fname, data);
            console.log(`Successfully wrote ${fname} to Pyodide FS.`);
        } catch (error) {
            console.error(`Failed to fetch or write file ${fname}:`, error);
            throw error;
        }
    }));
    console.log("All Python files loaded.");
}

/**
 * Runs the CCM simulation in Pyodide and returns the results.
 * @param {object} pyodide - The initialized Pyodide instance.
 * @param {number} numNodes - Number of nodes for the graph.
 * @param {Array<Array<number>>} graphEdges - Edges for the graph.
 * @param {number} numAgents - Number of agents.
 * @param {number} numRounds - Number of simulation rounds.
 * @returns {Promise<object>} A promise resolving to the simulation results object.
 */
export async function runSimulation(pyodide, numNodes, graphEdges, numAgents, numRounds) {
    const pythonFiles = ["graph_utils.py", "agent.py", "simulation.py"];
    await loadPythonFiles(pyodide, pythonFiles);

    const code = `
import networkx as nx
from graph_utils import create_port_labeled_graph, randomize_ports, assign_weights, get_port_to_neighbor
from agent import Agent, agentStatus
from simulation import run_simulation
import json
import js

nodes = ${numNodes}
edges_json = '${JSON.stringify(graphEdges)}'
edges = json.loads(edges_json)
agent_count = ${numAgents}
rounds = ${numRounds}
start_node = 0

js.console.log(f"Python: Setting up graph with {nodes} nodes and {len(edges)} edges.")
js.console.log(f"Python: Creating {agent_count} agents, all starting at node {start_node}.")
js.console.log(f"Python: Simulating for {rounds} rounds.")

if nodes <= 0:
    G = nx.Graph()
    agents = []
elif start_node >= nodes:
    js.console.error(f"Python: Start node {start_node} is invalid for {nodes} nodes. Setting start node to 0.")
    start_node = 0
    G = create_port_labeled_graph(nodes, edges)
    if G.number_of_nodes() > 0:
        randomize_ports(G)
    agents = [Agent(i, start_node) for i in range(agent_count)]
else:
    G = create_port_labeled_graph(nodes, edges)
    if G.number_of_nodes() > 0:
        randomize_ports(G)
    agents = [Agent(i, start_node) for i in range(agent_count)]

positions = []
statuses = []
if agents:
    positions.append([agent.currentnode for agent in agents])
    statuses.append([agent.status.name for agent in agents])
else:
    positions.append([])
    statuses.append([])

if agents and rounds > 0 and G.number_of_nodes() > 0:
    js.console.log("Python: Starting simulation...")
    run_simulation(G, agents, rounds)

    max_rounds_simulated = max(max(h[0] for h in agent.history) for agent in agents) if agents else 0
    positions = []
    statuses = []
    # Initial state (round 0)
    initial_pos = [a.history[0][1] for a in agents] if agents else []
    initial_status = [a.history[0][3] for a in agents] if agents else []
    positions.append(initial_pos)
    statuses.append(initial_status)

    # Collect data for rounds 1 to max_rounds_simulated
    for r in range(1, max_rounds_simulated + 1):
        round_positions = []
        round_statuses = []
        for agent in agents:
            history_entry = next((h for h in reversed(agent.history) if h[0] <= r), agent.history[0])
            round_positions.append(history_entry[1])
            round_statuses.append(history_entry[3])
        positions.append(round_positions)
        statuses.append(round_statuses)

    # Pad with final state if fewer rounds were simulated
    while len(positions) < rounds + 1:
        positions.append(positions[-1] if positions else [])
        statuses.append(statuses[-1] if statuses else [])

    js.console.log("Python: Simulation finished.")
else:
    js.console.log("Python: Simulation skipped (0 agents, 0 rounds, or 0 nodes).")
    for _ in range(rounds):
        positions.append(positions[0] if positions else [])
        statuses.append(statuses[0] if statuses else [])

js.console.log("Python: Preparing data for JavaScript...")
# Generate node positions using spring layout
pos = nx.spring_layout(G, scale=300)
nodes_data = [
    {
        "data": {"id": str(n)},
        "position": {"x": float(pos[n][0]), "y": float(pos[n][1])}
    }
    for n in G.nodes()
]
# Include port numbers in edges_data
edges_data = []
for u, v in G.edges():
    port_u = G[u][v].get(f"port_{u}")
    port_v = G[v][u].get(f"port_{v}")
    if port_v is None:
        port_v = get_port_to_neighbor(G, v, u)
    label = f"{port_u}/{port_v}" if port_u is not None and port_v is not None else ""
    edges_data.append({
        "data": {
            "id": f"{u}-{v}",
            "source": str(u),
            "target": str(v),
            "label": label
        }
    })

result = {"nodes": nodes_data, "edges": edges_data, "positions": positions, "statuses": statuses}
js.console.log("Python: Final statuses data for JS: " + str(statuses))

json.dumps(result)
    `;

    try {
        console.log("Running Python simulation script...");
        pyodide.setStdout({ batched: (msg) => console.log("PY_STDOUT:", msg) });
        pyodide.setStderr({ batched: (msg) => console.error("PY_STDERR:", msg) });

        const resultJson = await pyodide.runPythonAsync(code);
        console.log("Python script finished execution.");

        pyodide.setStdout();
        pyodide.setStderr();

        if (typeof resultJson !== 'string' || resultJson.trim() === '') {
            console.error("Python script did not return a valid JSON string. Check PY_STDERR logs.");
            throw new Error("Python script failed to return valid data.");
        }
        const data = JSON.parse(resultJson);
        if (!data || typeof data !== 'object' || !data.nodes || !data.edges || !data.positions || !data.statuses) {
            console.error("Parsed JSON data is missing expected keys:", data);
            throw new Error("Parsed simulation data is incomplete.");
        }
        console.log("Simulation data parsed (including statuses).");
        return data;
    } catch (error) {
        pyodide.setStdout();
        pyodide.setStderr();
        console.error("Error during Python execution or JSON parsing:", error);
        throw error;
    }
}