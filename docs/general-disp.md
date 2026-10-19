# GeneralDisp dispersion

`ccm_algorithms::general_disp` implements `GeneralDisp`, the general-setting
HEO-DFS of Sudo, Shibata, Nakamura, Kim and Masuzawa, *Near-linear Time
Dispersion of Mobile Agents* ([arXiv:2310.04376v3](https://arxiv.org/abs/2310.04376v3)),
Section 4 and Algorithms 2–5. Agents may start on any number of nodes. Every
group of co-located agents runs its own HEO-DFS, and groups merge through the
Zombie method of Shintaku et al. Like RootedDisp and P1Tree it is written from
the paper, not from earlier code in this repository.

> **Where it lives.** The algorithm is `crates/ccm-algorithms/src/general_disp.rs`
> and its tests `crates/ccm-experiments/tests/general_disp.rs` (hand traces,
> collisions, probing rules, Lemmas 3 and 4 over a sweep) and
> `crates/ccm-experiments/tests/comparison.rs` (multi-start comparison through
> the runner). The CLI accepts `--algorithm general-disp`; CSV rows carry
> `general_disp` and `algorithm_kind = paper`. `ccm transitions` prints its role
> transitions and merges.

## The model

The model of Section 2, as for [RootedDisp](rooted-disp.md#the-model): an
anonymous, connected, port-labelled graph; `k ≤ n` agents with unique IDs and
no global knowledge; synchronous steps in which an agent sees only the agents
on its own node. The one difference is the start: **any placement is allowed**,
so several agents may share a node and several nodes may hold agents.

Dispersion (Definition 1) is a legitimate configuration: every agent on its own
node, and no agent moves again.

## Groups, roles and memory

A **group** is labelled `(level, leader)` and ordered by level first, then by
leader ID: `(2, 3)` beats `(1, 7)`, and `(2, 7)` beats `(2, 6)`. Every agent
starts as a leader of its own group `(1, id)`.

| Role | What it does |
| --- | --- |
| active leader | shares its node with others and runs the HEO-DFS of its group |
| waiting leader | stands alone on a node and waits for zombies to catch up |
| zombie | a beaten leader. It follows the stronger group's trail until it reaches that group's leader, which settles it |
| settler | holds a node (its home) for its group; `ψ(w)` is the settler of `w` |
| helper | a settler visiting an adjacent probe centre, sent home at the end of the pass |

The Zombie method:

- a leader that meets a stronger leader or settler becomes a **zombie** of that group (line 6);
- a weaker group's settler met by a leader is **incorporated** into the leader's group (lines 9 and 17);
- a leader that meets a zombie of its own level **levels up** by one, and that zombie drops to level 0 (lines 13–16).

Lemma 3 bounds every level by `log₂ k + 1`.

| Variable | Held by | Meaning |
| --- | --- | --- |
| `settled` | every agent | settler (⊤) or not |
| `leader`, `level` | every agent | the agent's group: its own for a leader, the group that beat it for a zombie, its group for a settler |
| `levelL`, `levelS` | a zombie | location level (the level of the settler where it stands) and swarm level (the highest zombie level on its node) |
| `InitProbe` | a leader | start a fresh probe at the next centre |
| `parent` | a leader, `ψ(u)` | the port back to the DFS parent |
| `next`, `checked`, `done` | `ψ(w)` | the probe state, kept between units: a port to move through, the last port checked, the probe is over |
| `help` | a settler | the port at its home leading to the centre it is asked to help |
| `found` | a prober | whether it found a settler of its group in slot 7 |
| return port | a helper | the port at the centre leading back home |

The implementation keeps these in a `GeneralMemory` per agent: 14 words with
the ID (`MEMORY_WORDS_PER_AGENT`).

## The twelve-slot schedule

Time steps are grouped into units of twelve slots: step `t` is in slot
`(t mod 12) + 1` (Section 4.2, Table 2). Each slot is exactly one step, even
when nobody moves, so the step count is the paper's measure of time.

| Slot | Who acts | Lines | Counted in |
| ---: | --- | --- | --- |
| 1 | leaders: leader election; the weaker become zombies | 3–6 | `rounds` only |
| 2 | leaders: settle a zombie or incorporate the settler, level up, start a probe | 8–21 | `rounds` only |
| 3 | settlers with `help` set walk to the probe centre | 61 | `probe_rounds` |
| 4 | the centre checks which recruited helpers came; weaker helpers are dismissed | 31–36 | `movement_rounds` |
| 5 | a finished probe sends its helpers home and sets `done` | 38–40 | `movement_rounds` |
| 6 | probers go out, one port each | 43–47 | `probe_rounds` |
| 7 | probers look for a settler of their group, recruit it, come back | 49–54 | `probe_rounds` |
| 8 | the pass is tallied; helpers go home | 56–59 | `movement_rounds` |
| 9 | weak zombies chase | 63–65 | `movement_rounds` |
| 10 | weak and strong zombies chase | 66–68 | `movement_rounds` |
| 11 | leaders move forward through `next`, or back through `parent` | 23–26 | `movement_rounds` |
| 12 | leaders that moved record `parent ← pin` | 27–29 | `rounds` only |

A zombie is **weak** when its swarm level is below its location level, and
**strong** when they are equal. Weak zombies move in both chase slots, strong
ones only in slot 10. No zombie moves while a leader is on its node. A chasing
zombie follows the `next` port of the settler where it stands.

## How its probing differs from RootedDisp

`RootedDisp` runs the whole `Probe()` loop in consecutive steps. Here one pass
fits into one unit, and the probe state lives in `ψ(w)` between units. Four
rules differ. Each one has its own test in `tests/general_disp.rs`:

| Rule | RootedDisp | GeneralDisp | Test |
| --- | --- | --- | --- |
| Which settler marks a node visited | any settler | only a settler of the probing leader's group (line 49); another group's node counts as unvisited | `a_probe_counts_only_settlers_of_the_leaders_group` |
| How a helper reaches the centre | rides back with its prober | its `help` port is set in slot 7 and it walks over in slot 3 of the next unit | `helpers_follow_help_and_weaker_ones_are_dismissed` |
| A helper of a weaker group at the centre | cannot happen | sent home for good (lines 34–35); a level-up makes this happen to the group's own older settlers | same test |
| A recruited helper that does not come | cannot happen | its port is treated as leading out of the territory (lines 32–33): another group took it over | `a_helper_taken_by_a_stronger_group_is_detected_as_stolen` |

Because of these rules the probing code isn't shared with `rooted_disp.rs`. The
shared parts, `AgentStore` and `apply_move_intents`, already live in
`ccm-core`.

## Decisions where the pseudocode leaves a choice

The same list appears in the module documentation.

1. Line 11 settles "one zombie": the one with the smallest ID. Line 13 picks
   "a zombie of the same level": again the smallest ID.
2. Line 33 reads `ψ(a)`. It is taken as `ψ(w)`, the settler at the centre.
3. Line 49 compares the prober's own `(leader, level)` with the settler's. For
   a zombie prober that is the label of the group it came from, so it would
   almost never match. The prose ("a settler in the same group", Section 4.3)
   is followed instead: the settler is compared with the probing leader's
   group.
4. When several probers in one pass find no same-group settler, the smallest
   port wins (line 57), as in RootedDisp.
5. Probers take ports in ascending agent-ID order (lines 43–47).
6. **Termination.** In this setting the paper leaves termination to global
   knowledge (Note on termination, Section 1.1). The simulation stops as soon
   as the configuration is legitimate:
   - every agent is on its own node;
   - no agent is a zombie;
   - no settler is away from home or still asked to help;
   - no prober is out on a trip.

   From such a configuration no agent moves again. The agents don't detect
   this themselves; the simulator does.
7. A zombie that cannot chase stays put. This is a zombie on a node with no
   settler and no leader, or whose settler has `next = ⊥`. The paper's analysis
   implies it doesn't happen. It is counted in `GeneralStats::stuck_chases`,
   and the tests require that count to be 0.

## Hand trace: two groups meet on a path

This is the first exercise of the task. Path `0 – 1 – 2 – 3 – 4 – 5` with canonical
ports. Agents 0–3 start on node 0 and agents 4–5 on node 3. Groups are written
`level:leader`. The rows are the events of the run, exactly as
`ccm transitions` prints them, and the test `two_groups_meet_on_a_path` checks
them.

```sh
ccm transitions --algorithm general-disp --graph path --nodes 6 --agents 6 \
  --placement explicit:0,0,0,0,3,3
```

| Step | Slot | Group A (leader 3) | Group B (leader 5) |
| ---: | ---: | --- | --- |
| 0 | 1 | agents 0, 1, 2 become zombies of `1:3` | agent 4 becomes a zombie of `1:5` |
| 1 | 2 | settles zombie 0 at node 0. Meets zombie 1 of its own level, so A becomes `2:3` and zombie 1 drops to level 0; settler 0 carries `2:3` | settles zombie 4 at node 3 as `1:5` |
| 5–7 | 6–8 | probes node 1: no settler, so `next = 0` | probes node 2: no settler, so `next = 0` |
| 8 | 9 | zombies 1 and 2 are weak (location 2, swarm 1) but stay, since their leader is there | — |
| 22 | 11 | moves forward to node 1 with zombies 1, 2 | leader 5 moves alone to node 2 |
| 25 | 2 | settles zombie 1 at node 1 | leader 5 is alone: it **waits** |
| 46 | 11 | moves forward to node 2 with zombie 2 | |
| 48 | 1 | | **leader collision**: leader 5 meets the stronger leader 3 and becomes a zombie of `2:3` |
| 49 | 2 | settles zombie 2 at node 2 | |
| 56 | 9 | zombie 5 is weak (location 2, swarm 1) but stays with leader 3 | |
| 70 | 11 | probe at node 2 found B's settler 4 at node 3. B's group isn't A's, so node 3 counts as unvisited: forward to node 3 | |
| 73 | 2 | **incorporates** settler 4 (`1:5` → `2:3`) | B has no members left |
| 94 | 11 | forward to node 4 with zombie 5 | |
| 97 | 2 | settles zombie 5 at node 4 | |
| 130 | 11 | leader 3 moves alone to node 5 and waits there | |

The run is legitimate after step 130, so it ends after **131 steps** (11 units,
the last cut short). Final state: settlers 0, 1, 2, 4, 5 on nodes 0, 1, 2, 3, 4,
all in group `2:3`, and leader 3 waiting on node 5. The counts: 1 level-up, 1
incorporation, 5 settlements and no zombie chase. Every zombie stayed with a
leader.

## Three groups, both kinds of zombie

The collision fixture: a cycle of 8 nodes with canonical ports. Agents 0–7 start
on nodes 1, 5, 5, 4, 4, 1, 4, 1, so the groups are `1:7` at node 1, `1:6` at
node 4 and `1:2` at node 5. The test
`three_groups_collide_and_zombies_chase_by_strength` checks it.

```sh
ccm transitions --algorithm general-disp --graph cycle --nodes 8 --agents 8 \
  --placement explicit:1,5,5,4,4,1,4,1
```

| Step | Slot | Event |
| ---: | ---: | --- |
| 0–1 | 1–2 | the weaker agents on each node become zombies. Groups 7 and 6 each meet a zombie of their level and rise to `2:7` and `2:6`; group 2 stays `1:2` |
| 22 | 11 | the three leaders move: 2 to node 4, 6 to node 3, 7 to node 0 |
| 24 | 1 | **higher level wins**: leader 2 stands on settler 3 of `2:6` at node 4 and becomes a zombie of `2:6` |
| 32 | 9 | zombie 2 is **weak** (swarm 1 < location 2), so it chases already in slot 9, to node 3 |
| 49 | 2 | group `2:6` settles zombie 2 at node 2 |
| 72 | 1 | **equal levels, larger ID wins**: leader 6 meets settler 0 of `2:7` at node 1 and becomes a zombie of `2:7` |
| 81, 93 | 10 | zombie 6 is **strong** (both levels 2), so it chases only in slot 10: to node 0, then to node 7 |
| 97 | 2 | leader 7 settles zombie 6 at node 7 |
| 130 | 11 | leader 7 moves to node 6 and waits there |

There is one weak and two strong chase moves. Some settlers keep their old
labels: settlers 2, 3 and 4 keep `2:6`, and settler 1 keeps `1:2`. Nobody of
`2:7` visits them again, and Definition 1 doesn't require it.

## Running it

From the CLI (see `ccm --help`):

```sh
# One run from an explicit start list: agent i starts on the i-th node, and
# --agents must equal the list length.
ccm run --algorithm general-disp --graph path --nodes 6 --agents 6 --placement explicit:0,0,0,0,3,3

# Role transitions and merges of the same run, one CSV row per event.
ccm transitions --algorithm general-disp --graph path --nodes 6 --agents 6 --placement explicit:0,0,0,0,3,3

# Multi-start comparison: the same seeded graphs and starts for every algorithm.
# help-by-scouts, p1tree and rooted-disp are refused with RequiresRootedStart.
for a in drop-and-freeze help-by-scouts p1tree rooted-disp general-disp; do
  ccm sweep --algorithm $a --graph random-connected:10 --nodes 12,24 --agents 8 \
    --seeds 1,2,3 --placement uniform:3
done
```

`ccm transitions` writes `step,slot,event,agent,node,detail`. The events are:
- `became_zombie`, `settled`, `incorporated` and `level_up`: group changes;
- `forward` and `backward`: the leaders' DFS moves;
- `zombie_levels` and `chase`: zombie bookkeeping and movement.

`detail` holds `name=value` pairs.

**Placements.** Help-by-Scouts, P1Tree and RootedDisp need every agent on one
node. Given a scattered placement, the runner refuses them with
`RequiresRootedStart` before they run. Help-by-Scouts keeps its legacy code
unchanged: its historical multi-start failure is recorded as a parity test (see
[behavior-spec.md](behavior-spec.md)), so the refusal sits in the runner and the
CLI, not in the algorithm. Drop-and-Freeze accepts any placement but may reach
its round limit.

**Round limit.** Unless `--round-limit` is given, GeneralDisp gets
`480 · k · (⌈log₂ k⌉ + 1)²` steps. Theorem 2 bounds the time by
`O(k log k · log τ)`. The other algorithms keep `40 · k`.

## Counters

| Counter | Meaning for GeneralDisp |
| --- | --- |
| `rounds` | time steps, slots included when nobody moves |
| `macro_rounds` | twelve-step units started |
| `probe_rounds` | slots 3, 6 and 7 |
| `movement_rounds` | slots 4, 5 and 8–11 |
| `port_probes`, `probe_out_traversals`, `probe_back_traversals` | one per prober per pass |
| `settlements` | settlers at the end. A leader waiting alone does not settle |
| `chase_operations` | zombie chase moves |
| `peak_logical_memory_words` | `14 · k` |

The typed `GeneralStats` adds:
- group changes: zombifications, incorporations, level-ups and the highest level;
- probing: probe starts and passes, helper recruitments, dismissed and stolen helpers;
- movement: forward and backward moves, weak and strong chase moves, and stuck chases.

## What is not implemented

- `RootedOpt` (Algorithm 6).
- Termination detection by the agents themselves. In this setting the paper
  leaves it to global knowledge, and the simulator stops at the first
  legitimate configuration (decision 6).
