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

AgentPhase = {
    # phase of the agent
    "EXPLORE": 0,
    "IDLE": 1,
    "SCOUT_FORWARD": 2,
    "SCOUT_RETURN": 3,
    "JOIN_SCOUT": 4,
    "RETURN_HOME": 5,
    "CHASE_LEADER": 6,
    "WAIT_LEADER": 7,
    "WAIT_SCOUT": 8,
    "CHECK_SCOUT": 9
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
            "phase": AgentPhase["EXPLORE"],
            "level": 0,
            "leader": self,
            "home": None # assigned a node when agent settles.
        }
        # The ports are numbered 0 to degree of the node. 
        self.arrival_port = None # the port through which the agent reached the current node
        self.next_port = None # result of computed port
        self.parent_port = None # the arrival port when settled
        self.recent_port = None # the last port taken by leader
        self.checked_port = None # the last port checked at scouting node maintained by leader
        self.scout_port = None # assigned when scouting
        self.scouted_result = None # Empty or Occupied (for SCOUT_RETURN phase)
        self.scout_at_neighbor = None # the port from which scout invitation comes
        self.scout_return_port = None # the port to take to return to settled node after scout
        self.settled_round = None
        self.increment = False
        self.computed = False

    def initialize_new_leader(self, leader):
        # reset all variables and increase the level
        leader.state['level'] += 1
        leader.recent_port = None
        leader.scout_port = None
        leader.scouted_result = None
        leader.scout_at_neighbor = None
        leader.scout_return_port = None
        leader.parent_port = None
        leader.checked_port = None
        leader.settled_round = None
        leader.computed = False
        leader.increment = False
        leader.state['phase'] = AgentPhase["EXPLORE"]
        leader.state['status'] = AgentStatus["UNSETTLED"]
        leader.state['role'] = AgentRole["LEADER"]
        leader.state['leader'] = leader
        leader.state['home'] = None

    def find_settled_agent(self, G, leader):
        settled_agent = G.nodes[self.currentnode]['settled_agent']
        if settled_agent is None:
            return None
        if settled_agent.state['leader'] != leader:
            return None
        if settled_agent.currentnode != self.currentnode:
            return None
        return settled_agent
    
    def assign_settled_agent(self, G, leader):
        colocated_agents = G.nodes[leader.currentnode]['agents']
        if len(colocated_agents)== 1 and leader in colocated_agents:
            leader.state['status'] = AgentStatus["SETTLED_WAIT"]
            leader.state['phase'] = AgentPhase["IDLE"]
            leader.state['leader'] = leader
            leader.state['home'] = leader.currentnode
            leader.settled_round = self.round_number
            leader.parent_port = self.arrival_port
            G.nodes[leader.currentnode]['settled_agent'] = leader
            leader.computed = True
            return leader
        sorted_agents = sorted(colocated_agents, key=lambda a: a.id)
        highest_id_agent = sorted_agents[-1]
        highest_id_agent.state['status'] = AgentStatus["SETTLED"]
        highest_id_agent.state['phase'] = AgentPhase["IDLE"]
        highest_id_agent.state['leader'] = leader
        highest_id_agent.state['home'] = leader.currentnode
        highest_id_agent.settled_round = self.round_number
        G.nodes[leader.currentnode]['settled_agent'] = highest_id_agent
        highest_id_agent.state['role'] = AgentRole["FOLLOWER"]
        return highest_id_agent

    def settle_heo(self, G, leader):
        settled_agent = self.find_settled_agent(G, leader)
        if settled_agent is None:
            settled_agent = self.assign_settled_agent(G, leader)
        else:
            pass
        return settled_agent
        
    def scout_forward(self, G):
        # checks if current node is empty or not, then returns back to previous node
        self.state['phase'] = AgentPhase["SCOUT_RETURN"]
        self.next_port = self.arrival_port
        print(f'Agent {self.id} computed next port to be {self.next_port}')
        # check for helpers in the current node
        settled_agent = self.find_settled_agent(G, self.state['leader'])
        if settled_agent is None:
            self.scouted_result = NodeStatus["EMPTY"]
        else:
            self.scouted_result = NodeStatus["OCCUPIED"]
            settled_agent.scout_at_neighbor = self.arrival_port
            settled_agent.state['phase'] = AgentPhase["JOIN_SCOUT"]
            settled_agent.state['role'] = AgentRole["HELPER"]
            settled_agent.next_port = self.arrival_port
            settled_agent.scout_return_port = self.scout_port
            settled_agent.computed = True
        return None

    def assign_scout_port(self, G, leader, scout_pool, settled_agent):
        sorted_pool = sorted(scout_pool, key=lambda a:a.id)
        # assign ports from checked_port to degree max
        degree = G.degree(leader.currentnode)
        if settled_agent.checked_port is None:
            start_port = 0
        else:
            start_port = settled_agent.checked_port + 1
        for i, agent in enumerate(sorted_pool):
            port = start_port + i
            if port < degree:
                agent.scout_port = port
                agent.next_port = port
                agent.computed = True
                agent.state['phase'] = AgentPhase['SCOUT_FORWARD']
                print(f'Agent {agent.id} assigned scout port {port} at {self.currentnode}')
            else:
                agent.scout_port = None
                agent.computed = True
                agent.state['phase'] = AgentPhase['WAIT_SCOUT'] # extra scouts are idle and do not do any scouting
        settled_agent.checked_port = min(start_port + len(sorted_pool) -1, G.degree(settled_agent.currentnode) -1)
        return

    def scout_return(self, G):
        # check if the scout result is occupied or empty
        empty_node_found = False
        colocated_agents = G.nodes[self.currentnode]['agents']
        settled_agent = self.find_settled_agent(G, self.state['leader'])
        if settled_agent is None:
            settled_agent = self.settle_heo(G, self.state['leader'])
        scouted_ports = []
        for a in colocated_agents:
            if a.scouted_result == NodeStatus["EMPTY"] and a.state['phase'] == AgentPhase['SCOUT_RETURN']:
                empty_node_found = True
                print(f'Agent {a.id} scout return: empty node found at {a.scout_port}')
                scouted_ports.append(a.scout_port)
        if not empty_node_found:
            if settled_agent.checked_port is None or settled_agent.checked_port < G.degree(self.currentnode) - 1:
                scout_pool = [a for a in colocated_agents if a != settled_agent]
                self.state['leader'].assign_scout_port(G, self.state['leader'], scout_pool, settled_agent)
            else:
                empty_port = settled_agent.parent_port
                self.follow_leader(G, empty_port)
        else:
            empty_port = min(scouted_ports, key=lambda x: x)
            self.follow_leader(G, empty_port)
        return

    def follow_leader(self, G, empty_port):
        # find the agents in phase "SCOUT_RETURN"
        for a in G.nodes[self.currentnode]['agents']:
            if a.state['role'] == AgentRole['HELPER']:
                a.next_port = a.scout_return_port
                a.scout_port = None
                a.scouted_result = None
                a.computed = True
                a.state['phase'] = AgentPhase['IDLE']
                a.state['role'] = AgentRole['FOLLOWER']
            elif (a.state['phase'] == AgentPhase['SCOUT_RETURN'] or a.state['phase'] == AgentPhase['CHECK_SCOUT']) and a.state['status'] != AgentStatus['SETTLED']:
                a.next_port = empty_port
                a.computed = True
                a.state['phase'] = AgentPhase['EXPLORE']
        self.state['leader'].checked_port = None
        return

    def make_new_leader(self, G, leaders):
        # Determine the leader: highest level, then lowest id
        max_level = max(a.state['level'] for a in leaders)
        top_level_agents = [a for a in leaders if a.state['level'] == max_level]
        if len(top_level_agents) > 1: 
            # Now break ties by lowest id
            new_leader = min(top_level_agents, key=lambda a: a.id)
            self.initialize_new_leader(new_leader)
        else:
            new_leader = top_level_agents[0]
        return new_leader

    def elect_leader(self, G):
        # elect a leader from the current node
        colocated_agents = G.nodes[self.currentnode]['agents']
        leaders = [a for a in colocated_agents if a.state['role'] == AgentRole['LEADER'] and a.state['phase'] == AgentPhase['EXPLORE']]
        if len(leaders) > 1:
            leader = self.make_new_leader(G, leaders)
            for a in G.nodes[leader.currentnode]['agents']:
                if a != leader:
                    a.state['role'] = AgentRole['FOLLOWER']
                a.state['leader'] = leader
                a.state['level'] = leader.state['level']
        elif len(leaders) == 0:
            print(f'Agent {self.id}, phase {get_dict_key(AgentPhase, self.state["phase"])}: no leader found at {self.currentnode}')
            leader = max(colocated_agents, key=lambda a: a.state['level'])
            self.initialize_new_leader(leader)
            for a in G.nodes[leader.currentnode]['agents']:
                if a != leader:
                    a.state['role'] = AgentRole['FOLLOWER']
                a.state['leader'] = leader
                a.state['level'] = leader.state['level']
        else:
            leader = leaders[0]
        return leader

    def find_leader(self, G):
        leaders = [a for a in G.nodes[self.currentnode]['agents'] if a.state['role'] == AgentRole['LEADER'] and a.state['phase'] == AgentPhase['EXPLORE']]
        new_leader_elected = False
        if len(leaders) > 1:
            leader = self.elect_leader(G)
            new_leader_elected = True
        elif len(leaders) == 0:
            leader = self.elect_leader(G)
            new_leader_elected = True
        else:
            leader = leaders[0]
        return leader, new_leader_elected
            

    def check_meeting(self, G):
        leader, new_leader_elected = self.find_leader(G)
        if new_leader_elected:
            return leader, True
        settled_agent_at_node = G.nodes[self.currentnode]['settled_agent']
        if settled_agent_at_node is not None:
            if settled_agent_at_node.currentnode == self.currentnode and settled_agent_at_node.state['leader'] != leader:
                if settled_agent_at_node.state['level'] == leader.state['level']:
                    self.initialize_new_leader(leader)
                    return leader, True
        return leader, False


    def explore(self, G):
        leader, meeting = self.check_meeting(G)
        if meeting:
            print(f'Agent {self.id} meeting: new leader elected at {self.currentnode}')
        settled_agent = self.find_settled_agent(G, leader)
        if settled_agent is None:
            settled_agent = self.settle_heo(G, leader)
        scout_pool = [a for a in G.nodes[self.currentnode]['agents'] if a != settled_agent]
        self.state['leader'].assign_scout_port(G, self.state['leader'], scout_pool, settled_agent)
        return

    def chase_leader(self, G):
        # take the recent port at a settled node to chase a leader.
        settled_agent = self.find_settled_agent(G, self.state['leader'])
        if settled_agent is None:
            return
        if self.currentnode == self.state['leader'].currentnode:
            self.state['phase'] = AgentPhase['EXPLORE']
            self.state['role'] = AgentRole['FOLLOWER']
        else:
            self.next_port = settled_agent.recent_port
            self.computed = True
        return

    def wait_scout(self, G):
        # change to check scout phase
        self.state['phase'] = AgentPhase['CHECK_SCOUT']
        return

    def compute_heo(self, G, agents):
        if self.computed:     # already done
            return

        phase_handlers = {
            AgentPhase['SCOUT_FORWARD'] : self.scout_forward,
            AgentPhase['SCOUT_RETURN']  : self.scout_return,
            AgentPhase['EXPLORE']       : self.explore,
            AgentPhase['CHASE_LEADER']  : self.chase_leader,
            AgentPhase['WAIT_SCOUT']    : self.wait_scout,
            AgentPhase['CHECK_SCOUT']   : self.scout_return,
            AgentPhase['JOIN_SCOUT']    : lambda G: None,
            AgentPhase['IDLE']          : lambda G: None,
        }
        phase_handlers[self.state['phase']](G)
        self.computed = True


    def move_heo(self, G, r):
        self.round_number = r
        if self.computed == True and self.next_port is not None:
            next_node = G.nodes[self.currentnode]['port_map'].get(self.next_port)
            if next_node is None:
                raise ValueError(f"Agent {self.id} tried to move through invalid port {self.next_port} at node {self.currentnode} in round number {self.round_number}")
            self.arrival_port = G[self.currentnode][next_node][f'port_{next_node}']
            G.nodes[self.currentnode]['agents'].remove(self)
            print(f'agent {self.id} moved via port {self.next_port} at node {self.currentnode} to reach {next_node} using {self.arrival_port}')
            self.currentnode = next_node
            # add agent to the corresponding node in graph
            G.nodes[self.currentnode]['agents'].add(self)
        self.computed = False
        self.next_port = None
        return