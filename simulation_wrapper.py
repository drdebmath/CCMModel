# simulation_wrapper.py

import json
import networkx as nx
from collections import defaultdict
from graph_utils import create_port_labeled_graph, randomize_ports
from agent import Agent
import random
from agent import get_dict_key
from agent import run_simulation

# "nodes", "max degree", "agent_count", and "rounds" are provided by the runner
# nodes = 10
# max_degree = 4
# agent_count = 8
# rounds = 8
# starting_positions = 2
# seed = 42

G = create_port_labeled_graph(nodes, max_degree, seed)
print(f'Graph created with {G.number_of_nodes()} nodes and {G.number_of_edges()} edges')
randomize_ports(G, seed)
for node in G.nodes():
    G.nodes[node]['agents'] = set()
    G.nodes[node]['settled_agent'] = None
    G.nodes[node]['leader'] = None

# Initialize agents at start node (0)
number_of_starting_positions = starting_positions
starting_positions = random.sample(range(G.number_of_nodes()), number_of_starting_positions)
agents = [Agent(i, random.choice(starting_positions)) for i in range(agent_count)]

# Execute simulation rounds
if agents and rounds > 0 and G.number_of_nodes() > 0:
    all_positions, all_statuses, all_leaders, all_levels = run_simulation(G, agents, max_degree, rounds, starting_positions)

    print(f'Simulation finished')

# Compute layout once via spring
pos = nx.spring_layout(G, scale=300, seed= seed)

# Prepare JSON output
nodes_data = [
    {"data": {"id": str(n)}, "position": {"x": float(pos[n][0]), "y": float(pos[n][1])}}
    for n in G.nodes()
]
# simulation_wrapper.py  – edge list
edges_data = [
    {
        "data": {
            "id":       f"{u}-{v}",
            "source":   str(u),
            "target":   str(v),
            "srcPort":  G[u][v][f"port_{u}"],
            "dstPort":  G[u][v][f"port_{v}"]
        }
    }
    for u, v in G.edges()
]


result = {
  "nodes":     nodes_data,
  "edges":     edges_data,
  "positions": all_positions,
  "statuses":  all_statuses,
  "leaders":   all_leaders,
  "levels":    all_levels
}


# Return JSON string
json.dumps(result)
