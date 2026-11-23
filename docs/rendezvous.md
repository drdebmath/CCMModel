# Rendezvous of two agents

`ccm_algorithms::rendezvous` implements deterministic rendezvous of two labelled
agents in a tree, after Dessmark, Fraigniaud, Kowalski and Pelc,
*Deterministic Rendezvous in Graphs* (Algorithmica 46, 2006), in the simplified
form where both agents start in the same round. It is the first algorithm in
this repository that is not a dispersion: the agents start apart and the run
ends when they meet.

> **Where it lives.** The algorithm is `crates/ccm-algorithms/src/rendezvous.rs`
> and its tests `crates/ccm-experiments/tests/rendezvous.rs`. The CLI accepts
> `--algorithm rendezvous` with `--labels A,B` (default `1,2`); CSV rows carry
> `rendezvous`, `algorithm_kind = paper`, the labels in the `labels` column and
> the peak memory of the more demanding agent in `memory_bits_per_agent`. In
> the browser it is "Rendezvous (2 agents)": a panel under the playback bar
> shows each agent's string with the current block highlighted, and whether it
> is exploring or waiting.

## The model

- The graph is a tree with `n` nodes, anonymous, with local port labels
  `0 … deg(v) − 1` at each node, chosen independently of the neighbours. Ports
  are numbered from 0 here; nothing in the algorithm depends on the offset.
- Two agents with different labels from `{1, …, L}` start on two different
  nodes in the same round. Each knows only its own label: not `n`, not `L`, not
  the other label. `λ` is the smaller label.
- Time is synchronous. In a round each agent either stays or moves along one
  edge; an agent sees the degree of its node and the port it entered by.
- The agents **meet** when they stand on the same node at the end of a round.
  Crossing each other inside an edge in the same round is not a meeting, and the
  simulation never looks inside an edge.

The simulator checks that the graph is a tree before the first round (`is_tree`)
and refuses anything else with `NotATree`, along with a count of agents other
than two, equal or zero labels, and two agents on one start node.

## The algorithm

1. **First tour.** Explore the tree by depth-first search and come back to the
   start node. This takes `2(n − 1)` rounds, which the agent counts; that count
   is the length of a **block**.
2. **String.** Write the label in binary, write every bit twice and append
   `0 1`. 5 = `101` becomes `11 00 11 01`; 1 becomes `11 01`; 3 becomes
   `11 11 01`.
3. **Blocks.** Go through the string one symbol per block: on a 1, explore the
   tree again and come back; on a 0, wait at the start node for the whole block.
4. **Why they meet.** The doubled bits and the closing `01` make the strings
   prefix-free, and two different labels give strings that differ before the
   shorter one ends, at a symbol that is 1 in one string and 0 in the other
   (`two_strings_disagree_before_the_shorter_one_ends` checks every pair up to
   64). In that block one agent waits at its start node while the other's tour
   visits every node, so it comes to the waiting agent.

Both agents have the same block length, since it depends only on the tree, and
both start in the same round, so their blocks line up exactly.

## Decisions

The task description leaves some choices open. These are the ones taken:

1. **The tour is the basic walk.** Leave the start node by port 0, and leave
   every node entered by port `p` by port `(p + 1) mod deg`. In a tree this is a
   depth-first search that crosses every edge once in each direction and ends on
   the start node after exactly `2(n − 1)` moves. Every tour of an agent is the
   same walk.
2. **Ending the first tour without knowing `n`.** In the first tour the agent
   cannot count to `2(n − 1)`, so it keeps a stack of the ports leading back up
   the tree. It leaves a node by the port on top of the stack only to go up,
   and then pops it; any other move goes down, and it pushes the port it
   entered by. The tour is over when the stack is empty and the agent entered
   its start node by the start node's last port. The rounds counted so far are
   the block length; the stack is then dropped, and later tours are counted.
3. **After the string ends** the agent waits at its start node for good. The
   agents always meet before that; if both strings ended without a meeting the
   run would stop with `NeverMet`, which would mean the simulation is wrong.
   (Gathering will need its own rule here; the task fixes it as "wait at the
   start node".)
4. **Blocks are numbered from 0.** Block 0 is the first tour; block `b ≥ 1` is
   symbol `b` of the string, counting from 1. Round `r` belongs to block
   `⌊(r − 1) / 2(n − 1)⌋`.
5. **The string is never stored.** `symbol(label, i)` computes symbol `i` from
   the label when a block starts, so the agent's memory holds the label and an
   index, not the string.

## Time

The exact limit of the task follows from the steps: `2(n − 1)` rounds for the
first tour plus `2(n − 1)` for every symbol of the shorter string, whose length
is `2⌈log₂(λ + 1)⌉ + 2`:

    round_bound = 2(n − 1) · (1 + min(|s(a)|, |s(b)|))

That is `O(n log λ)`. `rendezvous::round_bound` computes it, every test checks
the meeting round against it, and the CSV runner's default round limit is twice
it, so a run that overshoots still finishes and the overshoot is visible in the
`rounds` column. Dessmark et al. reach `O(n + log λ)` on trees, even with a
delay between the starts; that algorithm is not implemented here.

Every round is one call to `apply_move_intents`: both agents decide from their
own `Walker` and the degree of their node, and the moves are committed together.
`rounds` counts rounds, waiting ones included; `agent_moves` counts edge
traversals.

## Memory

An agent's memory is its `Walker`: the label, the stage (first tour, symbol
index, or finished), the block length, a round counter, and the port it last
entered by; plus, in the first tour only, the stack of up-ports.
`Walker::memory_bits` counts the peak as

    bits(L) + bits(|s|) + 2 · bits(2(n − 1)) + bits(Δ − 1) · (1 + peak stack depth)

which is `O(log L + log n)` outside the first tour and `O(log L + D log Δ)`
during it, where `D` is the depth of the tree below the start node. The CSV's
`memory_bits_per_agent` reports the larger of the two agents' peaks, and
`peak_logical_memory_words` the repository's word count: five words per agent
plus the stacks.

## Hand traces

Both are tests in `crates/ccm-experiments/tests/rendezvous.rs`.

### Labels 1 and 3 on a six-node tree

The tree has edges 0–1, 1–2, 1–3, 3–4 and 3–5 with canonical ports, ordered by
neighbour ID: node 1 has ports 0 → 0, 1 → 2, 2 → 3; node 3 has 0 → 1, 1 → 4,
2 → 5. Label 1 starts on node 0 and label 3 on node 5. `n = 6`, so a block is
10 rounds. The strings are `11 01` and `11 11 01`, and the limit is
`10 · (1 + 4) = 50` rounds.

| Rounds | Block | Label 1 (from 0) | Label 3 (from 5) |
| --- | --- | --- | --- |
| 1–10 | 0, first tour | 1 2 1 3 4 3 5 3 1 0 | 3 1 0 1 2 1 3 4 3 5 |
| 11–20 | 1, symbols 1 / 1 | the same tour | the same tour |
| 21–30 | 2, symbols 1 / 1 | the same tour | the same tour |
| 31–33 | 3, symbols **0** / **1** | waits on 0 | 3 1 **0** |

In the first three blocks the two walks never put both agents on one node at
the end of a round. In block 3 label 1 waits while label 3 explores, and label
3's walk reaches node 0 in the block's third round: **they meet on node 0 in
round 33**, block 3. Label 3's first-tour stack goes `[2]`, `[2,2]`, `[2,2,0]`,
`[2,2]`, `[2,2,0]`, `[2,2]`, `[2]`, `[2,0]`, `[2]`, `[]`, and it entered node 5
by port 0 = `deg(5) − 1`, which ends the tour at round 10.

### Crossing inside an edge

On the two-node path 0–1 with labels 1 (`11 01`) and 2 (`11 00 01`) starting
on nodes 0 and 1, a block is 2 rounds. In blocks 0–2 both agents explore and
swap ends every round, crossing inside the edge: never a meeting. In block 3
both strings have 0 and both wait. In block 4 label 1 has 1 and label 2 has 0,
so label 1 walks onto node 1: they meet in round 9, within the limit of
`2 · (1 + 4) = 10`.

## Tests

- The task's string examples, and that every pair of labels up to 64 disagrees
  before the shorter string ends.
- Both hand traces, round by round.
- Every label pair up to 64, from three start pairs on three trees, meets
  within the exact limit.
- Every tree family (path, star, two balanced trees and the random tree) at
  sizes 2 to 200, with every port assignment, several seeds, and sampled start
  pairs and labels, meets within the exact limit, and both agents stand on the
  meeting node.
- From the trace: the meeting is the first round that ends with both agents on
  one node, every block ends with each agent on its start node, and an agent
  moves exactly in the rounds of its exploring blocks.
- The walker learns the block length and drops its stack; memory stays within
  the count above.
- Inputs outside the model are refused; the round limit stops a run and says
  so; tracing changes nothing (`all_algorithms.rs`).

## New in the core for this algorithm

- **A random tree family.** `GraphFamily::RandomTree` (CLI `--graph
  random-tree`, the page's "Random tree") decodes a seeded random Prüfer
  sequence, which gives every labelled tree on `n` nodes with equal
  probability, so the shapes range from paths to stars. Before it the only
  trees were the path, the star and the balanced trees.
- **Labels chosen by the run.** `RunRequest::labels` (CLI `--labels`, the
  page's Labels box, `WasmSimulation::set_labels`); empty means 1, 2, …. Only
  rendezvous reads them, and the dispersion algorithms' results are unchanged:
  the golden sweep differs only by the two appended, empty columns.
- **The meeting as the end condition.** The run completes when the agents
  meet. The WASM output reports the meeting round and node, the block length,
  the exact limit and the memory per agent.

## What is not implemented

- **A delay between the starts.** The paper handles agents that wake up in
  different rounds; here both start in round 0.
- **The `O(n + log λ)` tree algorithm** of the paper, and rendezvous in general
  graphs.
- **More than two agents.** Gathering, which builds on this schedule (the
  `Walker` type is public for it), is a separate task.
