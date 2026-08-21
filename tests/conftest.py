"""Shared builders for deterministic Python behavior fixtures.

The graph declarations in ``fixtures/reference_cases.json`` are intentionally
explicit.  In particular, port labels are part of a case, rather than being
derived from NetworkX iteration order.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import networkx as nx
import pytest


FIXTURE_PATH = Path(__file__).parent / "fixtures" / "reference_cases.json"


@pytest.fixture(scope="session")
def reference_cases() -> dict[str, dict[str, Any]]:
    with FIXTURE_PATH.open(encoding="utf-8") as handle:
        return json.load(handle)


def build_port_graph(case: dict[str, Any]) -> nx.Graph:
    """Build and validate one explicit port-labeled undirected graph."""

    graph = nx.Graph()
    ports = {int(node): {int(port): int(neighbor) for port, neighbor in mapping.items()}
             for node, mapping in case["ports"].items()}
    graph.add_nodes_from(sorted(ports))
    graph.add_edges_from(tuple(edge) for edge in case["edges"])

    assert set(graph) == set(ports)
    for node, mapping in ports.items():
        assert set(mapping) == set(range(len(mapping))), (node, mapping)
        assert set(mapping.values()) == set(graph.neighbors(node)), (node, mapping)
        graph.nodes[node]["port_map"] = dict(mapping)
        graph.nodes[node]["nbr_to_port"] = {neighbor: port for port, neighbor in mapping.items()}

    for node, mapping in ports.items():
        for local_port, neighbor in mapping.items():
            assert node in ports[neighbor].values(), (node, local_port, neighbor)
            remote_port = next(port for port, value in ports[neighbor].items() if value == node)
            assert graph.has_edge(node, neighbor)
            graph[node][neighbor][f"port_{node}"] = local_port
            graph[node][neighbor][f"port_{neighbor}"] = remote_port

    # Reciprocal traversal is the central port-graph invariant.
    for node, mapping in ports.items():
        for local_port, neighbor in mapping.items():
            remote_port = graph[node][neighbor][f"port_{neighbor}"]
            assert graph.nodes[neighbor]["port_map"][remote_port] == node

    return graph


def make_agents(module: Any, starts: list[int]) -> list[Any]:
    return [module.Agent(agent_id, node) for agent_id, node in enumerate(starts)]


def trace_lookup(trace: list[tuple[str, Any]], label: str) -> Any:
    for current_label, value in trace:
        if current_label == label:
            return value
    raise AssertionError(f"missing trace label {label!r}")

