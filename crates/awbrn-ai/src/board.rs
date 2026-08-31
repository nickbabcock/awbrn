//! The Amber Valley board the arena plays.
//!
//! Map 61748, "Amber Valley", is a two-player board with land, bases, and
//! neutral cities. It starts one Teal Galaxy infantry. [`SEATS`] seats Pink
//! Cosmos first and Teal Galaxy second, so the compensation lands on the side
//! that moves second. The arena still swaps the agents between those seats.

use awbrn_game::{GameSetup, PlayerSetup, state_from_setup};
use awbrn_map::{AwbrnMap, AwbwMap};
use awbrn_types::{Co, PlayerFaction};
use awvm::ruleset::UnitKind;
use awvm::semantic::State;

const AMBER_VALLEY_MAP: &str = include_str!("../../../assets/maps/61748.json");

/// The seats the arena plays, in roster order.
///
/// The order matters three times: it is the turn order, it is the index into
/// the agent list the harness takes, and it decides which side the map's
/// first-turn compensation lands on. Pink Cosmos moves first and Teal Galaxy
/// holds the map's one predeployed unit.
pub const SEATS: [PlayerFaction; 2] = [PlayerFaction::PinkCosmos, PlayerFaction::TealGalaxy];

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
    state_from_map(amber_valley_map(), &SEATS, fog, seed)
}

/// The Amber Valley starting position.
pub fn amber_valley(fog: bool, seed: u64) -> State {
    arena(fog, seed)
}

fn state_from_map(map: AwbrnMap, seats: &[PlayerFaction; 2], fog: bool, seed: u64) -> State {
    try_state_from_map(map, seats, fog, seed).expect("the board setup is valid")
}

/// Build a deterministic two-seat state from a normalized map.
pub fn try_state_from_map(
    map: AwbrnMap,
    seats: &[PlayerFaction; 2],
    fog: bool,
    seed: u64,
) -> Result<State, awbrn_game::SetupError> {
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
    let mut state = state_from_setup(&setup)?;
    state.settings.unit_bans = BANNED_UNITS.to_vec();
    Ok(state)
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

    /// The arena map gives its extra infantry to the second seat.
    #[test]
    fn arena_starts_the_second_seat_with_the_extra_unit() {
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
        let state = arena(false, 1);
        for kind in BANNED_UNITS {
            assert!(
                state.settings.unit_bans.contains(&kind),
                "{kind:?} is banned"
            );
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
