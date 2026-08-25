#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoundKind {
    ProbeOut,
    ProbeBack,
    Movement,
    Scout,
    Vacate,
    Chase,
    Follow,
    Retrace,
    Other,
}

pub trait Metrics {
    fn macro_round(&mut self) {}
    fn logical_rounds(&mut self, _kind: RoundKind, _count: u64) {}
    fn agent_moves(&mut self, _count: u64) {}
    fn group_move(&mut self, _size: usize) {}
    fn port_probe(&mut self) {}
    fn probe_out_traversal(&mut self) {}
    fn probe_back_traversal(&mut self) {}
    fn settlement(&mut self) {}
    fn backtrack(&mut self) {}
    fn scout_operation(&mut self) {}
    fn vacate(&mut self) {}
    fn chase(&mut self) {}
    fn follow(&mut self) {}
    fn retrace(&mut self) {}
    fn observe_state(&mut self, _unsettled: usize, _tree_depth: usize, _owned_nodes: usize) {}
    fn observe_memory(&mut self, _memory: LogicalMemory) {}
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoMetrics;

impl Metrics for NoMetrics {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LogicalMemory {
    pub agent_state_words: u64,
    pub ownership_words: u64,
    pub tree_words: u64,
    pub probe_words: u64,
    pub auxiliary_words: u64,
}

impl LogicalMemory {
    #[must_use]
    pub const fn total_words(self) -> u64 {
        self.agent_state_words
            + self.ownership_words
            + self.tree_words
            + self.probe_words
            + self.auxiliary_words
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ComplexityMetrics {
    pub rounds: u64,
    pub macro_rounds: u64,
    pub probe_rounds: u64,
    pub movement_rounds: u64,
    pub agent_moves: u64,
    pub edge_traversals: u64,
    pub port_probes: u64,
    pub probe_out_traversals: u64,
    pub probe_back_traversals: u64,
    pub settlements: u64,
    pub backtracks: u64,
    pub group_moves: u64,
    pub maximum_group_size: usize,
    pub maximum_simultaneously_unsettled: usize,
    pub maximum_tree_depth: usize,
    pub peak_owned_nodes: usize,
    pub scout_operations: u64,
    pub vacates: u64,
    pub chase_operations: u64,
    pub follow_operations: u64,
    pub retraces: u64,
    pub peak_logical_memory_words: u64,
}

impl Metrics for ComplexityMetrics {
    fn macro_round(&mut self) {
        self.macro_rounds += 1;
    }

    fn logical_rounds(&mut self, kind: RoundKind, count: u64) {
        self.rounds += count;
        match kind {
            RoundKind::ProbeOut | RoundKind::ProbeBack | RoundKind::Scout => {
                self.probe_rounds += count;
            }
            RoundKind::Movement
            | RoundKind::Vacate
            | RoundKind::Chase
            | RoundKind::Follow
            | RoundKind::Retrace => self.movement_rounds += count,
            RoundKind::Other => {}
        }
    }

    fn agent_moves(&mut self, count: u64) {
        self.agent_moves += count;
        self.edge_traversals += count;
    }

    fn group_move(&mut self, size: usize) {
        self.group_moves += 1;
        self.maximum_group_size = self.maximum_group_size.max(size);
    }

    fn port_probe(&mut self) {
        self.port_probes += 1;
    }

    fn probe_out_traversal(&mut self) {
        self.probe_out_traversals += 1;
    }

    fn probe_back_traversal(&mut self) {
        self.probe_back_traversals += 1;
    }

    fn settlement(&mut self) {
        self.settlements += 1;
    }

    fn backtrack(&mut self) {
        self.backtracks += 1;
    }

    fn scout_operation(&mut self) {
        self.scout_operations += 1;
    }

    fn vacate(&mut self) {
        self.vacates += 1;
    }

    fn chase(&mut self) {
        self.chase_operations += 1;
    }

    fn follow(&mut self) {
        self.follow_operations += 1;
    }

    fn retrace(&mut self) {
        self.retraces += 1;
    }

    fn observe_state(&mut self, unsettled: usize, tree_depth: usize, owned_nodes: usize) {
        self.maximum_simultaneously_unsettled =
            self.maximum_simultaneously_unsettled.max(unsettled);
        self.maximum_tree_depth = self.maximum_tree_depth.max(tree_depth);
        self.peak_owned_nodes = self.peak_owned_nodes.max(owned_nodes);
    }

    fn observe_memory(&mut self, memory: LogicalMemory) {
        self.peak_logical_memory_words = self.peak_logical_memory_words.max(memory.total_words());
    }
}
