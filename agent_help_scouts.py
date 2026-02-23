from typing import List
import inspect
import copy

BOTTOM = None
PORT_ONE = 0

class SIM_DATA:
    def __init__(self):
        self.all_positions = []
        self.all_statuses = []
        self.all_node_states = []
        self.all_homes = []
        self.all_tree_edges = []
        self.rounds = 0
    def clearr(self):
        self.all_positions = []
        self.all_statuses = []
        self.all_node_states = []
        self.all_homes = []
        self.all_tree_edges = []
        self.rounds = 0

simmer = SIM_DATA()

def _compute_tree_edges(G, arr):
    edges = []
    seen = set()
    for a in arr:
        if a.state not in ("settled", "settledScout"):
            continue
        if a.home is None or a.home is BOTTOM:
            continue
        if a.parentPort is None or a.parentPort is BOTTOM:
            continue
        u = int(a.home) if str(a.home).isdigit() else a.home
        v = _port_neighbor(G, u, a.parentPort)
        key = (min(u, v), max(u, v))
        if key in seen:
            continue
        seen.add(key)
        edges.append({
            "u": str(u),
            "v": str(v),
            "srcPort": a.parentPort,
            "dstPort": _port(G, v, u),
        })
    return edges

def _snapshot(label, G, agents, round_number, agent_id=-1):
    # print(label)
    arr = [agents[k] for k in sorted(agents.keys())]
    cur_positions = [[str(a.node)] for a in arr]
    cur_statuses = [[str(a.state)] for a in arr]
    cur_homes = [[str(a.home)] for a in arr]
    cur_tree_edges = _compute_tree_edges(G, arr)
    simmer.rounds = len(simmer.all_positions)

    def _agent_index_in_arr(aid):
        for i, a in enumerate(arr):
            if a.ID == aid:
                return i

    def _get_agent_value(container, aid):
        if isinstance(container, dict):
            return container[aid]
        if isinstance(container, (list, tuple)):
            idx = _agent_index_in_arr(aid)
            return container[idx]

    def _set_agent_value(container, aid, value):
        if isinstance(container, dict):
            container[aid] = value
            return container
        if isinstance(container, list):
            idx = _agent_index_in_arr(aid)
            container[idx] = value
            return container
        if isinstance(container, tuple):
            tmp = list(container)
            idx = _agent_index_in_arr(aid)
            tmp[idx] = value
            return tuple(tmp)

    def _update_label_at(idx, new_label):
        simmer.all_positions[idx]   = (new_label, simmer.all_positions[idx][1])
        simmer.all_statuses[idx]    = (new_label, simmer.all_statuses[idx][1])
        simmer.all_node_states[idx] = (new_label, simmer.all_node_states[idx][1])
        simmer.all_homes[idx]     = (new_label, simmer.all_homes[idx][1])
        simmer.all_tree_edges[idx]      = (new_label, simmer.all_tree_edges[idx][1])

    def _insert_new_round(new_label, base_positions, base_statuses, base_node_states, base_homes, base_tree_edges):
        simmer.all_positions.append((new_label, base_positions))
        simmer.all_statuses.append((new_label, base_statuses))
        simmer.all_node_states.append((new_label, base_node_states))
        simmer.all_homes.append((new_label, base_homes))
        simmer.all_tree_edges.append((new_label, base_tree_edges))
        simmer.rounds = len(simmer.all_positions)
    
    if round_number > simmer.rounds:
        raise ValueError(f"round_number={round_number} > simmer.rounds={simmer.rounds}")

    if agent_id == -1:
        if round_number < simmer.rounds:
            _update_label_at(round_number, label)
            simmer.all_tree_edges[round_number]  = (label, cur_tree_edges)
        elif round_number == simmer.rounds:
            base_positions = copy.deepcopy(cur_positions)
            base_statuses = copy.deepcopy(cur_statuses)
            base_node_states = []
            base_homes = copy.deepcopy(cur_homes)
            base_tree_edges = copy.deepcopy(cur_tree_edges)
            _insert_new_round(label, base_positions, base_statuses, base_node_states, base_homes, base_tree_edges)
        return
    
    if round_number < simmer.rounds:
        _update_label_at(round_number, label)
        stored_positions = copy.deepcopy(simmer.all_positions[round_number][1])
        stored_statuses  = copy.deepcopy(simmer.all_statuses[round_number][1])
        stored_homes  = copy.deepcopy(simmer.all_homes[round_number][1])
        new_agent_pos = _get_agent_value(cur_positions, agent_id)
        new_agent_sta = _get_agent_value(cur_statuses, agent_id)
        new_agent_hom = _get_agent_value(cur_homes, agent_id)
        stored_positions = _set_agent_value(stored_positions, agent_id, new_agent_pos)
        stored_statuses  = _set_agent_value(stored_statuses, agent_id, new_agent_sta)
        stored_homes  = _set_agent_value(stored_homes, agent_id, new_agent_hom)
        simmer.all_positions[round_number] = (label, stored_positions)
        simmer.all_statuses[round_number]  = (label, stored_statuses)
        simmer.all_homes[round_number]  = (label, stored_homes)
        simmer.all_tree_edges[round_number]  = (label, cur_tree_edges)

    elif round_number == simmer.rounds:
        base_positions   = copy.deepcopy(cur_positions)
        base_statuses    = copy.deepcopy(cur_statuses)
        base_node_states = []
        base_homes       = copy.deepcopy(cur_homes)
        base_tree_edges = copy.deepcopy(cur_tree_edges)
        new_agent_pos = _get_agent_value(cur_positions, agent_id)
        new_agent_sta = _get_agent_value(cur_statuses, agent_id)
        new_agent_hom = _get_agent_value(cur_homes, agent_id)
        base_positions = _set_agent_value(base_positions, agent_id, new_agent_pos)
        base_statuses  = _set_agent_value(base_statuses, agent_id, new_agent_sta)
        base_homes  = _set_agent_value(base_homes, agent_id, new_agent_hom)
        _insert_new_round(label, base_positions, base_statuses, base_node_states, base_homes, base_tree_edges)

    

class Agent:
    def __init__(self, id: int, start_node: int):

        self.node = start_node

        self.ID = id
        self.state = "unsettled"
        self.arrivalPort = BOTTOM
        self.treeLabel = BOTTOM

        self.nodeType = "unvisited"
        self.parentID = BOTTOM
        self.parentPort = BOTTOM
        self.portAtParent = None  
        self.P1Neighbor = BOTTOM
        self.portAtP1Neighbor = BOTTOM
        self.vacatedNeighbor = False
        self.recentChild = BOTTOM
        self.sibling = BOTTOM
        self.recentPort = BOTTOM
        self.probeResult = BOTTOM
        self.checked = 0

        self.scoutPort = BOTTOM
        self.scoutEdgeType = BOTTOM
        self.scoutP1Neighbor = BOTTOM
        self.scoutPortAtP1Neighbor = BOTTOM
        self.scoutP1P1Neighbor = BOTTOM
        self.scoutPortAtP1P1Neighbor = BOTTOM
        self.scoutResult = BOTTOM

        self.prevID = BOTTOM
        self.childPort = BOTTOM
        self.siblingDetails = BOTTOM
        self.childDetails = BOTTOM
        self.nextAgentID = BOTTOM
        self.nextPort = BOTTOM

        self.home = BOTTOM
        self.returnPort = BOTTOM
        self.returnreturnPort = BOTTOM
        self.returnreturnreturnPort = BOTTOM
        self.probeResultsByPort = {}

    @property
    def aid(self) -> int:
        return self.ID


def _port(G, u, v):
    return G[u][v][f"port_{u}"]

def _port_neighbor(G, u, port=PORT_ONE):
    pm = G.nodes[u]["port_map"]
    if (port not in pm.keys()):
        raise RuntimeError(f"tried to access port {port} at node {u}, ports = {pm.keys()}")
    return pm[port]

def edge_type(G, u, v):
    puv = _port(G, u, v)
    pvu = _port(G, v, u)
    u_is_1 = (puv == PORT_ONE)
    v_is_1 = (pvu == PORT_ONE)
    if u_is_1 and v_is_1:
        return "t11"
    if (not u_is_1) and v_is_1:
        return "tp1"
    if u_is_1 and (not v_is_1):
        return "t1q"
    return "tpq"

def _edge_rank(edge_type: str) -> int:
    if edge_type == "tp1":
        return 0
    if edge_type in ("t11", "t1q"):
        return 1
    if edge_type == "tpq":
        return 2

def update_node_type_after_probe(G, x, psi_x, scout_results):
    empty = [r for r in scout_results if r[2] == "unvisited"]
    if not empty:
        psi_x.nodeType = "fullyVisited"
        return
    if psi_x.parentPort is not None:
        parent_node = _port_neighbor(G, x, psi_x.parentPort)
        parent_edge_type = edge_type(G, x, parent_node)
        if parent_edge_type == "tpq" and all(r[1] == "tpq" for r in empty):
            psi_x.nodeType = "partiallyVisited"
            return
    psi_x.nodeType = "visited"


def reconfigure_if_needed(agents, psi_x, port_to_w, psi_w, arrival_port_at_w):
    if psi_w.nodeType == "partiallyVisited" and arrival_port_at_w == PORT_ONE:
        psi_w.parentID = psi_x.ID
        print(f"[DBG_RECONF] psi_w={psi_w.ID}@{psi_w.node} NEW parent={psi_w.parentID} parentPort={psi_w.parentPort}")
        psi_w.portAtParent = port_to_w 
        psi_w.parentPort = arrival_port_at_w  ################
        psi_w.nodeType = "visited"

def _candidate_rank(scout_result):
    pxy, etype, ntype, _ = scout_result

    if ntype == "unvisited":
        node_rank = 0
    elif ntype == "partiallyVisited" and etype in ("tp1", "t11"):
        node_rank = 1
    else:
        node_rank = 99

    return (node_rank, _edge_rank(etype), pxy) 


def _xi_id(G, w, exclude_ids=None, agents=None):
    # returns the ID of an agent settled at w else none
    exclude_ids = exclude_ids or set()
    agents_here = [aid for aid in G.nodes[w]["agents"] if ((aid not in exclude_ids) and ((agents[aid].state=="settled") or (agents[aid].state=="settledScout" and agents[aid].home==w)))]
    return agents_here[0] if agents_here else None


def _move_agent(G, agents, agent_id, from_node, out_port, round_number):
    if agents[agent_id].node != from_node:
        raise RuntimeError(f"Agent {agent_id} not at {from_node}, at {agents[agent_id].node}")
    to_node = _port_neighbor(G, from_node, out_port)
    if to_node is None:
        raise ValueError(f"Invalid port {out_port} at node {from_node}")

    G.nodes[from_node]["agents"].discard(agent_id)
    G.nodes[to_node]["agents"].add(agent_id)

    a = agents[agent_id]
    a.node = to_node
    a.arrivalPort = _port(G, to_node, from_node)

    _snapshot(f"move_agent(a={agent_id},from={from_node},p={out_port},to={to_node})", G, agents, round_number, agent_id)

    in_port = G[to_node][from_node][f"port_{to_node}"]
    return to_node, in_port


def _move_group(G, agents, agent_ids, from_node, out_port, round_number):
    to_node = _port_neighbor(G, from_node, out_port)

    for aid in list(agent_ids):
        _move_agent(G, agents, aid, from_node, out_port, round_number)

    _snapshot(f"move_group(from={from_node},p={out_port},to={to_node},|A|={len(agent_ids)})", G, agents, round_number)
    return to_node


def can_vacate(G, agents: List["Agent"], x, psi_x, A_vacated, round_number):
    _snapshot(f"can_vacate:enter(x={x}, psi={psi_x.ID})", G, agents, round_number)
    round_number+=1

    if psi_x.parentPort is None:
        _snapshot(f"can_vacate:exit(x={x})", G, agents, round_number)
        return "settled", 2

    if ((psi_x.nodeType == "visited") or ((psi_x.nodeType == "fullyVisited") and (psi_x.vacatedNeighbor is False))):
        w, p_wx = _move_agent(G, agents, psi_x.ID, x, PORT_ONE, round_number)
        xi_w_id = _xi_id(G, w, {psi_x.ID}, agents)
        if xi_w_id is not None:
            psi_w_id = xi_w_id
            agents[psi_w_id].vacatedNeighbor = True
            psi_x.portAtP1Neighbor = p_wx
            psi_x.P1Neighbor = psi_w_id
            _move_agent(G, agents, psi_x.ID, w, p_wx,round_number+1)
            _snapshot(f"can_vacate:exit(x={x})", G, agents, round_number+2)
            return "settledScout", 4
        _move_agent(G, agents, psi_x.ID, w, p_wx, round_number+1)
        _snapshot(f"can_vacate:exit(x={x})", G, agents, round_number+2)
        return "settled", 4

    if psi_x.nodeType == "partiallyVisited":
        _snapshot(f"can_vacate:exit(x={x})", G, agents, round_number)
        return "settledScout", 2

    if psi_x.portAtParent == PORT_ONE:
        z, _ = _move_agent(G, agents, psi_x.ID, x, psi_x.parentPort, round_number)
        psi_z_id = _xi_id(G, z, {psi_x.ID}, agents)
        if psi_z_id is None:  ################
            _move_agent(G, agents, psi_x.ID, z, psi_x.portAtParent, round_number+1)  ################
            _snapshot(f"can_vacate:exit(x={x})", G, agents, round_number+2)  ################
            return "settled", 4  ################
        psi_z = agents[psi_z_id]
        if psi_z.vacatedNeighbor==False:
            psi_z.state = "settledScout"
            psi_z.portAtP1Neighbor = psi_x.parentPort
            psi_z.P1Neighbor = psi_x.ID
            A_vacated.add(psi_z.ID)
            _move_group(G, agents, {psi_x.ID, psi_z.ID}, z, psi_x.portAtParent, round_number+1)
            psi_x.vacatedNeighbor = True
            _snapshot(f"can_vacate:exit(x={x})", G, agents, round_number+2)
            return "settled", 4
        else:
            _move_agent(G, agents, psi_x.ID, z, psi_x.portAtParent, round_number+1)
            _snapshot(f"can_vacate:exit(x={x})", G, agents, round_number+2)
            return "settled", 4
        
    _snapshot(f"can_vacate:exit(x={x})", G, agents, round_number)
    return "settled", 2


def parallel_probe(G, agents: List["Agent"], x, psi_x, A_scout, round_number_og_og):
    _snapshot(f"parallel_probe:enter(x={x})", G, agents, round_number_og_og)
    round_number_og_og+=1

    psi_x.probeResultsByPort = {}
    psi_x.probeResult = None
    psi_x.checked = 0
    delta_x = G.degree[x]
    rounds_max = 0
    while psi_x.checked < delta_x:
        A_scout = sorted(A_scout)
        s = len(A_scout)
        Delta_prime = min(s, delta_x - psi_x.checked)
        j = 0
        jk = 0
        round_number_og = round_number_og_og+rounds_max
        while j<Delta_prime:
            round_number = round_number_og
            port = j + psi_x.checked
            if psi_x.parentPort is not None and port == psi_x.parentPort:
                j += 1
                Delta_prime = min(s + 1, delta_x - psi_x.checked)
                continue
            a = agents[A_scout[jk]]
            a.scoutPort = port
            y, a.returnPort = _move_agent(G, agents, a.ID, x, a.scoutPort, round_number)
            round_number+=1
            a.scoutEdgeType = edge_type(G, x, y)
            xi_y_id = _xi_id(G, y, set(A_scout), agents)
            if xi_y_id is not None:
                psi_y_id = xi_y_id
                _move_agent(G, agents, a.ID, y, a.returnPort, round_number)
                round_number+=1
            else:
                if False:
                    pass
                else:
                    z, a.returnreturnPort = _move_agent(G, agents, a.ID, y, PORT_ONE, round_number)
                    round_number+=1
                    xi_z_id = _xi_id(G, z, set(A_scout), agents)
                    if xi_z_id is not None:
                        _move_agent(G, agents, a.ID, z, a.returnreturnPort, round_number)
                        _move_agent(G, agents, a.ID, y, a.returnPort, round_number+1)
                        round_number+=2
                        b_id = next((bid for bid in A_scout if agents[bid].P1Neighbor == xi_z_id and agents[bid].portAtP1Neighbor == G[z][y][f"port_{z}"]), None)
                        if b_id is not None:
                            psi_y_id = b_id
                        else:
                            psi_y_id = None
                    else:
                        if (G[z][y][f"port_{z}"]==PORT_ONE):
                            psi_y_id = None
                            _move_agent(G, agents, a.ID, z, a.returnreturnPort, round_number)
                            _move_agent(G, agents, a.ID, y, a.returnPort, round_number+1)
                            round_number+=2
                        else:
                            w, a.returnreturnreturnPort = _move_agent(G, agents, a.ID, z, PORT_ONE, round_number)
                            round_number+=1
                            xi_w_id = _xi_id(G, w, set(A_scout), agents)
                            a.scoutP1P1Neighbor = _xi_id(G, w, set(A_scout), agents)
                            a.scoutPortAtP1P1Neighbor = G[w][z][f"port_{w}"]
                            if _xi_id(G, w, set(A_scout), agents) is None:
                                _move_agent(G, agents, a.ID, w, a.returnreturnreturnPort, round_number)
                                _move_agent(G, agents, a.ID, z, a.returnreturnPort, round_number+1)
                                _move_agent(G, agents, a.ID, y, a.returnPort, round_number+2)
                                round_number+=3
                                psi_y_id = None
                            else:
                                _move_agent(G, agents, a.ID, w, a.returnreturnreturnPort, round_number)
                                _move_agent(G, agents, a.ID, z, a.returnreturnPort, round_number+1)
                                _move_agent(G, agents, a.ID, y, a.returnPort, round_number+2)
                                round_number+=3
                                c_id = next((cid for cid in A_scout if agents[cid].P1Neighbor == xi_w_id and agents[cid].portAtP1Neighbor == G[w][z][f"port_{w}"]), None)
                                if c_id is not None:
                                    b_id = next((bid for bid in A_scout if agents[bid].P1Neighbor == c_id and agents[bid].portAtP1Neighbor == G[z][y][f"port_{z}"]), None)
                                    if b_id is not None:
                                        psi_y_id = b_id
                                    else:
                                        psi_y_id = None
                                else:
                                    psi_y_id = None

            a.scoutResult = (G[x][y][f"port_{x}"], a.scoutEdgeType, (agents[psi_y_id].nodeType if psi_y_id is not None else "unvisited"), psi_y_id)
            psi_x.probeResultsByPort[G[x][y][f"port_{x}"]] = a.scoutResult
            j+=1
            jk+=1
            rounds_max = max(rounds_max, round_number-round_number_og_og)

        psi_x.checked = psi_x.checked+Delta_prime
        results = list(psi_x.probeResultsByPort.values())
        best = min(results, key=_candidate_rank, default=None)
        if best is None or _candidate_rank(best)[0] == 99:
            psi_x.probeResult = None
        else:
            psi_x.probeResult = best

    _snapshot(f"parallel_probe:exit", G, agents, round_number_og_og+rounds_max)
    return (psi_x.probeResult[0] if psi_x.probeResult is not None else None), (rounds_max+2)


def retrace(G, agents, A_vacated, round_number, max_rounds):
    _snapshot("retrace:enter", G, agents, round_number)
    round_number+=1
    siblingDetails = None
    nextAgentID = None
    nextPort = None
    while A_vacated:
        if round_number>max_rounds:
            raise RuntimeError("Round limit exceeded in retrace")
        amin_id = min(A_vacated)
        amin = agents[amin_id]
        v = amin.node
        xi_v_id = _xi_id(G, v, set(), agents)
        psi_v_id = xi_v_id
        if xi_v_id is None:
            target_id = nextAgentID
            a = agents[target_id]
            a.state = "settled"
            A_vacated.discard(target_id)
            amin_id = min(A_vacated)
            amin = agents[amin_id]
            psi_v_id = a.ID

        if not A_vacated:
            break

        psi_v = agents[psi_v_id]
        if psi_v.recentChild is not None:
            if psi_v.recentChild == amin.arrivalPort:
                if siblingDetails is None:
                    psi_v.recentChild = None
                    nextAgentID, nextPort = psi_v.parentID, psi_v.parentPort
                    siblingDetails = psi_v.sibling
                    if (nextPort is None):
                        raise RuntimeError(f"{A_vacated} -- {amin.ID} nextport is None, psi_v is {psi_v.ID}, home is {psi_v.home}, stat is {psi_v.state}")
                else:
                    nextAgentID, nextPort = siblingDetails
                    siblingDetails = None
                    psi_v.recentChild = nextPort
            else:
                nextPort = psi_v.recentChild
                found = None
                for aid in A_vacated:
                    if (agents[aid].parentID, agents[aid].portAtParent) == (psi_v.ID, psi_v.recentChild):
                        found = aid
                        break
                if found is not None:
                    nextAgentID = found
                    nextPort = psi_v.recentChild
        else:
            parentID = psi_v.parentID
            nextAgentID = parentID
            nextPort = psi_v.parentPort
            siblingDetails = psi_v.sibling

        A_vacated.discard(psi_v.ID)
        psi_v.state = "settled"
        _move_group(G, agents, A_vacated, v, nextPort, round_number)
        round_number+=1

    _snapshot("retrace:exit", G, agents, round_number)
    round_number+=1
    return round_number


def rooted_async(G, agents, root_node, max_rounds):
    _snapshot(f"rooted_async:enter(root={root_node})", G, agents, 0)
    round_number = 1
    A = set(agents.keys())
    A_unsettled = set(A)
    A_vacated = set()
    siblingDetails = None
    childDetails = None
    while A_unsettled:
        if round_number>max_rounds:
            raise RuntimeError("Round limit exceeded in rooted async")
        v = agents[min(A_unsettled | A_vacated)].node
        A_scout = set(A_unsettled) | set(A_vacated)
        amin = agents[min(A_scout)]
        psi_v_id = _xi_id(G, v, {}, agents)
        if psi_v_id is None:
            candidates = [aid for aid in A_unsettled if agents[aid].node == v]
            if not candidates:
                raise RuntimeError(f"No unsettled agent at v={v} to settle (this breaks invariants).")
            psi_v_id = max(candidates)
            psi_v = agents[psi_v_id]
            psi_v.state = "settled"
            psi_v.home = psi_v.node
            if amin.prevID is None:
                psi_v.parentID = None
                psi_v.parentPort = None
                psi_v.portAtParent = None
            else:
                if amin.arrivalPort is None:
                    raise RuntimeError(f"Settling {psi_v.ID}@{v} but amin.arrivalPort=None (non-root).")
                if amin.childPort is None and amin.prevID is not None:
                    prev = agents[amin.prevID]
                    amin.childPort = prev.recentPort
                if amin.childPort is None:
                    raise RuntimeError(
                        f"Settling {psi_v.ID}@{v} but amin.childPort=None "
                        f"(prevID={amin.prevID}, arrivalPort={amin.arrivalPort})."
                    )
                psi_v.parentPort = amin.arrivalPort
                psi_v.parentID = amin.prevID
                psi_v.portAtParent = amin.childPort
            amin.childPort = None
            A_unsettled.remove(psi_v_id)
            A_scout = set(A_unsettled) | set(A_vacated)
            _snapshot(f"rooted_async:settled(psi={psi_v_id},v={v})", G, agents, round_number)
            round_number+=1
            if not A_unsettled:
                psi_v.sibling = siblingDetails
                break
        psi_v = agents[psi_v_id]
        amin.prevID = psi_v.ID
        k = len(A)
        delta_v = G.degree[v]
        if delta_v >= k - 1:
            print("shortcut taken")
            _, rounds_max = parallel_probe(G, agents, v, agents[psi_v_id], A_scout, round_number)
            round_number+=rounds_max
            probe_items = sorted(agents[psi_v_id].probeResultsByPort.items(), key=lambda kv: kv[0])
            empty_ports = [sr[0] for _, sr in probe_items[: (k - 1)] if sr and sr[3] is None]
            movers = sorted(A_unsettled - {psi_v_id})
            if len(empty_ports) >= len(movers):
                for aid, out_port in zip(movers, empty_ports):
                    y, _ = _move_agent(G, agents, aid, v, out_port, round_number)
                    agents[aid].state = "settled"
                    agents[aid].home = y
                    A_unsettled.discard(aid)
                round_number+=1
                break

        nextPort, rounds_max = parallel_probe(G, agents, v, psi_v, A_scout, round_number)
        round_number+=rounds_max
        scout_results = list(psi_v.probeResultsByPort.values())
        update_node_type_after_probe(G, v, psi_v, scout_results)
        psi_v.state, rounds_max = can_vacate(G, agents, v, psi_v, A_vacated, round_number)
        round_number+=rounds_max
        if psi_v.state=="settled":
            A_unsettled.discard(psi_v.ID)
            A_vacated.discard(psi_v.ID)
            A_scout = set(A_unsettled) | set(A_vacated)
        if psi_v.state == "settledScout":
            A_vacated.add(psi_v.ID)
            A_scout = set(A_unsettled) | set(A_vacated)
        
        if nextPort is not None:
            psi_v.recentPort = nextPort
            amin.childPort = nextPort
            if psi_v.recentChild is None:
                psi_v.sibling = siblingDetails
                siblingDetails = None
                psi_v.recentChild = nextPort
            else:
                siblingDetails = childDetails
                childDetails = None
                psi_v.recentChild = nextPort
            w = _move_group(G, agents, A_scout, v, nextPort, round_number)
            _snapshot(f"rooted_async:move_forward(v={v},p={nextPort})", G, agents, round_number)
            round_number+=1
            # psi_w_id = _xi_id(G, w, exclude_ids=set(), agents=agents)
            # if psi_w_id is not None:
            #     psi_w = agents[psi_w_id]
            #     arrival_port_at_w = amin.arrivalPort
            #     reconfigure_if_needed(agents, psi_v, nextPort, psi_w, arrival_port_at_w)
        else:
            if psi_v.parentPort is None:
                if A_unsettled:
                    raise RuntimeError(
                        f"Stuck at root v={v} with nextPort=None but A_unsettled still nonempty: {sorted(A_unsettled)}"
                    )
            childDetails = (psi_v.ID, psi_v.portAtParent)
            if psi_v.recentChild is None:
                psi_v.sibling = siblingDetails
            siblingDetails = None
            amin.childPort = None
            psi_v.recentPort = psi_v.parentPort
            _move_group(G, agents, A_scout, v, psi_v.parentPort, round_number)
            _snapshot(f"rooted_async:backtrack(v={v},p={psi_v.parentPort})", G, agents, round_number)
            round_number+=1

    round_number = retrace(G, agents, A_vacated, round_number, max_rounds)
    _snapshot("rooted_async:exit", G, agents, round_number)
    round_number+=1


def run_simulation(G, agents, max_rounds=-1):
    if len(agents)>len(G):
        raise RuntimeError("Agents should not be more than nodes")
    max_rounds = 40*len(agents)
    simmer.clearr()
    for u in G.nodes():
        G.nodes[u]["agents"] = set()
    if isinstance(agents, list):
        agents = {a.ID: a for a in agents}
    for aid, a in agents.items():
        G.nodes[a.node]["agents"].add(aid)

    root_node = agents[sorted(agents.keys())[0]].node #For rooted only
    rooted_async(G, agents, root_node, max_rounds)
    # try:
    #     rooted_async(G, agents, root_node, max_rounds)
    # except:
    #     pass

    return (simmer.all_positions, simmer.all_statuses, simmer.all_node_states, simmer.all_homes, simmer.all_tree_edges)