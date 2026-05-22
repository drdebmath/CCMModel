from collections import defaultdict

AgentStatus = {
    "SETTLED": 0,
    "UNSETTLED": 1,
    "SETTLED_WAIT": 2,
}

NodeStatus = {
    "EMPTY": 0,
    "OCCUPIED": 1,
}

class Agent:
    def __init__(self, id, initial_node):
        self.id = id
        self.currentnode = initial_node

        self.state = {
            "status": AgentStatus["UNSETTLED"],
            "level": 0,
            "leader": self,
            "home": None,
        }

        # probe memory (used across probe_out -> probe_back -> move_out)
        self.probe_home = None          # node we will return to
        self.probe_port = None          # which port we probed from home
        self.probe_result_empty = None  # True/False after probing
        # DFS bookkeeping
        self.pin = None              # incoming port at currentnode (port that goes back)
        self.parent_port = None      # saved when this agent becomes settled
        self.next_port_to_try = 0    # DFS cursor at settled nodes
        self.entry_pin = None  


def _ordered_ports(G, u):
    port_map = G.nodes[u]["port_map"]
    nbr_to_port_u = G.nodes[u]["nbr_to_port"]

    def rank(p):
        v = port_map[p]
        p_vu = G.nodes[v]["nbr_to_port"][u]
        local_is_p1  = (p == 0)
        remote_is_p1 = (p_vu == 0)
        if (not local_is_p1) and remote_is_p1:
            return (0, p)
        if local_is_p1:
            return (1, p)
        return (2, p)

    return sorted(port_map.keys(), key=rank)



def _positions_and_statuses(agents):
    return [a.currentnode for a in agents], [a.state["status"] for a in agents]


def _snapshot(all_positions, all_statuses, all_leaders, all_levels, all_node_states,
              label, G, agents):
    positions, statuses = _positions_and_statuses(agents)

    # node settled states for UI (keep keys compatible with existing JSON)
    node_states = {}
    for node_id in G.nodes():
        sa = G.nodes[node_id].get("settled_agent")
        if sa is None:
            node_states[str(node_id)] = None
        else:
            node_states[str(node_id)] = {
                "settled_agent_id": sa.id,
                "parent_port": None,
                "checked_port": None,
                "max_scouted_port": None,
                "next_port": None,
            }

    all_positions.append((label, positions))
    all_statuses.append((label, statuses))
    all_node_states.append((label, node_states))
    all_leaders.append((label, [a.state["leader"].id for a in agents]))
    all_levels.append((label, [a.state["level"] for a in agents]))
    return positions, statuses

def _init_ports(G):
    # Deterministic local ports + inverse map (neighbor -> port)
    for u in G.nodes:
        nbrs = sorted(G.neighbors(u))
        G.nodes[u]["port_map"] = {i: v for i, v in enumerate(nbrs)}
        G.nodes[u]["nbr_to_port"] = {v: i for i, v in enumerate(nbrs)}

    # keep edge attrs consistent if your visualizer expects them
    for u, v in G.edges:
        G[u][v][f"port_{u}"] = G.nodes[u]["nbr_to_port"][v]
        G[u][v][f"port_{v}"] = G.nodes[v]["nbr_to_port"][u]


def _probe_out(G, node_to_agents):
    """
    All UNSETTLED (non-settled) agents at each node fan out to distinct ports.
    One agent per port (up to degree).
    """
    moves = []  # list of (agent, src, dst, port)

    for u, agents_here in node_to_agents.items():
        settled = G.nodes[u].get("settled_agent")
        unsettled = [a for a in agents_here if a is not settled and a.state["status"] == AgentStatus["UNSETTLED"]]
        if not unsettled:
            continue
        unsettled.sort(key=lambda a: a.id)
        for a in unsettled:
            a.probe_home = None
            a.probe_port = None
            a.probe_result_empty = None

        if settled is None and len(unsettled) == 1:
            continue

        deg = G.degree[u]
        if deg == 0:
            continue

        sa = G.nodes[u].get("settled_agent")
        ports = _ordered_ports(G, u)

        start = sa.next_port_to_try if sa is not None else 0
        ports_to_probe = ports[start:]
        for a, port in zip(unsettled, ports_to_probe):
            v = G.nodes[u]["port_map"].get(port)
            if v is None:
                continue
            a.probe_home = u
            a.probe_port = port
            a.probe_result_empty = None
            a.state["status"] = AgentStatus["SETTLED_WAIT"]
            moves.append((a, u, v, port))

    for a, u, v, _port in moves:
        back_port = G[u][v][f"port_{v}"]
        G.nodes[u]["agents"].remove(a)
        a.currentnode = v
        G.nodes[v]["agents"].add(a)


def _probe_back(G, agents):
    """
    Every PROBING agent checks whether its current node is empty (no settled_agent),
    then returns to its probe_home.
    """
    moves = []  # list of (agent, src, home)

    for a in agents:
        if a.state["status"] != AgentStatus["SETTLED_WAIT"]:
            continue
        src = a.currentnode
        # "empty" means no settled agent at that node
        a.probe_result_empty = (G.nodes[src].get("settled_agent") is None)
        home = a.probe_home
        if home is None:
            a.state["status"] = AgentStatus["UNSETTLED"]
            continue
        moves.append((a, src, home))

    # execute returns simultaneously
    for a, src, home in moves:
        back_port = G[src][home][f"port_{home}"]
        G.nodes[src]["agents"].remove(a)
        a.currentnode = home
        G.nodes[home]["agents"].add(a)
        a.state["status"] = AgentStatus["UNSETTLED"]

def _move_out(G, node_to_agents):
    planned_moves = []
    unsettled_by_node = {}
    AS = AgentStatus
    NS = NodeStatus

    # Collect unsettled movers at each node (excluding the local settled agent object)
    for u, agents_here in node_to_agents.items():
        settled = G.nodes[u].get("settled_agent")
        movers = [a for a in agents_here
                  if a is not settled and a.state["status"] == AS["UNSETTLED"]]
        if movers:
            movers.sort(key=lambda a: a.id)
            unsettled_by_node[u] = movers

    # Phase 1: settle one agent at any empty node that currently has movers
    newly_settled_nodes = set()
    for u, movers in unsettled_by_node.items():
        if G.nodes[u].get("settled_agent") is None and movers:
            to_settle = movers.pop(0)
            to_settle.state["status"] = AS["SETTLED"]
            to_settle.next_port_to_try = 0

            # Parent is the port used to enter this node (if valid)
            to_settle.parent_port = to_settle.entry_pin
            pnode = G.nodes[u]["port_map"].get(to_settle.parent_port)
            if pnode is None:
                to_settle.parent_port = None
            if to_settle.parent_port not in G.nodes[u]["port_map"]:
                to_settle.parent_port = None

            G.nodes[u]["settled_agent"] = to_settle
            G.nodes[u]["node_status"] = NS["OCCUPIED"]
            newly_settled_nodes.add(u)

    # Phase 2: GROUP DFS move
    for u, movers in unsettled_by_node.items():
        if not movers:
            continue

        sa = G.nodes[u].get("settled_agent")
        if sa is None:
            continue

        port_map = G.nodes[u]["port_map"]
        ports = _ordered_ports(G, u)
        if not ports:
            continue

        cursor_idx = sa.next_port_to_try if sa.next_port_to_try is not None else 0
        port_to_idx = {p:i for i,p in enumerate(ports)}

        # 1) Prefer ports confirmed empty by probe results (this round)
        probe_empty_ports = []
        seen = set()

        scouts = list(movers)
        if u in newly_settled_nodes:
            scouts.append(sa)

        for a in scouts:
            if a.probe_home != u or a.probe_result_empty is not True:
                continue
            p = a.probe_port
            if p in port_map and p not in seen :
                seen.add(p)
                probe_empty_ports.append(p)

        probe_empty_ports.sort(key=lambda p: port_to_idx.get(p, 10**9))

        chosen_port = None

        # DFS: choose first candidate >= cursor
        for p in probe_empty_ports:
            i = port_to_idx.get(p)
            if i is not None and i >= cursor_idx:
                chosen_port = p
                break

        # Forward DFS move: ALL movers go together
        if chosen_port is not None:
            v = port_map[chosen_port]
            sa.next_port_to_try = port_to_idx[chosen_port] + 1
            for a in movers:
                planned_moves.append((a, u, v))
            continue


        if chosen_port is None:
            probed_ports = [
                a.probe_port
                for a in scouts
                if a.probe_home == u and a.probe_port is not None
            ]
            probed_count = len(set(probed_ports))
            sa.next_port_to_try = min(len(ports), cursor_idx + probed_count)
            if sa.next_port_to_try < len(ports):
                continue  


        # 3) No empty neighbor left -> backtrack to parent as a group
        if sa.parent_port is not None and sa.parent_port in port_map:
            v = port_map[sa.parent_port]
            sa.next_port_to_try = len(ports)
            for a in movers:
                planned_moves.append((a, u, v))

    # Execute moves (your original logic)
    for a, u, v in planned_moves:
        back_port = G.nodes[v]["nbr_to_port"][u]
        G.nodes[u]["agents"].remove(a)

        a.currentnode = v
        a.pin = back_port
        a.entry_pin = back_port

        a.probe_home = None
        a.probe_port = None
        a.probe_result_empty = None

        G.nodes[v]["agents"].add(a)

def run_simulation(G, agents, rounds):
    for node in G.nodes():
        G.nodes[node]["agents"] = set()
        G.nodes[node]["settled_agent"] = None
        G.nodes[node]["node_status"] = NodeStatus["EMPTY"]
    _init_ports(G)
    for a in agents:
        a.state["status"] = AgentStatus["UNSETTLED"]
        a.probe_home = None
        a.probe_port = None
        a.probe_result_empty = None
        a.pin = None
        a.parent_port = None
        a.next_port_to_try = 0
        a.entry_pin = None

    for a in agents:
        G.nodes[a.currentnode]["agents"].add(a)

    all_positions, all_statuses = [], []
    all_leaders, all_levels = [], []
    all_node_states = []

    _snapshot(all_positions, all_statuses, all_leaders, all_levels, all_node_states,
              "start", G, agents)

    # Each macro-round = 3 synchronous sub-rounds
    for r in range(1, rounds + 1):
        if all(a.state["status"] == AgentStatus["SETTLED"] for a in agents):
            break

        node_to_agents = defaultdict(list)
        for a in agents:
            node_to_agents[a.currentnode].append(a)

        _probe_out(G, node_to_agents)
        _snapshot(all_positions, all_statuses, all_leaders, all_levels, all_node_states,
                  f"round{r}:probe_out", G, agents)

        _probe_back(G, agents)
        _snapshot(all_positions, all_statuses, all_leaders, all_levels, all_node_states,
                  f"round{r}:probe_back", G, agents)

        node_to_agents = defaultdict(list)
        for a in agents:
            node_to_agents[a.currentnode].append(a)

        _move_out(G, node_to_agents)
        _snapshot(all_positions, all_statuses, all_leaders, all_levels, all_node_states,
                  f"round{r}:move_out", G, agents)

    return all_positions, all_statuses, all_leaders, all_levels, all_node_states