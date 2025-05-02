from collections import defaultdict

def get_dict_key(d, value):
    for k, v in d.items():
        if v == value:
            return k
    return str(value)

AgentStatus = {
    # status of the agent
    "SETTLED": 0,
    "UNSETTLED": 1,
    "SETTLED_WAIT": 2
}

AgentRole = {
    # role of the agent
    "LEADER": 0,
    "FOLLOWER": 1,
    "HELPER": 2,
    "CHASER": 3
}

NodeStatus = {
    # a node can be empty or occupied by a settled agent
    "EMPTY": 0,
    "OCCUPIED": 1
}

class Agent:
    """Class of agents. Contains the functions and variables stored at each agent."""
    def __init__(self, id, initial_node):
        self.id = id
        self.currentnode = initial_node
        self.round_number = 0
        self.state = {
            "status": AgentStatus["UNSETTLED"],
            "role": AgentRole["LEADER"],
            "level": 0,
            "leader": self,
            "home": None # assigned a node when agent settles.
        }
        self.pin = None
        self.next = None
        self.scout_forward = None
        self.scout_return = None
        self.scout_port = None
        self.scout_result = None
        self.scout_return_port = None
        self.checked_port = None
        self.max_scouted_port = None
        self.checked_result = None
        self.parent_port = None
        self.help_port = None
        self.help_return_port = None
        self.going_to_help = False

    def reset(self, leader, level, role, status):
        self.state['status'] = status
        self.state['role'] = role
        self.state['level'] = level
        self.state['leader'] = leader
        self.pin = None
        self.next = None
        self.scout_forward = None
        self.scout_return = None
        self.scout_port = None
        self.scout_result = None
        self.scout_return_port = None
        self.checked_port = None
        self.max_scouted_port = None
        self.checked_result = None
        self.parent_port = None
        self.help_port = None
        self.help_return_port = None
        self.going_to_help = False

def get_agent_positions_and_statuses(G, agents):
    positions = [a.currentnode for a in agents]
    statuses  = [a.state['status'] for a in agents]
    return positions, statuses

def elect_leader(G, agents):
    leaders = set()
    for a in agents:
        if a.state['role'] == AgentRole['LEADER']:
            leaders.add((a.state['level'],a))
    if len(leaders) == 0:
        print(f"No leader found at node {agents[0].currentnode}")
        print(f"Agents at node {agents[0].currentnode}: {[(a.id, a.state['level'], a.state['role'], a.state['status']) for a in agents]}")
        return
    elif len(leaders) == 1:
        leader = leaders.pop()
        print(f"Only one leader: {leader[1].id, leader[0]} at node {leader[1].currentnode} with level {leader[0]}")
        return leader[1]
    else:
        sorted_leaders = sorted(leaders, key=lambda x: (-x[0], x[1].id))
        leader = sorted_leaders[0]
        print(f"Leader elected: {leader[1].id, leader[0]} at node {leader[1].currentnode} with level {leader[0]}")
        return leader[1]

def increase_level(G, agents, leader):
    max_level = max(a.state['level'] for a in agents)
    if leader.state['level'] < max_level:
        # everyone becomes a chaser except the max level settled agent
        # find the max level agent
        max_agent = None
        for a in agents:
            if a.state['level'] == max_level:
                max_agent = a
                break
        if max_agent.state['status'] != AgentStatus['SETTLED']:
            print(f"Max agent {max_agent.id} is not settled")
            raise Exception("Max agent not settled")
        # make everyone a chaser with level set to max_level and role set to chaser and leader is the leader of the max agent
        for a in agents:
            if a != max_agent:
                a.reset(max_agent.state['leader'], max_level, AgentRole['CHASER'], AgentStatus['UNSETTLED'])
                print(f"Agent {a.id} changed leader to {max_agent.state['leader'].id, max_level} and role to CHASER {a.state['role']}")
        print(f"Leader {leader.id} is not max level agent, making all agents a chaser for leader {max_agent.state['leader'].id, max_level}")

    elif leader.state['level'] == max_level:
        # find the number of agents with the same level
        max_level_agents = [a for a in agents if a.state['level'] == max_level and a.state['leader']!= leader]
        if leader not in max_level_agents:
            max_level_agents.append(leader)
        if len(max_level_agents) > 1:
            # increase level of all agents to max_level + 1
            for a in agents:
                a.state['level'] = max_level + 1
                a.state['leader'] = leader
                if a != leader:
                    a.reset(leader, max_level + 1, AgentRole['FOLLOWER'], AgentStatus['UNSETTLED'])
            G.nodes[leader.currentnode]['settled_agent'] = None
            print(f"Leader {leader.id} is not unique max level agent, increasing level of all agents to {max_level + 1}")
        else:
            print(f"Leader {leader.id, leader.state['level']} is unique max level agent that is a leader already at max level {max_level}")
            # check for the leader of other agents
            G.nodes[leader.currentnode]['settled_agent'] = None
            for a in agents:
                a.state['level'] = max_level
                if a.state['leader'] != leader:
                    a.reset(leader, max_level, AgentRole['FOLLOWER'], AgentStatus['UNSETTLED'])
                    print(f"Agent {a.id} changed leader to {leader.id, leader.state['level']}")
    else:
        print(f"Leader {leader.id} is unique max level agent, no need to increase level")

def settle_an_agent(G, agent):
    print(f"Checking for settled agent at Node {agent.currentnode}")
    settled_agent = G.nodes[agent.currentnode]['settled_agent']
    if settled_agent is None:
        # settle an agent
        agents_at_node = G.nodes[agent.currentnode]['agents']
        if len(agents_at_node) == 1:
            print(f"Leader {agent.id} is alone at node {agent.currentnode}, becomes settled_wait")
            agent.state['status'] = AgentStatus['SETTLED_WAIT']
            agent.parent_port = agent.pin
            G.nodes[agent.currentnode]['settled_agent'] = agent
            agent.checked_port = None
            agent.max_scouted_port = None
        else:
            non_leader_agents = [a for a in agents_at_node if a.state['role'] != AgentRole['LEADER']]
            max_id_agent = max(non_leader_agents, key=lambda x: x.id)
            max_id_agent.state['status'] = AgentStatus['SETTLED']
            max_id_agent.parent_port = agent.pin
            G.nodes[agent.currentnode]['settled_agent'] = max_id_agent
            print(f"Leader {agent.id, agent.state['level']} settled {max_id_agent.id} at node {agent.currentnode}")
            agent.checked_port = None
            agent.max_scouted_port = None
    else:
        print(f"Settled agent {settled_agent.id} at node {agent.currentnode} is already settled")
        # check if the settled agent has the same leader
        if settled_agent.state['leader'] != agent:
            print(f"Settled agent {settled_agent.id} at node {agent.currentnode} has different leader {settled_agent.state['leader'].id}")
            raise Exception("Settled agent has different leader")
    return settled_agent

def move_to_scout(G, agent):
    if agent.state['role'] != AgentRole['HELPER']:
        raise Exception("Agent is not a helper")
    if agent.state['status'] != AgentStatus['SETTLED']:
        raise Exception("Agent is not settled")
    if agent.help_port is None:
        raise Exception("No help port found for agent")
    help_node = G.nodes[agent.currentnode]['port_map'][agent.help_port]
    if help_node is None:
        raise Exception("No help node found for agent")
    else:
        agent.going_to_help = True
        print(f"Agent {agent.id} moved forward to help node {help_node} via help port {agent.help_port}")
        # move to the help node
        G.nodes[agent.currentnode]['agents'].remove(agent)
        agent.pin = G[agent.currentnode][help_node][f"port_{help_node}"]
        agent.currentnode = help_node
        G.nodes[help_node]['agents'].add(agent)
    return

def ensure_scout(G, agent):
    settled_agent = G.nodes[agent.currentnode]['settled_agent']
    if settled_agent is None or settled_agent.state['leader']!= agent.state['leader'] or agent.state['leader'].currentnode != agent.currentnode:
        print(f"Help mismatch {agent.currentnode}")
        agent.help_port = None
        home_node = G.nodes[agent.currentnode]['port_map'][agent.help_return_port]
        if home_node is None:
            raise Exception("No home node found for agent")
        else:
            print(f"Agent {agent.id} moved back to home {home_node} via help return port {agent.help_return_port} because no settled agent")
            # move to the home
            G.nodes[agent.currentnode]['agents'].remove(agent)
            agent.currentnode = home_node
            G.nodes[home_node]['agents'].add(agent)
            agent.help_return_port = None
            agent.going_to_help = False
            agent.state['role'] = AgentRole['FOLLOWER']
        return

def helper_return(G, agent):
    agent.going_to_help = False
    if agent.state['role'] != AgentRole['HELPER']:
        raise Exception("Agent is not a helper")
    if agent.state['status'] != AgentStatus['SETTLED']:
        raise Exception("Agent is not settled")
    if agent.help_port is None:
        agent.state['role'] = AgentRole['FOLLOWER']
        print(f"Agent {agent.id} is not a helper anymore")
    home_node = G.nodes[agent.currentnode]['port_map'][agent.help_return_port]
    if home_node is None:
        raise Exception("No home node found for agent")
    else:
        print(f"Agent {agent.id} returned back to home {home_node} via help port {agent.help_return_port}")
        # move to the home
        G.nodes[agent.currentnode]['agents'].remove(agent)
        agent.currentnode = home_node
        G.nodes[home_node]['agents'].add(agent)
    return

def scout_forward(G, agent):
    unsettled_agents = list(G.nodes[agent.currentnode]['agents'])
    settled_agent = G.nodes[agent.currentnode]['settled_agent']
    if settled_agent is None:
        raise Exception("No settled agent at node")
    if settled_agent is not None and settled_agent in unsettled_agents:
        print(f"Scouting: Settled agent {settled_agent.id} at node {agent.currentnode}")
        unsettled_agents.remove(settled_agent)
    if len(unsettled_agents) == 0:
        print(f"No unsettled agents at node {agent.currentnode}")
        return
    else:
        print(f"Unsettled agents at node {agent.currentnode}: {[a.id for a in unsettled_agents]}")
        # assign the scout ports to the unsettled agents increasing order of their ids starting from the checked port
        if settled_agent is None:
            raise Exception("No settled agent at node")
        checked_port = settled_agent.checked_port
        if checked_port is None:
            checked_port = -1
            settled_agent.max_scouted_port = -1
        unsettled_agents.sort(key=lambda x: x.id)
        for i, a in enumerate(unsettled_agents):
            a.scout_port = (checked_port + i) + 1
            if a.scout_port < G.degree[agent.currentnode]:
                a.scout_forward = True
                print(f"Unsettled agent {a.id} at node {agent.currentnode} assigned scout port {a.scout_port}")
                if a.scout_port > settled_agent.max_scouted_port:
                    settled_agent.max_scouted_port = a.scout_port
            else:
                print(f"Unsettled agent {a.id} at node {agent.currentnode} not assigned a scout port as it exceeds degree")
                a.scout_port = None
                settled_agent.max_scouted_port = G.degree[agent.currentnode] - 1
                break
    return

def scout_neighbor(G, agent):
    agent.scout_forward = False
    agent.scout_return = True
    if agent.scout_port is None:
        return
    if agent.scout_port is not None:
        neighbor = G.nodes[agent.currentnode]["port_map"][agent.scout_port]
        if neighbor is None:
            raise Exception("No neighbor found for scout port")
        else:
            print(f"Agent {agent.id} moved to neighbor {neighbor} via scout port {agent.scout_port}")
            # move to the neighbor
            agent.pin = G[agent.currentnode][neighbor][f"port_{neighbor}"]
            G.nodes[agent.currentnode]['agents'].remove(agent)
            agent.currentnode = neighbor
            G.nodes[neighbor]['agents'].add(agent)
            agent.scout_return_port = agent.pin
            settled_agent = G.nodes[neighbor]['settled_agent']
            if settled_agent is None:
                print(f"No settled agent at node {neighbor}")
                agent.scout_result = NodeStatus['EMPTY']
            else:
                print(f"Settled agent {settled_agent.id} found at node {neighbor}")
                if settled_agent.state['leader'] == agent.state['leader'] and settled_agent.state['level'] == agent.state['level']:
                    print(f"Settled agent {settled_agent.id} at node {neighbor} has the same leader and level as agent {agent.id}")
                    agent.scout_result = NodeStatus['OCCUPIED']
                    # recruit the settled agent as helper
                    settled_agent.state['role'] = AgentRole['HELPER']
                    settled_agent.help_port = agent.scout_return_port
                    settled_agent.help_return_port = agent.scout_port
                else:
                    print(f"Settled agent {settled_agent.id} at node {neighbor} has different leader or level than agent {agent.id}")
                    agent.scout_result = NodeStatus['EMPTY']
    return

def scout_return(G, agent):
    home = G.nodes[agent.currentnode]['port_map'][agent.scout_return_port]
    if home is None:
        raise Exception("No home found for scout return port")
    else:
        print(f"Agent {agent.id} moved back to home {home} via scout return port {agent.scout_return_port}")
        # move to the home
        G.nodes[agent.currentnode]['agents'].remove(agent)
        agent.currentnode = home
        G.nodes[home]['agents'].add(agent)
    return

def chase_leader(G, agent):
    print(f"Chasing leader, Agent {agent.id} at node {agent.currentnode} chasing leader {agent.state['leader'].id, agent.state['level']}")
    settled_agent = G.nodes[agent.currentnode]['settled_agent']
    if settled_agent is None:
        raise Exception("No settled agent at node")
    leader_match = agent.state['leader'] == settled_agent.state['leader'] and agent.state['level'] == settled_agent.state['level']
    if leader_match:
        leader_position_match = agent.currentnode == settled_agent.state['leader'].currentnode
        print(f"Leader match, Agent {agent.id} at node {agent.currentnode} has the same leader and level as settled agent {settled_agent.id}. The leader is at node {settled_agent.state['leader'].currentnode}")
        if leader_position_match:
            print(f"Reached the leader at node {agent.currentnode} where {settled_agent.id} is settled and leader of settled agent is {settled_agent.state['leader'].id} and leader's position is {settled_agent.state['leader'].currentnode}")
            agent.state['role'] = AgentRole['FOLLOWER']
        else:
            print(f"Chasing leader {agent.state['leader'].id} by moving to next node by port {settled_agent.next}")
            next_node = G.nodes[agent.currentnode]['port_map'][settled_agent.next]
            if next_node is None:
                raise Exception("No next node found for agent")
            else:
                agent.pin = G[agent.currentnode][next_node][f"port_{next_node}"]
                G.nodes[agent.currentnode]['agents'].remove(agent)
                agent.currentnode = next_node
                G.nodes[next_node]['agents'].add(agent)


def check_scout_result(G, agent):
    settled_agent = G.nodes[agent.currentnode]['settled_agent']
    if settled_agent.state['status'] == AgentStatus['SETTLED_WAIT']:
        print(f"Settled agent {settled_agent.id} at node {agent.currentnode} is waiting")
        return
    if settled_agent is None:
        raise Exception("No settled agent at node")
    empty_ports = []
    for a in G.nodes[agent.currentnode]['agents']:
        if a.scout_return == True:
            a.scout_return = False
            if a.scout_result == NodeStatus['EMPTY']:
                a.scout_result = None
                empty_ports.append(a.scout_port)
                print(f"Adding empty port by Agent {a.id} found empty port {a.scout_port} at node {agent.currentnode}")
    if len(empty_ports) > 0:
        empty_port = min(empty_ports)
        print(f"In if, Leader agent {agent.id} found empty port {empty_port} at node {agent.currentnode}")
        settled_agent.checked_port = empty_port
        # return helper agents
        for a in G.nodes[agent.currentnode]['agents']:
            if a.state['role'] == AgentRole['HELPER']:
                a.help_port = None
                print(f"Helper agent {a.id} not needed anymore.")
    else:
        print(f"Leader agent {agent.id} found no empty port at node {agent.currentnode} and {settled_agent.max_scouted_port} is the max scouted port. Update checked port!!")
        empty_port = None
        if settled_agent.max_scouted_port < G.degree[agent.currentnode] - 1:
            settled_agent.checked_port = settled_agent.max_scouted_port
        else:
            settled_agent.checked_port = None
            # return to parent
            if settled_agent.parent_port is None:
                raise Exception("No parent port found for settled agent")
            else:
                empty_port = settled_agent.parent_port
    agent.next = empty_port
    return

def follow_leader(G, agent):
    if agent.next is None:
        print(f"Agent {agent.id} has no next port to follow")
        return
    unsettled_agents = [a for a in G.nodes[agent.currentnode]['agents'] if a.state['leader'] == agent and a.state['status'] == AgentStatus['UNSETTLED']]
    settled_agent = G.nodes[agent.currentnode]['settled_agent']
    if settled_agent is None:
        raise Exception("No settled agent at node")
    if settled_agent is not None and settled_agent in unsettled_agents:
        print(f"Following leader, Settled agent {settled_agent.id} remains at node {agent.currentnode}")
        unsettled_agents.remove(settled_agent)
    if len(unsettled_agents) == 0:
        print(f"No unsettled agents at node {agent.currentnode}")
        return
    else:
        print(f"Unsettled agents at node {agent.currentnode}: {[a.id for a in unsettled_agents]} follow leader {agent.id} to port {agent.next}")
        settled_agent.next = agent.next
        # all unsettled agents including the leader to the next port
        next_node = G.nodes[agent.currentnode]['port_map'][agent.next]
        if next_node is None:
            raise Exception("No next node found for agent")
        else:
            for a in unsettled_agents:
                print(f"Follow leader Agent {a.id} moved to next node {next_node} from node {a.currentnode} via port {agent.next}. Leader is {agent.id} at node {agent.currentnode}")
                a.pin = G[a.currentnode][next_node][f"port_{next_node}"]
                G.nodes[a.currentnode]['agents'].remove(a)
                a.currentnode = next_node
                G.nodes[next_node]['agents'].add(a)
    return

def get_safe_attr(obj, attr, default=None):
    """Safely get an attribute from an object, returning default if missing."""
    return getattr(obj, attr, default)

def run_simulation(G, agents, max_degree, rounds, starting_positions):
    all_positions = []
    all_statuses = []
    all_leader_ids = []
    all_leader_levels = []
    all_node_settled_states = []

    round_number = 1

    def snapshot(label):
        positions, statuses = get_agent_positions_and_statuses(G, agents)
        current_node_states = {}
        for node_id in G.nodes():
            settled_agent = G.nodes[node_id]['settled_agent']
            if settled_agent is not None:
                # Collect desired attributes safely
                # Note: 'next' is often associated with the leader's decision,
                # but stored/used by the settled agent for followers.
                state_dict = {
                    "settled_agent_id": settled_agent.id,
                    # Use get_safe_attr for robustness if attributes might not exist yet
                    "parent_port": get_safe_attr(settled_agent, 'parent_port'),
                    "checked_port": get_safe_attr(settled_agent, 'checked_port'),
                    "max_scouted_port": get_safe_attr(settled_agent, 'max_scouted_port'),
                    "next_port": get_safe_attr(settled_agent, 'next') # Collect the 'next' port known by settled agent
                    # Add any other attributes you might want later
                }
                current_node_states[str(node_id)] = state_dict # Use string key for JSON
            else:
                current_node_states[str(node_id)] = None # Indicate no settled agent

        # node → status → [agent-ids]
        grouped = defaultdict(lambda: defaultdict(list))
        for agent, node, st in zip(agents, positions, statuses):
            grouped[node][st].append(agent.id)

        # pretty print
        for node, status_map in grouped.items():
            # Skip printing if there is only one settled robot at the node
            if len(status_map.get(AgentStatus["SETTLED"], [])) == 1 and len(status_map) == 1:
                continue
            parts = [
            f"{get_dict_key(AgentStatus, st)}: {ids}"
            for st, ids in status_map.items()
            ]
            print(f"label {label}, node {node} → " + " | ".join(parts))

        all_positions.append((label, positions))
        all_statuses.append((label, statuses))
        all_node_settled_states.append((label, current_node_states))
        # capture the current leader‐ID and level for each agent
        leader_ids    = [a.state['leader'].id    for a in agents]
        leader_levels = [a.state['level']         for a in agents]
        all_leader_ids.append((label, leader_ids))
        all_leader_levels.append((label, leader_levels))
        return positions, statuses

    # Initialization (outside main loop, before starting rounds)
    for node in G.nodes():
        G.nodes[node]['last_election_round'] = -1

    for a in agents:
        G.nodes[a.currentnode]['agents'].add(a)

    positions, statuses = snapshot("start")
    old_positions = positions
    old_statuses = statuses

    max_rounds = rounds
    print(f"max_rounds: {max_rounds}")

    while any(s == AgentStatus['UNSETTLED'] for s in statuses) and round_number <= max_rounds:

        round_number += 1
        print(f'------\nround Number {round_number - 1}\n------')


        agents_by_node = defaultdict(list)
        for a in agents:
            agents_by_node[a.currentnode].append(a)

        for node, agents_at_node in agents_by_node.items():
            if len(agents_at_node) > 1:
                print(f'Electing leader at {node}: {[a.id for a in agents_at_node]}')
                leader = elect_leader(G, agents_at_node)
                print(f'Increase level at {node}: {[(a.id,(a.state['leader'].id,a.state['level']) ) for a in agents_at_node]}')
                if leader is not None:
                    increase_level(G, agents_at_node, leader)
        # positions, statuses = snapshot("elect_leader")

        for a in agents:
            if a.state['role'] == AgentRole['LEADER']:
                settle_an_agent(G, a)
        # positions, statuses = snapshot("settle_an_agent")

        for a in agents:
            if a.state['role'] == AgentRole['HELPER']:
                move_to_scout(G, a)
        positions, statuses = snapshot("move_to_scout")

        for a in agents:
            if a.state['role'] == AgentRole['HELPER']:
                ensure_scout(G, a)
            if a.state['role'] == AgentRole['LEADER']:
                scout_forward(G, a)
        positions, statuses = snapshot("scout_forward") # unnecessary! remove later

        for a in agents:
            if a.scout_forward == True:
                scout_neighbor(G, a)
        positions, statuses = snapshot("scout_neighbor")

        for a in agents:
            if a.scout_return == True:
                scout_return(G, a)
        positions, statuses = snapshot("scout_return")

        for a in agents:
            if a.state['role'] == AgentRole['LEADER']:
                check_scout_result(G, a)
        positions, statuses = snapshot("check_scout_result")

        for a in agents:
            if a.going_to_help == True:
                helper_return(G, a)
        positions, statuses = snapshot("helper_return")

        for a in agents:
            if a.state['role'] == AgentRole['CHASER']:
                chase_leader(G, a)
        positions, statuses = snapshot("chase_leader")

        for a in agents:
            if a.state['role'] == AgentRole['LEADER']:
                follow_leader(G, a)
        positions, statuses = snapshot("follow_leader")

        if positions == old_positions and statuses == old_statuses:
            print(f'round Number {round_number - 1}: No change in positions and statuses')
            print(positions)
            print(statuses)
            repeat_count += 1
        else:
            repeat_count = 0
        if repeat_count > 10:
            break

        old_positions = positions
        old_statuses = statuses

    return all_positions, all_statuses, all_leader_ids, all_leader_levels, all_node_settled_states


