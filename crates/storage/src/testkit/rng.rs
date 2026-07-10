//! Deterministic seeded RNG shared across the testkit harnesses.
//!
//! A single-source `SplitMix64` so every harness (stress, recovery oracle, …)
//! draws from the same well-tested generator and any failure replays from its
//! seed.

/// Minimal deterministic PRNG. Identical seeds produce identical sequences.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub(crate) const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    /// Uniform draw in `0..n` as a `u8` (treats `n == 0` as `1`).
    pub(crate) fn gen_u8_below(&mut self, n: u8) -> u8 {
        let bound = u64::from(n.max(1));
        u8::try_from(self.next_u64() % bound).unwrap_or(0)
    }

    pub(crate) fn gen_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}
