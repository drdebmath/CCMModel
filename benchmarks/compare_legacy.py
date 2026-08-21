"""Compare legacy Python with optimized headless Rust simulation execution."""

from __future__ import annotations

import argparse
import contextlib
import io
import subprocess
import statistics
import sys
import time
from pathlib import Path

import networkx as nx

ROOT = Path(__file__).resolve().parents[1]
RUST_BINARY = ROOT / "target" / "release" / "ccm-bench"
sys.path.insert(0, str(ROOT))

import agent_drop_freeze  # noqa: E402
import agent_help_scouts  # noqa: E402


def path_graph(nodes: int) -> nx.Graph:
    graph = nx.path_graph(nodes)
    for node in graph:
        neighbors = sorted(graph.neighbors(node))
        graph.nodes[node]["port_map"] = dict(enumerate(neighbors))
        graph.nodes[node]["nbr_to_port"] = {
            neighbor: port for port, neighbor in enumerate(neighbors)
        }
    for left, right in graph.edges:
        graph[left][right][f"port_{left}"] = graph.nodes[left]["nbr_to_port"][right]
        graph[left][right][f"port_{right}"] = graph.nodes[right]["nbr_to_port"][left]
    return graph


def python_run(algorithm: str, nodes: int, agents: int, iterations: int) -> tuple[int, int]:
    graph = path_graph(nodes)
    checksum = 0
    started = time.perf_counter_ns()
    for _ in range(iterations):
        if algorithm == "drop-and-freeze":
            values = [agent_drop_freeze.Agent(index, 0) for index in range(agents)]
            agent_drop_freeze.run_simulation(graph, values, 40 * agents)
            checksum += sum(agent.currentnode + 1 for agent in values)
        else:
            values = [agent_help_scouts.Agent(index, 0) for index in range(agents)]
            with contextlib.redirect_stdout(io.StringIO()):
                agent_help_scouts.run_simulation(graph, values, 40 * agents)
            checksum += sum(agent.node + 1 for agent in values)
    return time.perf_counter_ns() - started, checksum


def rust_run(algorithm: str, nodes: int, agents: int, iterations: int) -> tuple[int, int]:
    output = subprocess.check_output(
        [str(RUST_BINARY), algorithm, str(nodes), str(agents), str(iterations)],
        cwd=ROOT,
        text=True,
    ).strip()
    _, _, _, _, elapsed, checksum = output.split(",")
    return int(elapsed), int(checksum)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sizes", default="25,50,100")
    parser.add_argument("--iterations", type=int, default=3)
    parser.add_argument("--samples", type=int, default=5)
    args = parser.parse_args()
    sizes = [int(value) for value in args.sizes.split(",")]
    subprocess.run(
        [str(Path.home() / ".cargo/bin/cargo"), "build", "--release", "-p", "ccm-bench"],
        cwd=ROOT,
        check=True,
    )
    if args.iterations < 1 or args.samples < 1:
        parser.error("--iterations and --samples must be positive")
    print(
        "algorithm,nodes,agents,iterations,samples,python_median_ns,"
        "python_min_ns,python_max_ns,python_stdev_ns,rust_median_ns,"
        "rust_min_ns,rust_max_ns,rust_stdev_ns,speedup,checksum_equal"
    )
    for algorithm in ("drop-and-freeze", "help-by-scouts"):
        for size in sizes:
            # Warm both implementations before sampling to avoid measuring
            # dynamic imports, process cold-start effects, or first allocation.
            python_run(algorithm, size, size, 1)
            rust_run(algorithm, size, size, 1)
            python_samples = []
            rust_samples = []
            checksums_equal = True
            for _ in range(args.samples):
                python_ns, python_checksum = python_run(
                    algorithm, size, size, args.iterations
                )
                rust_ns, rust_checksum = rust_run(
                    algorithm, size, size, args.iterations
                )
                python_samples.append(python_ns)
                rust_samples.append(rust_ns)
                checksums_equal &= python_checksum == rust_checksum
            python_median = int(statistics.median(python_samples))
            rust_median = int(statistics.median(rust_samples))
            print(
                f"{algorithm},{size},{size},{args.iterations},{args.samples},"
                f"{python_median},{min(python_samples)},{max(python_samples)},"
                f"{statistics.pstdev(python_samples):.0f},{rust_median},"
                f"{min(rust_samples)},{max(rust_samples)},"
                f"{statistics.pstdev(rust_samples):.0f},"
                f"{python_median / rust_median:.2f},{checksums_equal}"
            )


if __name__ == "__main__":
    main()
