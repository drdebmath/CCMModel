import networkx as nx
import random

def create_port_labeled_graph(nodes, edges):
    G = nx.Graph()
    # Add nodes first, handling the case of 0 nodes or 1 node
    if nodes > 0:
        G.add_nodes_from(range(nodes))
    else:
        return G # Return empty graph immediately if no nodes

    # Add edges only if there are at least 2 nodes
    if nodes > 1:
        G.add_edges_from(edges)

    # Assign local ports 1…deg(node) at each node
    for u in G.nodes():
        neighs = list(G.neighbors(u))
        ports = list(range(1, len(neighs) + 1))
        # Store port mapping directly on the node
        port_map = {}
        for i, v in enumerate(neighs):
            port_num = ports[i]
            G[u][v][f"port_{u}"] = port_num # Port on u leading to v
            port_map[port_num] = v
        G.nodes[u]['port_map'] = port_map

    # Second pass to store the reverse port mapping info on edges (optional but can be useful)
    # Alternatively, use get_port_to_neighbor as implemented below
    # for u, v in G.edges():
    #     if f"port_{v}" not in G[u][v]: # If reverse port not assigned yet
    #          port_v_to_u = get_port_to_neighbor(G, v, u) # Requires get_port_to_neighbor
    #          if port_v_to_u is not None:
    #               G[u][v][f"port_{v}"] = port_v_to_u

    return G

def randomize_ports(G):
    # reshuffle each node’s ports 1…deg
    for u in G.nodes():
        neighs = list(G.neighbors(u))
        if not neighs: continue # Skip nodes with no neighbors
        new_ports = random.sample(range(1, len(neighs) + 1), len(neighs))
        # Update edge data
        for v, p in zip(neighs, new_ports):
            G[u][v][f"port_{u}"] = p
        # Rebuild the lookup table
        pm = {}
        for v in G.neighbors(u):
             # Check if edge exists before accessing - robustness
             if G.has_edge(u, v) and f"port_{u}" in G[u][v]:
                 pm[G[u][v][f"port_{u}"]] = v
             # else: print(f"Warning: Edge data missing for {u}-{v} during port randomization") # Debug log
        G.nodes[u]['port_map'] = pm


def assign_weights(G, min_weight=0.0, max_weight=10.0):
    for u, v in G.edges():
        G[u][v]['weight'] = random.gauss((min_weight + max_weight) / 2, (max_weight - min_weight) / 3)

def get_neighbor_by_port(G, u, port):
    """Gets the neighbor node connected via a specific port number departing from node u."""
    try:
        # Ensure port_map exists and handle potential errors
        port_map = G.nodes[u].get('port_map', {})
        if port in port_map:
            return port_map[port]
        else:
             # More specific error or return None
             # raise ValueError(f"Node {u} has no port {port}. Available: {list(port_map.keys())}")
             print(f"Warning: Node {u} has no port {port}. Available: {list(port_map.keys())}. Agent cannot move via this port.")
             return None # Or handle differently (e.g., stay put)
    except KeyError:
        raise ValueError(f"Node {u} data issue or node does not exist.")
    except AttributeError:
         raise ValueError(f"Node {u} is missing 'port_map' attribute.")

def get_port_to_neighbor(G, u, v_neighbor):
    """Gets the port number on node u that leads to neighbor v_neighbor."""
    try:
        port_map = G.nodes[u].get('port_map', {})
        for port, neighbor in port_map.items():
            if neighbor == v_neighbor:
                return port
        # If loop finishes without finding, the neighbor isn't connected via any known port
        # This might indicate an inconsistent graph state or v_neighbor not being a neighbor
        # print(f"Warning: Could not find port from {u} to {v_neighbor}. Neighbors of {u}: {list(G.neighbors(u))}")
        return None # Indicate port not found
    except KeyError:
         # This happens if node u doesn't exist
         print(f"Warning: Node {u} not found in get_port_to_neighbor.")
         return None
    except AttributeError:
        # This happens if node u exists but is missing 'port_map'
        print(f"Warning: Node {u} missing 'port_map' attribute in get_port_to_neighbor.")
        return None