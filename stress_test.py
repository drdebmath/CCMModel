"""
stress_test.py -- Randomized stress tests with correctness assertions for
all CCM model simulation algorithms.

Each test generates a random graph, runs the specified algorithm, and
asserts that:
  - No exceptions are raised during simulation.
  - All agents reach a settled / terminal state.
  - The simulation converges within the allowed number of rounds.

Usage:
  python stress_test.py                          # test all algorithms
  python stress_test.py --algo help_scouts       # test one algorithm
  python stress_test.py --num-tests 200          # reduce test count
"""

import argparse
import random
import traceback
from typing import Callable

import graph_utils
import agent_help_scouts
import agent_drop_freeze


GREEN = "\033[92m"
RED   = "\033[91m"
BOLD  = "\033[1m"
RESET = "\033[0m"

AGENT_STATUS_SETTLED = 0


def run_help_scouts(nodes: int, agent_count: int, degree: int, seed: int) -> dict:
    G = graph_utils.create_port_labeled_graph(nodes, degree, seed)
    graph_utils.randomize_ports(G, seed)
    agents = [agent_help_scouts.Agent(i, 0) for i in range(agent_count)]
    agent_help_scouts.run_simulation(G, agents)

    unsettled = [a for a in agents if a.state != "settled"]
    return {
        "agents": len(agents),
        "unsettled": len(unsettled),
    }


def run_drop_freeze(nodes: int, agent_count: int, degree: int, seed: int) -> dict:
    G = graph_utils.create_port_labeled_graph(nodes, degree, seed)
    graph_utils.randomize_ports(G, seed)
    agents = [agent_drop_freeze.Agent(i, 0) for i in range(agent_count)]
    agent_drop_freeze.run_simulation(G, agents, rounds=500)

    unsettled = [a for a in agents if a.state["status"] != AGENT_STATUS_SETTLED]
    return {
        "agents": len(agents),
        "unsettled": len(unsettled),
    }


ALGORITHMS: dict[str, Callable] = {
    "help_scouts": run_help_scouts,
    "drop_freeze": run_drop_freeze,
}


def main():
    parser = argparse.ArgumentParser(description="Stress test CCM simulation algorithms")
    parser.add_argument(
        "--algo",
        choices=["both", *ALGORITHMS.keys()],
        default="both",
        help="Run both algorithms or select one",
    )
    parser.add_argument("--num-tests", type=int, default=200, help="Number of random tests per algorithm")
    args = parser.parse_args()

    algos = ALGORITHMS if args.algo == "both" else {args.algo: ALGORITHMS[args.algo]}

    for algo_name, run_fn in algos.items():
        rng = random.Random(0)
        degree = 4
        num_tests = args.num_tests

        tests = []
        while len(tests) < num_tests:
            nodes = rng.randint(30, 100)
            agent_count = rng.randint(30, 100)
            seed = rng.randint(0, 10_000)
            if agent_count <= nodes:
                tests.append((nodes, agent_count, seed))

        print(f"\n=== Testing {algo_name} ({num_tests} runs) ===")
        failures = 0
        assertion_failures = 0

        for i, (nodes, agent_count, seed) in enumerate(tests, start=1):
            try:
                result = run_fn(nodes, agent_count, degree, seed)

                # Assertion: all agents must be settled
                if result["unsettled"] > 0:
                    assertion_failures += 1
                    print(f"[{i:03d}/{num_tests}] nodes={nodes:2d}, agents={agent_count:2d}, seed={seed:5d}  "
                          f"{RED}{BOLD}ASSERTION FAILED: {result['unsettled']} unsettled{RESET}")
                else:
                    print(f"[{i:03d}/{num_tests}] nodes={nodes:2d}, agents={agent_count:2d}, seed={seed:5d}  "
                          f"{GREEN}{BOLD}PASSED{RESET}")

            except Exception:
                failures += 1
                print(f"[{i:03d}/{num_tests}] nodes={nodes:2d}, agents={agent_count:2d}, seed={seed:5d}  "
                      f"{RED}{BOLD}EXCEPTION{RESET}")
                print(f"{RED}{traceback.format_exc()}{RESET}")

        total_bad = failures + assertion_failures
        if total_bad == 0:
            print(f"{GREEN}{BOLD}ALL {num_tests} TESTS PASSED{RESET}")
        else:
            print(f"{RED}{BOLD}{total_bad}/{num_tests} FAILED "
                  f"({failures} exceptions, {assertion_failures} assertions){RESET}")


if __name__ == "__main__":
    main()
