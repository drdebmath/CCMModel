/// Repository-owned deterministic RNG. `SplitMix64` is small, reproducible on
/// every target, and sufficient for graph/placement generation. Its exact
/// output is part of fixture metadata once explicit instances are persisted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[must_use]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    #[must_use]
    pub fn index(&mut self, upper_exclusive: usize) -> Option<usize> {
        if upper_exclusive == 0 {
            return None;
        }
        let upper = upper_exclusive as u64;
        let threshold = upper.wrapping_neg() % upper;
        loop {
            let value = self.next_u64();
            if value >= threshold {
                return usize::try_from(value % upper).ok();
            }
        }
    }

    pub fn shuffle<T>(&mut self, values: &mut [T]) {
        for end in (1..values.len()).rev() {
            let Some(selected) = self.index(end + 1) else {
                unreachable!("shuffle range is non-empty and fits usize")
            };
            values.swap(end, selected);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_reproducible() {
        let mut first = DeterministicRng::new(42);
        let mut second = DeterministicRng::new(42);
        for _ in 0..100 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
    }
}
