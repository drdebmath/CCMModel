# simulation_wrapper.py

import json
import networkx as nx
from collections import defaultdict
from graph_utils import create_port_labeled_graph, randomize_ports
from agent import Agent
import random
from agent import get_dict_key
from agent import AgentStatus, AgentRole, AgentPhase

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
        print(f'round Number {r - 1}')
        for a in agents:
            print(f"Round {r - 1}: Agent {a.id} is {get_dict_key(AgentStatus, a.state['status'])} located at {a.currentnode}. Role: {get_dict_key(AgentRole, a.state['role'])}. Phase: {get_dict_key(AgentPhase, a.state['phase'])}")
        for a in agents:
            a.compute_heo(G, agents)
        for a in agents:
            a.move_heo(G, r)
        positions, statuses = get_agent_positions_and_statuses(G, agents)
        all_positions.append(positions)
        all_statuses.append(statuses)
    return all_positions, all_statuses

# "nodes", "max degree", "agent_count", and "rounds" are provided by the runner

G = create_port_labeled_graph(nodes, max_degree, seed)
print(f'Graph created with {G.number_of_nodes()} nodes and {G.number_of_edges()} edges')
randomize_ports(G, seed)
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
    "nodes": nodes_data,
    "edges": edges_data,
    "positions": all_positions,
    "statuses": all_statuses
}

# Return JSON string
json.dumps(result)
