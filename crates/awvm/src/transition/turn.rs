//! Turn boundaries: ending a turn and everything the next one starts with.
//!
//! Normative source:
//! * `spec/semantics/turn.md`
//! * `spec/semantics/turn-hooks.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::{self, PowerLevel};
use crate::event::{Event, RandomKind, RandomValue, SupplySource};
use crate::random::{RandomTape, RandomToken};
use crate::ruleset::{self, Domain, Relation, TerrainTrait};
use crate::semantic::{
    Concealment, Location, Match, Outcome, Phase, PlayerId, PlayerStatus, PowerState, ReasonId,
    State, UnitAction, WeatherKind, WeatherSetting,
};
use crate::violation::{Action, Violation};
use std::collections::HashSet;

pub(crate) fn day_limit_outcome(state: &State) -> Result<Outcome, ExecuteError> {
    let mut scores = Vec::new();
    for player in state
        .players
        .iter()
        .filter(|player| player.status == PlayerStatus::Active)
    {
        let properties = state
            .board
            .tiles()
            .filter(|tile| tile.owner.player().is_some_and(|owner| &player.id == owner))
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
            reason: "day-limit".into(),
        }
    } else {
        Outcome::Draw {
            teams: leading_teams,
            reason: "day-limit".into(),
        }
    })
}

pub(crate) fn turns_until_player_selection(
    state: &State,
    player: &str,
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
        if candidate.id == player {
            return Ok(selections);
        }
    }
    Err(ExecuteError::InvalidState(
        "active player is not selectable in turn order".into(),
    ))
}

pub(crate) fn execute_end_turn(
    state: &State,
    player: &str,
    random: &[RandomToken],
) -> Result<Execution, ExecuteError> {
    execute_turn_boundary(state, player, BoundaryCommand::EndTurn, random)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoundaryCommand {
    EndTurn,
    Tag,
    Resign,
}

pub(crate) fn execute_turn_boundary(
    state: &State,
    player: &str,
    command: BoundaryCommand,
    random: &[RandomToken],
) -> Result<Execution, ExecuteError> {
    if state.ruleset.id != "awbw" || state.ruleset.revision != "2026-07-10" {
        return Err(ExecuteError::UnsupportedRuleset);
    }
    if matches!(state.match_state, Match::Finished { .. }) {
        return Err(violation(Violation::MatchFinished));
    }
    if state.turn.phase != Phase::UnitAction {
        return Err(violation(Violation::WrongPhase {
            expected: Phase::UnitAction,
            actual: state.turn.phase,
        }));
    }
    if state.turn.active_player != player {
        return Err(violation(Violation::NotActivePlayer {
            player: PlayerId::from(player),
        }));
    }
    let player_index = state
        .player_index(player)
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    if command == BoundaryCommand::Tag && !state.settings.tags {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::Tag,
        }));
    }
    if command == BoundaryCommand::Tag
        && (state.player(player_index).commanders.len() != 2
            || state
                .player(player_index)
                .commanders
                .iter()
                .filter(|commander| commander.active)
                .count()
                != 1)
    {
        return Err(ExecuteError::InvalidState(
            "tag player must have two commander slots and exactly one active slot".into(),
        ));
    }

    let mut next = state.clone();
    let mut tape = RandomTape::new(random);
    let mut events = vec![Event::PhaseChanged {
        player: PlayerId::from(player),
        from: Phase::UnitAction,
        to: Phase::TurnEnd,
    }];
    next.turn.phase = Phase::TurnEnd;
    if command == BoundaryCommand::Tag {
        let from_slot = next
            .player_mut(player_index)
            .commanders
            .iter()
            .position(|commander| commander.active)
            .expect("tag commander shape was checked");
        let to_slot = 1 - from_slot;
        let active_power = match next.player_mut(player_index).power_state {
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
            let commander_id = next.player_mut(player_index).commanders[from_slot].id;
            next.player_mut(player_index).power_state = PowerState::None;
            events.push(Event::PowerEnded {
                player: PlayerId::from(player),
                commander: commander_id,
                power,
            });
        }
        next.player_mut(player_index).commanders[from_slot].active = false;
        next.player_mut(player_index).commanders[to_slot].active = true;
        events.push(Event::CommanderSwapped {
            player: PlayerId::from(player),
            from_slot,
            to_slot,
        });
    }
    if command == BoundaryCommand::Resign
        && eliminate_player(&mut next, player, "resignation", None, None, &mut events)?
    {
        return Ok(Execution {
            state: next,
            events,
            random_consumed: tape.consumed(),
        });
    }

    loop {
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
        let successor = (1..=order_len).find_map(|offset| {
            let position = (next.turn.position + offset) % order_len;
            let id = &next.turn.order[position];
            next.find_player(id)
                .filter(|candidate| candidate.status == PlayerStatus::Active)
                .map(|_| (position, id.clone()))
        });
        let (successor_position, successor_id) = successor.ok_or_else(|| {
            ExecuteError::InvalidState("turn order contains no active successor".into())
        })?;
        let crossed_round_boundary = successor_position <= next.turn.position;
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
                random_consumed: tape.consumed(),
            });
        }

        let successor_player_index = next
            .player_index(&successor_id)
            .expect("successor selection established the player");

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
        next.turn.position = successor_position;
        next.turn.active_player = successor_id.clone();
        events.push(Event::TurnSelected {
            player: successor_id.clone(),
            position: successor_position,
        });
        if next.turn.phase == Phase::TurnEnd {
            next.turn.phase = Phase::TurnStart;
            events.push(Event::PhaseChanged {
                player: successor_id.clone(),
                from: Phase::TurnEnd,
                to: Phase::TurnStart,
            });
        }

        let expired_power = match next.player_mut(successor_player_index).power_state {
            crate::semantic::PowerState::None => None,
            crate::semantic::PowerState::Cop { commander_slot } => {
                Some((commander_slot, PowerLevel::Cop))
            }
            crate::semantic::PowerState::Scop { commander_slot } => {
                Some((commander_slot, PowerLevel::Scop))
            }
        };
        if let Some((commander_slot, power)) = expired_power {
            let commander = next
                .player_mut(successor_player_index)
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
            let commander_id = commander.id;
            next.player_mut(successor_player_index).power_state = crate::semantic::PowerState::None;
            events.push(Event::PowerEnded {
                player: successor_id.clone(),
                commander: commander_id,
                power,
            });
        }

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
                reason: ReasonId::from("expiry"),
            });
        } else if next.settings.weather == WeatherSetting::Random {
            let selected = tape.weather()?;
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
                    reason: ReasonId::from("random-weather"),
                });
            }
        }

        let income_tiles = next
            .board
            .tiles()
            .filter(|tile| {
                tile.owner
                    .player()
                    .is_some_and(|owner| owner == &successor_id)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
            })
            .count();
        let income_per_property = commander::effective_income_per_property(&next, &successor_id);
        let income = u64::try_from(income_tiles)
            .ok()
            .and_then(|count| count.checked_mul(income_per_property))
            .ok_or_else(|| ExecuteError::InvalidState("turn-start income overflow".into()))?;
        if income > 0 {
            let funds_before = next.player_mut(successor_player_index).funds;
            let funds_after = funds_before
                .checked_add(income)
                .ok_or_else(|| ExecuteError::InvalidState("player funds overflow".into()))?;
            next.player_mut(successor_player_index).funds = funds_after;
            events.push(Event::FundsChanged {
                player: successor_id.clone(),
                from: funds_before,
                to: funds_after,
                reason: ReasonId::from("turn-start-income"),
            });
        }

        let mut property_sources = Vec::new();
        {
            for (position, tile) in next.board.iter() {
                let Some(unit) = next.units.iter().find(|unit| {
                    unit.owner == successor_id && board_position(unit) == Some(position)
                }) else {
                    continue;
                };
                if tile
                    .owner
                    .player()
                    .is_some_and(|owner| owner == &successor_id)
                    && terrain_repairs_unit(tile.terrain, unit.kind)
                {
                    property_sources.push((position, unit.id));
                }
            }
        }
        let mut resupplied = HashSet::new();

        for (position, unit_id) in &property_sources {
            resupplied.insert(*unit_id);
            let unit = next
                .units
                .iter_mut()
                .find(|unit| unit.id == *unit_id)
                .expect("property supply unit remains present");
            if refill_unit(unit) {
                events.push(Event::AutomaticSupply {
                    source: SupplySource::Tile(*position),
                    units: vec![*unit_id],
                });
            }
        }

        let mut apc_ids: Vec<_> = next
            .units
            .iter()
            .filter(|unit| {
                unit.owner == successor_id
                    && ruleset::profile(unit.kind)
                        .supply
                        .is_some_and(|supply| supply.relation == Relation::Adjacent)
                    && board_position(unit).is_some()
            })
            .map(|unit| unit.id)
            .collect();
        apc_ids.sort();
        for apc_id in apc_ids {
            let source = next.units.get(apc_id).expect("APC source remains on board");
            let source_position = board_position(source).expect("APC source remains on board");
            let source_owner = source.owner.clone();
            let source_team = next.player(successor_player_index).team.clone();
            let supply_targets = ruleset::profile(source.kind)
                .supply
                .ok_or(ExecuteError::UnsupportedRuleset)?
                .targets;
            let mut target_ids: Vec<_> = next
                .units
                .iter()
                .filter(|unit| {
                    unit.id != apc_id
                        && supply_target_eligible(
                            &next,
                            &source_owner,
                            &source_team,
                            &unit.owner,
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
                    .expect("APC supply target remains present");
                if refill_unit(target) {
                    changed.push(target_id);
                }
            }
            if !changed.is_empty() {
                events.push(Event::AutomaticSupply {
                    source: SupplySource::Unit(apc_id),
                    units: changed,
                });
            }
        }

        let mut cargo_supply_ids: Vec<_> = next
            .units
            .iter()
            .filter(|unit| {
                unit.owner == successor_id
                    && ruleset::profile(unit.kind)
                        .supply
                        .is_some_and(|supply| supply.relation == Relation::Cargo)
            })
            .map(|unit| unit.id)
            .collect();
        cargo_supply_ids.sort();
        for transport_id in cargo_supply_ids {
            let mut cargo_ids: Vec<_> = next
                .units
                .iter()
                .filter(|unit| {
                    unit.owner == successor_id
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

        if next.turn.day >= 2 {
            let mut upkeep_ids: Vec<_> = next
                .units
                .iter()
                .filter(|unit| {
                    unit.owner == successor_id
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
                let unit_snapshot = next
                    .units
                    .get(unit_id)
                    .expect("upkeep unit remains present")
                    .clone();
                let profile = ruleset::profile(unit_snapshot.kind);
                let base_upkeep = if unit_snapshot.concealment == Concealment::Hidden {
                    profile
                        .fuel_per_turn
                        .hidden
                        .unwrap_or(profile.fuel_per_turn.normal)
                } else {
                    profile.fuel_per_turn.normal
                };
                let upkeep = commander::effective_upkeep(
                    &next,
                    &unit_snapshot,
                    base_upkeep,
                    profile.domain.as_str(),
                );
                let unit = next
                    .units
                    .get_mut(unit_id)
                    .expect("upkeep unit remains present");
                let fuel_before = unit.fuel;
                unit.fuel = unit.fuel.saturating_sub(upkeep);
                if unit.fuel > 0 && unit.fuel < fuel_before {
                    events.push(Event::UnitResourced {
                        unit: unit_id,
                        fuel_before,
                        fuel_after: unit.fuel,
                        ammo_before: unit.ammo,
                        ammo_after: unit.ammo,
                        reason: ReasonId::from("fuel-upkeep"),
                    });
                }
            }
        }

        let mut crash_ids: Vec<_> = next
            .units
            .iter()
            .filter(|unit| {
                unit.owner == successor_id
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
            let mut cargo: Vec<_> = next
                .units
                .iter()
                .filter_map(|unit| match &unit.location {
                    Location::Cargo { transport, slot } if transport == &unit_id => {
                        Some((*slot, unit.id))
                    }
                    _ => None,
                })
                .collect();
            cargo.sort();
            next.units.retain(|unit| {
                unit.id != unit_id
                    && !matches!(
                        &unit.location,
                        Location::Cargo { transport, .. } if transport == &unit_id
                    )
            });
            events.push(Event::UnitRemoved {
                unit: unit_id,
                reason: ReasonId::from("fuel-depleted"),
            });
            for (_, cargo_id) in cargo {
                events.push(Event::UnitRemoved {
                    unit: cargo_id,
                    reason: ReasonId::from("carrier-lost"),
                });
            }
        }

        let mut repair_units: Vec<_> = property_sources
            .iter()
            .filter(|(_, unit_id)| next.units.iter().any(|unit| unit.id == *unit_id))
            .map(|(position, unit_id)| (*unit_id, *position))
            .collect();
        repair_units.sort_by_key(|left| left.0);
        for (unit_id, position) in repair_units {
            let unit_index = next
                .units
                .index_of(unit_id)
                .expect("repair unit remains present");
            let hp_before = next.units[unit_index].hp;
            let visual_hp = u64::from(hp_before).div_ceil(10);
            let missing_bars = 10 - visual_hp;
            if missing_bars == 0 {
                continue;
            }
            let heal_cost = ruleset::profile(next.units[unit_index].kind)
                .cost
                .checked_div(10)
                .ok_or(ExecuteError::UnsupportedRuleset)?;
            let affordable_bars = next
                .player_mut(successor_player_index)
                .funds
                .checked_div(heal_cost)
                .unwrap_or(missing_bars);
            let bars = commander::effective_repair_bars(&next, &successor_id)
                .min(missing_bars)
                .min(affordable_bars);
            if bars == 0 {
                continue;
            }
            let cost = bars.checked_mul(heal_cost).ok_or_else(|| {
                ExecuteError::InvalidState("property repair cost overflow".into())
            })?;
            next.player_mut(successor_player_index).funds -= cost;
            let hp_after = u8::try_from((visual_hp + bars).min(10) * 10)
                .map_err(|_| ExecuteError::InvalidState("property repair HP overflow".into()))?;
            next.units[unit_index].hp = hp_after;
            events.push(Event::AutomaticRepair {
                unit: unit_id,
                position,
                hp_restored: hp_after - hp_before,
                cost,
            });
        }

        if removed_units && !next.units.iter().any(|unit| unit.owner == successor_id) {
            if eliminate_player(&mut next, &successor_id, "rout", None, None, &mut events)? {
                return Ok(Execution {
                    state: next,
                    events,
                    random_consumed: tape.consumed(),
                });
            }
            continue;
        }

        let mut unit_indices: Vec<_> = next
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| unit.owner == successor_id && unit.action != UnitAction::Ready)
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
                reason: ReasonId::from("turn-start"),
            });
        }
        next.turn.phase = Phase::UnitAction;
        events.push(Event::PhaseChanged {
            player: successor_id,
            from: Phase::TurnStart,
            to: Phase::UnitAction,
        });

        return Ok(Execution {
            state: next,
            events,
            random_consumed: tape.consumed(),
        });
    }
}
