# ===== ./simulation_wrapper.py =====

import json
import networkx as nx
from collections import defaultdict
from graph_utils import create_port_labeled_graph, randomize_ports
from agent import Agent, run_simulation # Make sure run_simulation is imported
import random
# from agent import get_dict_key # Might not be needed here

# Parameters are injected by simulation-runner.js
# nodes = 10
# max_degree = 4
# agent_count = 8
# rounds = 8
# starting_positions = 2
# seed = 42

# --- Graph and Agent Initialization (remains the same) ---
G = create_port_labeled_graph(nodes, max_degree, seed)
print(f'Graph created with {G.number_of_nodes()} nodes and {G.number_of_edges()} edges')
randomize_ports(G, seed)
for node in G.nodes():
    G.nodes[node]['agents'] = set()
    G.nodes[node]['settled_agent'] = None
    # G.nodes[node]['leader'] = None # 'leader' on node might not be used

number_of_starting_positions = min(starting_positions, G.number_of_nodes()) # Ensure not more than nodes
start_nodes = random.sample(list(G.nodes()), number_of_starting_positions) if G.number_of_nodes() > 0 else [0]
agents = [Agent(i, random.choice(start_nodes)) for i in range(agent_count)]
print(f"Initialized {len(agents)} agents at nodes: {start_nodes}")

# --- Execute Simulation ---
# Initialize return variables in case simulation doesn't run
all_positions, all_statuses, all_leaders, all_levels, all_node_settled_states = [], [], [], [], []

if agents and rounds > 0 and G.number_of_nodes() > 0:
    # <-- NEW: Unpack the additional returned list -->
    all_positions, all_statuses, all_leaders, all_levels, all_node_settled_states = run_simulation(
        G, agents, max_degree, rounds, start_nodes
    )
    print(f'Simulation finished after {len(all_positions) - 1} recorded steps.')
else:
    print("Simulation prerequisites not met (no agents, rounds, or nodes). Skipping run_simulation.")


# --- Compute Layout (remains the same) ---
pos = nx.spring_layout(G, scale=300, seed=seed) if G.number_of_nodes() > 0 else {}

# --- Prepare JSON Output ---
nodes_data = [
    {"data": {"id": str(n)}, "position": {"x": float(pos[n][0]), "y": float(pos[n][1])}}
    for n in G.nodes()
]
edges_data = [
    {
        "data": {
            "id":       f"{u}-{v}",
            "source":   str(u),
            "target":   str(v),
            # Ensure port data exists before accessing
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
  "node_settled_states": all_node_settled_states # <-- NEW: Add the collected node states
}

# Return JSON string
json.dumps(result)