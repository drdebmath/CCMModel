# Current Python behavior specification

> **Archival document.** This describes the Python implementation that this
> repository has since removed; the repository is now Rust end to end. File and
> line citations below point at code that no longer exists in this tree and are
> historical references, not live pointers. It is retained because it is the
> provenance for the Rust ports: it records the behavior they were built to
> reproduce, and the reasoning behind the choices they encode. See
> [migration.md](migration.md) for the current state.

This document records the behavior of the Python code as it exists in this
repository. It is a compatibility/reference document, not a claim that every
shortcut or failure mode is part of the intended distributed algorithms. The
main implementations covered here are:

- `agent_drop_freeze.py` (`Drop-and-Freeze` in `simulation_wrapper.py`);
- `agent_help_scouts.py` (`Help by Scouts` in `simulation_wrapper.py`).

The older `agent.py` implementation is described briefly where its state and
return contract affect compatibility. Deterministic reference coverage now
lives in `tests/test_python_reference_fixtures.py`; the older randomized
`stress_test.py` exercises only Help by Scouts.

## 1. Common input model

The simulator expects a NetworkX undirected graph. A node has a `port_map`
mapping local integer ports to neighboring node IDs. An edge carries the
reciprocal attributes `port_<u>` and `port_<v>`. The graph also receives an
`agents` set on each node. Node IDs are normally integers, but the algorithm
code does not consistently require that.

`graph_utils.create_port_labeled_graph` creates a connected `gnm_random_graph`
with `floor(nodes * max_degree / 2)` edges and initially assigns ports in
NetworkX neighbor iteration order. `randomize_ports` then independently
permutes each node's local ports. This is the wrapper's input behavior
(`graph_utils.py:6-34`, `simulation_wrapper.py:90-98`).

The wrapper chooses a starting-node set, then places each agent independently
at a random member of that set. Thus `starting_positions` is a bound on the
number of possible starting nodes, not a guarantee that each is occupied
(`simulation_wrapper.py:100-121`). Agent IDs are `0..agent_count-1` in the
wrapper.

The two current algorithms do not actually consume this input identically:

- Drop-and-Freeze calls `_init_ports`, replacing every local port assignment
  with sorted-neighbor order and rewriting reciprocal edge attributes
  (`agent_drop_freeze.py:86-96`). The wrapper's `randomize_ports` therefore has
  no effect on this algorithm.
- Help by Scouts preserves the graph's existing `port_map` and edge ports.
  Port `0` is the algorithm's distinguished “port 1” (`PORT_ONE = 0`,
  `agent_help_scouts.py:4-5`).

The wrapper initializes `settled_agent = None` before either run. Direct calls
to the algorithm functions should provide equivalent graph attributes when
the implementation reads them.

## 1.1 Paper baseline and terminology

The cited source is [Sudo et al., *Near-linear Time Dispersion of Mobile
Agents*, arXiv:2310.04376v3](https://arxiv.org/html/2310.04376v3). The paper does
not use the repository names “Drop-and-Freeze” or “Help by Scouts”; the closest
paper baselines are its rooted HEO-DFS algorithm (Section 3, Algorithm 1) and
general HEO-DFS/Zombie-method algorithm (Section 4, Algorithms 2–5). The
mapping below is therefore a working comparison, not evidence that the Python
files are faithful ports.

The paper's model is materially narrower and more formal than this simulator:

- the graph is simple, connected, undirected, anonymous, and has independent
  local port labels at the two endpoints;
- agents have unique IDs, communicate only with co-located agents, have no
  node whiteboards/global graph state, and execute synchronously in atomic
  steps;
- all agents start as explorers. Once an agent becomes a settler, that role is
  permanent and its home is the node where it settled. A settler may travel
  temporarily, but its home does not change;
- the rooted algorithm assumes one starting node. The general algorithm allows
  arbitrary initial placement, with leaders/zombies/settlers and explicit
  group merging;
- time is the number of synchronous steps to a legitimate configuration (all
  agents at distinct nodes and stationary). Paper space is persistent
  per-agent state, including the ID, not Python object size, graph-wide
  working memory, or trace storage.

The repository code should therefore be described as an executable behavioral
reference, not as an implementation that inherits the paper's bounds.

## 2. Output contract and the wrapper mismatch

Each current function returns five parallel histories. A history item is
usually `(label, value)`, and the histories are intended to be indexed in
lockstep. The wrapper unconditionally assigns the five slots as
`positions, statuses, node_settled_states, homes, tree_edges`
(`simulation_wrapper.py:127-176`). That assignment is correct only for Help by
Scouts.

### Drop-and-Freeze return values

`agent_drop_freeze.run_simulation` returns, in order
(`agent_drop_freeze.py:295-346`):

```text
1. all_positions
2. all_statuses
3. all_leaders
4. all_levels
5. all_node_states
```

Consequently the wrapper serializes the third value under
`node_settled_states`, the fourth under `homes`, and the fifth under
`tree_edges`. The actual meanings are therefore:

```text
result.node_settled_states  = [(label, [leader id per agent])]
result.homes                = [(label, [level per agent])]
result.tree_edges           = [(label, {node: settled-state})]
```

The actual `all_node_states` history is not lost, but is mislabeled as
`tree_edges`; Drop-and-Freeze never produces homes or tree edges.

### Help-by-Scouts return values

`agent_help_scouts.run_simulation` returns
(`agent_help_scouts.py:560-579`):

```text
1. simmer.all_positions
2. simmer.all_statuses
3. simmer.all_node_states
4. simmer.all_homes
5. simmer.all_tree_edges
```

The third history is currently populated with `[]` for every ordinary
snapshot: `_insert_new_round` receives `base_node_states = []`, and no later
code supplies node states (`agent_help_scouts.py:91-112`). The fourth and fifth
histories are the meaningful homes and tree-edge views.

### Legacy `agent.py` return values

The separate `agent.py` loop returns
`(all_positions, all_statuses, all_leader_ids, all_leader_levels,
all_node_settled_states)` (`agent.py:371-520`). It is not imported by the
current wrapper, but has the same five-slot shape as Drop-and-Freeze and would
therefore be mislabeled by that wrapper in exactly the same way.

No current return value contains an explicit termination reason or an explicit
theoretical round count. An empty history can also be returned by the wrapper
when its preconditions fail (`simulation_wrapper.py:127-143`).

## 3. Drop-and-Freeze

### Agent and node state

`agent_drop_freeze.Agent` has (`agent_drop_freeze.py:14-35`):

- `id`, `currentnode`;
- `state.status`: `UNSETTLED = 1`, transient `SETTLED_WAIT = 2`, or
  `SETTLED = 0`;
- `state.level`, `state.leader`, and unused/unassigned `state.home`;
- probe scratch: `probe_home`, `probe_port`, `probe_result_empty`;
- DFS scratch: `pin`, `entry_pin`, `parent_port`, `next_port_to_try`.

The node's `settled_agent` pointer and `node_status` (`EMPTY = 0` or
`OCCUPIED = 1`) are maintained by `run_simulation` and `_move_out`. A settled
agent remains in the node's `agents` set; “movers” are selected by excluding
that object and selecting status `UNSETTLED` (`agent_drop_freeze.py:173-187`).

At initialization all agents are reset to `UNSETTLED`, all probe/DFS fields
are cleared, and all node occupancy is cleared (`agent_drop_freeze.py:295-312`).
`leader` remains the agent itself and `level` remains zero throughout this
implementation; no home is ever assigned.

### Local port order

`_ordered_ports(G, u)` sorts local ports by the tuple (`agent_drop_freeze.py:37-52`):

1. local port nonzero and remote port zero;
2. local port zero;
3. all other ports;

with local port as the tie breaker. This is a code-level ordering rule, not
NetworkX edge order. The DFS cursor is an index into this ordered list, although
the cursor is stored in `next_port_to_try` and compared with ordered-list
indices (`agent_drop_freeze.py:217-255`).

### One macro-round

The implementation labels one loop iteration as a macro-round and explicitly
comments that it consists of three synchronous sub-rounds
(`agent_drop_freeze.py:321-344`):

1. **Probe out (`_probe_out`)**. At each node, unsettled agents are sorted by
   ID and assigned distinct ports starting at the settled agent's cursor (or
   from the beginning if there is no settled agent). A special case skips
   probing when exactly one unsettled agent is at a node with no settled agent.
   Each assigned agent changes to `SETTLED_WAIT`, records its home/port, and
   moves one edge. Probes from an empty node with multiple agents are also
   allowed. Probes are planned first and then all planned moves are executed
   (`agent_drop_freeze.py:99-144`).
2. **Probe back (`_probe_back`)**. Each `SETTLED_WAIT` agent reads only whether
   the destination has a `settled_agent`; no settled agent means “empty”, even
   if other agents are there. It then returns to `probe_home` and becomes
   `UNSETTLED` (`agent_drop_freeze.py:146-171`).
3. **Move out (`_move_out`)**. The post-return occupancy is rebuilt by node.
   First, at every node with movers and no settled pointer, the lowest-ID
   mover settles, gets `node_status = OCCUPIED`, and receives
   `parent_port = entry_pin` if that incoming port is valid. Second, remaining
   movers at each node use probe results from this macro-round. The first
   confirmed empty port at or after the settled agent's cursor is selected;
   **all** remaining movers move together to that neighbor. The cursor advances
   after a forward move. If no empty port was selected, the cursor advances by
   the number of distinct probed ports; if ports remain, the group stays put.
   Once no ports remain, the group backtracks together through `parent_port`
   when one exists (`agent_drop_freeze.py:173-293`).

Moves in each helper are applied sequentially to the node sets, but the probe
out/back lists and the move-out list are planned before execution. There is no
collision/conflict resolution beyond the code's ordering.

### Termination and trace

The loop stops before a macro-round when every agent has status `SETTLED`, or
after the requested number of macro-rounds. Reaching the limit is silent: the
function returns the partial histories and does not report success/failure.
The initial `start` snapshot is followed by up to three snapshots per
macro-round (`probe_out`, `probe_back`, `move_out`). Therefore history length is
`1 + 3 * executed_macro_rounds` except for early termination.

Each snapshot stores positions and numeric statuses in input-agent order. Node
state entries are keyed by string node ID and contain only
`settled_agent_id`, `parent_port`, `checked_port`, `max_scouted_port`, and
`next_port`; the last three fields are always `None` in this implementation
(`agent_drop_freeze.py:60-84`). Leader and level histories are also captured
at every snapshot, but the wrapper labels them as described in §2.

### Drop-and-Freeze invariants and hazards

The behavior relies on these invariants during a normal run:

- each agent is in exactly one node's `agents` set and its `currentnode` agrees;
- a node has at most one `settled_agent` pointer;
- a settled pointer, when present, is also in that node's agent set;
- each move uses a key in the source node's `port_map` and writes the reciprocal
  incoming port to `pin`/`entry_pin`;
- a group move preserves the set of agent IDs in the group;
- at most one agent is settled in the phase-1 pass at a node.

Important current ambiguities/bugs:

- `_init_ports` silently discards randomized/adversarial port labels.
- `entry_pin` is set only by a group move. An agent that arrives by a probe
  and is selected to settle can have no parent even when it came from an edge.
- A probe destination with moving (but no settled) agents is classified empty.
- Probe metadata is retained through `_probe_back` and consumed by
  `_move_out`; if a group stays in place it remains observable until the next
  macro-round's `_probe_out` reset, although no later sub-phase consumes it.
- An empty node with one agent skips probing and settles that agent; an empty
  node with multiple agents probes first. This makes behavior depend on local
  multiplicity.
- There are no move/probe/round counters separate from the visual history, and
  no explicit memory accounting.

### Drop-and-Freeze versus the paper's rooted HEO-DFS

The following are the important semantic discrepancies from the rooted
algorithm in Section 3 of the paper:

- **Starting condition.** The paper assumes every agent starts at one root and
  initially settles one designated explorer there. The wrapper deliberately
  permits several starting nodes and the Python function accepts arbitrary
  placements.
- **Port order.** The paper uses the local port numbers as given. Python
  rewrites them to sorted-neighbor order and then uses a custom remote-port
  ranking; randomized port assignments therefore cannot reproduce paper
  executions.
- **Helping scouts.** In the paper, explorers probe neighbors in parallel and,
  when they find a settler, bring that settler to the current node so the
  number of probing agents can grow (Section 3, Algorithm 1 and Lemma 1).
  Python probes only the currently unsettled agents at a node, never recruits
  settled agents, and directly reads `settled_agent` at the destination.
- **Probing result.** The paper stops probing when an explorer finds an
  unsettled neighbor, or concludes all relevant neighbors are settled; the
  chosen port need not be the minimum port leading to an unsettled neighbor.
  Python probes a fixed slice of `_ordered_ports`, collects every returned
  “empty” result, and chooses the first result at or after its cursor.
- **Home and movement state.** The paper assigns an immutable home at settle
  time and permits temporary helper travel. Python leaves `state["home"]`
  `None` and never moves a settled agent, so its `settled_agent` pointer is
  not the paper's home-owner abstraction.
- **Rounds and termination.** A paper probe round trip is two synchronous
  steps and forward/backward DFS moves are separate steps. Python groups probe
  out, probe back, and move out into a caller-labeled macro-round, has no
  logical step counter, and silently returns on its configured history limit.
  It also does not implement the paper's explicit rooted termination
  propagation procedure (Appendix A).

Accordingly, parity with this Python file must not be presented as parity with
the paper's rooted algorithm or its time/space theorem.

## 4. Help by Scouts

### Agent and global state

`agent_help_scouts.Agent` uses string states `unsettled`, `settled`, and
`settledScout` (`agent_help_scouts.py:142-179`). It stores a home vertex,
parent/arrival ports and IDs, node classification (`unvisited`, `visited`,
`partiallyVisited`, `fullyVisited`), vacated-neighbor state, DFS breadcrumbs,
scout scratch, and per-port probe results. `simmer` is a module-global
`SIM_DATA`; every run clears and reuses it (`agent_help_scouts.py:7-23,
560-569`).

The implementation maintains two logical sets in `rooted_async`: unsettled
agents and vacated scouts. A settled agent owns its `home`; a `settledScout`
is a settled owner temporarily traveling with the group. The graph's node
`agents` sets contain integer agent IDs, not Agent objects.

### Initialization and scheduler

`run_simulation` rejects `len(agents) > len(G)` and then **overwrites the
caller-supplied `max_rounds` with `40 * len(agents)`**, including when a
positive limit was supplied (`agent_help_scouts.py:560-564`). A list is locally
converted to an ID-keyed dictionary; a dictionary is used as supplied. The
root is the starting node of the lowest-ID agent (`agent_help_scouts.py:567-573`).

`rooted_async` is a centralized asynchronous DFS scheduler, not a bulk
synchronous per-agent scheduler. Each iteration chooses `v` as the current
node of the lowest-ID agent in `A_unsettled ∪ A_vacated`; the lowest-ID member
of that set is also the scout/group representative (`amin`). A node without a
physical settled owner is settled by the **highest-ID** unsettled agent at
that node. Parent fields are filled from `amin`'s breadcrumb state
(`agent_help_scouts.py:468-517`).

### Probe and choose phase

`parallel_probe` scans ports at `x` in numeric order, excluding
`parentPort`. Up to `len(A_scout)` agents probe concurrently in the modeled
batch; each probe is implemented as an out-and-back pair of `_move_agent`
calls (`agent_help_scouts.py:348-398`). A probe records `(local_port,
edge_type, neighbor_node_type, owner_id)`. Neighbor ownership is determined by
the centralized `_owner` scan over all agents' homes: no home means
`unvisited`, a home owner supplies its current `nodeType` whether physically
present or vacated (`agent_help_scouts.py:253-263, 371-384`).

Candidate ranking prefers an `unvisited` neighbor, then a
`partiallyVisited` neighbor reached by `tp1` or `t11`; all other candidates
rank 99 and are rejected. Ties use the local port (`_candidate_rank`,
`agent_help_scouts.py:233-243`). The function returns a selected port (or
`None`) and a synthetic `rounds_max + 2` value. Its internal `round_number`
values are bookkeeping passed to snapshots, not a separate scheduler clock.

After probing, `update_node_type_after_probe` sets the current owner to:

- `fullyVisited` if no result says `unvisited`;
- `partiallyVisited` if there is a parent, its edge is `tpq`, and every empty
  result is `tpq`;
- otherwise `visited` (`agent_help_scouts.py:212-223`).

### Vacate, move, and backtrack phase

`can_vacate` applies the node's current type and returns a state plus synthetic
round cost (`agent_help_scouts.py:296-345`):

- root (`parentPort is None`) returns `settled`, cost 2;
- `visited`, or `fullyVisited` with no dependent vacated neighbor, walks over
  port 0 and back. If the destination has a physical settled owner that owner
  is marked `vacatedNeighbor`; the current agent may become `settledScout`.
  Cost is 4;
- `partiallyVisited` returns `settledScout`, cost 2, without moving;
- when `portAtParent == 0`, the agent walks to its parent and may pull the
  parent owner into the group; cost is 4;
- otherwise it returns `settled`, cost 2.

The caller then adds/removes the agent from `A_unsettled`/`A_vacated` according
to the returned state. If a probe selected a port, all agents in
`A_unsettled ∪ A_vacated` move as a group through that port, breadcrumbs are
updated, and a partially visited physical owner may be re-parented. If no port
was selected, the same group backtracks through `parentPort`; at the root this
raises rather than terminating successfully when unsettled agents remain
(`agent_help_scouts.py:517-553`).

### Retrace and termination

When `A_unsettled` becomes empty, `retrace` walks the parent-pointer tree in
DFS order. It reconstructs child lists by scanning all agents, moves the
vacated group one tree edge at a time, and changes each vacated owner back to
`settled` on reaching its home (`agent_help_scouts.py:401-465`). It raises on an
invalid tree location, round-limit overrun, or leftover vacated scouts. With
no vacated scouts it still emits enter/exit snapshots and adds synthetic
rounds.

Normal completion emits a final `rooted_async:exit` snapshot; exceptions
propagate out of `run_simulation`. `stress_test.py` runs 500 random
Help-by-Scouts cases, but reports failures rather than defining a formal
pass/fail contract (`stress_test.py:11-57`).

### Help-by-Scouts history representation

Positions, statuses, and homes are lists of one-element string lists, sorted
by agent ID (`agent_help_scouts.py:49-55`). Tree edges are dictionaries with
`u`, `v`, `srcPort`, and `dstPort`, deduplicated by undirected endpoint pair.
Labels identify internal events such as `move_agent`, `parallel_probe`,
`can_vacate`, `retrace`, and forward/backtrack moves; they are not theoretical
round names.

`_snapshot` coalesces events into an existing history index when the supplied
`round_number` is less than the current history length and appends otherwise.
It uses deep copies for positions/statuses/homes, but stores a fresh tree-edge
view and an empty node-state list. Thus history length and `simmer.rounds`
(`len(simmer.all_positions)`) are counts of trace slots, not algorithmic
rounds. Several logical moves may overwrite one slot, and event labels can be
replaced (`agent_help_scouts.py:49-140`).

### Help-by-Scouts invariants and hazards

The intended runtime invariants include:

- every agent ID is unique and appears in exactly one node agent set;
- every `node` agrees with that set membership;
- a settled home is a graph node and parent/arrival ports are reciprocal;
- `A_unsettled` and `A_vacated` are disjoint logical roles;
- every tree edge reconstructed from parent pointers uses an existing port;
- all agents are settled after the rooted traversal and retrace.

The current code leaves several behaviors underspecified or fragile:

- `_owner` scans all agents (a centralized shortcut) and treats a vacated home
  as occupied metadata; this is not an explicit distributed message/probe
  sequence.
- `_xi_id` takes the first matching ID from a set. `_move_group` iterates a
  set converted to a list. If multiple matching agents exist, selection and
  snapshot order can depend on set iteration order.
- The `parallel_probe` parent-port skip changes `Delta_prime` while also using
  separate `j` and `jk` indices; unusual parent/agent counts can make the
  agent-port assignment difficult to interpret.
- `edge_type`/candidate ranking is defined for the four port-0 combinations,
  but `_edge_rank` has no fallback for an unexpected type.
- The theoretical “parallel” probes are executed centrally and sequentially;
  the returned round cost is synthetic and is not derived from a common
  scheduler phase.
- `max_rounds` is not configurable at the public entry point despite its
  parameter, and the error is an exception rather than a result status.
- node-state output is always empty, while tree edges are reconstructed from
  agent fields and can lag or reflect stale parent fields during intermediate
  events.

### Help-by-Scouts versus the paper's general HEO-DFS

The Python name suggests the paper's helping-scout technique, but its execution
does not implement the paper's general algorithm (Section 4):

- **Groups and roles.** The paper starts each occupied starting node as a
  group with a leader, elects the strongest leader in slot 1, turns weaker
  leaders into zombies, and merges territories. Python has no leader/zombie
  role or group identifier. It runs one centralized DFS selected by the
  minimum ID and can therefore accept multiple starting nodes without
  simulating the paper's parallel group/merge behavior.
- **Twelve-slot synchrony.** The paper has a repeating 12-slot schedule:
  election (1), settle/level update (2), helping-settler movement (3), probe
  (4–8), zombie chasing (9–10), and DFS movement (11–12). Python executes
  `parallel_probe`, `can_vacate`, group movement, and retrace in ordinary
  Python calls with synthetic round increments; there are no slots, weak/strong
  zombie speeds, or synchronous chase phases.
- **Helping protocol.** In the paper, a same-group settler moves to the leader
  in slot 3, participates in later probe rounds, and is explicitly returned to
  its home in slot 8 (and slot 5 cleanup). Python's `_owner` performs a global
  scan of home fields and infers neighbor status without moving or recruiting
  a helping settler. Its out-and-back scout is not the paper's helping-settler
  protocol.
- **Information model.** The paper permits only co-located communication and
  no node memory. `_owner`, tree reconstruction, and occupancy sets use
  simulator-wide data structures and therefore are centralized shortcuts.
- **Settling and homes.** The paper settles one accompanying zombie at an
  unsettled node and preserves a unique immutable home. Python chooses the
  highest-ID unsettled agent physically at `v`; a `settledScout` can travel as
  part of a group, and retrace centrally uses the recorded homes to rebuild
  the traversal order.
- **Termination.** The paper's general algorithm reaches a configuration of
  waiting leaders/settlers and may use synchronous/global knowledge for
  termination; Appendix A describes propagation for the rooted algorithms.
  Python ends its traversal when `A_unsettled` is empty, performs an internal
  retrace, and returns histories without a legitimate/stationary termination
  flag.

The code does preserve a few paper-shaped ideas—zero-based port 0 represents
the paper's distinguished port 1, homes/parent ports are recorded, and
`settledScout` represents a temporarily traveling settler—but those similarities
do not establish algorithmic or complexity parity.

## 5. Legacy `agent.py` note

`agent.py` contains a third, older role/election implementation with roles
`LEADER`, `FOLLOWER`, `HELPER`, and `CHASER`, and the same numeric status values
as Drop-and-Freeze (`agent.py:9-76`). Its loop performs leader election and
settlement, level increase, helper scouting, scout return/result processing,
chasing, and follower movement, recording snapshots after each helper phase
(`agent.py:443-505`). It is not selected by `simulation_wrapper.py`.

It currently has a direct runtime bug: the leader-election diagnostic at
`agent.py:454-458` refers to undefined `agents_at_node`, so a normal run with a
leader reaches a `NameError` before settlement. Other code assumes settled
state before checking for `None` (for example `check_scout_result`,
`agent.py:298-304`). Treat this file as historical provenance, not as a third
reference algorithm.

## 6. Compatibility decisions for a future port

The following distinctions must remain explicit until a deliberate semantics
decision is recorded:

1. Preserve the two algorithms' different agent state models; do not map
   Drop-and-Freeze's leader/level arrays into Help-by-Scouts homes/tree state.
2. Use a typed result with named algorithm-specific fields rather than the
   wrapper's five positional slots.
3. Decide whether randomized input port labels should affect Drop-and-Freeze;
   current behavior says no because `_init_ports` overwrites them.
4. Define theoretical round, probe, movement, and logical-memory counters
   independently of snapshot count and synthetic `round_number` arithmetic.
5. Decide whether centralized `_owner`, set iteration, and the current
   max-ID/lowest-ID choices are intended semantics or implementation shortcuts;
   add fixtures before changing them.
6. Distinguish successful completion from round-limit termination and from an
   exception/invariant failure. The current APIs do not do so.
