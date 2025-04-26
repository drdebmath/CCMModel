# main.py
import random
from collections import defaultdict
import networkx as nx
import graph_utils
import agent


# ------------------------------------------------------------------
def run_simulation(G, agents, rounds):
    for r in rounds:
        for a in agents:
            a.compute_dfs_rooted()


def main():
    # ─── demo topology ───
    nodes  = 5
    edges  = [(0, 1), (0, 2), (1, 2), (1, 3), (2, 4), (3, 4)]
    rounds = 8
    agent_count = 4

    G = graph_utils.create_port_labeled_graph(nodes, edges)
    # randomize_ports(G)                       # random port labels
    graph_utils.assign_weights(G, 1.0, 10.0)             # purely cosmetic

    # start each agent at node 0
    agents = [agent.Agent(i, 0) for i in range(agent_count)]

    # ─── run ───
    run_simulation(G, agents, rounds)

    print("\nAgent final states:")
    for a in agents:
        state = "SETTLED" if a.status == Agent.SETTLED else "UNSETTLED"
        print(f"  A{a.id} @ node {a.currentnode:>2}  →  {state}")


if __name__ == "__main__":
    main()
