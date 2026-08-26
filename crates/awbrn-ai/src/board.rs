//! The boards the arena can play on.
//!
//! Map 174183, "Close Encounters", is the default arena board. It is a 10 by
//! 10 two-player board with land only and a mirror under a half turn. It is a
//! fair mirror, cheap enough that a game costs little, and it holds bases and
//! neutral cities, because capture and income are the two terms a hand-written
//! tactics AI most often gets wrong.
//!
//! The board is not symmetric in its units, and that is the point. The map
//! starts one Blue Moon infantry and nothing for Orange Star, because Orange
//! Star moves first: the extra unit is how the author pays for the first-turn
//! advantage. [`SEATS`] therefore seats Orange Star first, so the compensation
//! lands on the side it was written for. The arena still swaps seats, which
//! swaps the free unit with them.

use awbrn_game::{GameSetup, PlayerSetup, state_from_setup};
use awbrn_map::{AwbrnMap, AwbwMap};
use awbrn_types::{Co, PlayerFaction};
use awvm::ruleset::UnitKind;
use awvm::semantic::State;

const ARENA_MAP: &str = include_str!("../../../assets/maps/174183.json");
const AMBER_VALLEY_MAP: &str = include_str!("../../../assets/maps/61748.json");

/// The seats the arena plays, in roster order.
///
/// The order matters three times: it is the turn order, it is the index into
/// the agent list the harness takes, and it decides which side the map's
/// first-turn compensation lands on. Orange Star moves first and Blue Moon
/// holds the map's one predeployed unit, which is the pairing the map was
/// drawn for.
pub const SEATS: [PlayerFaction; 2] = [PlayerFaction::OrangeStar, PlayerFaction::BlueMoon];

/// The seats for Amber Valley, in roster order.
pub const AMBER_VALLEY_SEATS: [PlayerFaction; 2] =
    [PlayerFaction::TealGalaxy, PlayerFaction::PinkCosmos];

/// Funds each seat starts with, on top of what its properties pay.
///
/// Zero, which is what AWBW gives a match by default. Turn one is paid for by
/// day-one income: every seat opens holding one turn of its own properties,
/// three thousand on this board, and no purse the board did not hand it. An
/// extra purse would let turn one decide a game the arena means to measure
/// over thirty days.
pub const STARTING_FUNDS: u32 = 0;

/// The units no arena board may build.
///
/// A stealth is invisible to an agent that keeps no belief about what it
/// cannot see, so the seat that buys one wins for a reason the arena is not
/// measuring. A black bomb removes a stack of units for a price no combat
/// term prices, which does the same to the weightings that fight. Both are
/// banned until an agent can answer them.
///
/// The ban is a setting, so the reducer refuses the build and
/// [`awvm::session::Legal`] never offers it. No agent needs to know.
pub const BANNED_UNITS: [UnitKind; 2] = [UnitKind::Stealth, UnitKind::BlackBomb];

/// The graphical map used by the arena.
pub fn arena_map() -> AwbrnMap {
    let map = AwbwMap::parse_json(ARENA_MAP.as_bytes()).expect("the arena map parses");
    AwbrnMap::from_map(&map)
}

/// The graphical Amber Valley map.
pub fn amber_valley_map() -> AwbrnMap {
    let map =
        AwbwMap::parse_json(AMBER_VALLEY_MAP.as_bytes()).expect("the Amber Valley map parses");
    AwbrnMap::from_map(&map)
}

/// The arena's starting position.
///
/// `seed` reaches the setup only, which uses it for nothing the harness does:
/// the harness draws its own entropy. It is here so that a caller cannot build
/// two setups that differ in a field it did not choose.
pub fn arena(fog: bool, seed: u64) -> State {
    state_from_map(arena_map(), &SEATS, fog, seed)
}

/// The Amber Valley starting position.
pub fn amber_valley(fog: bool, seed: u64) -> State {
    state_from_map(amber_valley_map(), &AMBER_VALLEY_SEATS, fog, seed)
}

fn state_from_map(map: AwbrnMap, seats: &[PlayerFaction; 2], fog: bool, seed: u64) -> State {
    let setup = GameSetup {
        map,
        players: seats
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
    let mut state = state_from_setup(&setup).expect("the arena setup is valid");
    state.settings.unit_bans = BANNED_UNITS.to_vec();
    state
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

    #[test]
    fn amber_valley_opens_on_an_active_two_player_match() {
        let state = amber_valley(false, 1);
        assert_eq!(state.players.len(), 2);
        assert_eq!(state.teams.len(), 2);
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

    /// The ban reaches the position, and through it the action space.
    #[test]
    fn no_arena_board_may_build_a_stealth_or_a_black_bomb() {
        for state in [arena(false, 1), amber_valley(false, 1)] {
            for kind in BANNED_UNITS {
                assert!(
                    state.settings.unit_bans.contains(&kind),
                    "{kind:?} is banned"
                );
            }
        }
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
