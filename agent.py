def get_dict_key(d, value):
    for k, v in d.items():
        if v == value:
            return k
    return str(value)

AgentStatus = {
    # status of the agent
    "SETTLED": 0,
    "UNSETTLED": 1
}

AgentRole = {
    # role of the agent
    "LEADER": 0,
    "FOLLOWER": 1
}

AgentPhase = {
    # phase of the agent
    "EXPLORE": 0,
    "IDLE": 1,
    "SCOUT_FORWARD": 2,
    "SCOUT_RETURN": 3,
    "JOIN_SCOUT": 4,
    "RETURN_SCOUT": 5,
    "CHASE_LEADER": 6,
    "WAIT_LEADER": 7,
    "WAIT_SCOUT": 8
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
        # The ports are numbered 1 to degree of the node. 
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

    def get_colocated_agents(self, G):
        colocated_agents = G.nodes[self.currentnode]['agents']
        return colocated_agents

    def sort_colocated_agents(colocated_agents):
        return sorted(colocated_agents, key=lambda agent:agent.id)

    def settle_dfs_rooted(self, G, agents):
        # if there is no settled agent among the colocated agents, settle the highest ID agent
        colocated_agents = self.get_colocated_agents(G)
        for a in colocated_agents:
            if a.state['status'] == AgentStatus["SETTLED"]:
                return
        # no settled agent, then settle the highest
        highest_id_agent = max(colocated_agents, key=lambda a: a.id)
        highest_id_agent.state['status'] = AgentStatus["SETTLED"]
        highest_id_agent.parent_port = highest_id_agent.arrival_port
        highest_id_agent.recent_port = highest_id_agent.arrival_port
        G.nodes[self.currentnode]['settled_agent'] = highest_id_agent
        print(f'agent {highest_id_agent.id} settled at {self.currentnode}. Recent port is {highest_id_agent.recent_port}')
        return

    def move_dfs_rooted(self, G, round_number):
        self.round_number += 1
        if self.next_port is None and self.state['status'] == AgentStatus["SETTLED"]:
            if self.increment == True:
                if self.recent_port == None:
                    self.recent_port = 0
                else:
                    self.recent_port = (self.recent_port +1)%G.degree(self.currentnode)
                self.increment = False
            return
        # remove agent from current node
        G.nodes[self.currentnode]['agents'].remove(self)
        next_node = G.nodes[self.currentnode]['port_map'].get(self.next_port)
        if next_node is None:
            raise ValueError(f"Agent {self.id} tried to move through invalid port {self.next_port} at node {self.currentnode} in round number {round_number}")

        self.arrival_port = G[self.currentnode][next_node][f'port_{next_node}']
        print(f'agent {self.id} moved via port {self.next_port} at node {self.currentnode} to reach {next_node} using {self.arrival_port}')
        self.currentnode = next_node
        # add agent to the corresponding node in graph
        G.nodes[self.currentnode]['agents'].add(self)
        self.next_port = None
        return


    def compute_dfs_rooted(self, G, agents):
        # find the recent port at settled agent
        settled_agent = G.nodes[self.currentnode]['settled_agent']
        print(f'Agent: {self.id}; currentnode:{self.currentnode}; Arrival port: {self.arrival_port}; Next Port: {self.next_port} (should be None); Settled Agent: {settled_agent.id}; Recent Port: {settled_agent.recent_port}')
        if settled_agent is None:
            raise ValueError(f"Agent {self.id} at node {self.currentnode} in round number {self.round_number} did not find settled agent")
        if self.id == settled_agent.id:
            self.next_port = None
            return
        recent_port = settled_agent.recent_port
        if recent_port is None:
            self.next_port = 0
            settled_agent.increment = True
            return
        else:
            if self.arrival_port == recent_port:
                recent_port = (recent_port + 1) % G.degree(settled_agent.currentnode)
                # need to update the settled_agent memory
                settled_agent.increment = True
                self.next_port = recent_port
                return
            else:
                self.next_port = self.arrival_port
                return

    def settle_heo(self, G, leader):
        # highest id follower agent settles if there are no settled agents
        settled_agent = G.nodes[self.currentnode]['settled_agent']
        if settled_agent is not None and settled_agent.currentnode == self.currentnode:
            if settled_agent.state['leader'] == leader and settled_agent.state['level'] == leader.state['level']:
                pass
            else:
                settled_agent.state['leader'] = leader
                settled_agent.state['level'] = leader.state['level']
            return settled_agent
        colocated_agents = G.nodes[self.currentnode]['agents']
        highest_id_agent = max(colocated_agents, key=lambda a: a.id)
        highest_id_agent.state['status'] = AgentStatus["SETTLED"]
        highest_id_agent.state['leader'] = leader
        highest_id_agent.state['level'] = leader.state['level']
        print(f'agent {highest_id_agent.id} settled at {self.currentnode}. Leader: {highest_id_agent.state['leader'].id} with level {highest_id_agent.state['level']}')
        settled_agent = highest_id_agent
        settled_agent.state['leader'] = leader
        settled_agent.state['level'] = leader.state['level']
        return settled_agent

    def compute_heo(self, G, agents):
        print(f"Round {self.round_number}: Agent {self.id} is {get_dict_key(AgentStatus, self.state['status'])} located at {self.currentnode}. Role: {get_dict_key(AgentRole, self.state['role'])}. Phase: {get_dict_key(AgentPhase, self.state['phase'])}")
        if self.computed == True:
            return
        colocated_agents = G.nodes[self.currentnode]['agents']
        # check state scout
        if self.state['phase'] == AgentPhase["SCOUT_FORWARD"]:
            self.state['phase'] = AgentPhase["SCOUT_RETURN"]
            self.next_port = self.arrival_port
            print(f'Agent {self.id} computed next port to be {self.next_port}')
            # check for helpers in the current node
            if len(colocated_agents) == 1: # empty node
                # return and report empty neighbor
                self.scouted_result = NodeStatus["EMPTY"]
            else:
                for a in colocated_agents:
                    if a.state['status'] == AgentStatus["SETTLED"] and a.state['phase'] == AgentPhase["IDLE"]: # the settled robot is not a scout
                    # check if the settled robot belongs to the same group, then take its help, otherwise report empty
                        if a.state['leader'] == self.state['leader'] and a.state['level'] == self.state['level']:
                            self.scouted_result = NodeStatus["OCCUPIED"]
                            a.scout_at_neighbor = self.arrival_port
                            a.state['phase'] = AgentPhase["JOIN_SCOUT"]
                            a.next_port = self.arrival_port
                            a.computed = True
                            break
                        self.scouted_result = NodeStatus["EMPTY"]
                        break
        # check state scout return
        elif self.state['phase'] == AgentPhase["SCOUT_RETURN"]:
            # leader must check for the status of the scout result
            self.activate_leader(G, agents)
            
        # check state explore
        elif self.state['phase'] == AgentPhase["EXPLORE"]:
            self.activate_leader(G, agents)
        # check leader meeting other leader/follower
        # check 

        self.computed = True
        return

    def activate_leader(self, G, agents):
        colocated_agents = G.nodes[self.currentnode]['agents']
        leaders = [a for a in colocated_agents if a.state['role'] == AgentRole["LEADER"]]
        if len(leaders) > 1:
            new_leader = self.make_new_leader(G, leaders)
            for a in colocated_agents:
                if a != new_leader:
                    a.state['role'] = AgentRole["FOLLOWER"]
                    a.state['level'] = new_leader.state['level']
            settled_agent = self.settle_heo(G, new_leader)
            settled_agent.state['phase']= AgentPhase['WAIT_SCOUT']
            # leader will start scout()
            scout_pool = [a for a in colocated_agents if a != settled_agent]
            self.assign_scout_port(G, new_leader, scout_pool)
        elif len(leaders) == 0:
            print(f'Electing leader for the first time. Round No {self.round_number}')
        else: # only one leader
            # the leader checks for next port to visit if exists otherwise backtrack; the helpers return to their home base.
            leader = leaders[0]
            scout_pool = [a for a in colocated_agents if a.state['status'] == AgentStatus["UNSETTLED"]]
            scout_returns = [a for a in colocated_agents if a.state['phase'] == AgentPhase["SCOUT_RETURN"]]
            if len(scout_returns) > 0:
                empty_node_found = False
                for a in scout_returns:
                    if a.scouted_result == NodeStatus["EMPTY"]:
                        empty_node_found = True
                if not empty_node_found and leader.checked_port < G.degree(leader.currentnode):
                    self.assign_scout_port(G, leader, scout_pool)
                else:
                    # go to the empty node with all unsettled followers
                    unsettled_followers = [a for a in scout_pool if a.state['status'] == AgentStatus["UNSETTLED"]]
                    # find empty node
                    empty_port = None
                    empty_ports = [a.scout_port for a in scout_returns if a.scouted_result == NodeStatus["EMPTY"]]
                    empty_port = min(empty_ports) if empty_ports else None
                    if empty_port is not None:
                        # if empty node is found, go to the empty node with all unsettled followers
                        for a in unsettled_followers:
                            a.next_port = empty_port
                            a.state['phase'] = AgentPhase['EXPLORE']
                            a.computed = True 
                            a.scout_port = None
                        leader.next_port = empty_port
                        leader.state['phase'] = AgentPhase['EXPLORE']
                        leader.computed = True
                        leader.scout_port = None
                    else: # no empty node found
                        if leader.checked_port < G.degree(leader.currentnode):
                            self.assign_scout_port(G, leader, scout_pool)
                        else: # no more ports to check
                            # go back to parent port at settled robot
                            # find settled agent 
                            settled_agent = [a for a in colocated_agents if a.state['status'] == AgentStatus["SETTLED"]][0]
                            leader.next_port = settled_agent.parent_port
                            leader.computed = True
                            leader.scout_port = None
                            for a in scout_pool:
                                a.state['phase'] = AgentPhase['EXPLORE']
                                a.next_port = settled_agent.parent_port
                                a.computed = True
                                a.scout_port = None
            else: # no scout returns
                if leader.state['phase'] == AgentPhase['EXPLORE']:
                    settled_agent = self.settle_heo(G, leader)
                    settled_agent.state['phase']= AgentPhase['WAIT_SCOUT']
                    # leader will start scout()
                    scout_pool = [a for a in colocated_agents if a != settled_agent]
                    self.assign_scout_port(G, leader, scout_pool)


    def assign_scout_port(self, G, leader, scout_pool):
        sorted_pool = sorted(scout_pool, key=lambda a:a.id)
        # assign ports from checked_port to degree max
        degree = G.degree(leader.currentnode)
        if leader.checked_port is None:
            start_port = 0
        else:
            start_port = leader.checked_port + 1
        for i, agent in enumerate(sorted_pool):
            port = start_port + i
            if port < degree:
                agent.scout_port = port
                agent.next_port = port
                agent.computed = True
                agent.state['phase'] = AgentPhase['SCOUT_FORWARD']
            else:
                agent.scout_port = None
                agent.computed = True
                agent.state['phase'] = AgentPhase['WAIT_SCOUT'] # extra scouts are idle and do not do any scouting
        return


    def make_new_leader(self, G, leaders):
        # Determine the leader: highest level, then lowest id
        max_level = max(a.state['level'] for a in leaders)
        top_level_agents = [a for a in leaders if a.state['level'] == max_level]
        if len(top_level_agents) > 1: 
            # Now break ties by lowest id
            new_leader = min(top_level_agents, key=lambda a: a.id)
            new_leader.state['level'] += 1
            new_leader.checked_port = None # changes only if the leader level changes.
        else:
            new_leader = top_level_agents[0]
        return new_leader

    def move_heo(self, G, r):
        self.round_number = r
        if self.computed == True and self.next_port is not None:
            next_node = G.nodes[self.currentnode]['port_map'].get(self.next_port)
            if next_node is None:
                raise ValueError(f"Agent {self.id} tried to move through invalid port {self.next_port} at node {self.currentnode} in round number {round_number}")
            self.arrival_port = G[self.currentnode][next_node][f'port_{next_node}']
            G.nodes[self.currentnode]['agents'].remove(self)
            print(f'agent {self.id} moved via port {self.next_port} at node {self.currentnode} to reach {next_node} using {self.arrival_port}')
            self.currentnode = next_node
            # add agent to the corresponding node in graph
            G.nodes[self.currentnode]['agents'].add(self)
        self.computed = False
        self.next_port = None
        return