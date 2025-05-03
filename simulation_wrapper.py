# simulation_wrapper.py

import json
import networkx as nx
from collections import defaultdict
from graph_utils import create_port_labeled_graph, randomize_ports
from agent import Agent, run_simulation
import random
import argparse # Import argparse for command-line arguments
import datetime # Import datetime for timestamps
import os     # Import os for path manipulation
import sys    # Import sys for error output

# Default Parameters (can now be potentially overridden via CLI in a future version)
DEFAULT_NODES = 13
DEFAULT_MAX_DEGREE = 4
DEFAULT_AGENT_COUNT = 13
DEFAULT_STARTING_POSITIONS = 2
DEFAULT_SEED = 42
DEFAULT_ROUNDS = 100 # A reasonable default, maybe based on graph size

# --- Command Line Argument Parsing ---
parser = argparse.ArgumentParser(
    description="Run agent simulation and optionally save results to a timestamped JSON file."
)
parser.add_argument(
    "--output-dir", "-o",
    help="Directory to save the simulation results as a timestamped JSON file. If not provided, prints JSON to stdout.",
    metavar="DIRECTORY"
)
# Optional: Add arguments to override default simulation parameters
# parser.add_argument("--nodes", type=int, default=DEFAULT_NODES, help="Number of nodes in the graph")
# parser.add_argument("--agents", type=int, default=DEFAULT_AGENT_COUNT, help="Number of agents")
# ... etc for other parameters

args = parser.parse_args()

# --- Use browser‑injected values if they exist; otherwise fall back to defaults ---
def _get_or_default(name, default):
    return globals().get(name, default)

nodes               = _get_or_default("nodes",               DEFAULT_NODES)
max_degree          = _get_or_default("max_degree",          DEFAULT_MAX_DEGREE)
agent_count         = _get_or_default("agent_count",         DEFAULT_AGENT_COUNT)
starting_positions  = _get_or_default("starting_positions",  DEFAULT_STARTING_POSITIONS)
seed                = _get_or_default("seed",                DEFAULT_SEED)
rounds              = _get_or_default("rounds",              DEFAULT_ROUNDS)


# --- Graph and Agent Initialization ---
G = create_port_labeled_graph(nodes, max_degree, seed)
if __name__ == "__main__": # Only print graph info when run directly
    print(f'Graph created with {G.number_of_nodes()} nodes and {G.number_of_edges()} edges', file=sys.stderr)

randomize_ports(G, seed)

for node in G.nodes():
    G.nodes[node]['agents'] = set()
    G.nodes[node]['settled_agent'] = None

number_of_starting_positions = min(starting_positions, G.number_of_nodes()) if G.number_of_nodes() > 0 else 0
# Ensure we have at least one node to start on if nodes > 0
start_nodes = random.sample(list(G.nodes()), number_of_starting_positions) if number_of_starting_positions > 0 else (list(G.nodes())[0:1] if G.number_of_nodes() > 0 else [])

if G.number_of_nodes() == 0:
     print("Error: Graph has 0 nodes, cannot initialize agents.", file=sys.stderr)
     agents = []
elif len(start_nodes) == 0:
     print("Warning: No starting nodes selected (perhaps nodes=0 or starting_positions=0). Initializing agents at node 0 if available.", file=sys.stderr)
     start_nodes = [list(G.nodes())[0]] # Fallback to node 0 if exists
     agents = [Agent(i, start_nodes[0]) for i in range(agent_count)] if G.number_of_nodes() > 0 else []
else:
     agents = [Agent(i, random.choice(start_nodes)) for i in range(agent_count)]

if __name__ == "__main__": # Only print agent info when run directly
    print(f"Initialized {len(agents)} agents at nodes: {start_nodes}", file=sys.stderr)


# --- Execute Simulation ---
# Initialize return variables in case simulation doesn't run
all_positions, all_statuses, all_leaders, all_levels, all_node_settled_states = [], [], [], [], []

if agents and rounds > 0 and G.number_of_nodes() > 0:
    # run_simulation returns the collected data
    all_positions, all_statuses, all_leaders, all_levels, all_node_settled_states = run_simulation(
        G, agents, max_degree, rounds, start_nodes # Pass start_nodes if needed by run_simulation
    )
    if __name__ == "__main__": # Only print simulation finished info when run directly
        print(f'Simulation finished after {len(all_positions) - 1} recorded steps.', file=sys.stderr)
else:
    if __name__ == "__main__": # Only print skip info when run directly
        print("Simulation prerequisites not met (no agents, rounds > 0, or nodes > 0). Skipping run_simulation.", file=sys.stderr)


# --- Compute Layout ---
# Use a layout for saving, even if not visualized by the browser
pos = nx.spring_layout(G, scale=300, seed=seed) if G.number_of_nodes() > 0 else {}

# --- Prepare JSON Output ---
nodes_data = [
    {"data": {"id": str(n)}, "position": {"x": float(pos[n][0]), "y": float(pos[n][1])}, "classes": "graph-node"}
    for n in G.nodes()
]
edges_data = [
    {
        "data": {
            "id":       f"{u}-{v}",
            "source":   str(u),
            "target":   str(v),
            "srcPort":  G[u][v].get(f"port_{u}", '?'),
            "dstPort":  G[u][v].get(f"port_{v}", '?')
        }
    }
    for u, v in G.edges()
]

# Build the final result dictionary
result = {
  "nodes":     nodes_data,
  "edges":     edges_data,
  "positions": all_positions,
  "statuses":  all_statuses,
  "leaders":   all_leaders,
  "levels":    all_levels,
  "node_settled_states": all_node_settled_states
}

resultJson = json.dumps(result, indent=2) # Convert to JSON string for printing
# --- Save to File or Print to Stdout based on args ---
if args.output_dir:
    # Ensure the output directory exists
    output_dir = args.output_dir
    try:
        os.makedirs(output_dir, exist_ok=True)
    except OSError as e:
        print(f"Error creating output directory {output_dir}: {e}", file=sys.stderr)
        sys.exit(1) # Exit if directory cannot be created

    # Generate timestamped filename
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    filename = f"simulation_data_{timestamp}.json"
    filepath = os.path.join(output_dir, filename)

    # Save the JSON data to the file
    try:
        with open(filepath, 'w') as f:
            json.dump(result, f, indent=2) # Use json.dump for writing to a file object
        print(f"Simulation results saved to {filepath}", file=sys.stderr) # Print confirmation to stderr
    except IOError as e:
        print(f"Error saving simulation results to {filepath}: {e}", file=sys.stderr)
        sys.exit(1)
elif __name__ == "__main__":
    # print(resultJson) # Use json.dumps for getting the string
    pass
resultJson