//! What the simulator page shows for rendezvous: each agent's string with the
//! current block highlighted, whether it is exploring or waiting, and one
//! note per frame.
//!
//! Nothing here simulates. The strings come from the algorithm crate's own
//! encoding, and the block of a frame is its round divided by the block length
//! the engine reported, so the page cannot drift from what the agents did.

use crate::Frame;
use ccm_algorithms::rendezvous::{string, symbol};
use serde::{Deserialize, Serialize};

/// What the engine reports about a rendezvous run besides positions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RendezvousData {
    /// The node the agents met on, or -1.
    #[serde(rename = "meetingNode")]
    pub meeting_node: i32,
    /// The round they met in, or 0.
    #[serde(rename = "meetingRound")]
    pub meeting_round: u64,
    /// Rounds per block, `2(n − 1)`.
    #[serde(rename = "blockLen")]
    pub block_len: u64,
    /// The task's exact round limit.
    #[serde(rename = "roundBound")]
    pub round_bound: u64,
    /// The most memory any agent used, in bits.
    #[serde(rename = "memoryBits")]
    pub memory_bits: u64,
}

/// The graph families the page offers that are trees.
#[must_use]
pub fn is_tree_family(family: &str) -> bool {
    matches!(family, "path" | "star" | "tree" | "random_tree")
}

/// Parses the Labels box: two distinct whole numbers of at least 1.
///
/// # Errors
///
/// Returns a sentence for the status line.
pub fn parse_labels(text: &str) -> Result<Vec<u32>, String> {
    let labels = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<u32>()
                .map_err(|_| format!("\u{201c}{part}\u{201d} is not a label: use whole numbers."))
        })
        .collect::<Result<Vec<u32>, String>>()?;
    match labels.as_slice() {
        [a, b] if *a >= 1 && *b >= 1 && a != b => Ok(labels),
        [a, b] if a == b => Err("The two labels must differ.".to_owned()),
        [_, _] => Err("Labels start at 1.".to_owned()),
        _ => Err("Give exactly two labels, such as 1, 3.".to_owned()),
    }
}

/// The two start nodes: the first two of the seeded node order, so they
/// always differ.
#[must_use]
pub fn start_pair(order: &[u32]) -> Vec<u32> {
    order.iter().copied().take(2).collect()
}

/// Rendezvous's exact round limit, as the engine computes it: the first tour
/// plus one block per symbol of the shorter string.
#[must_use]
pub fn round_bound(nodes: usize, labels: &[u32]) -> u64 {
    ccm_algorithms::rendezvous::round_bound(nodes, labels)
}

/// The block that round `round` (from 1) belongs to; 0 is the first tour.
/// Round 0, the start, is shown as block 0 too.
#[must_use]
pub fn block_of(round: u64, block_len: u64) -> u64 {
    round.saturating_sub(1).checked_div(block_len).unwrap_or(0)
}

/// What an agent with `label` does in `block`.
#[must_use]
pub fn exploring(label: u32, block: u64) -> bool {
    block == 0
        || u32::try_from(block - 1)
            .ok()
            .and_then(|index| symbol(label, index))
            == Some(true)
}

fn block_name(block: u64, labels: &[u32]) -> String {
    if block == 0 {
        return "block 0 (first tour)".to_owned();
    }
    let ended = labels
        .iter()
        .all(|&label| u64::from(ccm_algorithms::rendezvous::string_len(label)) < block);
    if ended {
        format!("block {block} (both strings ended)")
    } else {
        format!("block {block} (symbol {block})")
    }
}

/// One note per frame: the round, the block, and each agent's activity. The
/// final frame says where they met.
#[must_use]
pub fn notes(frames: &[Frame], labels: &[u32], data: &RendezvousData) -> Vec<String> {
    frames
        .iter()
        .map(|frame| {
            if frame.step == 0 {
                return "Start · both agents begin the first tour".to_owned();
            }
            let block = block_of(frame.step, data.block_len);
            let activity: Vec<String> = labels
                .iter()
                .map(|&label| {
                    let what = if exploring(label, block) {
                        "explores"
                    } else {
                        "waits"
                    };
                    format!("label {label} {what}")
                })
                .collect();
            let met = data.meeting_round == frame.step && data.meeting_node >= 0;
            let tail = if met {
                format!(" · they meet on node {}", data.meeting_node)
            } else {
                String::new()
            };
            format!(
                "Round {} · {} · {}{tail}",
                frame.step,
                block_name(block, labels),
                activity.join(", ")
            )
        })
        .collect()
}

/// The panel under the canvas for the frame at `step`: one row per agent with
/// its string, the current symbol highlighted (the `T` chip is the first
/// tour), and whether it explores or waits. Only numbers and fixed words go
/// into the markup.
#[must_use]
pub fn panel_html(labels: &[u32], data: &RendezvousData, step: u64) -> String {
    let block = block_of(step, data.block_len);
    let mut html = format!(
        "<p class=\"rv-head\">Round {step} \u{b7} {} \u{b7} {} rounds per block \u{b7} limit {} rounds</p>",
        block_name(block, labels),
        data.block_len,
        data.round_bound
    );
    for (agent, &label) in labels.iter().enumerate() {
        let mode = if exploring(label, block) {
            "exploring"
        } else {
            "waiting"
        };
        let current = |position: u64| if position == block { " current" } else { "" };
        let mut chips = format!("<span class=\"rv-sym rv-tour{}\">T</span>", current(0));
        for (index, one) in string(label).into_iter().enumerate() {
            let position = u64::try_from(index).unwrap_or(u64::MAX) + 1;
            if index % 2 == 0 {
                chips.push_str("<span class=\"rv-gap\"></span>");
            }
            chips.push_str(&format!(
                "<span class=\"rv-sym{}\">{}</span>",
                current(position),
                u8::from(one)
            ));
        }
        html.push_str(&format!(
            "<div class=\"rv-agent rv-agent-{agent}\"><span class=\"rv-name\">Agent {agent} \u{b7} label {label}</span><span class=\"rv-string\">{chips}</span><span class=\"rv-mode rv-{mode}\">{mode}</span></div>"
        ));
    }
    if data.meeting_node >= 0 && step >= data.meeting_round {
        html.push_str(&format!(
            "<p class=\"rv-met\">Met on node {} in round {} (block {}); at most {} bits of memory per agent.</p>",
            data.meeting_node,
            data.meeting_round,
            block_of(data.meeting_round, data.block_len),
            data.memory_bits
        ));
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(step: u64) -> Frame {
        Frame {
            step,
            ..Frame::default()
        }
    }

    #[test]
    fn labels_are_two_distinct_positive_numbers() {
        assert_eq!(parse_labels("1, 3"), Ok(vec![1, 3]));
        assert_eq!(parse_labels("5 12"), Ok(vec![5, 12]));
        assert!(parse_labels("2,2").is_err());
        assert!(parse_labels("0,2").is_err());
        assert!(parse_labels("1").is_err());
        assert!(parse_labels("1,2,3").is_err());
        assert!(parse_labels("a,b").is_err());
    }

    /// The hand trace of docs/rendezvous.md: blocks of 10 rounds, labels 1
    /// and 3, meeting on node 0 in round 33.
    #[test]
    fn notes_follow_the_hand_trace() {
        let data = RendezvousData {
            meeting_node: 0,
            meeting_round: 33,
            block_len: 10,
            round_bound: 50,
            memory_bits: 20,
        };
        let frames: Vec<Frame> = [0, 1, 10, 11, 31, 33].map(frame).to_vec();
        let notes = notes(&frames, &[1, 3], &data);
        assert!(notes[0].starts_with("Start"));
        assert_eq!(
            notes[1],
            "Round 1 · block 0 (first tour) · label 1 explores, label 3 explores"
        );
        assert!(notes[2].contains("block 0"));
        assert!(notes[3].contains("block 1 (symbol 1)"));
        assert_eq!(
            notes[4],
            "Round 31 · block 3 (symbol 3) · label 1 waits, label 3 explores"
        );
        assert!(notes[5].ends_with("they meet on node 0"));
    }

    #[test]
    fn the_panel_highlights_the_current_symbol() {
        let data = RendezvousData {
            meeting_node: -1,
            block_len: 10,
            round_bound: 50,
            ..RendezvousData::default()
        };
        let html = panel_html(&[1, 3], &data, 31);
        // Block 3 is symbol 3 of each string: the third chip after T.
        assert_eq!(html.matches(" current").count(), 2);
        assert!(html.contains("rv-waiting"));
        assert!(html.contains("rv-exploring"));
        assert!(!html.contains("Met on node"));
    }
}
