//! The board the arena plays on.
//!
//! Map 174183, "Close Encounters": 10 by 10, two players, land only, and a
//! mirror under a half turn. The fixtures under `spec/fixtures/` are 7 by 3 or
//! smaller and none is symmetric, so a win rate over one of them measures the
//! fixture. This board is a fair mirror, cheap enough that a game costs little,
//! and it holds bases and neutral cities, because capture and income are the
//! two terms a hand-written tactics AI most often gets wrong.
//!
//! The board is not symmetric in its units, and that is the point. The map
//! starts one Blue Moon infantry and nothing for Orange Star, because Orange
//! Star moves first: the extra unit is how the author pays for the first-turn
//! advantage. [`SEATS`] therefore seats Orange Star first, so the compensation
//! lands on the side it was written for. The arena still swaps seats, which
//! swaps the free unit with them.

use awbrn_map::{AwbrnMap, AwbwMap, GameSetup, PlayerSetup, state_from_setup};
use awbrn_types::{Co, PlayerFaction};
use awvm::semantic::State;

const ARENA_MAP: &str = include_str!("../../../assets/maps/174183.json");

/// The seats the arena plays, in roster order.
///
/// The order matters three times: it is the turn order, it is the index into
/// the agent list the harness takes, and it decides which side the map's
/// first-turn compensation lands on. Orange Star moves first and Blue Moon
/// holds the map's one predeployed unit, which is the pairing the map was
/// drawn for.
pub const SEATS: [PlayerFaction; 2] = [PlayerFaction::OrangeStar, PlayerFaction::BlueMoon];

/// Funds each seat starts with.
///
/// Enough for one infantry on turn one and no more. A larger purse would let
/// turn one decide a game the arena means to measure over thirty days.
pub const STARTING_FUNDS: u32 = 1_000;

/// The graphical map used by the arena.
pub fn arena_map() -> AwbrnMap {
    let map = AwbwMap::parse_json(ARENA_MAP.as_bytes()).expect("the arena map parses");
    AwbrnMap::from_map(&map)
}

/// The arena's starting position.
///
/// `seed` reaches the setup only, which uses it for nothing the harness does:
/// the harness draws its own entropy. It is here so that a caller cannot build
/// two setups that differ in a field it did not choose.
pub fn arena(fog: bool, seed: u64) -> State {
    let setup = GameSetup {
        map: arena_map(),
        players: SEATS
            .iter()
            .map(|faction| PlayerSetup {
                faction: *faction,
                // Free for all. Two seats on no team is two teams, which is
                // what makes one seat's victory the other's loss.
                team: None,
                starting_funds: STARTING_FUNDS,
                // The same commander on both sides. A commander difference is
                // a term the arena is not yet measuring, and the probe that
                // measures it comes later.
                co: Co::Andy,
            })
            .collect(),
        fog_enabled: fog,
        rng_seed: seed,
    };
    state_from_setup(&setup).expect("the arena setup is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use awvm::semantic::Match;

    #[test]
    fn the_arena_opens_on_an_active_two_player_match() {
        let state = arena(false, 1);
        assert_eq!(state.players.len(), 2);
        assert_eq!(state.teams.len(), 2, "two seats off a team are two teams");
        assert!(matches!(state.match_state, Match::Active { .. }));
    }

    /// The map's compensation for moving second reaches the board.
    #[test]
    fn the_second_seat_starts_with_the_extra_unit() {
        let state = arena(false, 1);
        let second = state
            .players
            .seats()
            .nth(1)
            .map(|(seat, _)| seat)
            .expect("the arena seats two players");
        let units: Vec<_> = state.units.iter().collect();
        assert_eq!(units.len(), 1, "the map starts one unit");
        assert_eq!(
            units[0].owner, second,
            "it belongs to the seat that moves second"
        );
        assert_eq!(units[0].hp, 100, "the map writes a full unit as 10");
    }

    #[test]
    fn each_seat_holds_a_headquarters_and_two_bases() {
        use awvm::ruleset::Terrain;
        use awvm::semantic::TileOwner;

        let state = arena(false, 1);
        for (seat, _) in state.players.seats() {
            let held = |terrain| {
                state
                    .board
                    .tiles()
                    .filter(|tile| tile.terrain == terrain && tile.owner == TileOwner::Owned(seat))
                    .count()
            };
            assert_eq!(held(Terrain::Hq), 1, "seat {seat:?} has one headquarters");
            assert_eq!(held(Terrain::Base), 2, "seat {seat:?} has two bases");
        }
    }
}
