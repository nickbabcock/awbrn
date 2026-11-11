//! When to use a commander power.
//!
//! The greedy policy gives each legal power the flat score
//! [`crate::agents::Weights::power`], and the planner adds a plan that uses
//! each legal power first. Both use the normal power (COP) as soon as the
//! meter can pay for it. The meter then never gets to the cost of the super
//! power (SCOP), because each use spends the meter and makes the next power
//! cost 20% more.
//!
//! Strong human players do not do this with most commanders. In the Division
//! 1 games of Amarriner Cups 1 to 6 and the Global League games, the share of
//! the super power in all powers is 71% for Andy, 97% for Eagle, 61% for
//! Lash, 53% for Drake, 91% for Olaf and 68% for Jake. On a turn when the
//! meter can pay for the normal power but not for the super power, an Andy
//! player uses the normal power on 5% of turns and an Eagle player on 1%. The
//! share is 9% for Kindle, 28% for Max and 23% for Sami, and those players
//! use the normal power soon after the meter can pay for it.
//!
//! The board does not change this much. Army ratio, attack potential and
//! property captures do not separate the turns with a normal power from the
//! turns without one. One condition does: when an enemy capturer stands on
//! the headquarters, a player who keeps the meter for the super power uses
//! the normal power on 6 of 11 turns.
//!
//! The power policy ([`crate::agents::Weights::power_policy`]) is the
//! smallest rule that agrees with these facts. See [`power_use`].
//!
//! # The position of the power in the turn
//!
//! Without the policy, the flat greedy score puts the power between the
//! orders with a higher score (most attacks) and the orders with a lower
//! score (most moves). The position thus comes from the score scale, not
//! from the effect of the power.
//!
//! With the policy, the greedy policy uses the power before all other
//! orders. The planner tries the later positions, and takes one only when its
//! score is better than the score with the power first by a margin. See
//! [`crate::planner`]. When the position does not change the score, the power
//! thus comes first, as for most human powers. A power that lets units act
//! again is used last, and the planner does not move it.
//!
//! In the Division 1 games, human players use most powers after some orders
//! (median of 1 to 4 unit orders), and almost never after all unit orders.
//! Eagle players use the super power after a median of 18 unit orders and
//! 2.3 builds, because the super power also lets the units built in that
//! turn act.

use awvm::commander::PowerLevel;
use awvm::ruleset::{self, CommanderKind, TerrainTrait};
use awvm::semantic::{CAPTURE_REQUIRED_POINTS, Location, PlayerIdx, State, TileOwner};

/// The powers that a plan holds for one seat in this turn.
///
/// The planner sets it when it prepares a decisive capture with a power
/// ([`crate::planner`], `march`). The greedy policy then gives a held power no
/// score for that seat, and the planner adds no power plan for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerHold {
    /// The seat that holds the powers.
    pub seat: PlayerIdx,
    /// Whether the normal power is held.
    pub cop: bool,
    /// Whether the super power is held.
    pub scop: bool,
}

impl PowerHold {
    /// Whether this hold holds the power `level` of `seat`.
    pub fn holds(&self, seat: PlayerIdx, level: PowerLevel) -> bool {
        self.seat == seat
            && match level {
                PowerLevel::Cop => self.cop,
                PowerLevel::Scop => self.scop,
            }
    }
}

/// What the policy does with one legal power.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerUse {
    /// The policy is off: use the power with the normal power score, and let
    /// the planner try it before all other orders.
    Now,
    /// Use the power after all other orders that have a score above zero.
    /// The planner does not try it first.
    Last,
    /// Use the power before all other orders. The planner tries later
    /// positions, and takes one only when it is better by a margin.
    First,
    /// Do not use the power in this position.
    Hold,
}

/// How a commander uses the meter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    /// Use the normal power when the meter can pay for it.
    Normal,
    /// Keep the meter for the super power.
    Save,
}

/// The style of each commander for the power policy.
///
/// A commander gets [`Style::Save`] when the super power is at least 40% of
/// the powers of its human players, over at least 8 powers. Hawke (36%),
/// Kindle, Max, Sami, Sasha, Adder and Javier keep [`Style::Normal`].
///
/// The human games contain no power of Grit. Grit keeps [`Style::Normal`]:
/// in a 56-pair Grit mirror on the research branch, a configuration with the
/// save style for Grit got a mean of -0.205 +/- 0.161 against the same
/// configuration with each power used at once.
///
/// The other commanders with no human powers in the games keep
/// [`Style::Normal`], so that their play does not change.
const fn style(commander: CommanderKind) -> Style {
    match commander {
        CommanderKind::Andy
        | CommanderKind::Drake
        | CommanderKind::Eagle
        | CommanderKind::Jake
        | CommanderKind::Jess
        | CommanderKind::Koal
        | CommanderKind::Lash
        | CommanderKind::Olaf
        | CommanderKind::Rachel => Style::Save,
        _ => Style::Normal,
    }
}

/// What the power policy does with the legal power `level` of `seat` in
/// `state`. `on` is [`crate::agents::Weights::power_policy`].
///
/// With the policy off, each power is [`PowerUse::Now`]. With the policy on:
///
/// - A commander with the save style holds the normal power, except when an
///   enemy capturer stands on a property of `seat` whose capture defeats
///   `seat`. A lab counts only on a map with no headquarters.
/// - A power that lets units act again ([`refreshes_units`]) is
///   [`PowerUse::Last`]. Only the Eagle super power does this. It is worth
///   most after the other units have acted. Human Eagle players make 8
///   attacks on average before it.
/// - Each other power is [`PowerUse::First`]. The planner then chooses its
///   position.
pub fn power_use(on: bool, state: &State, seat: PlayerIdx, level: PowerLevel) -> PowerUse {
    if !on {
        return PowerUse::Now;
    }
    let commander = active_commander(state, seat);
    let saves = commander.is_some_and(|commander| style(commander) == Style::Save);
    if level == PowerLevel::Cop && saves && !decisive_capture(state, seat) {
        return PowerUse::Hold;
    }
    if commander.is_some_and(|commander| refreshes_units(commander, level)) {
        PowerUse::Last
    } else {
        PowerUse::First
    }
}

/// Whether a power of the active commander of `seat` lets units act again
/// ([`refreshes_units`]).
pub fn has_refresh_power(state: &State, seat: PlayerIdx) -> bool {
    active_commander(state, seat).is_some_and(|commander| {
        refreshes_units(commander, PowerLevel::Cop) || refreshes_units(commander, PowerLevel::Scop)
    })
}

/// The active commander of `seat`.
fn active_commander(state: &State, seat: PlayerIdx) -> Option<CommanderKind> {
    state
        .players
        .get(seat.get())
        .and_then(|player| player.commanders.iter().find(|co| co.active))
        .map(|co| co.id)
}

/// Whether the power `level` of `commander` lets own units that have acted
/// act again.
///
/// The test applies the power with the engine on a small board: the active
/// seat has one of each unit kind, and each of them has acted. The power
/// refreshes units when at least one of them can act after it. The result
/// is kept for each commander and level.
pub fn refreshes_units(commander: CommanderKind, level: PowerLevel) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static KNOWN: OnceLock<Mutex<HashMap<(CommanderKind, PowerLevel), bool>>> = OnceLock::new();
    let known = KNOWN.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(value) = known
        .lock()
        .ok()
        .and_then(|map| map.get(&(commander, level)).copied())
    {
        return value;
    }
    let value = refresh_probe(commander, level);
    if let Ok(mut map) = known.lock() {
        map.insert((commander, level), value);
    }
    value
}

fn refresh_probe(commander: CommanderKind, level: PowerLevel) -> bool {
    use awvm::ruleset::{Terrain, UnitKind};
    use awvm::semantic::{Concealment, Unit, UnitAction, UnitId};
    use awvm::session::Session;
    use awvm::transition::Command;
    let mut state = crate::board::arena(false, 1);
    let Some(seat) = state.players.seat(&state.turn.active_player) else {
        return false;
    };
    for co in state.player_mut(seat).commanders.iter_mut() {
        co.id = commander;
        co.power_charge = 100_000_000;
        co.power_uses = 0;
    }
    state.units.retain(|_| false);
    let land: Vec<_> = state
        .board
        .iter()
        .filter(|(_, tile)| tile.terrain == Terrain::Plain)
        .map(|(position, _)| position)
        .collect();
    let kinds = [
        UnitKind::Infantry,
        UnitKind::Mech,
        UnitKind::Recon,
        UnitKind::Tank,
        UnitKind::Artillery,
        UnitKind::BCopter,
    ];
    for (index, (kind, position)) in kinds.into_iter().zip(land).enumerate() {
        let profile = ruleset::profile(kind);
        state.units.push(Unit {
            id: UnitId::new(index as u32 + 1),
            kind,
            owner: seat,
            hp: 100,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: UnitAction::Spent,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        });
    }
    state.next_unit_id = Some(kinds.len() as u32 + 1);
    let player = state.turn.active_player.clone();
    let mut session = Session::new(state);
    let Ok(order) = session.resolve(&Command::ActivatePower { player, level }) else {
        return false;
    };
    if session
        .apply(order, &mut crate::planner::MeanLuck, &mut ())
        .is_err()
    {
        return false;
    }
    session
        .state()
        .units
        .iter()
        .any(|unit| unit.owner == seat && unit.action == UnitAction::Ready)
}

/// Whether an enemy unit captures a property of `seat` whose capture
/// defeats `seat`.
pub(crate) fn decisive_capture(state: &State, seat: PlayerIdx) -> bool {
    let headquarters = state
        .board
        .tiles()
        .any(|tile| ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner));
    state.units.iter().any(|unit| {
        if unit.owner == seat {
            return false;
        }
        let Location::Board { position } = unit.location else {
            return false;
        };
        let Some(tile) = state.board.get(position) else {
            return false;
        };
        let decides = if headquarters {
            ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner)
        } else {
            ruleset::terrain_has(tile.terrain, TerrainTrait::LabVictory)
        };
        decides
            && tile.owner == TileOwner::Owned(seat)
            && tile
                .capture_points
                .is_some_and(|points| points < CAPTURE_REQUIRED_POINTS)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_styles_follow_the_human_shares() {
        for commander in [
            CommanderKind::Andy,
            CommanderKind::Eagle,
            CommanderKind::Lash,
            CommanderKind::Drake,
        ] {
            assert_eq!(style(commander), Style::Save, "{commander:?}");
        }
        for commander in [
            CommanderKind::Grit,
            CommanderKind::Kindle,
            CommanderKind::Max,
            CommanderKind::Sami,
        ] {
            assert_eq!(style(commander), Style::Normal, "{commander:?}");
        }
    }

    /// The active seat of an arena with `commander` and a meter that can pay
    /// for both powers.
    fn arena_with(commander: CommanderKind) -> (State, PlayerIdx) {
        let mut state = crate::board::arena(false, 3);
        let seat = state.players.seat(&state.turn.active_player).unwrap();
        for co in state.player_mut(seat).commanders.iter_mut() {
            co.id = commander;
            co.power_charge = 1_000_000;
        }
        (state, seat)
    }

    /// Put an enemy infantry on the headquarters of `seat`, with a capture
    /// started.
    fn capture_headquarters(state: &mut State, seat: PlayerIdx) {
        let position = state
            .board
            .iter()
            .find(|(_, tile)| {
                tile.owner == TileOwner::Owned(seat)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner)
            })
            .map(|(position, _)| position)
            .expect("the arena has a headquarters");
        state
            .units
            .retain(|unit| unit.location != Location::Board { position });
        let mut unit = state
            .units
            .iter()
            .find(|unit| unit.owner != seat)
            .copied()
            .expect("the enemy has a unit");
        unit.location = Location::Board { position };
        state.units.retain(|other| other.id != unit.id);
        state.units.push(unit);
        state.board.tile_mut(position).capture_points = Some(10);
    }

    #[test]
    fn the_policy_holds_the_normal_power_of_a_saving_commander() {
        let (state, seat) = arena_with(CommanderKind::Andy);
        assert_eq!(
            power_use(false, &state, seat, PowerLevel::Cop),
            PowerUse::Now
        );
        assert_eq!(
            power_use(true, &state, seat, PowerLevel::Cop),
            PowerUse::Hold
        );
        assert_eq!(
            power_use(true, &state, seat, PowerLevel::Scop),
            PowerUse::First
        );

        let (state, seat) = arena_with(CommanderKind::Kindle);
        assert_eq!(
            power_use(true, &state, seat, PowerLevel::Cop),
            PowerUse::First
        );

        // The Eagle super power lets units act again, so it goes last.
        let (state, seat) = arena_with(CommanderKind::Eagle);
        assert_eq!(
            power_use(true, &state, seat, PowerLevel::Cop),
            PowerUse::Hold
        );
        assert_eq!(
            power_use(true, &state, seat, PowerLevel::Scop),
            PowerUse::Last
        );
        assert_eq!(
            power_use(false, &state, seat, PowerLevel::Scop),
            PowerUse::Now
        );
    }

    #[test]
    fn a_capture_of_the_headquarters_releases_the_normal_power() {
        let (mut state, seat) = arena_with(CommanderKind::Andy);
        capture_headquarters(&mut state, seat);
        assert_eq!(
            power_use(true, &state, seat, PowerLevel::Cop),
            PowerUse::First
        );
    }

    /// The greedy policy gives a held power no score, a power kept for last
    /// the least score above zero, and a power used first a score above all
    /// other orders.
    #[test]
    fn the_greedy_scores_follow_the_policy() {
        use crate::agents::{GreedyAgent, Weights};
        use awvm::semantic::{AwbwVisibility, observe};
        use awvm::session::OrderKind;
        let score = |commander, policy| {
            let (state, _) = arena_with(commander);
            let view = observe(&AwbwVisibility, &state, &state.turn.active_player).unwrap();
            let mut agent = GreedyAgent::with_weights(
                1,
                Weights {
                    power_policy: policy,
                    ..Weights::DEFAULT
                },
            );
            let scored = agent.scored_unitless_orders(&view).unwrap();
            let mut scores = scored
                .candidates()
                .iter()
                .filter_map(|candidate| match candidate.order.kind() {
                    OrderKind::Power(level) => Some((level, candidate.score)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            scores.sort_by_key(|(level, _)| matches!(level, PowerLevel::Scop));
            scores
        };
        let power = Weights::DEFAULT.power;
        assert_eq!(
            score(CommanderKind::Andy, false),
            vec![(PowerLevel::Cop, power), (PowerLevel::Scop, power)]
        );
        // With the policy, each power that it does not hold comes before all
        // other orders, except a power that lets units act again, which
        // stays last.
        let first = 1.0e12;
        assert_eq!(
            score(CommanderKind::Andy, true),
            vec![(PowerLevel::Cop, 0.0), (PowerLevel::Scop, first)]
        );
        assert_eq!(
            score(CommanderKind::Kindle, true),
            vec![(PowerLevel::Cop, first), (PowerLevel::Scop, first)]
        );
        assert_eq!(
            score(CommanderKind::Eagle, true),
            vec![
                (PowerLevel::Cop, 0.0),
                (PowerLevel::Scop, f64::MIN_POSITIVE)
            ]
        );
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;

    /// Of all the powers of all commanders, only Eagle's super power lets
    /// units act again.
    #[test]
    fn only_the_eagle_super_power_refreshes_units() {
        for commander in CommanderKind::ALL {
            for level in [PowerLevel::Cop, PowerLevel::Scop] {
                assert_eq!(
                    refreshes_units(commander, level),
                    commander == CommanderKind::Eagle && level == PowerLevel::Scop,
                    "{commander:?} {level:?}"
                );
            }
        }
    }
}
