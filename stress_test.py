# stress_test.py
import random
import traceback

import graph_utils
import agent_help_scouts


GREEN = "\033[92m"
RED   = "\033[91m"
BOLD  = "\033[1m"
RESET = "\033[0m"


def run_one(nodes: int, agent_count: int, degree: int, seed: int):
    G = graph_utils.create_port_labeled_graph(nodes, degree, seed)
    graph_utils.randomize_ports(G, seed)
    agents = [agent_help_scouts.Agent(i, 0) for i in range(agent_count)]
    agent_help_scouts.run_simulation(G, agents)

def main():
    rng = random.Random(0)

    degree = 4
    num_tests = 500

    tests = []
    while len(tests)<num_tests:
        nodes = rng.randint(30, 100)
        agent_count = rng.randint(30, 100)
        seed = rng.randint(0, 10_000)
        if (agent_count<=nodes):
            tests.append((nodes, agent_count, seed))

    failures = 0

    for i, (nodes, agent_count, seed) in enumerate(tests, start=1):
        try:
            run_one(nodes, agent_count, degree, seed)
            print(f"[{i:03d}/{num_tests}] nodes={nodes:2d}, agents={agent_count:2d}, seed={seed:5d}  "
                  f"{GREEN}{BOLD}PASSED{RESET}")
        except Exception:
            failures += 1
            print(f"[{i:03d}/{num_tests}] nodes={nodes:2d}, agents={agent_count:2d}, seed={seed:5d}  "
                  f"{RED}{BOLD}FAILED{RESET}")
            print(f"{RED}{traceback.format_exc()}{RESET}")

            # Uncomment to stop on first failure:
            # break

    if failures == 0:
        print(f"\n{GREEN}{BOLD}ALL {num_tests} TESTS PASSED ✅{RESET}")
    else:
        print(f"\n{RED}{BOLD}{failures}/{num_tests} TESTS FAILED ❌{RESET}")


if __name__ == "__main__":
    main()