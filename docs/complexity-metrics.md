# Complexity metrics and counting semantics

This document defines the metrics that the Rust simulator should report for the
two CCM dispersion algorithms currently represented by
`agent_drop_freeze.py` (Drop-and-Freeze) and `agent_help_scouts.py`
(Help-by-Scouts).  It deliberately separates three things:

1. the behavior observable in the current Python reference;
2. the theoretical counters proposed for the Rust core; and
3. implementation work (NetworkX lookups, snapshots, JSON serialization, and
   wall-clock time), which is not an algorithmic complexity measure.

The current Python code does not instrument the counters below.  The proposed
Rust counters are therefore a specification for instrumentation, not claims
about values already returned by the Python wrappers.

## 0. Authority and scope of the paper comparison

The primary theoretical reference is [Sudo et al., *Near-linear Time
Dispersion of Mobile Agents*, arXiv:2310.04376v3](https://arxiv.org/abs/2310.04376v3),
especially Section 2 (synchronous model and memory convention), Section 3
(`RootedDisp`, including Algorithm 1 and `Probe`), and Section 4
(`GeneralDisp`, including the 12-slot schedule, Algorithms 2--5, and Theorem
2 in v3).  The paper's rooted bound is `O(k log tau)` time (Theorem 1 in v3)
and its general bound is `O(k log tau log k)` time, where `tau = min(k, Delta)`;
the paper's memory
convention is persistent per-agent memory, not centralized simulator memory.

The paper does **not** define algorithms named Drop-and-Freeze or
Help-by-Scouts, nor does it define `can_vacate` or `retrace`.  The dedicated
Python files are therefore a behavioral reference for this repository, not a
literal implementation to which the paper's theorems may automatically be
applied.  In particular, the paper's `RootedDisp` uses one leader, settlers,
explorers, and a doubling `Probe` (Section 3); its `GeneralDisp` uses leaders,
zombies, helping settlers, and 12 slots (Section 4).  The current
`rooted_async` code has no leader/zombie 12-slot scheduler and instead has
`settledScout`, vacate, and retrace states.  Any Rust result claiming a paper
bound must first establish parity with the corresponding paper algorithm and
assumptions.  Counters below marked **paper** are source concepts; counters
marked **repository/Rust** are proposed instrumentation for the current code.

## 1. Notation and common counting rules

Let:

* `A` be the number of agents;
* `V` be the number of graph nodes;
* `E` be the number of undirected graph edges;
* `d(v)` be the degree of node `v`; and
* `Delta` be the maximum degree.

The graph is a port-labeled graph.  A traversal records both the source node
and the local port used.  The reciprocal port is determined by the graph and
does not create a second traversal.

### 1.1 Agent moves and edge traversals

An **agent-edge traversal** is one agent crossing one graph edge in one
direction.  It increments `agent_moves` and `edge_traversals` by one.  A
return over the same edge is a second traversal.  A move is counted whether
the edge is used for exploration, a probe, a backtrack, a vacate operation, or
retrace.

A **group move event** is one logical operation that sends a set of agents over
one edge.  It increments `group_moves` once and increments
`edge_traversals` by the group cardinality.  It must not be counted as one edge
traversal regardless of group size.  `group_move_agent_count` is the sum of
the group cardinalities.  The current Python `_move_group` calls `_move_agent`
once per member, so this definition is observable from the state transitions
even though there is no counter.

`agent_moves` and `edge_traversals` are aliases under this model.  They are
both retained in the proposed result schema only when compatibility with a
caller requires both names.  A separate `group_moves` counter is not added to
either total.

### 1.2 Probes

A **port probe** is one scout/agent assigned one local port, crossing to that
port's neighbor, reading the neighbor classification, and returning to the
probe origin.  The probe has:

* one `probe_out_traversal`;
* one `probe_back_traversal`; and
* one `probe_operation`.

Thus, for a complete probe, `probe_out_traversals = probe_back_traversals =
probe_operations`, and probe traversals contribute `2 * probe_operations` to
`edge_traversals`.  A port skipped because it is the parent port is not a
probe.  A port with an occupied, vacated, or empty neighbor is still a probe;
the result, not the classification, determines the next action.

The centralized Python implementation may read a result directly (for
example, Help-by-Scouts uses `_owner`), but this does not erase the conceptual
probe.  The direct read is an implementation shortcut and is recorded, if at
all, as implementation work rather than as a reduction in probe complexity.

### 1.3 Synchronous subrounds

A **subround** is one globally ordered synchronous phase in which all actions
of that phase are computed from the same phase-start state and committed at a
barrier.  Concurrent traversals in one subround count separately per agent,
but consume one subround of time.  Local classification and state updates do
not consume a traversal subround unless an algorithm specification explicitly
models a communication phase for them.

The simulator must expose logical phase counters rather than deriving them
from trace frames.  Trace frames are optional and may be sampled.

In the paper model, one synchronous **time step** is one global step in which
an agent may make one atomic edge move (or stay put) and communicate only with
co-located agents; agents are never represented as being partway along an edge
(Section 2).  A paper `time_steps` counter must therefore be distinct from
repository trace-frame counts and from an implementation loop counter.  For
the paper's `RootedDisp`, a probe round trip is explicitly two time steps
(Section 3, `Probe` discussion).  For its `GeneralDisp`, twelve time steps
form one repeating slot unit (Section 4.2, Table 2), with slots 1--2 for
leader election/settling, slot 3 for helping-settler movement, slots 4--8 for
probing, slots 9--10 for zombie chase, and slots 11--12 for DFS forward or
backward movement.

### 1.4 Termination and maxima

`completed` is true only when the algorithm's completion predicate holds.
Hitting a configured round limit is `round_limit`, not completion.

The following maxima are measured over the live algorithm state at phase
boundaries and, where relevant, immediately before and after a transition:

* `maximum_group_size`: largest set sent by one group move event;
* `maximum_simultaneously_unsettled`: largest number of agents whose logical
  status is not settled (a transient `SETTLED_WAIT` is included);
* `maximum_tree_depth`: largest parent-tree depth reached; and
* `peak_active_probe_records`: largest number of probe-result records live in
  the algorithm's auxiliary state.

These are not process-RSS measurements.  Allocation, hash-table capacity,
Python object headers, Rust allocator overhead, and trace history are reported
separately as engineering measurements when needed.

## 2. Drop-and-Freeze

### 2.1 Current Python phase semantics

`agent_drop_freeze.run_simulation` first initializes node occupancy, resets
agent state, and records one `start` snapshot.  It then executes up to the
requested number of iterations.  The loop stops before starting an iteration
if every agent already has status `SETTLED`.

Each executed iteration is one **Drop-and-Freeze macro-round** with exactly
three synchronous subrounds, in this order:

1. **`probe_out`**: at each node, unsettled agents are sorted by ID and are
   assigned distinct ports beginning at the settled agent's DFS cursor (or
   port zero when no settled agent exists).  Each assigned agent crosses one
   edge and becomes `SETTLED_WAIT`.  A node with no settled agent and exactly
   one unsettled agent, and a degree-zero node, do not probe.
2. **`probe_back`**: every `SETTLED_WAIT` agent tests whether its current node
   has a `settled_agent`, saves that Boolean result, and crosses one edge back
   to `probe_home`.  It becomes `UNSETTLED`.
3. **`move_out`**: first, one mover is settled at each currently empty node.
   Remaining movers at a node then move together over the first port confirmed
   empty by the probes and compatible with the settled agent's DFS cursor.  If
   all ports have been exhausted, the group backtracks through the settled
   agent's `parent_port` when one exists.  A settlement itself is a local state
   change and consumes no edge traversal; every selected group member crossing
   an edge is an agent-edge traversal.

The Python code commits the probe-out and probe-back lists after constructing
the full list of actions, so those two phases are simultaneous with respect to
movement.  `move_out` also constructs a plan before executing planned moves,
but its node iteration and list/set bookkeeping are centralized implementation
details and must not be treated as extra theoretical rounds.

The Python observable trace contains one start frame plus three labeled frames
per executed macro-round: `1 + 3R`, where `R` is the number of iterations that
actually began.  This is a trace-frame count, not a round counter.

There is no corresponding Drop-and-Freeze algorithm or theorem in Sudo et
al. v3.  The `3R` accounting is therefore a repository semantic definition;
it must not be reported as the paper's `RootedDisp` time without a separate
proof of behavioral equivalence.

### 2.2 Proposed Drop-and-Freeze counters

For `R` executed macro-rounds, the canonical counters are:

* `macro_rounds = R`;
* `subrounds = 3R`;
* `probe_out_subrounds = R` and `probe_back_subrounds = R`;
* `move_out_subrounds = R`;
* `probe_operations`: number of agents assigned a probe port;
* `probe_out_traversals` and `probe_back_traversals`: one each per probe;
* `settlements`: number of agents changed to `SETTLED` in move-out phase 1;
* `group_moves`: number of distinct source-node/port group transitions in
  move-out phase 2;
* `backtracks`: number of those group transitions that use `parent_port`;
* `group_move_agent_count`: sum of the number of movers in those transitions;
* `agent_moves = edge_traversals`: all probe and move-out edge crossings;
* `maximum_group_size`, `maximum_simultaneously_unsettled`, and
  `peak_active_probe_records` as defined above; and
* `maximum_tree_depth` only if Rust stores an explicit depth; the Python
  reference stores parent ports but does not maintain a depth counter.

`probe_operations` counts an assigned probe even when the destination is
occupied.  It does not count the later DFS choice as another probe.  A failed
probe result may advance `next_port_to_try`, but cursor advancement is not a
traversal.

The Python variable `planned_moves` contains one tuple per agent, even when
the tuples form one logical group.  Rust should derive `group_moves` by
coalescing equal `(source, destination, phase)` transitions; it should not
report `len(planned_moves)` as the group count.  Conversely, every tuple does
represent one agent traversal when executed and therefore contributes one to
`edge_traversals`.

### 2.3 Drop-and-Freeze logical memory

The word model is shared by both algorithms:

* one `Word` holds one dense `AgentId`, `NodeId`, `PortId`, enum/status,
  Boolean, or bounded integer/cursor;
* an optional value reserves one word (the presence bit is folded into that
  word); and
* immutable input graph topology and port labels are not algorithmic working
  memory.  Trace history, snapshots, and metrics are also excluded.

Report two memory views. `persistent_words_per_agent` follows the paper's
Section 2 Note 1 convention: maximum state an agent carries across an edge
move, including its identifier, excluding node-local or centralized scheduler
working memory. `logical_total_words` is the repository experiment view and
includes explicitly modeled node owners and auxiliary records. The latter is
useful for comparing Rust representations but is not the paper's per-agent
space bound. Conservatively, one word has
`ceil(log2(max(A,V,Delta)+1))` bits in this simulator. An anonymous-graph
implementation must not silently turn a globally stored `NodeId` into a claim
about the paper's `O(log(k+Delta))` per-agent bound.

For Drop-and-Freeze, count the following transition-visible local fields per
agent (nine words when dense identity and simulator location are counted):

| Field | Words |
| --- | ---: |
| agent identity and current node | 2 |
| status | 1 |
| `probe_home`, `probe_port`, `probe_result_empty` | 3 |
| `entry_pin`, `parent_port`, `next_port_to_try` | 3 |
| **total per agent** | **9** |

Under the paper-compatible persistent view, exclude the physical simulator
location and retain the other eight words. This is a repository field count,
not a claim that Drop-and-Freeze meets a Sudo et al. theorem: the paper's
algorithms have a different state machine and assume no node-local whiteboard.

The node owner (`settled_agent`) is one word per node, for `V` words, in the
repository logical view. Node status (`EMPTY`/`OCCUPIED`) is derivable from that
owner in the proposed Rust model and is not counted twice. Therefore the
proposed logical working-memory bound is:

```text
Drop-and-Freeze: 9A + V words
```

If IDs are implicit array indexes, subtract `A` from this expression; the
chosen convention must be held constant across experiments.  The compatibility
fields `state.level`, `state.leader`, `state.home`, and `pin` exist in the
Python `Agent` object but are not read by the dedicated Drop-and-Freeze
transition rules.  They are excluded from the algorithmic total.  A faithful
object-for-object port may report them as an implementation-state addendum,
not as a different theoretical bound.

Probe scratch is already charged through the per-agent probe fields.  The
Python `node_to_agents`, `unsettled_by_node`, `planned_moves`, and temporary
port lists are centralized scheduler scratch; report their peak separately if
engineering memory is studied, but do not call it distributed algorithmic
memory.

## 3. Help-by-Scouts

### 3.1 Current Python control flow

Help-by-Scouts is not implemented as a fixed number of synchronous phases per
outer loop.  `rooted_async` maintains an internal `round_number` and repeatedly
activates the lowest-ID agent in `A_unsettled ∪ A_vacated` at the current
vertex.  An activation consists of:

1. settling an agent at an unowned node, if necessary, and assigning its
   parent breadcrumbs;
2. `parallel_probe` of the current node's ports;
3. classifying the node as `visited`, `partiallyVisited`, or `fullyVisited`;
4. `can_vacate`, which may leave the owner settled, turn it into a
   `settledScout`, or move it through a port-1/parent path; and
5. one forward group move through the selected port, or one group backtrack
   through the parent port.  A forward arrival may reconfigure a partially
   visited node.

After `A_unsettled` is empty, `retrace` walks the parent tree in DFS order,
moving the `A_vacated` scouts as a group and re-settling each scout at its
home.  The current code invokes `retrace` once even when the vacated set is
empty.

In `parallel_probe`, at most `s = |A_scout|` scouts are intended to probe
distinct ports concurrently.  Each assigned scout crosses out and back.  The
parent port is skipped.  The implementation obtains the neighbor owner through
the centralized `_owner` scan rather than walking the port-1 structure used by
the distributed description.

The returned `rounds_max + 2` values from `parallel_probe` and the `2`/`4`
values returned by `can_vacate` are internal scheduling hints.  They are not a
stable public theoretical round definition: the arithmetic mixes snapshots,
local decisions, and edge crossings, and `run_simulation` returns no round
counter.  In addition, `run_simulation` overwrites its `max_rounds` argument
with `40 * len(agents)`.

### 3.2 Proposed Help-by-Scouts round semantics

There are two valid, but different, semantic targets.  They must not be
silently mixed.

**Repository/Rust model for the current `rooted_async` transitions.**  The
Rust core should use two explicit levels of time:

* A **DFS activation** (also called a macro-step) is one execution of the
  `rooted_async` body for a selected current vertex.  It includes local
  settle/classify/vacate decisions, all probe batches at that vertex, and at
  most one forward or backtrack group edge.  `dfs_activations` counts these
  activations.  It is an algorithmic work counter, not a synchronous round.
* A **traversal subround** is one synchronous edge-crossing phase.  One probe
  batch has an outbound subround and a return subround.  A forward group move,
  a backtrack group move, a vacate-path crossing, and each retrace tree-edge
  crossing each consume one traversal subround.  Agents in one group cross
  concurrently but contribute separately to traversal counts.

For a node `x`, let `q_x` be the number of ports eligible for probing after
excluding the parent port, and let `s_x = |A_scout|` at probe start.  Under the
proposed bulk-synchronous semantics, the number of parallel probe batches is:

```text
B_x = 0                         if q_x = 0
B_x = ceil(q_x / s_x)           if q_x > 0 and s_x > 0
```

`s_x = 0` while `q_x > 0` is an invariant violation/blocked execution, not an
implicit zero-cost probe.  Each batch increments `parallel_probe_phases` by
one, `probe_out_subrounds` by one, and `probe_back_subrounds` by one.  Each
assigned scout increments `scout_operations`, `probe_operations`, one
`probe_out_traversal`, and one `probe_back_traversal`.  Thus probe parallelism
reduces time (`B_x` batches) but does not reduce the number of probe records or
edge traversals.

This definition intentionally does not copy the current `round_number` math.
If compatibility requires exposing it, call it `python_internal_round_hint`
and never label it `rounds` in research output.  The canonical Help-by-Scouts
round fields should be:

* `dfs_activations`;
* `probe_batches` / `parallel_probe_phases`;
* `traversal_subrounds`;
* `retrace_subrounds`; and
* `total_logical_subrounds`, the sum of all traversal subrounds and any
  explicitly modeled local communication barriers.

The default proposed model charges no subround to a local settle, node-type
classification, or state-only vacate decision.  If the theoretical paper
requires a communication round for one of these decisions, add a named phase
counter and add it consistently; do not hide it in `dfs_activations`.

**Paper model.**  If this code is instead brought into parity with Sudo et
al.'s algorithms, report `paper_time_steps` directly.  For rooted `RootedDisp`
(Section 3, Algorithm 1), each concurrent probe round trip is exactly two time
steps; the number of explorers doubles between probe iterations, and the
probe takes `O(log tau)` time.  A DFS forward or backward move is one time
step.  For general `GeneralDisp` (Section 4.2 and Table 2), report the repeating
12-slot schedule exactly: `slot_1_2_steps`, `helping_settler_steps` (slot 3),
`probe_steps` (slots 4--8), `chase_steps` (slots 9--10), and
`dfs_move_steps` (slots 11--12), with
`paper_time_steps = 12 * slot_units + partial_final_unit`.  Theorem 1 and
Theorem 2 in v3 apply to those paper algorithms, not automatically to the current
Python `rooted_async` control flow.

### 3.3 Help-by-Scouts operation counters

The proposed counters are:

* `scout_operations` / `probe_operations`: one per assigned scout-port pair;
* `probe_out_traversals` and `probe_back_traversals`: one per scout operation;
* `vacate_attempts`: one call to `can_vacate`;
* `vacates`: one successful logical transition of an owner into the
  vacated/scout set (`settledScout` and `A_vacated`).  A temporary port-1
  excursion that returns to the same home is still a vacate attempt, but is a
  successful `vacate` only if the state becomes `settledScout`/`A_vacated`;
* `vacate_traversals`: every agent-edge crossing performed inside
  `can_vacate`, including a return crossing and the second member of a
  two-agent `_move_group`;
* `forward_group_moves`: one per `rooted_async` forward `_move_group`;
* `backtrack_group_moves`: one per `rooted_async` backtrack `_move_group`;
* `group_moves`: all forward, backtrack, vacate group, and retrace group move
  events, with the phase-specific counters retained as subsets;
* `agent_moves = edge_traversals`: every `_move_agent` crossing, including
  probes, vacate paths, exploration group moves, and retrace moves;
* `retrace_runs`: one per invocation of `retrace`;
* `retrace_group_moves`: one per tree-edge group transition in retrace;
* `retraces`: one per vacated scout re-settled at its home (a settlement event,
  not an edge crossing); and
* `retrace_traversals`: the agent-edge crossings made by the retrace group.

The names `chase` and `follow` do not occur as operations or roles in the
dedicated `agent_help_scouts.py` implementation.  To make future Rust reports
unambiguous, use these definitions only if the algorithm model explicitly
retains those labels:

* `follow_operations`: one for each non-head agent following the deterministic
  head of a group across one group-move edge.  `follow_operations` is therefore
  a count of follower participations, while their edge crossings are already
  included in `edge_traversals`.
* `chase_operations`: one when a settled scout is explicitly brought into a
  vacated group to catch/join it (the V5 `psi_z` branch is the current closest
  analogue).  Count the join event, not each edge crossing.

The current Python code has no explicit head, follower, or chase marker, so
`chase_operations` and `follow_operations` are **undefined in the current
observable output**, not zero.  A Rust implementation must either expose the
definitions above as a declared modeling choice or omit the counters.  It
must not infer them from arbitrary set iteration order.

For comparison, the paper's `GeneralDisp` has a precise **paper chase**: a
zombie that is not accompanying a leader moves through the current settler's
`next` port in slots 9 and/or 10 (weak zombies move in both slots; strong
zombies only in slot 10; Section 4.1--4.2 and Algorithm 5).  Count one
`paper_chase_step` per zombie-edge crossing and one `paper_chase_operation`
per zombie that takes such a scheduled move.  The paper does not use
`follow_operations` as a named counter; an agent accompanying a leader is
counted as a group member traversal in slot 11/12.  If a Rust implementation
uses `follow_operations`, define it as one non-head group-member
participation, as above, and report it as repository instrumentation rather
than a paper metric.

For a paper-compatible implementation, also retain these disjoint movement
subtotals: `paper_probe_round_trips` (one explorer's out-and-back visit),
`paper_probe_out_steps` and `paper_probe_back_steps` (one time step each per
round trip), `paper_helping_settler_steps` (slot-3 moves to the probe center
and slot-8 returns), and `paper_dfs_forward_steps`/`paper_dfs_backward_steps`
(slot-11/12 leader or group moves).  These are synchronous time-step
subtotals; they are not the repository's `vacate_traversals` or
`retrace_traversals`.  In the paper's rooted algorithm a probe round trip is
exactly two steps (Section 3); in its general algorithm the helping-settler and
probe movements occupy the distinct slots in Table 2 (Section 4.2).

### 3.4 Help-by-Scouts logical memory

Using the common one-word model, count the transition-visible `Agent` fields as
follows.  Scalar fields (ID, node, state, home, node type, parent ID, parent
port, port-at-parent, arrival port, vacated-neighbor flag, previous ID, child
port, recent port, scout port, scout edge type, return port, and checked count)
use 17 words per agent.  `scoutResult` and `probeResult` are each a four-word
record `(port, edge type, node type, owner ID)`, adding eight words per agent.
The fixed per-agent total is therefore 25 words.

`probeResultsByPort` is a variable map of four-word probe records.  Let `P` be
the peak number of live records in this map; under the proposed single-active-
vertex execution, `P <= Delta` (and normally `P <= d(x)` for the current
vertex).  The proposed logical bound is:

```text
Help-by-Scouts: 25A + 4P + 2S words
```

where `S` is the maximum retrace DFS stack depth and two words per stack frame
represent the current tree vertex and next-neighbor cursor.  If retrace is
implemented as a distributed parent-pointer walk with no explicit stack, set
`S = 0` and report `maximum_tree_depth` separately.  Membership in
`A_unsettled` and `A_vacated` is represented by agent status in the logical
model and is not charged a second word per agent.

The Python `probeResultsByPort` dictionary, `owners` map, `tree_neighbors`
lists, `frames`, node agent sets, and full snapshot arrays are centralized
implementation structures.  Their peak allocation may be benchmarked, but it
must not be substituted for the formula above.  The `_owner` scan is likewise
time work (`O(A)` in the current code), not an additional theoretical memory
entry.

Under the paper-compatible persistent view, exclude the simulator location and
the per-node `probeResultsByPort`/retrace structures; the fixed carried state is
24 words per agent in this conservative field model. The paper's actual
`GeneralDisp` uses a different constant-size set of variables and proves
`O(log(k+Delta))` bits per agent (Theorem 2 in v3); the 24-word value here is only an
accounting baseline for the current Python fields. The centralized `_owner`
scan and node dictionaries are not substitutes for the paper's local
communication model.

## 4. Current wrapper/schema behavior and ambiguities

The current Python outputs are not a complexity API:

* Drop-and-Freeze returns
  `(all_positions, all_statuses, all_leaders, all_levels, all_node_states)`.
* Help-by-Scouts returns
  `(all_positions, all_statuses, all_node_states, all_homes, all_tree_edges)`.
* `simulation_wrapper.py` assigns both tuples to the same names.  Therefore,
  for Drop-and-Freeze, the third slot is actually leader history and the fourth
  is level history, despite being assigned to `all_node_settled_states` and
  `all_homes`.  This is the known five-return-value mismatch and must not be
  copied into typed Rust state.
* Drop snapshots contain `start` plus three labels per executed macro-round;
  Help snapshots are mutation-oriented and may be overwritten/merged by
  `_snapshot`.  `SIM_DATA.rounds` is the number of stored position entries,
  not a theoretical round count.
* Help's `all_node_states` entries are initialized as empty lists in the
  current snapshot path, so their presence does not imply node-state data.
* `simulation_wrapper.py` uses NetworkX and optional visualization histories;
  `simulation_parallel.py` measures a different leader-election algorithm and
  its `edge_traversals` field must not be reused for either CCM algorithm.

These behaviors are useful compatibility observations, but the Rust result
must use explicit algorithm variants and typed metric fields.

## 5. Messages, runtime, and reporting discipline

No current CCM implementation sends explicit messages.  Do not report
`message_complexity` unless the Rust model defines a message event and states
whether one broadcast, one recipient delivery, or one edge crossing is one
message.  A probe is not automatically a message count.

Wall-clock time, Python snapshot cost, JSON size, NetworkX lookup time, and
allocator/RSS measurements are engineering metrics.  They may be emitted in a
separate benchmark result, never as substitutes for rounds, moves, probes, or
logical memory words.

Every research row should include algorithm/version, graph and port model,
placement model, `A`, `V`, `E`, seed, termination reason, and the counters
defined above.  Raw per-run counters must be retained; averages alone cannot
establish worst-case complexity.
