# simulation_wrapper.py

import json
import networkx as nx
from collections import defaultdict
from graph_utils import create_port_labeled_graph, randomize_ports
from agent import Agent

def get_agent_positions_and_statuses(G, agents):
    positions = [a.currentnode for a in agents]
    statuses  = [a.state['status'] for a in agents]
    return positions, statuses


def run_simulation(G, agents, rounds):
    all_positions = []
    all_statuses = []
    positions, statuses = get_agent_positions_and_statuses(G, agents)
    all_positions.append(positions)
    all_statuses.append(statuses)
    for a in agents:
        G.nodes[a.currentnode]['agents'].add(a)
    for r in range(1, rounds+1):
        print(f'round Number {r}')
        for a in agents:
            a.settle_dfs_rooted(G, agents)
        for a in agents:
            a.compute_dfs_rooted(G, agents)
        for a in agents:
            a.move_dfs_rooted(G, r)
        positions, statuses = get_agent_positions_and_statuses(G, agents)
        all_positions.append(positions)
        all_statuses.append(statuses)
    return all_positions, all_statuses

# "nodes", "edges", "agent_count", and "rounds" are provided by the runner

# Build and label graph ports
G = create_port_labeled_graph(nodes, edges)
randomize_ports(G)
for node in G.nodes():
    G.nodes[node]['agents'] = set()
    G.nodes[node]['settled_agent'] = None

# Initialize agents at start node (0)
agents = [Agent(i, 0) for i in range(agent_count)]

# Execute simulation rounds
if agents and rounds > 0 and G.number_of_nodes() > 0:
    all_positions, all_statuses = run_simulation(G, agents, rounds)
    print(f'Simulation finished')

# Compute layout once via spring
pos = nx.spring_layout(G, scale=300)

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
    "nodes": nodes_data,
    "edges": edges_data,
    "positions": all_positions,
    "statuses": all_statuses
}

# Return JSON string
json.dumps(result)
