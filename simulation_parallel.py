#!/usr/bin/env python3
"""
simulation_parallel.py
======================

Exactly the same logic as simulation.py, but every (graph_type, n, d, seed, run)
combination is executed in a separate worker process.  Requires only the standard
library.

Usage:
    python simulation_parallel.py -o simulation_results.json --workers 8
"""

from __future__ import annotations
import os, time, json, random, argparse, itertools, math, signal, sys
from copy import deepcopy
from concurrent.futures import ProcessPoolExecutor, as_completed

import networkx as nx

# —— your helper module: no change ————————————————————————————————
from agent_election import (build_graph, randomize_ports,
                            scatter_one_agent_per_node, run_leader_election)

# —— Defaults ————————————————————————————————————————————————————————
DEFAULT_GRAPH_TYPES        = ["erdos", "barabasi", "smallworld",
                               "grid", "hypercube", "complete", "tree"]
DEFAULT_NODE_COUNTS        = [16, 32, 64, 128, 256]
DEFAULT_GRAPH_SEEDS        = [42]
DEFAULT_NUM_RUNS_PER_GRAPH = 8
DEFAULT_MAX_ROUNDS         = 150_000
DEFAULT_OUTPUT_FILE        = "simulation_results.json"
DEFAULT_WORKERS            = os.cpu_count() or 1

# —— Worker function ————————————————————————————————————————————————
def _run_single(config: tuple[str,int,int,int,int,bool,int]) -> dict:
    """
    One independent simulation run.

    Parameters (packed for pickling):
        (graph_type, n, d, graph_seed, run_idx, max_rounds)
    Returns:
        dict – one entry in the results json
    """
    (kind, n, gseed, run_idx, max_rounds) = config

    entry = {
        "graph_type":     kind,
        "num_nodes":      n,
        "graph_seed":     gseed,
        "run_index":      run_idx,
        "max_degree_actual": None,
        "diameter":      None,
        "num_edges":      None,
        "rounds":         None,
        "leader":         None,
        "timeout":        None,
        "error":          None,
        "edge_traversals":       None,
        "max_edge_traversals":   None,
    }

    try:
        # Build base graph inside the worker (cheap, avoids cross‑process copy)
        G0 = build_graph(kind, n, gseed)
        entry["max_degree_actual"] = G0.graph.get("delta", -1)
        entry["diameter"]          = nx.diameter(G0)
        entry["num_edges"]         = G0.number_of_edges()
    except Exception as e:
        entry["error"] = f"graph build failure: {e}"
        return entry

    # Derive deterministic seeds for this *run*
    pr_seed = (gseed * 1_000_000) ^ (run_idx * 17_031) ^ 0xA5A5
    ap_seed = pr_seed + 987_654_321

    try:
        G  = deepcopy(G0)
        randomize_ports(G, pr_seed)
        agents = scatter_one_agent_per_node(G, ap_seed)
        leader, rounds, timeout = run_leader_election(
            G, agents, max_rounds=max_rounds)

        entry.update(rounds=rounds, leader=leader, timeout=timeout)

        per_agent = [a.edge_traversals for a in agents.values()]
        entry["edge_traversals"]     = sum(per_agent)
        entry["max_edge_traversals"] = max(per_agent, default=None)

    except Exception as e:
        entry["error"] = str(e)

    return entry


# —— Coordinator / CLI ——————————————————————————————————————————————
def run_simulation_suite(args):
    graph_types   = args.types
    node_counts   = args.nodes
    graph_seeds   = args.graph_seeds
    runs_per_graph= args.runs_per_graph

    combos = list(itertools.product(graph_types,
                                    node_counts,
                                    graph_seeds))
    total_runs = len(combos) * runs_per_graph
    print(f"Launching {total_runs} runs "
          f"({len(combos)} configs × {runs_per_graph} port randomisations)")

    # Create list of argument tuples for workers
    worker_args = []
    for kind, n, gseed in combos:
        for run_idx in range(runs_per_graph):
            worker_args.append((kind, n, gseed, run_idx, args.max_rounds))

    results = []
    start_all = time.time()

    # Allow Ctrl‑C to stop cleanly
    orig_sigint = signal.signal(signal.SIGINT, signal.SIG_IGN)
    with ProcessPoolExecutor(max_workers=args.workers) as pool:
        signal.signal(signal.SIGINT, orig_sigint)
        futures = [pool.submit(_run_single, wa) for wa in worker_args]

        done = 0
        for fut in as_completed(futures):
            try:
                res = fut.result()
            except Exception as e:      # should never happen
                res = {"error": f"worker exception: {e}"}
            results.append(res)
            done += 1
            if not args.quiet and (done % 20 == 0 or done == total_runs):
                elapsed = time.time() - start_all
                rate = done / elapsed if elapsed else 0
                print(f"  {done}/{total_runs} runs finished "
                      f"({rate:.1f} runs/s)")

    # — save —
    os.makedirs(os.path.dirname(args.output) or ".", exist_ok=True)
    with open(args.output, "w") as f:
        json.dump(results, f, indent=2)
    print(f"Saved {len(results)} records → {args.output}")

# ————————————————————————————————————————————————————————————————
if __name__ == "__main__":
    p = argparse.ArgumentParser(description="Parallel leader‑election sims")
    p.add_argument("-o","--output", default=DEFAULT_OUTPUT_FILE,
                   help="JSON file (default: %(default)s)")
    p.add_argument("--types",   nargs="+", default=DEFAULT_GRAPH_TYPES)
    p.add_argument("--nodes",   nargs="+", type=int, default=DEFAULT_NODE_COUNTS)
    p.add_argument("--graph-seeds", nargs="+", type=int, default=DEFAULT_GRAPH_SEEDS)
    p.add_argument("--runs-per-graph", type=int, default=DEFAULT_NUM_RUNS_PER_GRAPH)
    p.add_argument("--max-rounds", type=int, default=DEFAULT_MAX_ROUNDS)
    p.add_argument("--workers", type=int, default=DEFAULT_WORKERS,
                   help=f"Processes to launch (default = CPU count: {DEFAULT_WORKERS})")
    p.add_argument("--quiet", action="store_true",
                   help="Suppress progress output")
    args = p.parse_args()

    run_simulation_suite(args)
