//! Turn boundaries: ending a turn and everything the next one starts with.
//!
//! Normative source:
//! * `spec/semantics/turn.md`
//! * `spec/semantics/turn-hooks.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::{self, PowerLevel};
use crate::event::{Event, RandomKind, RandomValue, SupplySource};
use crate::query;

use crate::ruleset::{self, Domain, Relation};
use crate::semantic::{
    Concealment, DrawReason, KnownReason, Location, Outcome, Phase, PlayerId, PlayerIdx,
    PlayerStatus, PowerState, State, UnitAction, VictoryReason, WeatherKind, WeatherSetting,
};
use crate::violation::{Action, Violation};
use std::collections::HashSet;

pub(crate) fn day_limit_outcome(state: &State) -> Result<Outcome, ExecuteError> {
    let mut scores = Vec::new();
    for (seat, player) in state
        .players
        .seats()
        .filter(|(_, player)| player.status == PlayerStatus::Active)
    {
        let properties = state
            .board
            .tiles()
            .filter(|tile| tile.owner.is_owned_by(seat))
            .count();
        scores.push((player.team.clone(), properties));
    }
    let maximum = scores
        .iter()
        .map(|(_, properties)| *properties)
        .max()
        .ok_or_else(|| ExecuteError::InvalidState("day limit has no active players".into()))?;
    let mut leading_teams: Vec<_> = scores
        .into_iter()
        .filter(|(_, properties)| *properties == maximum)
        .map(|(team, _)| team)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    leading_teams.sort();
    Ok(if leading_teams.len() == 1 {
        Outcome::Victory {
            winners: leading_teams,
            reason: VictoryReason::DayLimit,
        }
    } else {
        Outcome::Draw {
            teams: leading_teams,
            reason: DrawReason::DayLimit,
        }
    })
}

pub(crate) fn turns_until_player_selection(
    state: &State,
    player: &PlayerId,
) -> Result<u64, ExecuteError> {
    let order_len = state.turn.order.len();
    if order_len == 0 || state.turn.position >= order_len {
        return Err(ExecuteError::InvalidState(
            "turn order or position is invalid".into(),
        ));
    }
    let mut index = state.turn.position;
    let mut selections = 0_u64;
    for _ in 0..order_len {
        index = (index + 1) % order_len;
        let candidate_id = &state.turn.order[index];
        let candidate = state.find_player(candidate_id).ok_or_else(|| {
            ExecuteError::InvalidState("turn order names a missing player".into())
        })?;
        if candidate.status != PlayerStatus::Active {
            continue;
        }
        selections = selections.checked_add(1).ok_or_else(|| {
            ExecuteError::InvalidState("weather duration selection count overflow".into())
        })?;
        if candidate.id() == player {
            return Ok(selections);
        }
    }
    Err(ExecuteError::InvalidState(
        "active player is not selectable in turn order".into(),
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoundaryCommand {
    EndTurn,
    Tag,
    Resign,
}

/// The player a turn-start hook runs for — `spec/semantics/turn-hooks.md` calls
/// it the selected active player `s`.
///
/// Every hook needs both the id, because units and tiles name their owner, and
/// the seat, because the roster is what income and repair spend from. Resolving
/// the pair once is what keeps the hooks from re-scanning the roster.
pub(crate) struct Incoming {
    id: PlayerId,
    seat: PlayerIdx,
}

/// End a turn and run the incoming player's whole `turn-start` sequence.
///
/// The hooks below are called in the fixed order `spec/semantics/turn-hooks.md`
/// gives them, per category rather than per unit: every supply source resolves
/// before any fuel upkeep, every crash before any repair. The loop exists for
/// the one case `spec/semantics/elimination.md` describes — an incoming player
/// routed by their own turn-start hooks, after which a successor is selected
/// again.
#[derive(Debug)]
pub(super) struct PreparedBoundary<'a> {
    turn: ActiveTurn<'a>,
    command: BoundaryCommand,
    seat: PlayerIdx,
}

/// Decide whether a turn boundary may be crossed, without crossing it.
pub(super) fn prepare_boundary(
    turn: ActiveTurn<'_>,
    command: BoundaryCommand,
) -> Result<PreparedBoundary<'_>, ExecuteError> {
    let state = turn.state();
    let seat = turn.seat();
    if command == BoundaryCommand::Tag {
        if !state.settings.tags {
            return Err(violation(Violation::ActionNotSupported {
                action: Action::Tag,
            }));
        }
        if state.player(seat).commanders.len() != 2
            || state
                .player(seat)
                .commanders
                .iter()
                .filter(|commander| commander.active)
                .count()
                != 1
        {
            return Err(ExecuteError::InvalidState(
                "tag player must have two commander slots and exactly one active slot".into(),
            ));
        }
    }

    Ok(PreparedBoundary {
        turn,
        command,
        seat,
    })
}

pub(super) fn execute_prepared_boundary(
    prepared: PreparedBoundary<'_>,
    draws: &mut Draws<'_>,
) -> Result<Execution, ExecuteError> {
    let PreparedBoundary {
        turn,
        command,
        seat,
    } = prepared;
    let state = turn.state();
    let player = turn.player();
    let mut next = state.clone();
    let mut events = vec![Event::PhaseChanged {
        player: player.clone(),
        from: Phase::UnitAction,
        to: Phase::TurnEnd,
    }];
    next.turn.phase = Phase::TurnEnd;
    if command == BoundaryCommand::Tag {
        swap_commanders(&mut next, seat, player, &mut events)?;
    }
    if command == BoundaryCommand::Resign
        && eliminate_player(
            &mut next,
            player,
            VictoryReason::Resignation,
            None,
            None,
            &mut events,
        )?
    {
        return Ok(Execution {
            state: next,
            events,
            random_consumed: draws.drawn(),
        });
    }

    loop {
        let (position, successor) = select_successor(&next)?;
        let crossed_round_boundary = position <= next.turn.position;
        if crossed_round_boundary
            && next
                .settings
                .day_limit
                .is_some_and(|limit| limit > 0 && limit == next.turn.day)
        {
            let outcome = day_limit_outcome(&next)?;
            complete_match(&mut next, outcome, &mut events);
            return Ok(Execution {
                state: next,
                events,
                random_consumed: draws.drawn(),
            });
        }
        let incoming = Incoming {
            seat: next
                .player_index(&successor)
                .expect("successor selection established the player"),
            id: successor,
        };

        if crossed_round_boundary {
            let from = next.turn.day;
            next.turn.day = next
                .turn
                .day
                .checked_add(1)
                .ok_or_else(|| ExecuteError::InvalidState("turn day overflow".into()))?;
            events.push(Event::DayAdvanced {
                from,
                to: next.turn.day,
            });
        }
        next.turn.position = position;
        next.turn.active_player = incoming.id.clone();
        events.push(Event::TurnSelected {
            player: incoming.id.clone(),
            position,
        });
        match run_turn_start(&mut next, &incoming, draws, &mut events)? {
            TurnStart::Open | TurnStart::Finished => {
                return Ok(Execution {
                    state: next,
                    events,
                    random_consumed: draws.drawn(),
                });
            }
            TurnStart::Routed => continue,
        }
    }
}

/// Open a match: run the first player's `turn-start` and hand them the board.
///
/// `spec/model/phases.md` builds a match at day one, in `turn-start`, holding
/// the first player. The hooks of that phase still owe them everything a later
/// turn is owed — day-one income above all — and no boundary command runs
/// here, so this is the operation that runs them. Starting funds and the
/// predeployed board are initialization inputs already in `state`; this adds
/// only what the phase itself grants.
///
/// The state must be in `turn-start`, which no accepted command produces: the
/// boundary loop leaves a match in `unit-action` or `finished`, so this is
/// callable once, on a state a host has just built.
pub(crate) fn begin_match(state: &State, draws: &mut Draws<'_>) -> Result<Execution, ExecuteError> {
    if !matches!(state.match_state, Match::Active { .. }) {
        return Err(ExecuteError::InvalidState(
            "a finished match cannot be opened".into(),
        ));
    }
    if state.turn.phase != Phase::TurnStart {
        return Err(ExecuteError::InvalidState(
            "a match opens from turn-start, the phase its initialization enters".into(),
        ));
    }
    let seat = state
        .player_index(&state.turn.active_player)
        .ok_or_else(|| ExecuteError::InvalidState("active player is not on the roster".into()))?;
    if state.player(seat).status != PlayerStatus::Active {
        return Err(ExecuteError::InvalidState(
            "a match opens on an active player".into(),
        ));
    }
    let incoming = Incoming {
        id: state.turn.active_player.clone(),
        seat,
    };

    let mut next = state.clone();
    let mut events = Vec::new();
    match run_turn_start(&mut next, &incoming, draws, &mut events)? {
        TurnStart::Open | TurnStart::Finished => Ok(Execution {
            state: next,
            events,
            random_consumed: draws.drawn(),
        }),
        // The opening hooks route a player only by crashing every unit they
        // hold, which a match cannot be built into: a predeployed unit opens
        // with the fuel its profile gives it.
        TurnStart::Routed => Err(ExecuteError::InvalidState(
            "the opening turn-start routed the first player".into(),
        )),
    }
}

/// What the incoming player's turn-start left behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TurnStart {
    /// The player holds an ordinary `unit-action` phase.
    Open,
    /// The hooks routed the player and the match continues, so a successor
    /// must be selected in their place.
    Routed,
    /// The hooks ended the match.
    Finished,
}

/// Run one player's whole `turn-start`, as `spec/semantics/turn-hooks.md`
/// orders it, and leave them in `unit-action`.
///
/// Both ways into the phase run this: the boundary that hands the turn on, and
/// the first turn of a match, which `spec/model/phases.md` gives day-one start
/// hooks with no boundary before it. Stating the sequence once is what keeps
/// day one from paying a different income than day two.
pub(crate) fn run_turn_start(
    next: &mut State,
    incoming: &Incoming,
    draws: &mut Draws<'_>,
    events: &mut Vec<Event>,
) -> Result<TurnStart, ExecuteError> {
    if next.turn.phase != Phase::TurnStart {
        let from = next.turn.phase;
        next.turn.phase = Phase::TurnStart;
        events.push(Event::PhaseChanged {
            player: incoming.id.clone(),
            from,
            to: Phase::TurnStart,
        });
    }

    end_expired_power(next, incoming, events)?;
    advance_weather(next, draws, events)?;
    collect_income(next, incoming, events)?;

    let sites = repair_sites(next, incoming);
    let mut resupplied = HashSet::new();
    supply_from_properties(next, &sites, &mut resupplied, events);
    supply_from_adjacent_transports(next, incoming, &mut resupplied, events)?;
    supply_cargo(next, incoming, &mut resupplied, events);
    apply_fuel_upkeep(next, incoming, &resupplied, events);
    let removed_units = crash_out_of_fuel(next, incoming, events);
    repair_on_properties(next, incoming, &sites, events)?;

    if removed_units && !next.units.iter().any(|unit| unit.owner == incoming.seat) {
        return if eliminate_player(next, &incoming.id, VictoryReason::Rout, None, None, events)? {
            Ok(TurnStart::Finished)
        } else {
            Ok(TurnStart::Routed)
        };
    }

    normalize_actions(next, incoming, events);
    next.turn.phase = Phase::UnitAction;
    events.push(Event::PhaseChanged {
        player: incoming.id.clone(),
        from: Phase::TurnStart,
        to: Phase::UnitAction,
    });
    Ok(TurnStart::Open)
}

/// Hand the turn to the outgoing player's other commander.
///
/// A power the leaving commander was running ends with them; the shape this
/// relies on — two slots, exactly one active — is checked before any of it.
fn swap_commanders(
    next: &mut State,
    seat: PlayerIdx,
    player: &PlayerId,
    events: &mut Vec<Event>,
) -> Result<(), ExecuteError> {
    let from_slot = next
        .player(seat)
        .commanders
        .iter()
        .position(|commander| commander.active)
        .expect("tag commander shape was checked");
    let to_slot = 1 - from_slot;
    let active_power = match next.player(seat).power_state {
        PowerState::None => None,
        PowerState::Cop { commander_slot } => Some((commander_slot, PowerLevel::Cop)),
        PowerState::Scop { commander_slot } => Some((commander_slot, PowerLevel::Scop)),
    };
    if let Some((commander_slot, power)) = active_power {
        if usize::from(commander_slot) != from_slot {
            return Err(ExecuteError::InvalidState(
                "power state does not name the active commander slot".into(),
            ));
        }
        let commander = next.player(seat).commanders[from_slot].id;
        next.player_mut(seat).power_state = PowerState::None;
        events.push(Event::PowerEnded {
            player: player.clone(),
            commander,
            power,
        });
    }
    next.player_mut(seat).commanders[from_slot].active = false;
    next.player_mut(seat).commanders[to_slot].active = true;
    events.push(Event::CommanderSwapped {
        player: player.clone(),
        from_slot,
        to_slot,
    });
    Ok(())
}

/// The next active player in turn order, with the position they occupy.
///
/// A position at or before the current one means the round wrapped, which is
/// what the day counter and the day limit key off.
fn select_successor(next: &State) -> Result<(usize, PlayerId), ExecuteError> {
    let order_len = next.turn.order.len();
    if order_len == 0 || next.turn.position >= order_len {
        return Err(ExecuteError::InvalidState(
            "turn order and position do not identify an active player".into(),
        ));
    }
    if next.turn.order[next.turn.position] != next.turn.active_player {
        return Err(ExecuteError::InvalidState(
            "turn active_player does not equal order[position]".into(),
        ));
    }
    (1..=order_len)
        .find_map(|offset| {
            let position = (next.turn.position + offset) % order_len;
            let id = &next.turn.order[position];
            next.find_player(id)
                .filter(|candidate| candidate.status == PlayerStatus::Active)
                .map(|_| (position, id.clone()))
        })
        .ok_or_else(|| ExecuteError::InvalidState("turn order contains no active successor".into()))
}

/// Turn-start step 2: a power the incoming player was running expires as their
/// turn comes back around.
fn end_expired_power(
    next: &mut State,
    incoming: &Incoming,
    events: &mut Vec<Event>,
) -> Result<(), ExecuteError> {
    let expired = match next.player(incoming.seat).power_state {
        PowerState::None => return Ok(()),
        PowerState::Cop { commander_slot } => (commander_slot, PowerLevel::Cop),
        PowerState::Scop { commander_slot } => (commander_slot, PowerLevel::Scop),
    };
    let (commander_slot, power) = expired;
    let commander = next
        .player(incoming.seat)
        .commanders
        .get(usize::from(commander_slot))
        .ok_or_else(|| {
            ExecuteError::InvalidState("power state names a missing commander slot".into())
        })?;
    if !commander.active {
        return Err(ExecuteError::InvalidState(
            "power state names an inactive commander slot".into(),
        ));
    }
    let commander = commander.id;
    next.player_mut(incoming.seat).power_state = PowerState::None;
    events.push(Event::PowerEnded {
        player: incoming.id.clone(),
        commander,
        power,
    });
    Ok(())
}

/// Turn-start step 3: a timed weather spell counts down and reverts to the
/// match setting when it runs out; a random-weather match instead consumes one
/// already-resolved semantic outcome.
fn advance_weather(
    next: &mut State,
    draws: &mut Draws<'_>,
    events: &mut Vec<Event>,
) -> Result<(), ExecuteError> {
    if next.weather.remaining_turns > 0 {
        let from = next.weather.kind;
        next.weather.remaining_turns -= 1;
        if next.weather.remaining_turns == 0 {
            next.weather.kind = match next.settings.weather {
                WeatherSetting::Clear => WeatherKind::Clear,
                WeatherSetting::Rain => WeatherKind::Rain,
                WeatherSetting::Snow => WeatherKind::Snow,
                WeatherSetting::Random => WeatherKind::Clear,
            };
        }
        events.push(Event::WeatherChanged {
            from,
            to: next.weather.kind,
            remaining_turns: next.weather.remaining_turns,
            reason: KnownReason::Expiry.into(),
        });
    } else if next.settings.weather == WeatherSetting::Random {
        let selected = draws.weather()?;
        events.push(Event::RandomOutcome {
            kind: RandomKind::WeatherSelection,
            outcome: RandomValue::Text(selected.as_str().into()),
        });
        if next.weather.kind != selected {
            let from = next.weather.kind;
            next.weather.kind = selected;
            events.push(Event::WeatherChanged {
                from,
                to: next.weather.kind,
                remaining_turns: 0,
                reason: KnownReason::RandomWeather.into(),
            });
        }
    }
    Ok(())
}

/// Turn-start step 4: one payment per owned income property.
fn collect_income(
    next: &mut State,
    incoming: &Incoming,
    events: &mut Vec<Event>,
) -> Result<(), ExecuteError> {
    let income = query::income(next, incoming.seat)
        .ok_or_else(|| ExecuteError::InvalidState("turn-start income overflow".into()))?;
    if income == 0 {
        return Ok(());
    }
    let from = next.player(incoming.seat).funds;
    let to = from
        .checked_add(income)
        .ok_or_else(|| ExecuteError::InvalidState("player funds overflow".into()))?;
    next.player_mut(incoming.seat).funds = to;
    events.push(Event::FundsChanged {
        player: incoming.id.clone(),
        from,
        to,
        reason: KnownReason::TurnStartIncome.into(),
    });
    Ok(())
}

/// Every owned property with an owned unit on it that the terrain can repair.
///
/// Turn-start steps 5 and 8 both work from this set, and step 8 must not see
/// the units step 7 removed, so it is resolved once up front and filtered for
/// survivors when repair runs.
fn repair_sites(next: &State, incoming: &Incoming) -> Vec<(Pos, UnitId)> {
    let mut sites = Vec::new();
    for (position, tile) in next.board.iter() {
        let Some(unit) = next
            .units
            .iter()
            .find(|unit| unit.owner == incoming.seat && board_position(unit) == Some(position))
        else {
            continue;
        };
        if tile.owner.is_owned_by(incoming.seat) && terrain_repairs_unit(tile.terrain, unit.kind) {
            sites.push((position, unit.id));
        }
    }
    sites
}

/// Turn-start step 5, properties.
fn supply_from_properties(
    next: &mut State,
    sites: &[(Pos, UnitId)],
    resupplied: &mut HashSet<UnitId>,
    events: &mut Vec<Event>,
) {
    for (position, unit_id) in sites {
        resupplied.insert(*unit_id);
        let unit = next
            .units
            .get_mut(*unit_id)
            .expect("property supply unit remains present");
        if refill_unit(unit) {
            events.push(Event::AutomaticSupply {
                source: SupplySource::Tile(*position),
                units: vec![*unit_id],
            });
        }
    }
}

/// Turn-start step 5, adjacent owned transports.
fn supply_from_adjacent_transports(
    next: &mut State,
    incoming: &Incoming,
    resupplied: &mut HashSet<UnitId>,
    events: &mut Vec<Event>,
) -> Result<(), ExecuteError> {
    let mut source_ids: Vec<_> = next
        .units
        .iter()
        .filter(|unit| {
            unit.owner == incoming.seat
                && ruleset::profile(unit.kind)
                    .supply
                    .is_some_and(|supply| supply.relation == Relation::Adjacent)
                && board_position(unit).is_some()
        })
        .map(|unit| unit.id)
        .collect();
    source_ids.sort();
    for source_id in source_ids {
        let source = next
            .units
            .get(source_id)
            .expect("supply source remains on board");
        let source_position = board_position(source).expect("supply source remains on board");
        let source_owner = source.owner;
        let source_team = next.player(incoming.seat).team.clone();
        let supply_targets = ruleset::profile(source.kind)
            .supply
            .ok_or(ExecuteError::UnsupportedRuleset)?
            .targets;
        let mut target_ids: Vec<_> = next
            .units
            .iter()
            .filter(|unit| {
                unit.id != source_id
                    && supply_target_eligible(
                        next,
                        source_owner,
                        &source_team,
                        unit.owner,
                        supply_targets,
                    )
                    && board_position(unit).is_some_and(|position| {
                        position.x.abs_diff(source_position.x)
                            + position.y.abs_diff(source_position.y)
                            == 1
                    })
            })
            .map(|unit| unit.id)
            .collect();
        target_ids.sort();
        let mut changed = Vec::new();
        for target_id in target_ids {
            resupplied.insert(target_id);
            let target = next
                .units
                .get_mut(target_id)
                .expect("supply target remains present");
            if refill_unit(target) {
                changed.push(target_id);
            }
        }
        if !changed.is_empty() {
            events.push(Event::AutomaticSupply {
                source: SupplySource::Unit(source_id),
                units: changed,
            });
        }
    }
    Ok(())
}

/// Turn-start step 5, owned cargo of an owned cruiser or carrier.
fn supply_cargo(
    next: &mut State,
    incoming: &Incoming,
    resupplied: &mut HashSet<UnitId>,
    events: &mut Vec<Event>,
) {
    let mut transport_ids: Vec<_> = next
        .units
        .iter()
        .filter(|unit| {
            unit.owner == incoming.seat
                && ruleset::profile(unit.kind)
                    .supply
                    .is_some_and(|supply| supply.relation == Relation::Cargo)
        })
        .map(|unit| unit.id)
        .collect();
    transport_ids.sort();
    for transport_id in transport_ids {
        let mut cargo_ids: Vec<_> = next
            .units
            .iter()
            .filter(|unit| {
                unit.owner == incoming.seat
                    && matches!(
                        &unit.location,
                        Location::Cargo { transport, .. } if transport == &transport_id
                    )
            })
            .map(|unit| unit.id)
            .collect();
        cargo_ids.sort();
        let mut changed = Vec::new();
        for cargo_id in cargo_ids {
            resupplied.insert(cargo_id);
            let cargo = next
                .units
                .get_mut(cargo_id)
                .expect("cargo supply target remains present");
            if refill_unit(cargo) {
                changed.push(cargo_id);
            }
        }
        if !changed.is_empty() {
            events.push(Event::AutomaticSupply {
                source: SupplySource::Unit(transport_id),
                units: changed,
            });
        }
    }
}

/// Turn-start step 6: air and sea units burn fuel, unless something just
/// resupplied them. Day one is exempt — a unit cannot have flown yet.
fn apply_fuel_upkeep(
    next: &mut State,
    incoming: &Incoming,
    resupplied: &HashSet<UnitId>,
    events: &mut Vec<Event>,
) {
    if next.turn.day < 2 {
        return;
    }
    let mut upkeep_ids: Vec<_> = next
        .units
        .iter()
        .filter(|unit| {
            unit.owner == incoming.seat
                && !resupplied.contains(&unit.id)
                && matches!(
                    ruleset::profile(unit.kind).domain,
                    Domain::Air | Domain::Sea
                )
        })
        .map(|unit| unit.id)
        .collect();
    upkeep_ids.sort();
    for unit_id in upkeep_ids {
        let snapshot = *next
            .units
            .get(unit_id)
            .expect("upkeep unit remains present");
        let profile = ruleset::profile(snapshot.kind);
        let base_upkeep = if snapshot.concealment == Concealment::Hidden {
            profile
                .fuel_per_turn
                .hidden
                .unwrap_or(profile.fuel_per_turn.normal)
        } else {
            profile.fuel_per_turn.normal
        };
        let upkeep = commander::effective_upkeep(next, &snapshot, base_upkeep, profile.domain);
        let unit = next
            .units
            .get_mut(unit_id)
            .expect("upkeep unit remains present");
        let fuel_before = unit.fuel;
        unit.fuel = unit.fuel.saturating_sub(upkeep);
        // A unit that hit zero crashes at step 7, which reports the removal
        // instead of the drain that caused it.
        if unit.fuel > 0 && unit.fuel < fuel_before {
            events.push(Event::UnitResourced {
                unit: unit_id,
                fuel_before,
                fuel_after: unit.fuel,
                ammo_before: unit.ammo,
                ammo_after: unit.ammo,
                reason: KnownReason::FuelUpkeep.into(),
            });
        }
    }
}

/// Turn-start step 7: an air or sea unit out of fuel is removed, and its cargo
/// with it. Reports whether anything went, which is what the elimination
/// checkpoint at step 9 keys off.
fn crash_out_of_fuel(next: &mut State, incoming: &Incoming, events: &mut Vec<Event>) -> bool {
    let mut crash_ids: Vec<_> = next
        .units
        .iter()
        .filter(|unit| {
            unit.owner == incoming.seat
                && unit.fuel == 0
                && matches!(
                    ruleset::profile(unit.kind).domain,
                    Domain::Air | Domain::Sea
                )
        })
        .map(|unit| unit.id)
        .collect();
    crash_ids.sort();
    let removed_units = !crash_ids.is_empty();
    for unit_id in crash_ids {
        if !next.units.contains(unit_id) {
            continue;
        }
        remove_unit_and_cargo(next, unit_id, KnownReason::FuelDepleted, events);
    }
    removed_units
}

/// Turn-start step 8: a unit resting on its owner's repairing property heals
/// whole HP bars, paid for out of that player's funds.
fn repair_on_properties(
    next: &mut State,
    incoming: &Incoming,
    sites: &[(Pos, UnitId)],
    events: &mut Vec<Event>,
) -> Result<(), ExecuteError> {
    let mut survivors: Vec<_> = sites
        .iter()
        .filter(|(_, unit_id)| next.units.contains(*unit_id))
        .map(|(position, unit_id)| (*unit_id, *position))
        .collect();
    survivors.sort_by_key(|left| left.0);
    for (unit_id, position) in survivors {
        let index = next
            .units
            .index_of(unit_id)
            .expect("repair unit remains present");
        let hp_before = next.units[index].hp;
        let visual_hp = u64::from(hp_before).div_ceil(10);
        let missing_bars = 10 - visual_hp;
        if missing_bars == 0 {
            continue;
        }
        let heal_cost = ruleset::profile(next.units[index].kind)
            .cost
            .checked_div(10)
            .ok_or(ExecuteError::UnsupportedRuleset)?;
        let affordable_bars = next
            .player(incoming.seat)
            .funds
            .checked_div(heal_cost)
            .unwrap_or(missing_bars);
        let bars = commander::effective_repair_bars(next, incoming.seat)
            .min(missing_bars)
            .min(affordable_bars);
        if bars == 0 {
            continue;
        }
        let cost = bars
            .checked_mul(heal_cost)
            .ok_or_else(|| ExecuteError::InvalidState("property repair cost overflow".into()))?;
        next.player_mut(incoming.seat).funds -= cost;
        let hp_after = u8::try_from((visual_hp + bars).min(10) * 10)
            .map_err(|_| ExecuteError::InvalidState("property repair HP overflow".into()))?;
        next.units[index].hp = hp_after;
        events.push(Event::AutomaticRepair {
            unit: unit_id,
            position,
            hp_restored: hp_after - hp_before,
            cost,
        });
    }
    Ok(())
}

/// Turn-start step 10: the incoming player's units become ready, except that a
/// unit Von Bolt immobilized spends its turn instead.
fn normalize_actions(next: &mut State, incoming: &Incoming, events: &mut Vec<Event>) {
    let mut unit_indices: Vec<_> = next
        .units
        .iter()
        .enumerate()
        .filter(|(_, unit)| unit.owner == incoming.seat && unit.action != UnitAction::Ready)
        .map(|(index, unit)| (unit.id, index))
        .collect();
    unit_indices.sort_by_key(|left| left.0);
    for (unit_id, index) in unit_indices {
        let from = next.units[index].action;
        next.units[index].action = if from == UnitAction::Immobilized {
            UnitAction::Spent
        } else {
            UnitAction::Ready
        };
        events.push(Event::UnitActionChanged {
            unit: unit_id,
            from,
            to: next.units[index].action,
            reason: KnownReason::TurnStart.into(),
        });
    }
}
