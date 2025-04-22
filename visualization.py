import matplotlib.pyplot as plt
import networkx as nx

def visualize_graph(graph, agents, round_num):
    plt.clf()
    pos = nx.spring_layout(graph)
    nx.draw(graph, pos, with_labels=True, node_color="lightblue", node_size=500)
    edge_labels = {(u, v): f"{d.get('weight', 1.0):.1f}" for u, v, d in graph.edges(data=True)}
    nx.draw_networkx_edge_labels(graph, pos, edge_labels=edge_labels, font_size=8)
    for agent in agents:
        plt.text(pos[agent.currentnode][0], pos[agent.currentnode][1] + 0.05, f"A{agent.id}", fontsize=12, color="red")
    plt.title(f"Round {round_num}")
    plt.show(block=False)
    plt.pause(1.5)
    plt.close()
