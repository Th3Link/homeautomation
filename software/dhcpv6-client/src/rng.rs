//! The randomness the protocol needs (transaction IDs, the delays and the
//! jitter of the retransmissions): not secret, so a small generator seeded
//! by the caller does.

/// xorshift64*.
#[derive(Debug, Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        // xorshift never leaves 0.
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// A number in `0..bound` (`bound` > 0).
    pub(crate) fn below(&mut self, bound: u32) -> u32 {
        self.next_u32() % bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_seed_still_varies() {
        let mut rng = Rng::new(0);
        let first = rng.next_u32();
        assert_ne!(first, rng.next_u32());
    }

    #[test]
    fn below_stays_below() {
        let mut rng = Rng::new(42);
        assert!((0..1000).all(|_| rng.below(7) < 7));
    }
}
