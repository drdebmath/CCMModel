# graph_utils.py

import networkx as nx
import random

def create_port_labeled_graph(nodes, max_degree, seed):
    """
    Build an undirected graph where each node u has ports 0..deg(u)-1 
    mapped to its neighbors in arbitrary order.
    """
    G = nx.random_regular_graph(max_degree, nodes, seed = seed)

    for u in G.nodes():
        neighs = list(G.neighbors(u))
        # assign ports 0..len(neighs)-1 to neighbors
        G.nodes[u]['port_map'] = {p: v for p, v in enumerate(neighs)}
        for p, v in enumerate(neighs):
            G[u][v][f'port_{u}'] = p
    return G

def randomize_ports(G, seed):
    """Shuffle each node’s ports, updating both edge data and node.port_map."""
    random.seed(seed)
    for u in G.nodes():
        neighs = list(G.neighbors(u))
        if not neighs:
            continue
        new_ports = random.sample(range(len(neighs)), len(neighs))
        port_map = {}
        for v, p in zip(neighs, new_ports):
            G[u][v][f'port_{u}'] = p
            port_map[p] = v
        G.nodes[u]['port_map'] = port_map

def assign_weights(G, min_weight=0.0, max_weight=10.0):
    """Assign a random Gaussian weight to each edge."""
    mid = (min_weight + max_weight) / 2
    sd = (max_weight - min_weight) / 3
    for u, v in G.edges():
        G[u][v]['weight'] = random.gauss(mid, sd)

def get_neighbor_by_port(G, u, port):
    return G.nodes[u].get('port_map', {}).get(port)
