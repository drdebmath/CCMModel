
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
