//! A seeded generator, for the agent's choices and for the reducer's luck.
//!
//! Every measurement of one agent against another compares games, so a game
//! must repeat. One seed gives one game.
//!
//! This is the xorshift `awbrn-server` seeds a match with. It is repeated here
//! rather than exported from that crate: the opponent depends on `awvm` alone,
//! and a self-play run wants two independent streams so that an agent choice
//! cannot move a combat draw.

use awvm::commander::Domain;
use awvm::random::{Entropy, Luck, RandomError};
use awvm::ruleset::WeatherKind;

#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub const fn from_seed(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    /// Return the complete generator state.
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// Restore a state returned by [`Rng::state`].
    pub const fn set_state(&mut self, state: u64) {
        self.state = state;
    }

    pub const fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// A value in `0..range`, without the bias a bare remainder has.
    pub const fn below(&mut self, range: u64) -> u64 {
        if range <= 1 {
            return 0;
        }
        let limit = u64::MAX - (u64::MAX % range);
        loop {
            let sample = self.next_u64();
            if sample < limit {
                return sample % range;
            }
        }
    }

    /// A seed for one game, mixed so that game `n` does not continue game
    /// `n - 1`.
    ///
    /// A run seed and a game index give one game's seed. That is what keeps
    /// the first ten games of a ten-game run and a two-hundred-game run the
    /// same ten games.
    pub const fn mix(value: u64) -> u64 {
        let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

impl Entropy for Rng {
    fn luck(&mut self, _polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        let width = u64::try_from(domain.maximum - domain.minimum)
            .expect("commander luck domains are ordered");
        Ok(domain.minimum + self.below(width + 1) as i64)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        Ok(match self.below(3) {
            0 => WeatherKind::Clear,
            1 => WeatherKind::Rain,
            _ => WeatherKind::Snow,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_seed_gives_one_sequence() {
        let mut first = Rng::from_seed(7);
        let mut second = Rng::from_seed(7);
        for _ in 0..64 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
    }

    #[test]
    fn a_saved_state_replays_the_same_suffix() {
        let mut rng = Rng::from_seed(7);
        let _ = rng.next_u64();
        let state = rng.state();
        let expected = [rng.next_u64(), rng.next_u64(), rng.next_u64()];
        rng.set_state(state);
        assert_eq!([rng.next_u64(), rng.next_u64(), rng.next_u64()], expected);
    }

    #[test]
    fn a_draw_stays_below_its_range() {
        let mut rng = Rng::from_seed(3);
        for range in 1..32 {
            for _ in 0..64 {
                assert!(rng.below(range) < range);
            }
        }
    }

    #[test]
    fn a_zero_seed_still_advances() {
        let mut rng = Rng::from_seed(0);
        assert_ne!(rng.next_u64(), 0);
    }
}
