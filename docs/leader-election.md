# Leader election by agents

`ccm_algorithms::leader_election` elects a leader among `n` agents that stand
one per node on an anonymous graph, by simulating message passing with an
**ID-bit schedule**. It follows Kshemkalyani, Kumar, Molla and Sharma,
*Agent-based Leader Election, MST, and Beyond*
([arXiv:2403.13716](https://arxiv.org/abs/2403.13716); brief announcement at
DISC 2024), Section 2: Lemma 4 and Theorem 1. The problem page is
[Leader election by agents](https://drdebmath.github.io/mobility-matters/problem/leader-election-agents/)
on Mobility Matters.

> **Where it lives.** The algorithm is
> `crates/ccm-algorithms/src/leader_election.rs` and its tests
> `crates/ccm-experiments/tests/leader_election.rs`. The CLI accepts
> `--algorithm leader-election`; CSV rows carry `leader_election` and
> `algorithm_kind = paper`.

Unlike the four dispersion algorithms, the result of a run is not only where
the agents end. Each agent also produces an **output**: whether it is the
leader, and the leader's ID. This is the first algorithm in the repository with
a per-agent output; see [Per-agent output](#per-agent-output).

## The model

From Section 1.2 of the paper and the problem page:

- The graph is simple, connected, undirected and anonymous, with `n` nodes and
  maximum degree `Δ`. Each node `v` labels its edges with local ports
  `0 … δ_v − 1`. The brief counts ports from 1 ("out by port 1 and back"); here
  they are counted from 0, as everywhere in this repository, and the order is
  the same.
- There are `n` agents with distinct IDs. Agents communicate only with agents
  on the same node. Nodes have no memory.
- Agents know `n` and `Δ` (Lemma 4 needs both). They do not know the diameter
  `D`.
- Time is synchronous. In one **round** every agent sees the ID and state of
  each agent on its node and the port it arrived through, updates its own
  memory, and then either stays or moves along one edge. This is the same step
  model as RootedDisp (see [rooted-disp.md](rooted-disp.md#the-model)).
- **Start.** Dispersed: one agent per node, and that node is the agent's
  **home**. A second mode starts rooted, with every agent on one node, and runs
  RootedDisp first; see [Rooted mode](#rooted-mode).

## IDs and bits

The repository numbers agents `0 … n − 1` (`AgentId`). The algorithm gives
agent `i` the paper ID `i + 1`, so IDs are `1 … n` as on the problem page, and
the leader is the agent with ID 1, which is `AgentId(0)`.

Every agent writes its ID in binary with the same number of bits,

```
b = ⌊log₂ n⌋ + 1,
```

adding leading zeros. That is the number of bits of `n` itself, so every ID in
`1 … n` fits. (`⌈log₂ n⌉` bits is one too few when `n` is a power of two: for
`n = 4` the ID 4 is `100`.) For `n = 3`, `b = 2` and the IDs are `01`, `10`,
`11`, as on the problem page.

Block `i` (`i = 1 … b`) uses bit `i` of the ID **counting from the left**, the
most significant bit first. This is the order of Lemma 4's proof ("start from
MSB").

## One run

The election simulates `n − 1` rounds of a message-passing algorithm in which
every node sends its smallest ID seen so far to all its neighbours.

1. **Memory.** Every agent starts with `min = ID` and an empty buffer
   `received = ID`.
2. **Simulated rounds.** Simulated round `r` (`r = 1 … n − 1`) is `b` blocks.
3. **Blocks.** Block `i` is `2Δ` rounds. Its round `2j + 1` (`j = 0 … Δ − 1`)
   is an **out** round and its round `2j + 2` a **back** round:
   - an agent whose bit `i` is 1 and whose home has a port `j` leaves home
     through port `j` in the out round, and goes back through the port it
     arrived by in the back round. This is a **visit**;
   - every other agent stays home: one whose bit `i` is 0 for the whole block,
     one whose home has fewer than `j + 1` ports for those two rounds.
4. **Meeting.** At the start of the back round a visitor looks at the agents on
   the node it is visiting. It takes the message only from an agent whose bit
   `i` is 0: that agent is at home all block, so it is the node's own agent.
   Other visitors, whose bit `i` is 1, are ignored. If there is such an agent,
   the two exchange messages: each adds the other's `min` to its own
   `received`, so `received` becomes the smaller of the two.
5. **The message is `min` as it was at the start of the simulated round.**
   What an agent receives goes into `received`, not into `min`, until the
   simulated round ends; then `min ← received`. Two neighbours may meet in
   several blocks of one simulated round; they then exchange the same message
   again, which changes nothing.
6. **The end.** After simulated round `n − 1` every agent stops. It is the
   leader if `min = ID`, and every agent's output is `min` as the leader's ID.

Two neighbours `u` and `v` have different IDs, so some bit `i` differs. In
block `i` the one with bit 1 visits the other, which is at home with bit 0.
Every edge therefore carries both its messages in every simulated round, and
after `D` simulated rounds every agent's `min` is 1. Since agents do not know
`D`, they run `n − 1 ≥ D` simulated rounds.

Steps 4 and 5 involve no movement and take no round. Applying `received` at the
end of a simulated round is local computation that each agent does when its own
round counter says the simulated round is over.

## Time model

| Round of a block | Who moves | Counted in |
| --- | --- | --- |
| out, `2j + 1` | every agent with bit `i` = 1 and a port `j`, through port `j` | `probe_rounds` |
| back, `2j + 2` | the same agents, back through their arrival port | `probe_rounds` |

Every round of the schedule is counted, **including rounds in which no agent
moves**: on a star whose hub has bit 0, only the out and back rounds for port 0
move anyone, and the other `2Δ − 2` rounds of the block are idle. Lemma 4
charges `2Δ` rounds per block, and the agents need the idle rounds to stay in
step with each other: an agent knows where the schedule is only by counting
rounds.

So a run from a dispersed start takes exactly

```
T = (n − 1) · b · 2Δ  rounds,   b = ⌊log₂ n⌋ + 1.
```

This is Theorem 1's `O(Δ T log n)` with `T = n − 1` simulated rounds, and the
constant is explicit: `b ≤ log₂ n + 1`, so `T ≤ 2Δ (n − 1)(log₂ n + 1)`. The
tests check the exact count.

Every agent knows the leader by round `D · b · 2Δ`, which is the page's
`O(DΔ log n)` (Remark 1 of Kshemkalyani et al., *Faster leader election via
mobile agents*). The agents cannot see that moment; the simulator records it
for the tests only, as `all_knew_round` on the typed result.

`n = 1` is a single agent with no neighbours: `Δ = 0`, no rounds, and the agent
is the leader.

### Agreed decisions

Agreed on 15 November 2026.

| # | Decision | Agreed choice |
| --- | --- | --- |
| 1 | ID width | IDs are `1 … n` (`AgentId + 1`), written in `⌊log₂ n⌋ + 1` bits. This replaces an earlier note of `⌈log₂ n⌉` bits, which is too few when `n` is a power of two. |
| 2 | When a received `min` takes effect | At the end of the simulated round, so the run is a faithful simulation of synchronous message passing. On the hand trace below this gives the page's animation: agent 3's `min` goes 3 → 2 → 1. Using it at once would give 3 → 1 in the first simulated round. |
| 3 | When the election stops | After `n − 1` simulated rounds, a number every agent can compute from `n`. The round by which all agents knew is reported for checking only. |
| 4 | The rooted mode | RootedDisp runs to completion, and the election starts in the next round. Total rounds are RootedDisp's plus the election's. |
| 5 | CSV schema | Unchanged, as for RootedDisp. The per-agent output and the election's own counts are on the typed result, the trace and the WASM output. |

## Rooted mode

The brief asks for a second mode: every agent starts on one node, RootedDisp
disperses them, and the election runs from the resulting placement. Each
agent's home is the node it settled on.

The election starts in the round after RootedDisp completes (decision 4). This
is a simplification: RootedDisp's agents do not learn that the dispersion has
ended (its termination detection, Appendix A of Sudo et al., is not
implemented), so a real agent would wait for a bound computed from `n` instead.
The problem page's total, `O(ΔD log n + n log² n)`, uses the paper's dispersion;
here the total is RootedDisp's steps plus `T`, reported separately as
`dispersion_rounds` and `election_rounds`.

A placement that is neither dispersed (all agents on distinct nodes) nor rooted
(all on one node) is rejected.

## Memory

The message is one ID, so each agent needs `O(log n)` bits. With `w(x)` the
number of bits needed to write `x` (and `w(0) = 1`), an agent stores:

| Variable | Range | Bits |
| --- | --- | --- |
| ID | `1 … n` | `b = w(n)` |
| `n` (known) | | `b` |
| `Δ` (known) | | `w(Δ)` |
| `min` | `1 … n` | `b` |
| `received` | `1 … n` | `b` |
| simulated round | `0 … n − 1` | `w(n − 1)` |
| block | `0 … b − 1` | `w(b − 1)` |
| round in block | `0 … 2Δ − 1` | `w(2Δ − 1)` |
| leader flag | yes / no | 1 |

A visitor goes back through the port it arrived by, which the model shows it
in the back round, so it stores no return port. For the hand trace below
(`n = 3`, `Δ = 2`, `b = 2`) this is 2 + 2 + 2 + 2 + 2 + 2 + 1 + 2 + 1 = 16 bits.

The typed result reports this as `memory_bits_per_agent`. The CSV column
`peak_logical_memory_words` counts the nine variables as words: `9n`. In rooted
mode an agent first holds RootedDisp's six words and keeps only its ID once the
election starts, so the peak is still `9n`.

## Per-agent output

A run's result has always been where each agent ended. This algorithm adds an
optional output per agent:

- `AgentOutput::Leader { is_leader, leader }` in `ccm-core`, where `leader` is
  the `AgentId` of the agent the run elected;
- the typed result and the runner's `RunResult` carry `Option<Vec<AgentOutput>>`,
  `None` for every dispersion algorithm, so their results and golden tests do
  not change;
- the trace gets an event when an agent's output is decided, and the WASM
  output an array with one entry per agent.

Later algorithms reuse it with their own variant; Deepak's MIS, for example,
outputs black or grey.

## Termination

- **Completed.** After `T` rounds. Every agent is home, exactly one agent says
  it is the leader, and every agent's output names it.
- **Round limit.** The limit counts rounds. Reaching it returns the partial
  state and counters as `RoundLimitReached`, with no output. With no limit set
  the runner allows exactly `T` rounds, plus RootedDisp's default limit in the
  rooted mode.
- **Rejected input.** An empty graph, no agents, a number of agents other than
  `n`, a start outside the graph, a placement that is neither dispersed nor
  rooted, or a graph that is not connected.

## Counters

| Counter | Meaning for leader election |
| --- | --- |
| `rounds` | every round of the schedule, idle rounds included; in rooted mode RootedDisp's steps as well |
| `macro_rounds` | simulated message-passing rounds, `n − 1` |
| `probe_rounds` | the election's rounds; each is an out or a back round |
| `movement_rounds` | 0, or RootedDisp's movement steps in rooted mode |
| `port_probes` | visits |
| `probe_out_traversals`, `probe_back_traversals` | one each per visit |
| `agent_moves`, `edge_traversals` | two per visit, plus RootedDisp's in rooted mode |
| `settlements` | 0 from a dispersed start, whose homes are given; `n` in rooted mode |
| `peak_logical_memory_words` | `9n` |

The typed result adds: `b`, `Δ`, the number of simulated rounds and blocks,
`election_rounds`, `dispersion_rounds`, `all_knew_round`, visits that met the
node's own agent, visits that met nobody (that agent was itself visiting), and
`memory_bits_per_agent`.

## Worked example

The problem page's animation: a path of three nodes with agent ID 3 (`11`) at
one end, ID 2 (`10`) in the middle and ID 1 (`01`) at the other end. Here the
path is `0 – 1 – 2` with canonical ports, so node 1's port 0 leads to node 0 and
its port 1 to node 2, and the placement is `explicit:2,1,0` (agent 0, ID 1, on
node 2; agent 1, ID 2, on node 1; agent 2, ID 3, on node 0).

`n = 3`, `Δ = 2`, `b = 2`: a block is 4 rounds, a simulated round 8 rounds,
and `n − 1 = 2` simulated rounds take `T = 16` rounds. Block 1 uses the first
bits (ID 3: 1, ID 2: 1, ID 1: 0) and block 2 the second bits (1, 0, 1).

| Round | Simulated round, block | Moves | Meetings, at the start of the back round |
| ---: | --- | --- | --- |
| 1 | 1, block 1 | ID 3: node 0 → 1 (port 0); ID 2: node 1 → 0 (port 0) | |
| 2 | | both back | ID 3 finds ID 2 away and ID 2 finds ID 3 away: no message |
| 3 | | ID 2: node 1 → 2 (port 1); ID 3 has no port 1 and stays | |
| 4 | | ID 2 back | ID 2 meets ID 1: sends 2, receives 1 |
| 5 | 1, block 2 | ID 3: node 0 → 1; ID 1: node 2 → 1 (both port 0) | |
| 6 | | both back | ID 3 and ID 1 both meet ID 2 and ignore each other: ID 3 sends 3, receives 2; ID 1 sends 1, receives 2 |
| 7–8 | | nobody: ID 3 and ID 1 have no port 1, ID 2's bit is 0 | |
| | end of simulated round 1 | | `min`: ID 1 → 1, ID 2 → 1, ID 3 → 2 |
| 9–16 | 2, blocks 1 and 2 | the same moves | round 12: ID 2 sends 1, receives 1; round 14: ID 3 sends 2, receives 1, and ID 1 sends 1, receives 1 |
| | end of simulated round 2 | | `min` = 1 for every agent |

16 rounds and 10 visits, 5 per simulated round, so 20 agent moves. 6 visits
meet the node's own agent; the 4 that leave in rounds 1 and 9 meet nobody. Agent ID 1 says it is the leader and every agent's output
names ID 1. Agent ID 3's `min` goes 3 → 2 → 1 and agent ID 2's goes 2 → 1, as
in the animation. Every agent knew by round 16; here `D = 2` and
`D · b · 2Δ = 16` too.

Round 6 is the case the problem page warns about: two visitors on the same
node in the same round. Each takes the message only from the agent whose
current bit is 0.

## What is not implemented

From the problem page, out of scope for this task:

- Algorithm 2 of the paper, for agents that know neither `n` nor `Δ`
  (candidates by smallest degree, depth-first walks with tags).
- The `O(n)`-round results, and fewer agents than nodes (`k < n`).
- The impossibility for identical agents.
- Termination detection for the rooted mode's dispersion (see
  [Rooted mode](#rooted-mode)).
