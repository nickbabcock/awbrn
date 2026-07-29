//! Recorded-outcome playback for historical AWBW archives.
//!
//! AWBW stores graphical post-action facts, not the entropy tape or exact HP
//! remainders needed to reproduce an AWVM execution. This reducer therefore
//! copies those recorded facts into a presentation state and derives typed
//! transition events from the before/after difference. It never calls
//! `awvm::transition::execute`.

use std::collections::{HashMap, HashSet};

use awbrn_map::AwbwMapData;
use awbrn_types::{AwbwTerrain, AwbwUnitId, UnitExt};
use awbw_replay::AwbwReplay;
use awbw_replay::turn_models::{
    Action, CombatUnit, HpEffect, MoveAction, PowerAction, RepairedUnit, TargetedPlayer,
    UnitChange, UnitMap, UnitProperty, UpdatedInfo, WeatherCode,
};
use awvm::commander::{self, PowerLevel};
use awvm::event::Event;
use awvm::ruleset::{
    AreaStrikeProfile, Domain, MISSILE_SILO_STRIKE, Terrain, UNIT_EXPLOSION, WeatherKind, profile,
};
use awvm::semantic::{
    AwbwVisibility, Concealment, Location, Match, ObserveError, ObservedTransition, Outcome,
    PlayerId, PlayerStatus, Pos, PowerState, Reason, ReasonId, Silo, State, StateInvariant,
    TileOwner, Unit, UnitAction, UnitId, VictoryReason, observe_transition,
};

use crate::targeting::{targeted_hidden, targeted_value, visible_targeted_unit, visible_unit};
use crate::{AdapterError, awbw_power_charge, initial_state, semantic_terrain};

const RECORDED_REASON: &str = "recorded-awbw-outcome";

/// Stateful legacy-replay presentation adapter.
#[derive(Clone, Debug)]
pub struct RecordedAdapter {
    state: State,
}

impl RecordedAdapter {
    pub fn new(replay: &AwbwReplay, map: &AwbwMapData) -> Result<Self, RecordedAdapterError> {
        Ok(Self {
            state: initial_state(replay, map)?,
        })
    }

    pub fn from_state(state: State) -> Result<Self, RecordedAdapterError> {
        state.validate()?;
        Ok(Self { state })
    }

    pub const fn state(&self) -> &State {
        &self.state
    }

    /// Apply one authoritative recorded outcome.
    ///
    /// The returned value says "derived from this archive payload", not
    /// "accepted by AWVM execution".
    pub fn advance(&mut self, action: &Action) -> Result<RecordedTransition, RecordedAdapterError> {
        // The action is applied to a candidate rather than in place, so a
        // rejected payload cannot leave the adapter holding a half-applied
        // state: the candidate is simply dropped and `self` never moved.
        let mut next = self.state.clone();
        apply_recorded(&mut next, action)?;
        next.validate()?;
        let recorded_events = diff_events(&self.state, &next, action);
        let post = next.clone();
        Ok(RecordedTransition {
            prior: std::mem::replace(&mut self.state, next),
            post,
            recorded_events,
        })
    }
}

/// A typed transition derived from recorded pre/post facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedTransition {
    prior: State,
    post: State,
    recorded_events: Vec<Event>,
}

impl RecordedTransition {
    pub const fn post_state(&self) -> &State {
        &self.post
    }

    /// Project the recorded transition through AWVM's normal visibility rules.
    pub fn observe(
        &self,
        recipient: &PlayerId,
    ) -> Result<ObservedTransition, RecordedAdapterError> {
        Ok(observe_transition(
            &AwbwVisibility,
            &self.prior,
            &self.post,
            &self.recorded_events,
            recipient,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecordedAdapterError {
    #[error(transparent)]
    InitialState(#[from] AdapterError),
    #[error("recorded action is missing {0}")]
    Missing(&'static str),
    #[error("recorded action names unknown unit {0}")]
    UnknownUnit(u32),
    #[error("recorded action names unknown player {0}")]
    UnknownPlayer(u32),
    #[error("recorded coordinate ({x}, {y}) exceeds AWVM's coordinate domain")]
    Coordinate { x: u32, y: u32 },
    #[error("recorded coordinate {0} is outside the board")]
    OffBoard(Pos),
    #[error("recorded cargo operation has no free slot in transport {0}")]
    NoCargoSlot(u32),
    #[error("recorded state violates an AWVM invariant: {0}")]
    InvalidState(#[from] StateInvariant),
    #[error("recorded transition cannot be observed: {0}")]
    Observation(#[from] ObserveError),
}

fn apply_recorded(state: &mut State, action: &Action) -> Result<(), RecordedAdapterError> {
    if let Some(movement) = action.move_action() {
        apply_move(
            state,
            movement,
            matches!(action, Action::Load { .. } | Action::Join { .. }),
        )?;
    }

    match action {
        Action::AttackSeam {
            attack_seam_action, ..
        } => {
            for combat in attack_seam_action.unit.values() {
                if let Some(unit) = combat.combat_info.get_value() {
                    apply_combat_unit(state, unit, true)?;
                }
            }
            let position = pos(attack_seam_action.seam_x, attack_seam_action.seam_y)?;
            let tile = state
                .board
                .get_mut(position)
                .ok_or(RecordedAdapterError::OffBoard(position))?;
            let terrain_id = u8::try_from(attack_seam_action.buildings_terrain_id)
                .ok()
                .and_then(|value| AwbwTerrain::try_from(value).ok())
                .ok_or(RecordedAdapterError::Missing("supported seam terrain"))?;
            tile.terrain = semantic_terrain(terrain_id);
            tile.set_destructible_hp(
                (tile.terrain == Terrain::PipeSeam)
                    .then(|| u64::try_from(attack_seam_action.buildings_hit_points).ok())
                    .flatten(),
            );
        }
        Action::Build { new_unit, .. } => apply_build(state, new_unit)?,
        Action::Capt {
            move_action,
            capture_action,
        } => {
            let position = pos(
                capture_action.building_info.buildings_x,
                capture_action.building_info.buildings_y,
            )?;
            let actor = move_action
                .as_ref()
                .and_then(visible_move_unit)
                .map(|unit| unit_id(unit.units_id))
                .or_else(|| unit_on_board(state, position));
            let owner = actor
                .and_then(|actor| state.units.get(actor))
                .map(|unit| unit.owner.clone())
                .or_else(|| {
                    capture_action
                        .income
                        .as_ref()
                        .and_then(|income| income.values().next())
                        .map(|income| player_id(income.player.as_u32()))
                });
            let tile = state
                .board
                .get_mut(position)
                .ok_or(RecordedAdapterError::OffBoard(position))?;
            let remaining = capture_action.building_info.buildings_capture.clamp(0, 20) as u8;
            if remaining >= 20 {
                if let Some(owner) = owner {
                    tile.owner = TileOwner::Owned(owner);
                }
                tile.capture_points = Some(20);
            } else {
                tile.capture_points = Some(remaining);
            }
            if let Some(actor) = actor {
                spend(state, actor)?;
            }
        }
        Action::End { updated_info } => apply_end(state, updated_info)?,
        Action::Fire { fire_action, .. } => {
            let mut seen = HashSet::new();
            for view in fire_action.combat_info_vision.values() {
                if let Some(attacker) = view.combat_info.attacker.get_value()
                    && seen.insert(attacker.units_id)
                {
                    apply_combat_unit(state, attacker, true)?;
                }
                if let Some(defender) = view.combat_info.defender.get_value()
                    && seen.insert(defender.units_id)
                {
                    apply_combat_unit(state, defender, false)?;
                }
            }
            update_combat_charge(
                state,
                fire_action.cop_values.attacker.player_id.as_u32(),
                fire_action.cop_values.attacker.cop_value,
                fire_action.cop_values.attacker.tag_value,
            )?;
            update_combat_charge(
                state,
                fire_action.cop_values.defender.player_id.as_u32(),
                fire_action.cop_values.defender.cop_value,
                fire_action.cop_values.defender.tag_value,
            )?;
        }
        Action::Join { join_action, .. } => {
            let survivor = visible_unit(&join_action.unit)
                .ok_or(RecordedAdapterError::Missing("join target"))?;
            update_from_property(state, survivor)?;
            let source = targeted_hidden(&join_action.join_id)
                .ok_or(RecordedAdapterError::Missing("joining unit"))?;
            remove_unit(state, UnitId::new(source))?;
            spend(state, unit_id(survivor.units_id))?;
            if let Some(funds) = targeted_value(&join_action.new_funds) {
                set_player_funds(state, join_action.player_id, *funds)?;
            }
        }
        Action::Launch { launch_action, .. } => {
            let silo = pos(launch_action.silo_x, launch_action.silo_y)?;
            state
                .board
                .get_mut(silo)
                .ok_or(RecordedAdapterError::OffBoard(silo))?
                .silo = Some(Silo::Spent);
            apply_area(
                state,
                MISSILE_SILO_STRIKE,
                pos(launch_action.target_x, launch_action.target_y)?,
                launch_action.hp,
                None,
            );
        }
        Action::Load { load_action, .. } => {
            let cargo = targeted_hidden(&load_action.loaded)
                .ok_or(RecordedAdapterError::Missing("loaded unit"))?;
            let transport = targeted_hidden(&load_action.transport)
                .ok_or(RecordedAdapterError::Missing("transport unit"))?;
            let transport = unit_id(transport);
            let transport_kind = state
                .units
                .get(transport)
                .ok_or(RecordedAdapterError::UnknownUnit(transport.get()))?
                .kind;
            let capacity = profile(transport_kind)
                .transport
                .ok_or(RecordedAdapterError::NoCargoSlot(transport.get()))?
                .capacity;
            let mut slot = (0..capacity).find(|slot| {
                !state.units.iter().any(|unit| {
                    unit.location
                        == (Location::Cargo {
                            transport,
                            slot: *slot,
                        })
                })
            });
            if slot.is_none() {
                // A recipient-targeted cargo row can remain stale after its
                // unit stopped being globally present. The successful Load is
                // authoritative evidence that the recorded slot was free.
                let stale = state.units.iter().find_map(|unit| match unit.location {
                    Location::Cargo {
                        transport: carrier,
                        slot,
                    } if carrier == transport => Some((unit.id, slot)),
                    _ => None,
                });
                if let Some((stale, freed)) = stale {
                    remove_unit(state, stale)?;
                    slot = Some(freed);
                }
            }
            let slot = slot.ok_or(RecordedAdapterError::NoCargoSlot(transport.get()))?;
            state
                .units
                .get_mut(unit_id(cargo))
                .ok_or(RecordedAdapterError::UnknownUnit(cargo.as_u32()))?
                .location = Location::Cargo { transport, slot };
        }
        Action::Move(_) => {}
        Action::Explode { explode_action, .. } => {
            let exploding = unit_id(explode_action.unit_id);
            let center = explode_action
                .vision
                .values()
                .find_map(|value| value.get_value())
                .map(|value| pos(value.x, value.y))
                .transpose()?
                .or_else(|| board_position(state, exploding));
            if let Some(center) = center {
                apply_area(
                    state,
                    UNIT_EXPLOSION,
                    center,
                    explode_action.hp,
                    Some(exploding),
                );
            }
            remove_unit(state, exploding)?;
        }
        Action::Power(power_action) => apply_power(state, power_action)?,
        Action::Repair { repair_action, .. } => {
            let repairing = targeted_hidden(&repair_action.unit)
                .ok_or(RecordedAdapterError::Missing("repairing unit"))?;
            let repaired = targeted_value(&repair_action.repaired)
                .ok_or(RecordedAdapterError::Missing("repaired unit"))?;
            apply_repaired(state, repaired)?;
            spend(state, UnitId::new(repairing))?;
            if let Some(funds) = targeted_hidden(&repair_action.funds) {
                let owner = state
                    .units
                    .get(UnitId::new(repairing))
                    .ok_or(RecordedAdapterError::UnknownUnit(repairing))?
                    .owner
                    .clone();
                let player = state
                    .find_player_mut(&owner)
                    .ok_or(RecordedAdapterError::Missing("repairing player"))?;
                player.funds = u64::from(funds);
            }
        }
        Action::Resign {
            resign_action,
            next_turn_action,
            game_over_action,
        } => {
            let id = player_id(resign_action.player_id.as_u32());
            state
                .find_player_mut(&id)
                .ok_or(RecordedAdapterError::UnknownPlayer(
                    resign_action.player_id.as_u32(),
                ))?
                .status = PlayerStatus::Resigned;
            if let Some(next) = next_turn_action {
                apply_end(state, &next.clone().into())?;
            }
            if let Some(game_over) = game_over_action {
                state.turn.phase = awvm::semantic::Phase::Finished;
                let mut winners = Vec::new();
                for winner in &game_over.winners {
                    if let Some(player) = state.find_player(&player_id(*winner))
                        && !winners.contains(&player.team)
                    {
                        winners.push(player.team.clone());
                    }
                }
                if !winners.is_empty() {
                    state.match_state = Match::Finished {
                        outcome: Outcome::Victory {
                            winners,
                            reason: VictoryReason::Resignation,
                        },
                    };
                }
            }
        }
        Action::Supply { supply_action, .. } => {
            let supplying = targeted_hidden(&supply_action.unit)
                .ok_or(RecordedAdapterError::Missing("supplying unit"))?;
            for supplied in targeted_vec_union(&supply_action.supplied) {
                refill(state, unit_id(supplied))?;
            }
            spend(state, UnitId::new(supplying))?;
        }
        Action::Unload {
            unit, transport_id, ..
        } => {
            let cargo = visible_unit(unit).ok_or(RecordedAdapterError::Missing("unloaded unit"))?;
            let destination = property_pos(cargo)?;
            let cargo_id = unit_id(cargo.units_id);
            update_from_property(state, cargo)?;
            let cargo = state
                .units
                .get_mut(cargo_id)
                .ok_or(RecordedAdapterError::UnknownUnit(cargo_id.get()))?;
            cargo.location = Location::Board {
                position: destination,
            };
            cargo.action = UnitAction::Spent;
            if !state.units.contains(unit_id(*transport_id)) {
                return Err(RecordedAdapterError::UnknownUnit(transport_id.as_u32()));
            }
        }
        Action::Delete { delete_action } => {
            let deleted = delete_action
                .unit_id
                .as_ref()
                .and_then(targeted_hidden)
                .ok_or(RecordedAdapterError::Missing("deleted unit"))?;
            remove_unit(state, unit_id(deleted))?;
        }
        Action::Hide { move_action } => {
            let unit = move_action
                .as_ref()
                .and_then(visible_move_unit)
                .ok_or(RecordedAdapterError::Missing("hidden unit"))?;
            state
                .units
                .get_mut(unit_id(unit.units_id))
                .ok_or(RecordedAdapterError::UnknownUnit(unit.units_id.as_u32()))?
                .concealment = Concealment::Hidden;
        }
        Action::Unhide { move_action } => {
            let unit = move_action
                .as_ref()
                .and_then(visible_move_unit)
                .ok_or(RecordedAdapterError::Missing("revealed unit"))?;
            state
                .units
                .get_mut(unit_id(unit.units_id))
                .ok_or(RecordedAdapterError::UnknownUnit(unit.units_id.as_u32()))?
                .concealment = Concealment::Exposed;
        }
        Action::Tag { updated_info } => {
            let active = state.turn.active_player.clone();
            let player = state
                .find_player_mut(&active)
                .ok_or(RecordedAdapterError::Missing("active tag player"))?;
            // AWBW tag switching ends the outgoing commander's active power
            // immediately, before the ordinary successor turn-start closure.
            player.power_state = PowerState::None;
            if player.commanders.len() >= 2 {
                for commander in &mut player.commanders {
                    commander.active = !commander.active;
                }
            }
            apply_end(state, updated_info)?;
        }
    }
    Ok(())
}

fn apply_move(
    state: &mut State,
    movement: &MoveAction,
    preserve_destination_occupant: bool,
) -> Result<(), RecordedAdapterError> {
    let Some((unit, x, y)) = complete_move_unit(movement) else {
        // A recipient-only fog view can disclose identity without coordinates.
        // The established replay reducer treats that Move payload as a no-op,
        // and only a payload without any moving unit at all is an error.
        return match visible_move_unit(movement) {
            Some(_) => Ok(()),
            None => Err(RecordedAdapterError::Missing("moving unit position")),
        };
    };
    let id = unit_id(unit.units_id);
    let destination = pos(x, y)?;
    if !state.units.contains(id) {
        let owner = player_id(unit.units_players_id);
        if state.find_player(&owner).is_none() {
            return Err(RecordedAdapterError::UnknownPlayer(unit.units_players_id));
        }
        state.units.push(Unit {
            id,
            kind: unit.units_name,
            owner,
            hp: display_hp(unit.units_hit_points.value().max(1)),
            fuel: u64::from(
                unit.units_fuel
                    .unwrap_or_else(|| unit.units_name.max_fuel()),
            ),
            ammo: u64::from(
                unit.units_ammo
                    .unwrap_or_else(|| unit.units_name.max_ammo()),
            ),
            action: UnitAction::Spent,
            concealment: if matches!(unit.units_sub_dive.as_str(), "D" | "Y") {
                Concealment::Hidden
            } else {
                Concealment::Exposed
            },
            location: Location::Board {
                position: destination,
            },
        });
        state.next_unit_id = Some(
            state
                .next_unit_id
                .unwrap_or_default()
                .max(id.get().saturating_add(1)),
        );
    }
    let prior = board_position(state, id);
    if !preserve_destination_occupant
        && let Some(occupant) = unit_on_board(state, destination)
        && occupant != id
    {
        remove_unit(state, occupant)?;
    }
    update_from_property(state, unit)?;
    let moving = state
        .units
        .get_mut(id)
        .ok_or(RecordedAdapterError::UnknownUnit(id.get()))?;
    moving.location = Location::Board {
        position: destination,
    };
    moving.action = UnitAction::Spent;
    if prior.is_some_and(|origin| origin != destination)
        && let Some(tile) = prior.and_then(|position| state.board.get_mut(position))
        && tile.capture_points.is_some()
    {
        tile.capture_points = Some(20);
    }
    Ok(())
}

fn apply_build(state: &mut State, units: &UnitMap) -> Result<(), RecordedAdapterError> {
    let mut seen = HashSet::new();
    for unit in units.values().filter_map(|value| value.get_value()) {
        if !seen.insert(unit.units_id) || state.units.contains(unit_id(unit.units_id)) {
            continue;
        }
        let owner = player_id(unit.units_players_id);
        let player = state
            .find_player_mut(&owner)
            .ok_or(RecordedAdapterError::UnknownPlayer(unit.units_players_id))?;
        let cost = u64::from(
            unit.units_cost
                .unwrap_or_else(|| unit.units_name.base_cost()),
        );
        player.funds = player.funds.saturating_sub(cost);
        let position = property_pos(unit)?;
        state.units.push(Unit {
            id: unit_id(unit.units_id),
            kind: unit.units_name,
            owner,
            hp: display_hp(unit.units_hit_points.value()),
            fuel: u64::from(
                unit.units_fuel
                    .unwrap_or_else(|| unit.units_name.max_fuel()),
            ),
            ammo: u64::from(
                unit.units_ammo
                    .unwrap_or_else(|| unit.units_name.max_ammo()),
            ),
            action: UnitAction::Spent,
            concealment: if matches!(unit.units_sub_dive.as_str(), "D" | "Y") {
                Concealment::Hidden
            } else {
                Concealment::Exposed
            },
            location: Location::Board { position },
        });
        state.next_unit_id = Some(
            state
                .next_unit_id
                .unwrap_or_default()
                .max(unit.units_id.as_u32().saturating_add(1)),
        );
    }
    Ok(())
}

fn apply_combat_unit(
    state: &mut State,
    recorded: &CombatUnit,
    spent: bool,
) -> Result<(), RecordedAdapterError> {
    let id = unit_id(recorded.units_id);
    let hp = recorded.units_hit_points.map(|value| value.value());
    if hp == Some(0) {
        remove_unit(state, id)?;
        return Ok(());
    }
    let Some(unit) = state.units.get_mut(id) else {
        // Recipient-targeted rows can repeat stale last-known units after a
        // globally visible outcome has already removed them.
        return Ok(());
    };
    unit.ammo = u64::from(recorded.units_ammo);
    if let Some(hp) = hp {
        unit.hp = display_hp(hp);
    }
    if spent {
        unit.action = UnitAction::Spent;
    }
    Ok(())
}

fn apply_power(state: &mut State, action: &PowerAction) -> Result<(), RecordedAdapterError> {
    let owner = player_id(action.player_id.as_u32());
    let player = state
        .find_player_mut(&owner)
        .ok_or(RecordedAdapterError::UnknownPlayer(
            action.player_id.as_u32(),
        ))?;
    let slot = player
        .commanders
        .iter()
        .position(|commander| commander.active)
        .ok_or(RecordedAdapterError::Missing("active commander"))?;
    player.power_state = match action.co_power.as_str() {
        "Y" | "Power" => PowerState::Cop {
            commander_slot: slot as u8,
        },
        "S" | "Super" => PowerState::Scop {
            commander_slot: slot as u8,
        },
        _ => return Err(RecordedAdapterError::Missing("known power level")),
    };
    let commander = &mut player.commanders[slot];
    commander.power_charge = awbw_power_charge(action.players_cop);
    commander.power_uses = commander.power_uses.saturating_add(1);

    if let Some(weather) = &action.weather {
        state.weather.kind = weather_kind(weather.weather_code);
    }
    if let Some(changes) = &action.hp_change {
        if let Some(effect) = &changes.hp_gain {
            apply_hp_effect(state, effect);
        }
        if let Some(effect) = &changes.hp_loss {
            apply_hp_effect(state, effect);
        }
    }
    if let Some(groups) = &action.unit_replace {
        let mut seen = HashSet::new();
        for change in groups
            .values()
            .filter_map(|group| group.units.as_ref())
            .flatten()
        {
            if seen.insert(change.units_id) {
                apply_unit_change(state, change)?;
            }
        }
    }
    // AWBW omits loaded units from Jess power recipient rows even though
    // Turbo Charge and Overdrive resupply their internal cargo state. The
    // updated fuel/ammo only becomes visible when that cargo is unloaded.
    if action.co_name.eq_ignore_ascii_case("jess")
        && matches!(action.power_name.as_str(), "Turbo Charge" | "Overdrive")
    {
        for unit in &mut state.units {
            if unit.owner == owner && matches!(unit.location, Location::Cargo { .. }) {
                let unit_profile = profile(unit.kind);
                unit.fuel = unit_profile.max_fuel;
                unit.ammo = unit_profile.max_ammo;
            }
        }
    }
    if let Some(groups) = &action.unit_add {
        let mut seen = HashSet::new();
        let spawned_hp = if action.co_name.eq_ignore_ascii_case("sensei") {
            90
        } else {
            100
        };
        for group in groups.values() {
            for spawned in &group.units {
                if !seen.insert(spawned.units_id) || state.units.contains(unit_id(spawned.units_id))
                {
                    continue;
                }
                let owner = player_id(group.player_id.as_u32());
                state.units.push(Unit {
                    id: unit_id(spawned.units_id),
                    kind: group.unit_name,
                    owner,
                    hp: spawned_hp,
                    fuel: profile(group.unit_name).max_fuel,
                    ammo: profile(group.unit_name).max_ammo,
                    action: UnitAction::Ready,
                    concealment: Concealment::Exposed,
                    location: Location::Board {
                        position: pos(spawned.units_x, spawned.units_y)?,
                    },
                });
                state.next_unit_id = Some(
                    state
                        .next_unit_id
                        .unwrap_or_default()
                        .max(spawned.units_id.as_u32().saturating_add(1)),
                );
            }
        }
    }
    if let Some(groups) = &action.player_replace {
        for changes in groups.values() {
            for (target, change) in changes {
                let TargetedPlayer::Player(id) = target else {
                    continue;
                };
                let player = state
                    .find_player_mut(&player_id(id.as_u32()))
                    .ok_or(RecordedAdapterError::UnknownPlayer(id.as_u32()))?;
                if let Some(funds) = change.players_funds {
                    player.funds = u64::from(funds);
                }
                if let Some(charge) = change.players_co_power
                    && let Some(commander) = player.commanders.first_mut()
                {
                    commander.power_charge = awbw_power_charge(charge);
                }
                if let Some(charge) = change.tags_co_power
                    && let Some(commander) = player.commanders.get_mut(1)
                {
                    commander.power_charge = awbw_power_charge(charge);
                }
            }
        }
    }
    Ok(())
}

fn apply_end(state: &mut State, updated: &UpdatedInfo) -> Result<(), RecordedAdapterError> {
    let next = player_id(updated.next_player_id);
    state.turn.day = u64::from(updated.day);
    state.turn.active_player = next.clone();
    state.turn.position = state
        .turn
        .order
        .iter()
        .position(|player| *player == next)
        .ok_or(RecordedAdapterError::UnknownPlayer(updated.next_player_id))?;
    state.weather.kind = weather_kind(updated.next_weather);
    state
        .find_player_mut(&next)
        .ok_or(RecordedAdapterError::UnknownPlayer(updated.next_player_id))?
        .power_state = PowerState::None;
    for unit in &mut state.units {
        if unit.owner == next {
            unit.action = UnitAction::Ready;
        }
    }
    if let Some(funds) = targeted_hidden(&updated.next_funds)
        && let Some(player) = state.find_player_mut(&next)
    {
        player.funds = u64::from(funds);
    }
    let resupplied = updated
        .supplied
        .as_ref()
        .map(targeted_vec_union)
        .unwrap_or_default()
        .into_iter()
        .map(unit_id)
        .collect::<HashSet<_>>();
    for id in &resupplied {
        refill(state, *id)?;
    }
    apply_recorded_upkeep(state, &next, &resupplied)?;
    if let Some(repaired) = &updated.repaired {
        for unit in targeted_vec_union(repaired) {
            apply_repaired(state, &unit)?;
        }
    }
    Ok(())
}

fn apply_recorded_upkeep(
    state: &mut State,
    incoming: &PlayerId,
    resupplied: &HashSet<UnitId>,
) -> Result<(), RecordedAdapterError> {
    if state.turn.day < 2 {
        return Ok(());
    }
    let upkeep = state
        .units
        .iter()
        .filter(|unit| {
            unit.owner == *incoming
                && !resupplied.contains(&unit.id)
                && matches!(profile(unit.kind).domain, Domain::Air | Domain::Sea)
        })
        .map(|unit| {
            let unit_profile = profile(unit.kind);
            let base = if unit.concealment == Concealment::Hidden {
                unit_profile
                    .fuel_per_turn
                    .hidden
                    .unwrap_or(unit_profile.fuel_per_turn.normal)
            } else {
                unit_profile.fuel_per_turn.normal
            };
            (
                unit.id,
                commander::effective_upkeep(state, unit, base, unit_profile.domain),
            )
        })
        .collect::<Vec<_>>();
    for (id, upkeep) in upkeep {
        if let Some(unit) = state.units.get_mut(id) {
            unit.fuel = unit.fuel.saturating_sub(upkeep);
        }
    }
    let crashed = state
        .units
        .iter()
        .filter(|unit| {
            unit.owner == *incoming
                && unit.fuel == 0
                && matches!(profile(unit.kind).domain, Domain::Air | Domain::Sea)
        })
        .map(|unit| unit.id)
        .collect::<Vec<_>>();
    for id in crashed {
        remove_unit(state, id)?;
    }
    Ok(())
}

fn apply_hp_effect(state: &mut State, effect: &HpEffect) {
    let owners = effect
        .players
        .iter()
        .map(|id| player_id(id.as_u32()))
        .collect::<HashSet<_>>();
    for unit in &mut state.units {
        if !owners.contains(&unit.owner) || !matches!(unit.location, Location::Board { .. }) {
            continue;
        }
        let visual = i32::from(unit.hp.div_ceil(10));
        unit.hp = display_hp((visual + effect.hp).clamp(1, 10) as u8);
        unit.fuel = ((unit.fuel as f64) * effect.units_fuel)
            .ceil()
            .clamp(0.0, profile(unit.kind).max_fuel as f64) as u64;
    }
}

fn apply_unit_change(state: &mut State, change: &UnitChange) -> Result<(), RecordedAdapterError> {
    let id = unit_id(change.units_id);
    let Some(unit) = state.units.get_mut(id) else {
        return Ok(());
    };
    if let Some(hp) = change.units_hit_points {
        unit.hp = display_hp(hp.value().max(1));
    }
    if let Some(ammo) = change.units_ammo {
        unit.ammo = u64::from(ammo);
    }
    if let Some(fuel) = change.units_fuel {
        unit.fuel = u64::from(fuel);
    }
    if let Some(moved) = change.units_moved {
        unit.action = if moved < 0 {
            UnitAction::Immobilized
        } else if moved == 0 {
            UnitAction::Ready
        } else {
            unit.action
        };
    }
    Ok(())
}

fn update_from_property(
    state: &mut State,
    recorded: &UnitProperty,
) -> Result<(), RecordedAdapterError> {
    let id = unit_id(recorded.units_id);
    let Some(unit) = state.units.get_mut(id) else {
        return Ok(());
    };
    unit.hp = display_hp(recorded.units_hit_points.value().max(1));
    if let Some(fuel) = recorded.units_fuel {
        unit.fuel = u64::from(fuel);
    }
    if let Some(ammo) = recorded.units_ammo {
        unit.ammo = u64::from(ammo);
    }
    Ok(())
}

fn apply_repaired(state: &mut State, repaired: &RepairedUnit) -> Result<(), RecordedAdapterError> {
    let id = unit_id(repaired.units_id);
    if repaired.units_hit_points.value() == 0 {
        return remove_unit(state, id);
    }
    let Some(unit) = state.units.get_mut(id) else {
        return Ok(());
    };
    unit.hp = display_hp(repaired.units_hit_points.value());
    unit.fuel = profile(unit.kind).max_fuel;
    unit.ammo = profile(unit.kind).max_ammo;
    Ok(())
}

fn refill(state: &mut State, id: UnitId) -> Result<(), RecordedAdapterError> {
    let Some(unit) = state.units.get_mut(id) else {
        return Ok(());
    };
    unit.fuel = profile(unit.kind).max_fuel;
    unit.ammo = profile(unit.kind).max_ammo;
    Ok(())
}

fn spend(state: &mut State, id: UnitId) -> Result<(), RecordedAdapterError> {
    if let Some(unit) = state.units.get_mut(id) {
        unit.action = UnitAction::Spent;
    }
    Ok(())
}

fn remove_unit(state: &mut State, id: UnitId) -> Result<(), RecordedAdapterError> {
    if let Some(Location::Board { position }) =
        state.units.get(id).map(|unit| unit.location.clone())
        && !state
            .units
            .iter()
            .any(|unit| unit.id != id && unit.location == (Location::Board { position }))
        && state
            .board
            .get(position)
            .and_then(|tile| tile.capture_points)
            .is_some_and(|points| points < 20)
    {
        state.board.tile_mut(position).capture_points = Some(20);
    }
    let cargo = state
        .units
        .iter()
        .filter_map(|unit| {
            matches!(
                unit.location,
                Location::Cargo { transport, .. } if transport == id
            )
            .then_some(unit.id)
        })
        .collect::<Vec<_>>();
    for cargo in cargo {
        if let Some(index) = state.units.index_of(cargo) {
            state.units.remove(index);
        }
    }
    if let Some(index) = state.units.index_of(id) {
        state.units.remove(index);
    }
    Ok(())
}

fn update_combat_charge(
    state: &mut State,
    player: u32,
    lead: u32,
    tag: Option<u32>,
) -> Result<(), RecordedAdapterError> {
    let player = state
        .find_player_mut(&player_id(player))
        .ok_or(RecordedAdapterError::UnknownPlayer(player))?;
    if let Some(commander) = player.commanders.first_mut() {
        commander.power_charge = awbw_power_charge(lead);
    }
    if let Some(tag) = tag
        && let Some(commander) = player.commanders.get_mut(1)
    {
        commander.power_charge = awbw_power_charge(tag);
    }
    Ok(())
}

fn set_player_funds(
    state: &mut State,
    player: u32,
    funds: u32,
) -> Result<(), RecordedAdapterError> {
    state
        .find_player_mut(&player_id(player))
        .ok_or(RecordedAdapterError::UnknownPlayer(player))?
        .funds = u64::from(funds);
    Ok(())
}

/// Spread one recorded area effect over the units the ruleset says it reaches.
///
/// The archive records the HP delta but not which units absorbed it, so the
/// shape comes from `strike` while the recorded magnitude stays authoritative.
fn apply_area(
    state: &mut State,
    strike: AreaStrikeProfile,
    center: Pos,
    delta: i32,
    excluded: Option<UnitId>,
) {
    for unit in &mut state.units {
        if Some(unit.id) == excluded {
            continue;
        }
        let Location::Board { position } = unit.location else {
            continue;
        };
        if strike.covers(center, position) {
            let visual = i32::from(unit.hp.div_ceil(10));
            unit.hp = display_hp((visual + delta).clamp(1, 10) as u8);
        }
    }
}

fn diff_events(prior: &State, post: &State, action: &Action) -> Vec<Event> {
    let mut events = Vec::new();
    let reason = || Reason::Other(ReasonId::from(RECORDED_REASON));
    let paths = recorded_paths(action);

    for before in &prior.units {
        let Some(after) = post.units.get(before.id) else {
            events.push(Event::UnitRemoved {
                unit: before.id,
                reason: reason(),
            });
            continue;
        };
        match (&before.location, &after.location) {
            (Location::Board { position: from }, Location::Board { position: to })
                if from != to =>
            {
                events.push(Event::UnitMoved {
                    unit: before.id,
                    from: *from,
                    to: *to,
                    path: paths
                        .get(&before.id)
                        .cloned()
                        .unwrap_or_else(|| vec![*from, *to]),
                    fuel_spent: before.fuel.saturating_sub(after.fuel),
                })
            }
            (Location::Board { .. }, Location::Cargo { transport, slot }) => {
                events.push(Event::UnitLoaded {
                    unit: before.id,
                    transport: *transport,
                    slot: *slot,
                })
            }
            (Location::Cargo { transport, .. }, Location::Board { position }) => {
                events.push(Event::UnitUnloaded {
                    unit: before.id,
                    transport: *transport,
                    position: *position,
                })
            }
            _ => {}
        }
        if before.hp != after.hp {
            events.push(if after.hp < before.hp {
                Event::UnitDamaged {
                    unit: before.id,
                    from_hp: before.hp,
                    to_hp: after.hp,
                    reason: reason(),
                }
            } else {
                Event::UnitRepaired {
                    unit: before.id,
                    from_hp: before.hp,
                    to_hp: after.hp,
                    reason: reason(),
                }
            });
        }
        if before.fuel != after.fuel || before.ammo != after.ammo {
            events.push(Event::UnitResourced {
                unit: before.id,
                fuel_before: before.fuel,
                fuel_after: after.fuel,
                ammo_before: before.ammo,
                ammo_after: after.ammo,
                reason: reason(),
            });
        }
        if before.action != after.action {
            events.push(Event::UnitActionChanged {
                unit: before.id,
                from: before.action,
                to: after.action,
                reason: reason(),
            });
        }
        if before.concealment != after.concealment {
            events.push(Event::ConcealmentChanged {
                unit: before.id,
                from: before.concealment,
                to: after.concealment,
            });
        }
    }
    for unit in &post.units {
        if prior.units.contains(unit.id) {
            continue;
        }
        if let Location::Board { position } = unit.location {
            events.push(Event::UnitCreated {
                unit: unit.id,
                kind: unit.kind,
                owner: unit.owner.clone(),
                position,
            });
        }
    }
    for position in prior.board.positions() {
        let before = prior.board.tile(position);
        let after = post.board.tile(position);
        if before.owner != after.owner {
            events.push(Event::TileOwnerChanged {
                position,
                from: before.owner.to_optional(),
                to: after.owner.to_optional(),
            });
        }
        if before.terrain != after.terrain {
            events.push(Event::TileTerrainChanged {
                position,
                from: before.terrain,
                to: after.terrain,
                reason: reason(),
            });
        }
        if before.capture_points != after.capture_points {
            events.push(Event::CaptureChanged {
                position,
                from: before.capture_points.unwrap_or(0),
                to: after.capture_points.unwrap_or(0),
            });
        }
        if before.silo != after.silo
            && let (Some(from), Some(to)) = (before.silo, after.silo)
        {
            events.push(Event::SiloChanged { position, from, to });
        }
        if before.destructible_hp() != after.destructible_hp()
            && let (Some(from), Some(to)) = (before.destructible_hp(), after.destructible_hp())
            && let (Ok(from_hp), Ok(to_hp)) = (u8::try_from(from), u8::try_from(to))
        {
            events.push(Event::DestructibleDamaged {
                position,
                from_hp,
                to_hp,
            });
        }
    }
    for before in &prior.players {
        let Some(after) = post.find_player(&before.id) else {
            continue;
        };
        if before.funds != after.funds {
            events.push(Event::FundsChanged {
                player: before.id.clone(),
                from: before.funds,
                to: after.funds,
                reason: reason(),
            });
        }
        if before.status != after.status {
            events.push(Event::PlayerStatusChanged {
                player: before.id.clone(),
                from: before.status,
                to: after.status,
            });
        }
        for (slot, (old, new)) in before.commanders.iter().zip(&after.commanders).enumerate() {
            if old.power_charge != new.power_charge {
                events.push(Event::PowerChargeChanged {
                    player: before.id.clone(),
                    commander_slot: slot,
                    from: old.power_charge,
                    to: new.power_charge,
                    reason: reason(),
                });
            }
        }
        let old_slot = before.commanders.iter().position(|value| value.active);
        let new_slot = after.commanders.iter().position(|value| value.active);
        if let (Some(from_slot), Some(to_slot)) = (old_slot, new_slot)
            && from_slot != to_slot
        {
            events.push(Event::CommanderSwapped {
                player: before.id.clone(),
                from_slot,
                to_slot,
            });
        }
        // A recorded power state can name a commander slot the roster does not
        // have; such a power change is simply not describable as an event.
        let commander_at = |commanders: &[awvm::semantic::Commander], slot: u8| {
            commanders.get(usize::from(slot)).map(|value| value.id)
        };
        match (&before.power_state, &after.power_state) {
            (PowerState::None, PowerState::Cop { commander_slot }) => {
                if let Some(commander) = commander_at(&after.commanders, *commander_slot) {
                    events.push(Event::PowerActivated {
                        player: before.id.clone(),
                        commander,
                        power: PowerLevel::Cop,
                    });
                }
            }
            (PowerState::None, PowerState::Scop { commander_slot }) => {
                if let Some(commander) = commander_at(&after.commanders, *commander_slot) {
                    events.push(Event::PowerActivated {
                        player: before.id.clone(),
                        commander,
                        power: PowerLevel::Scop,
                    });
                }
            }
            (PowerState::Cop { commander_slot }, PowerState::None) => {
                if let Some(commander) = commander_at(&before.commanders, *commander_slot) {
                    events.push(Event::PowerEnded {
                        player: before.id.clone(),
                        commander,
                        power: PowerLevel::Cop,
                    });
                }
            }
            (PowerState::Scop { commander_slot }, PowerState::None) => {
                if let Some(commander) = commander_at(&before.commanders, *commander_slot) {
                    events.push(Event::PowerEnded {
                        player: before.id.clone(),
                        commander,
                        power: PowerLevel::Scop,
                    });
                }
            }
            _ => {}
        }
    }
    if prior.turn.day != post.turn.day {
        events.push(Event::DayAdvanced {
            from: prior.turn.day,
            to: post.turn.day,
        });
    }
    if prior.turn.phase != post.turn.phase {
        events.push(Event::PhaseChanged {
            player: post.turn.active_player.clone(),
            from: prior.turn.phase,
            to: post.turn.phase,
        });
    }
    if prior.turn.active_player != post.turn.active_player {
        events.push(Event::TurnSelected {
            player: post.turn.active_player.clone(),
            position: post.turn.position,
        });
    }
    if prior.weather != post.weather {
        events.push(Event::WeatherChanged {
            from: prior.weather.kind,
            to: post.weather.kind,
            remaining_turns: post.weather.remaining_turns,
            reason: reason(),
        });
    }
    if matches!(prior.match_state, Match::Active { .. })
        && let Match::Finished { outcome } = &post.match_state
    {
        events.push(Event::MatchCompleted {
            outcome: outcome.clone(),
        });
    }
    events
}

fn recorded_paths(action: &Action) -> HashMap<UnitId, Vec<Pos>> {
    let mut paths = HashMap::new();
    let Some(movement) = action.move_action() else {
        return paths;
    };
    let Some((target, unit)) = visible_targeted_unit(&movement.unit) else {
        return paths;
    };
    let Some(recorded) = movement
        .paths
        .get(&TargetedPlayer::Global)
        .or_else(|| movement.paths.get(&target))
    else {
        return paths;
    };
    let path = recorded
        .iter()
        .filter_map(|tile| pos(tile.x, tile.y).ok())
        .collect::<Vec<_>>();
    // An unrepresentable path is treated as absent so the caller falls back to
    // the recorded endpoints instead of emitting an empty movement path.
    if !path.is_empty() {
        paths.insert(unit_id(unit.units_id), path);
    }
    paths
}

fn visible_move_unit(action: &MoveAction) -> Option<&UnitProperty> {
    visible_targeted_unit(&action.unit).map(|(_, unit)| unit)
}

/// The moving unit together with the coordinates it moved to.
///
/// Global rows win over recipient rows, but only a row that discloses both
/// coordinates describes a destination.
fn complete_move_unit(action: &MoveAction) -> Option<(&UnitProperty, u32, u32)> {
    fn with_position(unit: &UnitProperty) -> Option<(&UnitProperty, u32, u32)> {
        Some((unit, unit.units_x?, unit.units_y?))
    }
    action
        .unit
        .get(&TargetedPlayer::Global)
        .and_then(|value| value.get_value())
        .and_then(with_position)
        .or_else(|| {
            action
                .unit
                .values()
                .filter_map(|value| value.get_value())
                .find_map(with_position)
        })
}

fn targeted_vec_union<T: Clone + PartialEq>(
    values: &indexmap::IndexMap<TargetedPlayer, Vec<T>>,
) -> Vec<T> {
    let mut result = Vec::new();
    for item in values.values().flatten() {
        if !result.contains(item) {
            result.push(item.clone());
        }
    }
    result
}

fn unit_on_board(state: &State, position: Pos) -> Option<UnitId> {
    state
        .units
        .iter()
        .find_map(|unit| (unit.location == Location::Board { position }).then_some(unit.id))
}

fn board_position(state: &State, id: UnitId) -> Option<Pos> {
    match state.units.get(id)?.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
}

fn property_pos(unit: &UnitProperty) -> Result<Pos, RecordedAdapterError> {
    pos(
        unit.units_x
            .ok_or(RecordedAdapterError::Missing("unit x coordinate"))?,
        unit.units_y
            .ok_or(RecordedAdapterError::Missing("unit y coordinate"))?,
    )
}

fn pos(x: u32, y: u32) -> Result<Pos, RecordedAdapterError> {
    Ok(Pos::new(
        u8::try_from(x).map_err(|_| RecordedAdapterError::Coordinate { x, y })?,
        u8::try_from(y).map_err(|_| RecordedAdapterError::Coordinate { x, y })?,
    ))
}

fn unit_id(id: AwbwUnitId) -> UnitId {
    UnitId::new(id.as_u32())
}

fn player_id(id: u32) -> PlayerId {
    PlayerId::from(id.to_string())
}

fn display_hp(value: u8) -> u8 {
    value.clamp(1, 10) * 10
}

fn weather_kind(value: WeatherCode) -> WeatherKind {
    match value {
        WeatherCode::Clear => WeatherKind::Clear,
        WeatherCode::Rain => WeatherKind::Rain,
        WeatherCode::Snow => WeatherKind::Snow,
    }
}
