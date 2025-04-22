import sys
from graph_utils import create_port_labeled_graph, randomize_ports, assign_weights
import networkx as nx
import matplotlib.pyplot as plt
from matplotlib.animation import FuncAnimation
from collections import defaultdict
from agent import Agent
from simulation import run_simulation
from visualization import visualize_graph

# Main execution
if __name__ == "__main__":
    # Create graph with 5 nodes and some edges
    nodes = 5
    edges = [(0, 1), (0, 2), (1, 2), (1, 3), (2, 4), (3, 4)]
    G = create_port_labeled_graph(nodes, edges)
    
    # Randomize ports
    randomize_ports(G)
    
    # Assign weights to edges
    assign_weights(G, min_weight=1.0, max_weight=10.0)
    
    # Create agents
    agents = [Agent(i, i % nodes) for i in range(10)]  # 6 agents, distributed across nodes
    
    # Run simulation
    run_simulation(G, agents, 3)