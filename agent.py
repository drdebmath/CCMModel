from enum import Enum
import random

class agentStatus(Enum):
    SETTLED = "SETTLED"
    UNSETTLED = "UNSETTLED"

class Agent:
    def __init__(self, id, currentnode):
        self.id = id
        self.currentnode = currentnode
        self.status = agentStatus.UNSETTLED
        self.history = [(0, self.currentnode, None, self.status.name)]  # (round, node, port, status)
        self.memory = {}  # For storing communication data

    def communicate(self, agents):
        """Communicate with co-located agents to share IDs and statuses."""
        co_located = [a for a in agents if a.currentnode == self.currentnode and a != self]
        return [(a.id, a.status) for a in co_located]

    def compute(self, communication_data, round_number):
        """Compute next action based on communication and round number."""
        if round_number == 1:
            # Check for settlement in round 1
            all_agents_data = [(self.id, self.status)] + communication_data
            has_settled = any(status == agentStatus.SETTLED for _, status in all_agents_data)
            if not has_settled:
                max_id = max(id for id, _ in all_agents_data)
                if self.id == max_id:
                    self.status = agentStatus.SETTLED
                    print(f"Agent {self.id} settling at node {self.currentnode} in round 1.")
                    return None  # No movement for settled agent
        # Default computation for movement (if not settled)
        return "move"  # Placeholder for movement decision

    def move(self, G, action):
        """Move to a neighboring node via a port if action is 'move'."""
        if action != "move" or self.status == agentStatus.SETTLED:
            return
        neighbors = list(G.neighbors(self.currentnode))
        if neighbors:
            next_node = random.choice(neighbors)  # Simplified movement
            port = G[self.currentnode][next_node].get(f"port_{self.currentnode}")
            self.currentnode = next_node
            self.history.append((self.history[-1][0] + 1, self.currentnode, port, self.status.name))

    def step(self, G, agents, round_number):
        """Execute one round: communicate, compute, move."""
        communication_data = self.communicate(agents)
        action = self.compute(communication_data, round_number)
        self.move(G, action)
        # Update history with current round
        if len(self.history) > 1 and self.history[-1][0] < round_number:
            self.history.append((round_number, self.currentnode, self.history[-1][2], self.status.name))