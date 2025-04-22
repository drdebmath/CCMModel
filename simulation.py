def run_simulation(G, agents, rounds):
    for round_num in range(1, rounds + 1):
        for agent in agents:
            agent.step(G, agents, round_num)