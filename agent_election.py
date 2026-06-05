from __future__ import annotations
import math, random
import itertools
from dataclasses import dataclass, field
from typing import Dict, List, Tuple, Optional, Set

import networkx as nx
import numpy as np

def _pyseed(x):
    """Return a native `int` or `None` suitable for RNG seeding."""
    try:
        return None if x is None else int(x)
    except Exception:  # pragma: no cover
        return None

# ──────────────────────────────  Graph helpers  ──────────────────────────────
def _assign_ports(G: nx.Graph) -> None:
    for u in G.nodes():
        neighs = list(G.neighbors(u))
        rng = random.Random(_pyseed(G.graph.get("seed")))
        rng.shuffle(neighs)
        port_map = {p: v for p, v in enumerate(neighs)}
        G.nodes[u]["port_map"] = port_map
        for p, v in port_map.items():
            if not G.has_edge(u, v):
                continue
            if G[u][v] is None:
                G[u][v] = {}
            G[u][v][f"port_{u}"] = p
            if G.nodes.get(v) and "port_map" in G.nodes[v]:
                for vp, vu in G.nodes[v]["port_map"].items():
                    if vu == u:
                        G[u][v][f"port_{v}"] = vp
                        break


def randomize_ports(G: nx.Graph, seed: int) -> None:
    rng = random.Random(_pyseed(seed))
    for u in G.nodes():
        neighs = list(G.neighbors(u))
        if len(neighs) <= 1:
            if "port_map" not in G.nodes[u]:
                G.nodes[u]["port_map"] = {0: neighs[0]} if neighs else {}
            for p, v in G.nodes[u]["port_map"].items():
                if G.has_edge(u, v):
                    if G[u][v] is None:
                        G[u][v] = {}
                    G[u][v][f"port_{u}"] = p
            continue

        port_indices = list(range(len(neighs)))
        rng.shuffle(port_indices)
        new_port_map = {port_indices[i]: neighs[i] for i in range(len(neighs))}
        G.nodes[u]["port_map"] = new_port_map
        for p, v in new_port_map.items():
            if G.has_edge(u, v):
                if G[u][v] is None:
                    G[u][v] = {}
                G[u][v][f"port_{u}"] = p

    for u, v in G.edges():
        if "port_map" in G.nodes[u] and "port_map" in G.nodes[v]:
            u_port = next((p for p, x in G.nodes[u]["port_map"].items() if x == v), None)
            v_port = next((p for p, x in G.nodes[v]["port_map"].items() if x == u), None)
            if u_port is not None:
                G[u][v][f"port_{u}"] = u_port
            if v_port is not None:
                G[u][v][f"port_{v}"] = v_port


def assign_weights(G: nx.Graph, lo: float = 0.0, hi: float = 10.0, seed: int = 0) -> None:
    rng = random.Random(_pyseed(seed))
    mid, sd = (lo + hi) / 2, (hi - lo) / 3
    for u, v in G.edges():
        if G[u][v] is None:
            G[u][v] = {}
        G[u][v]["weight"] = rng.gauss(mid, sd)


def get_neighbor_by_port(G: nx.Graph, u: int, port: int) -> Optional[int]:
    return G.nodes[u].get("port_map", {}).get(port)


def id_to_bits(uid: int, width: int) -> List[int]:
    return [(uid >> i) & 1 for i in range(max(1, width))]


# ───────────────────────────────  Agent class  ───────────────────────────────
@dataclass
class Agent:
    id:            int
    current_node:  int
    id_bits:       List[int]
    delta:         int
    bit_width:     int
    known_max_id:  int
    global_max_id: int
    parent:        Optional[int]      = None
    children:      Set[int]           = field(default_factory=set)
    incoming_votes: Dict[int, str]    = field(default_factory=dict)
    vote_to_parent: Optional[str]     = None
    parent_stable_phases: int          = 0
    undecided_phases_left: int         = 0
    pending_return:  bool              = False
    origin_node:    Optional[int]      = None
    edge_traversals: int               = 0
    phase_duration:         int        = 0
    rounds_in_current_phase: int       = 0
    phase_index:            int        = 0
    aware_of_leader:  bool             = False
    is_leader:        bool             = False
    _parent_last_phase: Optional[int]  = None

    def __post_init__(self) -> None:
        self.phase_duration = max(1, 2 * self.delta * self.bit_width)


# ──────────────────  Mobility: “bit‑controlled schedule”  ─────────────────
def decide_and_move(ag: Agent, round_idx: int, G: nx.Graph) -> None:
    if ag.delta == 0:
        return
    CYCLE = 2 * ag.delta
    bit_idx = (round_idx // CYCLE) % ag.bit_width
    bit     = ag.id_bits[bit_idx]
    pos_in_cycle = round_idx % CYCLE
    port_k, in_out = divmod(pos_in_cycle, 2)

    if bit == 0:
        if ag.pending_return and in_out == 1:
            ag.current_node = ag.origin_node
            ag.pending_return = False
            ag.origin_node = None
        return

    if ag.pending_return:
        if in_out == 1 and ag.origin_node is not None:
            ag.current_node = ag.origin_node
            ag.pending_return = False
            ag.origin_node = None
        return

    if in_out == 0:
        dest = get_neighbor_by_port(G, ag.current_node, port_k)
        if dest is not None:
            ag.origin_node   = ag.current_node
            ag.current_node  = dest
            ag.pending_return = True
            ag.edge_traversals += 1


# ─────────────────  Message exchange (every round) ─────────────────
def exchange_and_update_state(buckets: Dict[int, List[Agent]], agents: Dict[int, Agent]) -> None:
    for node_id, here in buckets.items():
        if not here:
            continue

        # broadcast highest known_id
        local_max = max(a.known_max_id for a in here)
        broadcasters = set()
        for a in here:
            if a.known_max_id == local_max:
                broadcasters.add(a.id)
            if a.known_max_id < local_max:
                a.known_max_id = local_max

        # parent assignment
        for a in here:
            if a.id in broadcasters:
                continue
            if broadcasters:
                new_p = min(broadcasters)
                if a.parent != new_p:
                    if a.parent in agents:
                        agents[a.parent].children.discard(a.id)
                    a.parent = new_p
                    agents[new_p].children.add(a.id)

        # child→parent vote delivery
        present_ids = {a.id for a in here}
        for child in here:
            if child.vote_to_parent and child.parent in present_ids:
                parent = agents[child.parent]
                parent.incoming_votes[child.id] = child.vote_to_parent
                child.vote_to_parent = None


# ─────────────────────  Phase‑state updates ───────────────────────────────
class PhaseState:
    PROPOSAL  = "proposal"
    UNDECIDED = "undecided"
    WAITING   = "waiting"
    YES_SENT  = "yes_sent"
    LEADER    = "leader"
    INFORMED  = "informed"


def update_agent_phase_state(ag: Agent, current_round: int, agents: Dict[int, Agent]) -> None:
    ag.rounds_in_current_phase += 1
    if ag.rounds_in_current_phase < ag.phase_duration:
        return
    ag.rounds_in_current_phase = 0
    ag.phase_index += 1

    # track parent stability
    if ag.parent == ag._parent_last_phase and ag.parent is not None:
        ag.parent_stable_phases += 1
    else:
        ag.parent_stable_phases = 0
    ag._parent_last_phase = ag.parent

    # helper checks
    def all_yes():
        return ag.children and all(ag.incoming_votes.get(c) == "yes" for c in ag.children)
    def any_und():
        return any(v == "undecided" for v in ag.incoming_votes.values())

    # if already learned leader → just keep informing
    if ag.aware_of_leader:
        ag.vote_to_parent = None
        return

    # leader node
    if ag.is_leader:
        ag.aware_of_leader = True
        return

    # ─ PROPOSAL phase ─
    if ag.phase_index == 0:
        if ag.parent_stable_phases >= 2 and ag.parent is not None:
            ag.undecided_phases_left = 2
            ag.vote_to_parent = "undecided"
        # else stay in proposal (no explicit vote_to_parent)

    # ─ UNDECIDED ─
    elif ag.undecided_phases_left > 0:
        ag.undecided_phases_left -= 1
        ag.vote_to_parent = "undecided"

    # ─ WAITING / VOTE DECISION ─
    else:
        if not ag.children:
            ag.vote_to_parent = "yes"
            ag.is_leader = (ag.id == ag.global_max_id)
            if ag.is_leader:
                ag.aware_of_leader = True
                ag.vote_to_parent = None
        else:
            if all_yes() and not any_und():
                ag.vote_to_parent = "yes"
            else:
                ag.vote_to_parent = "undecided"
            if ag.id == ag.global_max_id and all_yes() and not any_und():
                ag.is_leader = True
                ag.aware_of_leader = True
                ag.vote_to_parent = None

    # clear for next phase
    ag.incoming_votes.clear()


# ─────────────────────────────  Main election loop  ─────────────────────────
def run_leader_election(G: nx.Graph,
                        agents: Dict[int, Agent],
                        max_rounds: int = 200_000
                       ) -> Tuple[Optional[int], int, bool]:
    if not agents:
        return None, 0, False

    global_max = max(agents)
    for ag in agents.values():
        ag.global_max_id = global_max
        ag.phase_duration = max(1, 2 * ag.delta * ag.bit_width)

    if len(agents) == 1:
        only = next(iter(agents.values()))
        only.is_leader = True
        return only.id, 0, False

    for r in range(max_rounds):
        for ag in agents.values():
            decide_and_move(ag, r, G)

        buckets: Dict[int, List[Agent]] = {}
        for ag in agents.values():
            buckets.setdefault(ag.current_node, []).append(ag)

        exchange_and_update_state(buckets, agents)
        for ag in agents.values():
            update_agent_phase_state(ag, r, agents)

        leader = agents[global_max]
        if leader.is_leader:
            return leader.id, r + 1, False

    return None, max_rounds, True


# ───────────────────  Scatter helper  ────────────────────────────────────────
def scatter_one_agent_per_node(G: nx.Graph, seed: int = 0) -> Dict[int, Agent]:
    rng   = random.Random(_pyseed(seed))
    nodes = list(G.nodes())
    if not nodes:
        return {}
    ids   = list(range(1, len(nodes) + 1))
    rng.shuffle(ids)

    width = math.ceil(math.log2(len(nodes))) if len(nodes) > 1 else 1
    delta = G.graph.get("delta", max(dict(G.degree()).values()))
    G.graph["delta"] = delta

    agents: Dict[int, Agent] = {}
    for node, aid in zip(nodes, ids):
        agents[aid] = Agent(
            id=aid,
            current_node=node,
            id_bits=id_to_bits(aid, width),
            delta=delta,
            bit_width=width,
            known_max_id=aid,
            global_max_id=0,
        )
    return agents


# ───────────────────  Graph builder  ────────────────────────────────────────

def build_graph(kind: str, n: int, seed: int, param: int = 3) -> nx.Graph:
    seed = int(seed)
    if n <= 0:
        return nx.Graph()
    
    def _connected(g: nx.Graph) -> bool:
        return n <= 1 or nx.is_connected(g)

    if kind == "regular":
        d = param  # Degree for regular graph
        if d >= n or (n * d) % 2:
            raise nx.NetworkXError("regular: need d < n and n·d even")
        G = nx.random_regular_graph(d, n, seed=seed)

    elif kind == "erdos":
        # Set p slightly above connectivity threshold
        p = min(1.0, (math.log(n) + math.log(math.log(n))) / max(1, n))
        G = nx.erdos_renyi_graph(n, p, seed=seed)
        # If not connected, add edges to connect components
        if not _connected(G):
            components = list(nx.connected_components(G))
            if len(components) > 1:
                # Connect components by adding edges between them
                for i in range(len(components) - 1):
                    # Pick one node from each component
                    node1 = random.choice(list(components[i]))
                    node2 = random.choice(list(components[i + 1]))
                    G.add_edge(node1, node2)


    elif kind == "barabasi":
        m = max(1, param)  # Number of edges to attach per new node
        if n <= m:
            raise nx.NetworkXError("barabasi: need n > m")
        G = nx.barabasi_albert_graph(n, m, seed=seed)

    elif kind == "smallworld":
        k = max(2, param)  # Initial degree for each node
        G = nx.watts_strogatz_graph(n, k, 0.1, seed=seed)
        if not _connected(G):
            components = list(nx.connected_components(G))
            if len(components) > 1:
                # Connect components by adding edges between them
                for i in range(len(components) - 1):
                    # Pick one node from each component
                    node1 = random.choice(list(components[i]))
                    node2 = random.choice(list(components[i + 1]))
                    G.add_edge(node1, node2)

    elif kind == "grid":
        side = round(math.sqrt(n))
        G = nx.grid_graph([side, side])
        G = nx.convert_node_labels_to_integers(G)

    elif kind == "tree":
        G = nx.balanced_tree(param, int(math.ceil(math.log(n * (param - 1) + 1, param))), create_using=nx.Graph())
        G = nx.convert_node_labels_to_integers(G)
        if len(G) > n:
            G = G.subgraph(range(n)).copy()

    elif kind == "hypercube":
        k = round(math.log2(n))
        if 2**k != n:
            raise nx.NetworkXError("hypercube: n must be 2^k")
        G = nx.hypercube_graph(k)

    elif kind == "complete":
        if n < 2:
            raise nx.NetworkXError("complete: need n >= 2")
        G = nx.complete_graph(n)

    else:
        raise ValueError(f"unknown graph kind '{kind}'")

    if not _connected(G):
        raise nx.NetworkXError(f"{kind}: not connected")

    # Assign maximum degree to graph attribute
    delta = max(dict(G.degree()).values(), default=0)
    G.graph["delta"] = delta
    return G
