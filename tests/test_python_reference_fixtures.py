"""Phase 1 deterministic behavior reference tests.

These tests intentionally describe today's Python behavior.  They are
fixtures for the Rust migration, not a claim that every current behavior is
the desired eventual algorithm semantics.
"""

from __future__ import annotations

import contextlib
import io
from typing import Any

import pytest

import agent_drop_freeze
import agent_help_scouts

from .conftest import build_port_graph, make_agents, trace_lookup


GRAPH_CASES = ("single_node", "path3", "cycle3", "star4", "tree5", "dense_k4")


def _drop_trace(case: dict[str, Any]) -> tuple[list[tuple[str, Any]], list[tuple[str, Any]], list[tuple[str, Any]]]:
    graph = build_port_graph(case)
    agents = make_agents(agent_drop_freeze, case["starts"])
    positions, statuses, _leaders, _levels, node_states = agent_drop_freeze.run_simulation(
        graph, agents, case["rounds"]
    )
    # Return the histories needed for the stable fixture assertions, including
    # node states for ownership checks in the caller.
    return positions, statuses, node_states


@pytest.mark.parametrize("case_name", GRAPH_CASES)
def test_explicit_port_graphs_have_reciprocal_ports(reference_cases, case_name):
    """Every structured fixture has explicit, reciprocal local port labels."""

    graph = build_port_graph(reference_cases[case_name])
    assert graph.number_of_edges() == len(reference_cases[case_name]["edges"])
    for node in graph:
        for port, neighbor in graph.nodes[node]["port_map"].items():
            remote_port = graph[node][neighbor][f"port_{neighbor}"]
            assert graph.nodes[neighbor]["port_map"][remote_port] == node


@pytest.mark.parametrize("case_name", GRAPH_CASES + ("path3_two_starts",))
def test_drop_and_freeze_final_and_intermediate_behavior(reference_cases, case_name):
    case = reference_cases[case_name]
    expected = case["drop_and_freeze"]
    positions, statuses, node_states = _drop_trace(case)

    for label, (expected_positions, expected_statuses) in expected["trace"].items():
        assert trace_lookup(positions, label) == expected_positions
        assert trace_lookup(statuses, label) == expected_statuses

    assert positions[-1][1] == expected["final_positions"]
    assert statuses[-1][1] == expected["final_statuses"]
    assert all(status == agent_drop_freeze.AgentStatus["SETTLED"] for status in statuses[-1][1])

    # The UI-facing node state is also part of the historical return tuple.
    owners = {
        int(node): state["settled_agent_id"]
        for node, state in trace_lookup(node_states, positions[-1][0]).items()
        if state is not None
    }
    assert sorted(owners.values()) == list(range(len(case["starts"])))


@pytest.mark.parametrize("case_name", GRAPH_CASES)
def test_help_by_scouts_final_and_intermediate_behavior(reference_cases, case_name):
    case = reference_cases[case_name]
    expected = case["help_by_scouts"]
    graph = build_port_graph(case)
    agents = make_agents(agent_help_scouts, case["starts"])

    # The implementation is intentionally verbose; traces must not depend on
    # diagnostic stdout, so suppress it while capturing the returned history.
    with contextlib.redirect_stdout(io.StringIO()):
        positions, statuses, node_states, homes, tree_edges = agent_help_scouts.run_simulation(
            graph, agents, case["rounds"]
        )

    for label, expected_values in expected["trace"].items():
        expected_positions, expected_statuses = expected_values
        assert trace_lookup(positions, label) == expected_positions
        assert trace_lookup(statuses, label) == expected_statuses

    assert positions[-1][1] == expected["final_positions"]
    assert statuses[-1][1] == expected["final_statuses"]
    assert homes[-1][1] == expected["final_homes"]
    assert tree_edges[-1][1] == expected["final_tree_edges"]
    assert all(status == ["settled"] for status in statuses[-1][1])


def test_help_by_scouts_records_its_algorithm_specific_slots(reference_cases):
    """Homes/tree edges are distinct from Drop-and-Freeze leader/level slots."""

    case = reference_cases["path3"]
    graph = build_port_graph(case)
    agents = make_agents(agent_help_scouts, case["starts"])
    with contextlib.redirect_stdout(io.StringIO()):
        positions, statuses, node_states, homes, tree_edges = agent_help_scouts.run_simulation(
            graph, agents, case["rounds"]
        )

    assert homes[-1][1] == [["2"], ["1"], ["0"]]
    assert tree_edges[-1][1]
    assert node_states[-1][1] == []
    # The public return shape is five algorithm-specific histories, but the
    # second and fourth slots do not have the same meaning across algorithms.
    assert statuses[-1][1] != homes[-1][1]


def test_help_by_scouts_documents_current_multi_start_limitation(reference_cases):
    case = reference_cases["path3_two_starts"]
    graph = build_port_graph(case)
    agents = make_agents(agent_help_scouts, case["starts"])

    with contextlib.redirect_stdout(io.StringIO()):
        with pytest.raises(RuntimeError, match=r"Agent 1 not at 0, at 2"):
            agent_help_scouts.run_simulation(graph, agents, case["rounds"])


def test_drop_and_freeze_round_limit_is_not_success(reference_cases):
    """A one-round cap leaves a multi-node run visibly unsettled."""

    case = reference_cases["path3"]
    graph = build_port_graph(case)
    agents = make_agents(agent_drop_freeze, case["starts"])
    positions, statuses, _leaders, _levels, _nodes = agent_drop_freeze.run_simulation(graph, agents, 1)

    assert positions[-1][0] == "round1:move_out"
    assert statuses[-1][1] == [agent_drop_freeze.AgentStatus["SETTLED"],
                               agent_drop_freeze.AgentStatus["UNSETTLED"],
                               agent_drop_freeze.AgentStatus["UNSETTLED"]]
    assert not all(agent.state["status"] == agent_drop_freeze.AgentStatus["SETTLED"] for agent in agents)
