//! GeneralDisp view data: roles, groups, slots and merge events decoded from
//! the `ccm-wasm` group stream (see `encode_groups` there).

use super::{Frame, Reader};

pub const ROLE_NAMES: [&str; 6] = [
    "settler",
    "helper",
    "active leader",
    "waiting leader",
    "weak zombie",
    "strong zombie",
];

pub const ROLE_COLORS: [&str; 6] = [
    "#39d3b4", "#f6b44f", "#ff7183", "#ffa66b", "#b69cf5", "#7c5cd6",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoleChange {
    pub frame: u64,
    pub agent: u32,
    pub role: u8,
    pub level: u32,
    pub leader: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupEvent {
    pub frame: u64,
    pub slot: u8,
    pub code: u8,
    pub agent: u32,
    pub node: u32,
    pub fields: [u32; 4],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    pub roles: Vec<RoleChange>,
    pub events: Vec<GroupEvent>,
}

impl Groups {
    /// `None` for an empty or malformed stream.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader::new(bytes);
        let (version, kind) = (reader.u8()?, reader.u8()?);
        let _ = (reader.u8()?, reader.u8()?);
        if (version, kind) != (1, 4) {
            return None;
        }
        let mut groups = Self::default();
        for _ in 0..reader.u32()? {
            let record = reader.u8()?;
            let frame = reader.u64()?;
            let slot = reader.u8()?;
            match record {
                1 => groups.roles.push(RoleChange {
                    frame,
                    agent: reader.u32()?,
                    role: reader.u8()?,
                    level: reader.u32()?,
                    leader: reader.u32()?,
                }),
                2 => groups.events.push(GroupEvent {
                    frame,
                    slot,
                    code: reader.u8()?,
                    agent: reader.u32()?,
                    node: reader.u32()?,
                    fields: [reader.u32()?, reader.u32()?, reader.u32()?, reader.u32()?],
                }),
                _ => return None,
            }
        }
        Some(groups)
    }

    /// Each agent's `(role, level, leader)` in the frame of `step`.
    pub fn at(&self, step: u64, agents: usize) -> Vec<(u8, u32, u32)> {
        let mut shown: Vec<(u8, u32, u32)> = (0..agents).map(|a| (2, 1, a as u32)).collect();
        for change in self.roles.iter().take_while(|change| change.frame <= step) {
            if let Some(entry) = shown.get_mut(change.agent as usize) {
                *entry = (change.role, change.level, change.leader);
            }
        }
        shown
    }
}

pub fn group_color(leader: u32) -> String {
    format!("hsl({:.0} 75% 55%)", (f64::from(leader) * 137.5) % 360.0)
}

/// The slot and unit of the step that produced the frame at `step`.
pub fn slot_text(step: u64) -> String {
    if step == 0 {
        return "start".to_owned();
    }
    format!(
        "slot {}/12 · unit {}",
        (step - 1) % 12 + 1,
        (step - 1) / 12 + 1
    )
}

pub fn describe(event: &GroupEvent) -> String {
    let [a, b, c, d] = event.fields;
    let (agent, node) = (event.agent, event.node);
    match event.code {
        0 => format!("agent {agent} became a zombie of {a}:{b} at node {node}"),
        1 => format!("agent {agent} settled at node {node} in {a}:{b}"),
        2 => format!("merge: settler {agent} at node {node} joined {c}:{d} from {a}:{b}"),
        3 => format!("leader {agent} rose to level {a} with zombie {b} at node {node}"),
        4 => format!("leader {agent} moved forward {node} → {a}"),
        5 => format!("leader {agent} moved back {node} → {a}"),
        6 => format!("zombie {agent}: location level {a}, swarm level {b}"),
        _ => format!(
            "{} zombie {agent} chased {node} → {a}",
            if b == 1 { "weak" } else { "strong" }
        ),
    }
}

/// One note per frame: step, slot, unit, and the group changes it shows.
pub fn notes(frames: &[Frame], groups: &Groups) -> Vec<String> {
    frames
        .iter()
        .map(|frame| {
            if frame.step == 0 {
                return "Start · every agent leads its own group".to_owned();
            }
            let changes: Vec<String> = groups
                .events
                .iter()
                .filter(|event| event.frame == frame.step && event.code <= 3)
                .map(describe)
                .collect();
            let mut note = format!("Step {} · {}", frame.step, slot_text(frame.step));
            if let Some(first) = changes.first() {
                note.push_str(" · ");
                note.push_str(first);
                if changes.len() > 1 {
                    note.push_str(&format!(" (+{} more)", changes.len() - 1));
                }
            }
            note
        })
        .collect()
}

/// The runner's default GeneralDisp limit: `480 · k · (⌈log₂ k⌉ + 1)²`.
pub fn round_limit(agents: usize) -> u64 {
    let k = agents.max(1) as u64;
    let log = u64::from(k.next_power_of_two().trailing_zeros()) + 1;
    480 * k * log * log
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn index(&mut self, upper: usize) -> usize {
        let upper = upper as u64;
        let threshold = upper.wrapping_neg() % upper;
        loop {
            let value = self.next();
            if value >= threshold {
                return (value % upper) as usize;
            }
        }
    }
}

/// The CLI's `--placement uniform:distinct --seed seed` starts.
pub fn uniform_starts(nodes: usize, agents: usize, distinct: usize, seed: u64) -> Vec<u32> {
    let mut rng = Rng(seed);
    let mut candidates: Vec<u32> = (0..nodes as u32).collect();
    for end in (1..candidates.len()).rev() {
        let selected = rng.index(end + 1);
        candidates.swap(end, selected);
    }
    candidates.truncate(distinct.clamp(1, nodes.max(1)));
    (0..agents)
        .map(|_| candidates[rng.index(candidates.len())])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_starts_match_the_cli() {
        assert_eq!(uniform_starts(12, 8, 3, 1), vec![6, 7, 6, 6, 7, 4, 7, 7]);
        assert_eq!(uniform_starts(9, 9, 9, 42), vec![4, 8, 1, 3, 6, 4, 1, 8, 6]);
        assert_eq!(
            uniform_starts(30, 10, 2, 123_456_789),
            vec![14, 14, 14, 25, 14, 14, 25, 25, 14, 14]
        );
    }

    #[test]
    fn a_stream_decodes_and_replays_roles() {
        let mut bytes = vec![1, 4, 0, 0, 3, 0, 0, 0];
        let role = |bytes: &mut Vec<u8>, frame: u64, agent: u32, role: u8, level: u32| {
            bytes.push(1);
            bytes.extend_from_slice(&frame.to_le_bytes());
            bytes.push(0);
            bytes.extend_from_slice(&agent.to_le_bytes());
            bytes.push(role);
            bytes.extend_from_slice(&level.to_le_bytes());
            bytes.extend_from_slice(&1_u32.to_le_bytes());
        };
        role(&mut bytes, 0, 0, 2, 1);
        role(&mut bytes, 1, 0, 5, 1);
        bytes.push(2);
        bytes.extend_from_slice(&2_u64.to_le_bytes());
        bytes.extend_from_slice(&[2, 2]);
        for value in [0_u32, 3, 1, 1, 2, 1] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let groups = Groups::decode(&bytes).unwrap();
        assert_eq!(groups.at(0, 2), vec![(2, 1, 1), (2, 1, 1)]);
        assert_eq!(groups.at(5, 2)[0], (5, 1, 1));
        assert_eq!(
            describe(&groups.events[0]),
            "merge: settler 0 at node 3 joined 2:1 from 1:1"
        );
        assert!(Groups::decode(&bytes[..bytes.len() - 1]).is_none());
        assert!(Groups::decode(&[]).is_none());
    }

    #[test]
    fn slots_follow_the_twelve_step_unit() {
        assert_eq!(slot_text(0), "start");
        assert_eq!(slot_text(1), "slot 1/12 · unit 1");
        assert_eq!(slot_text(74), "slot 2/12 · unit 7");
        assert_eq!(round_limit(9), 480 * 9 * 25);
    }
}
