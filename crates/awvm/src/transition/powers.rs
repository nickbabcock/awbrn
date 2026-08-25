//! Commander powers and tag swaps.
//!
//! Normative source:
//! * `spec/semantics/powers.md`
//! * `spec/semantics/tag.md`

use super::ReducerError as ExecuteError;
use super::*;
use crate::commander::{
    self, AreaStrikeCenterTarget, AreaStrikePolicy, CommanderSlotTarget, FriendlyContribution,
    ImmobilizationDuration, InstantEffect, OccupiedTileHandling, PlayerTarget, PropertyOrder,
    PropertyTarget, SpawnAction, SpawnConcealment, SpawnResources, SpawnUnitLimit,
    TargetedAreaStrikePolicy, TargetedUnitValue, UnitTarget, WeatherDuration, WeatherEffectKind,
};
use crate::ruleset::{self, PropertyKind, TerrainTrait};
use crate::semantic::{CAPTURE_REQUIRED_POINTS, TeamId, UnitAction};
use crate::violation::Action;
use std::collections::HashSet;

pub(crate) fn area_strike_centers(
    state: &State,
    player: &PlayerId,
    radius: usize,
    policies: &[AreaStrikePolicy],
) -> Result<Vec<Pos>, ExecuteError> {
    let actor_team = state
        .find_player(player)
        .map(|candidate| &candidate.team)
        .ok_or_else(|| ExecuteError::InvalidState("area-strike actor is missing".into()))?;
    let mut priced_units = Vec::new();
    for unit in &state.units {
        let Location::Board { position } = unit.location else {
            continue;
        };
        let base_cost = ruleset::profile(unit.kind).cost;
        let cost = commander::effective_build_cost(state, Some(unit.owner), base_cost)
            .ok_or_else(|| ExecuteError::InvalidState("area-strike cost overflow".into()))?;
        let friendly = state
            .players
            .get(unit.owner.get())
            .is_some_and(|owner| owner.team == actor_team);
        let capturing = matches!(unit.kind, UnitKind::Infantry | UnitKind::Mech)
            && state
                .board
                .tile(position)
                .capture_points
                .is_some_and(|points| points < CAPTURE_REQUIRED_POINTS);
        priced_units.push((unit, position, cost, friendly, capturing));
    }

    let mut centers = Vec::with_capacity(policies.len());
    for policy in policies {
        let mut best: Option<(i128, i128, Pos)> = None;
        for center in state.board.positions() {
            {
                let mut score = 0_i128;
                let mut enemy_tiebreak = 0_i128;
                for (unit, position, cost, friendly, capturing) in &priced_units {
                    if center.distance(*position) > radius as u64 {
                        continue;
                    }
                    let exact_hp = i128::from(unit.hp);
                    let capped_hp = exact_hp.clamp(1, 30);
                    let cost = i128::from(*cost);
                    let value = match policy {
                        AreaStrikePolicy::InfantryHp => {
                            let multiplier =
                                if matches!(unit.kind, UnitKind::Infantry | UnitKind::Mech)
                                    && unit.hp > 10
                                {
                                    if *capturing { 8 } else { 4 }
                                } else {
                                    1
                                };
                            capped_hp.checked_mul(multiplier).ok_or_else(|| {
                                ExecuteError::InvalidState(
                                    "area-strike infantry score overflow".into(),
                                )
                            })?
                        }
                        AreaStrikePolicy::UnitValue => {
                            if unit.hp < 10 {
                                2
                            } else {
                                capped_hp.checked_mul(cost).ok_or_else(|| {
                                    ExecuteError::InvalidState(
                                        "area-strike cost score overflow".into(),
                                    )
                                })?
                            }
                        }
                        AreaStrikePolicy::UnitHp => capped_hp,
                    };
                    score = if *friendly {
                        score.checked_sub(value)
                    } else {
                        score.checked_add(value)
                    }
                    .ok_or_else(|| {
                        ExecuteError::InvalidState("area-strike score overflow".into())
                    })?;
                    if !friendly {
                        let tie_value = match policy {
                            AreaStrikePolicy::InfantryHp => 0,
                            AreaStrikePolicy::UnitValue => {
                                exact_hp.checked_mul(cost).ok_or_else(|| {
                                    ExecuteError::InvalidState(
                                        "area-strike cost tiebreak overflow".into(),
                                    )
                                })?
                            }
                            AreaStrikePolicy::UnitHp => exact_hp,
                        };
                        enemy_tiebreak =
                            enemy_tiebreak.checked_add(tie_value).ok_or_else(|| {
                                ExecuteError::InvalidState("area-strike tiebreak overflow".into())
                            })?;
                    }
                }
                if best.as_ref().is_none_or(|(best_score, best_tie, _)| {
                    score > *best_score || (score == *best_score && enemy_tiebreak > *best_tie)
                }) {
                    best = Some((score, enemy_tiebreak, center));
                }
            }
        }
        centers.push(
            best.map(|(_, _, center)| center)
                .ok_or_else(|| ExecuteError::InvalidState("area-strike board is empty".into()))?,
        );
    }
    Ok(centers)
}

/// A power activation that passed every check but has changed nothing.
#[derive(Debug)]
pub(super) struct PreparedPower<'a> {
    turn: ActiveTurn<'a>,
    level: PowerLevel,
    player_index: PlayerIdx,
    active_slot: u8,
    activation: commander::PowerActivation,
}

/// Decide whether `level` may be activated, without activating it.
///
/// Everything up to the state clone is a check; splitting it out is what lets
/// a caller ask whether a power is available and pay for the answer once.
pub(super) fn prepare_power(
    turn: ActiveTurn<'_>,
    level: PowerLevel,
) -> Result<PreparedPower<'_>, ExecuteError> {
    let state = turn.state();
    let player = turn.player();
    let player_index = state
        .player_index(player)
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent from players".into()))?;
    let actor = state.player(player_index);
    let active_slot = actor
        .commanders
        .iter()
        .position(|candidate| candidate.active)
        .ok_or_else(|| ExecuteError::InvalidState("player has no active commander".into()))?;
    let active_slot = u8::try_from(active_slot)
        .map_err(|_| ExecuteError::InvalidState("active commander slot exceeds u8".into()))?;
    let active_commander = &actor.commanders[usize::from(active_slot)];
    if state.settings.powers != crate::semantic::Toggle::Enabled
        || !matches!(actor.power_state, crate::semantic::PowerState::None)
    {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::ActivatePower,
        }));
    }
    let activation =
        commander::power_activation(active_commander.id, level, active_commander.power_uses)
            .map_err(|error| {
                ExecuteError::InvalidState(
                    format!("commander power profile cannot activate: {error:?}").into(),
                )
            })?
            .ok_or_else(|| {
                violation(Violation::ActionNotSupported {
                    action: Action::ActivatePower,
                })
            })?;
    let cost = activation.cost;
    if active_commander.power_charge < cost {
        return Err(violation(Violation::InsufficientPower {
            required: cost,
            available: active_commander.power_charge,
        }));
    }

    Ok(PreparedPower {
        turn,
        level,
        player_index,
        active_slot,
        activation,
    })
}

pub(super) fn execute_prepared_power(
    prepared: PreparedPower<'_>,
) -> Result<Execution, ExecuteError> {
    let PreparedPower {
        turn,
        level,
        player_index,
        active_slot,
        activation,
    } = prepared;
    let state = turn.state();
    let player = turn.player();
    let active_commander = &state.player(player_index).commanders[usize::from(active_slot)];
    let cost = activation.cost;

    let mut next = state.clone();
    let commander = &mut next.player_mut(player_index).commanders[usize::from(active_slot)];
    commander.power_charge -= cost;
    commander.power_uses = commander
        .power_uses
        .checked_add(1)
        .ok_or_else(|| ExecuteError::InvalidState("commander power uses overflow".into()))?;
    next.player_mut(player_index).power_state = match level {
        PowerLevel::Cop => crate::semantic::PowerState::Cop {
            commander_slot: active_slot,
        },
        PowerLevel::Scop => crate::semantic::PowerState::Scop {
            commander_slot: active_slot,
        },
    };
    let mut events = vec![Event::PowerActivated {
        player: player.clone(),
        commander: active_commander.id,
        power: level,
    }];
    let mut cx = Activation {
        state,
        player,
        seat: player_index,
        next: &mut next,
        events: &mut events,
    };
    for effect in activation.instant_effects {
        match effect {
            InstantEffect::HealVisualHp {
                target: UnitTarget::Owned,
                amount,
            } => heal_visual_hp(&mut cx, amount)?,
            InstantEffect::HealExactHp {
                target: UnitTarget::Owned,
                amount,
            } => heal_exact_hp(&mut cx, amount)?,
            InstantEffect::DamageExactHp {
                target: target @ (UnitTarget::Enemy | UnitTarget::EnemyOnProperties),
                amount,
                minimum_hp,
            } => damage_exact_hp(&mut cx, target, amount, minimum_hp)?,
            InstantEffect::SetWeather {
                kind,
                duration: WeatherDuration::UntilOwnerNextTurn,
            } => set_weather(&mut cx, kind)?,
            InstantEffect::DrainCurrentFuelRatio {
                target: UnitTarget::Enemy,
                numerator,
                denominator,
            } => drain_current_fuel_ratio(&mut cx, numerator, denominator)?,
            InstantEffect::FireAreaStrikes {
                target: UnitTarget::AllBoard,
                radius,
                damage,
                minimum_hp,
                selection_policies,
                friendly_contribution: FriendlyContribution::Subtract,
            } => fire_area_strikes(&mut cx, radius, damage, minimum_hp, selection_policies)?,
            InstantEffect::ReducePowerChargeByFundsRatio {
                target: CommanderSlotTarget::EnemyCommanderSlots,
                funds_per_full_bar,
            } => reduce_power_charge_by_funds_ratio(&mut cx, funds_per_full_bar)?,
            InstantEffect::RefreshUnitAction {
                target: UnitTarget::Owned,
                exclude_unit_kinds,
            } => refresh_unit_action(&mut cx, exclude_unit_kinds)?,
            InstantEffect::ResupplyUnits {
                target: UnitTarget::Owned,
            } => resupply_units(&mut cx)?,
            InstantEffect::SpawnUnitsOnOwnedProperties {
                target: PropertyTarget::OwnedProperties,
                property_kinds,
                unit_kind,
                hp,
                resources: SpawnResources::UnitMaxima,
                action: SpawnAction::Ready,
                concealment: SpawnConcealment::Exposed,
                occupied_tiles: OccupiedTileHandling::Skip,
                order: PropertyOrder::YThenX,
                unit_limit: SpawnUnitLimit::Settings,
            } => spawn_units_on_owned_properties(&mut cx, property_kinds, unit_kind, hp)?,
            InstantEffect::FireTargetedAreaStrike {
                target: AreaStrikeCenterTarget::EnemyUnitCenters,
                radius,
                damage,
                minimum_hp,
                selection_policy: TargetedAreaStrikePolicy::UnitValue,
                friendly_contribution: FriendlyContribution::Subtract,
                unit_value: TargetedUnitValue::BaseBuildCost,
            } => fire_targeted_area_strike(&mut cx, radius, damage, minimum_hp)?,
            InstantEffect::FireImmobilizingAreaStrike {
                target: UnitTarget::Enemy,
                radius,
                damage,
                minimum_hp,
                selection_policy: TargetedAreaStrikePolicy::UnitValue,
                friendly_contribution: FriendlyContribution::Subtract,
                unit_value: TargetedUnitValue::BaseBuildCost,
                duration: ImmobilizationDuration::ThroughTargetNextTurn,
            } => fire_immobilizing_area_strike(&mut cx, radius, damage, minimum_hp)?,
            InstantEffect::MultiplyFundsRatio {
                target: PlayerTarget::ActivatingPlayer,
                numerator,
                denominator,
            } => multiply_funds_ratio(&mut cx, numerator, denominator)?,
            unsupported => {
                return Err(ExecuteError::InvalidState(
                    format!("unsupported instant-effect target combination: {unsupported:?}")
                        .into(),
                ));
            }
        }
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}

/// What an instant power effect operates on.
///
/// `execute_activate_power` was a 785-line function whose body was one `match`
/// over thirteen effects; each arm is now its own function, and this is the
/// environment they shared.
struct Activation<'a> {
    /// The state the command was validated against.
    state: &'a State,
    /// The activating player.
    player: &'a PlayerId,
    seat: crate::semantic::PlayerIdx,
    /// The state being built.
    next: &'a mut State,
    events: &'a mut Vec<Event>,
}

/// Restore whole HP bars, rounding as the visual scale does.
fn heal_visual_hp(cx: &mut Activation<'_>, amount: u8) -> Result<(), ExecuteError> {
    let mut targets: Vec<_> = cx
        .next
        .units
        .iter()
        .filter(|unit| unit.owner == cx.seat)
        .map(|unit| unit.id)
        .collect();
    targets.sort();
    for target_id in targets {
        let target = cx
            .next
            .units
            .get_mut(target_id)
            .expect("power target remains present");
        let from_hp = target.hp;
        let visual_hp = from_hp.div_ceil(10);
        let to_hp = visual_hp.saturating_add(amount).min(10) * 10;
        if to_hp == from_hp {
            continue;
        }
        target.hp = to_hp;
        cx.events.push(Event::UnitRepaired {
            unit: target_id,
            from_hp,
            to_hp,
            reason: KnownReason::CommanderPower.into(),
        });
    }
    Ok(())
}

/// Restore exact HP points.
fn heal_exact_hp(cx: &mut Activation<'_>, amount: u8) -> Result<(), ExecuteError> {
    let mut targets: Vec<_> = cx
        .next
        .units
        .iter()
        .filter(|unit| unit.owner == cx.seat && matches!(unit.location, Location::Board { .. }))
        .map(|unit| unit.id)
        .collect();
    targets.sort();
    for target_id in targets {
        let target = cx
            .next
            .units
            .get_mut(target_id)
            .expect("power target remains present");
        let from_hp = target.hp;
        let to_hp = from_hp.saturating_add(amount).min(100);
        if to_hp == from_hp {
            continue;
        }
        target.hp = to_hp;
        cx.events.push(Event::UnitRepaired {
            unit: target_id,
            from_hp,
            to_hp,
            reason: KnownReason::CommanderPower.into(),
        });
    }
    Ok(())
}

/// Remove exact HP points, never below `minimum_hp`.
fn damage_exact_hp(
    cx: &mut Activation<'_>,
    target: UnitTarget,
    amount: u8,
    minimum_hp: u8,
) -> Result<(), ExecuteError> {
    let actor_team = cx.next.player(cx.seat).team.clone();
    let properties_only = target == UnitTarget::EnemyOnProperties;
    let enemy_owners = enemy_seats(cx.next, &actor_team);
    let mut targets: Vec<_> = cx
        .next
        .units
        .iter()
        .filter(|unit| {
            enemy_owners.contains(&unit.owner)
                && match unit.location {
                    Location::Board { position } => {
                        !properties_only
                            || ruleset::terrain_has(
                                cx.next.board.tile(position).terrain,
                                TerrainTrait::Capturable,
                            )
                    }
                    Location::Cargo { .. } => false,
                }
        })
        .map(|unit| unit.id)
        .collect();
    targets.sort();
    for target_id in targets {
        let target = cx
            .next
            .units
            .get_mut(target_id)
            .expect("power target remains present");
        let from_hp = target.hp;
        let to_hp = from_hp.saturating_sub(amount).max(minimum_hp);
        if to_hp == from_hp {
            continue;
        }
        target.hp = to_hp;
        cx.events.push(Event::UnitDamaged {
            unit: target_id,
            from_hp,
            to_hp,
            reason: KnownReason::CommanderPower.into(),
        });
    }
    Ok(())
}

/// Force the weather for the effect's duration.
fn set_weather(cx: &mut Activation<'_>, kind: WeatherEffectKind) -> Result<(), ExecuteError> {
    let remaining_turns = turns_until_player_selection(cx.next, cx.player)?;
    let from = cx.next.weather.kind;
    let to = match kind {
        WeatherEffectKind::Clear => WeatherKind::Clear,
        WeatherEffectKind::Rain => WeatherKind::Rain,
        WeatherEffectKind::Snow => WeatherKind::Snow,
    };
    if cx.next.weather.kind == to && cx.next.weather.remaining_turns == remaining_turns {
        return Ok(());
    }
    cx.next.weather.kind = to;
    cx.next.weather.remaining_turns = remaining_turns;
    cx.events.push(Event::WeatherChanged {
        from,
        to: cx.next.weather.kind,
        remaining_turns,
        reason: KnownReason::CommanderPower.into(),
    });
    Ok(())
}

/// Every seat on a team other than `actor_team`.
///
/// A power names its targets by team, and the units name their owner by seat,
/// so the roster is turned into seats once and each unit is a lookup.
fn enemy_seats(state: &State, actor_team: &TeamId) -> HashSet<PlayerIdx> {
    state.players.seats_off_team(actor_team).collect()
}

/// Take a fraction of what each target still holds.
fn drain_current_fuel_ratio(
    cx: &mut Activation<'_>,
    numerator: u64,
    denominator: u64,
) -> Result<(), ExecuteError> {
    let actor_team = cx.next.player(cx.seat).team.clone();
    let enemy_owners = enemy_seats(cx.next, &actor_team);
    let mut targets: Vec<_> = cx
        .next
        .units
        .iter()
        .filter(|unit| {
            enemy_owners.contains(&unit.owner) && matches!(unit.location, Location::Board { .. })
        })
        .map(|unit| unit.id)
        .collect();
    targets.sort();
    for target_id in targets {
        let target = cx
            .next
            .units
            .get_mut(target_id)
            .expect("power target remains present");
        let fuel_before = target.fuel;
        let drained = fuel_before
            .checked_mul(numerator)
            .and_then(|value| value.checked_div(denominator))
            .ok_or_else(|| {
                ExecuteError::InvalidState("fuel-drain ratio arithmetic overflow".into())
            })?;
        let fuel_after = fuel_before.saturating_sub(drained);
        if fuel_after == fuel_before {
            continue;
        }
        target.fuel = fuel_after;
        cx.events.push(Event::UnitResourced {
            unit: target_id,
            fuel_before,
            fuel_after,
            ammo_before: target.ammo,
            ammo_after: target.ammo,
            reason: KnownReason::CommanderPower.into(),
        });
    }
    Ok(())
}

/// Fire one strike per selection policy, each at its own chosen centre.
fn fire_area_strikes(
    cx: &mut Activation<'_>,
    radius: usize,
    damage: u8,
    minimum_hp: u8,
    selection_policies: Vec<AreaStrikePolicy>,
) -> Result<(), ExecuteError> {
    let centers = area_strike_centers(cx.state, cx.player, radius, &selection_policies)?;
    for (strike, (policy, center)) in selection_policies.into_iter().zip(centers).enumerate() {
        cx.events.push(Event::AreaStrikeResolved {
            strike,
            policy,
            center,
            radius,
            damage,
        });
        let mut targets: Vec<_> = cx
            .next
            .units
            .iter()
            .filter_map(|unit| match unit.location {
                Location::Board { position } if center.distance(position) <= radius as u64 => {
                    Some(unit.id)
                }
                _ => None,
            })
            .collect();
        targets.sort();
        for target_id in targets {
            let target = cx
                .next
                .units
                .get_mut(target_id)
                .expect("area-strike target remains present");
            let from_hp = target.hp;
            let to_hp = from_hp.saturating_sub(damage).max(minimum_hp);
            if to_hp == from_hp {
                continue;
            }
            target.hp = to_hp;
            cx.events.push(Event::UnitDamaged {
                unit: target_id,
                from_hp,
                to_hp,
                reason: KnownReason::CommanderPower.into(),
            });
        }
    }
    Ok(())
}

/// Convert the target's funds into lost power charge.
fn reduce_power_charge_by_funds_ratio(
    cx: &mut Activation<'_>,
    funds_per_full_bar: u64,
) -> Result<(), ExecuteError> {
    let actor_team = cx.next.player(cx.seat).team.clone();
    let actor_funds = cx.next.player(cx.seat).funds;
    let mut target_players: Vec<_> = cx
        .next
        .players
        .off_team(&actor_team)
        .map(|(seat, candidate)| (seat, candidate.id().clone()))
        .collect();
    target_players.sort_by(|left, right| left.1.cmp(&right.1));
    for (target_seat, target_player_id) in target_players {
        for commander_slot in 0..cx.next.player(target_seat).commanders.len() {
            let target = &cx.next.player(target_seat).commanders[commander_slot];
            let from = target.power_charge;
            if from == 0 {
                continue;
            }
            let full_bar = commander::maximum_power_charge(target.id, target.power_uses)
                .map_err(|error| {
                    ExecuteError::InvalidState(
                        format!("enemy power profile cannot compute full bar: {error:?}").into(),
                    )
                })?
                .ok_or_else(|| {
                    ExecuteError::InvalidState(
                        format!(
                            "enemy commander {} has no complete power profile",
                            target.id
                        )
                        .into(),
                    )
                })?;
            let reduction = actor_funds
                .checked_mul(full_bar)
                .and_then(|value| value.checked_div(funds_per_full_bar))
                .ok_or_else(|| {
                    ExecuteError::InvalidState("power-charge reduction arithmetic overflow".into())
                })?;
            let to = from.saturating_sub(reduction);
            if to == from {
                continue;
            }
            cx.next.player_mut(target_seat).commanders[commander_slot].power_charge = to;
            cx.events.push(Event::PowerChargeChanged {
                player: target_player_id.clone(),
                commander_slot,
                from,
                to,
                reason: KnownReason::CommanderPower.into(),
            });
        }
    }
    Ok(())
}

/// Return units to `ready` so they may act again this turn.
fn refresh_unit_action(
    cx: &mut Activation<'_>,
    exclude_unit_kinds: Vec<UnitKind>,
) -> Result<(), ExecuteError> {
    let mut targets: Vec<_> = cx
        .next
        .units
        .iter()
        .filter(|unit| {
            unit.owner == cx.seat
                && matches!(unit.location, Location::Board { .. })
                && !exclude_unit_kinds.contains(&unit.kind)
        })
        .map(|unit| unit.id)
        .collect();
    targets.sort();
    for target_id in targets {
        let target = cx
            .next
            .units
            .get_mut(target_id)
            .expect("power target remains present");
        if target.action != UnitAction::Spent {
            continue;
        }
        let from = target.action;
        target.action = UnitAction::Ready;
        cx.events.push(Event::UnitActionChanged {
            unit: target_id,
            from,
            to: UnitAction::Ready,
            reason: KnownReason::CommanderPower.into(),
        });
    }
    Ok(())
}

/// Refill fuel and ammo to full.
fn resupply_units(cx: &mut Activation<'_>) -> Result<(), ExecuteError> {
    let mut targets: Vec<_> = cx
        .next
        .units
        .iter()
        .filter(|unit| unit.owner == cx.seat)
        .map(|unit| unit.id)
        .collect();
    targets.sort();
    for target_id in targets {
        let target = cx
            .next
            .units
            .get_mut(target_id)
            .expect("power target remains present");
        let fuel_before = target.fuel;
        let ammo_before = target.ammo;
        if !refill_unit(target) {
            continue;
        }
        cx.events.push(Event::UnitResourced {
            unit: target_id,
            fuel_before,
            fuel_after: target.fuel,
            ammo_before,
            ammo_after: target.ammo,
            reason: KnownReason::CommanderPower.into(),
        });
    }
    Ok(())
}

/// Place new units on the activating player's properties.
fn spawn_units_on_owned_properties(
    cx: &mut Activation<'_>,
    property_kinds: Vec<PropertyKind>,
    unit_kind: UnitKind,
    hp: u8,
) -> Result<(), ExecuteError> {
    let profile = ruleset::profile(unit_kind);
    let max_fuel = profile.max_fuel;
    let max_ammo = profile.max_ammo;
    let mut positions = Vec::new();
    let mut owned_unit_count = owned_unit_count(cx.next, cx.seat)?;
    'rows: for (position, tile) in cx.next.board.iter() {
        {
            if cx
                .next
                .settings
                .unit_limit
                .is_some_and(|limit| owned_unit_count >= limit)
            {
                break 'rows;
            }
            if !tile.owner.is_owned_by(cx.seat) {
                continue;
            }
            let property_kind = ruleset::terrain(tile.terrain).property_kind;
            if !property_kind.is_some_and(|kind| property_kinds.contains(&kind)) {
                continue;
            }
            if cx
                .next
                .units
                .iter()
                .any(|unit| board_position(unit) == Some(position))
            {
                continue;
            }
            positions.push(position);
            owned_unit_count = owned_unit_count
                .checked_add(1)
                .ok_or_else(|| ExecuteError::InvalidState("owned unit count overflow".into()))?;
        }
    }
    if positions.is_empty() {
        return Ok(());
    }
    let first_id = cx.next.next_unit_id.ok_or_else(|| {
        ExecuteError::InvalidState("unit-spawning power requires next_unit_id".into())
    })?;
    let count = u32::try_from(positions.len())
        .map_err(|_| ExecuteError::InvalidState("spawn count exceeds u32".into()))?;
    let after_id = first_id
        .checked_add(count)
        .ok_or_else(|| ExecuteError::InvalidState("next_unit_id overflow".into()))?;
    for offset in 0..count {
        let allocated_id = UnitId::new(first_id + offset);
        if cx.next.units.contains(allocated_id) {
            return Err(ExecuteError::InvalidState(
                format!("next_unit_id {} is not fresh", first_id + offset).into(),
            ));
        }
    }
    cx.next.next_unit_id = Some(after_id);
    for (offset, position) in positions.into_iter().enumerate() {
        let offset = u32::try_from(offset).expect("spawn offset fits validated count");
        let allocated_id = UnitId::new(first_id + offset);
        cx.next.units.push(Unit {
            id: allocated_id,
            kind: unit_kind,
            owner: cx.seat,
            hp,
            fuel: max_fuel,
            ammo: max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        });
        cx.events.push(Event::UnitCreated {
            unit: allocated_id,
            kind: unit_kind,
            owner: cx.player.clone(),
            position,
        });
    }
    Ok(())
}

/// Fire one strike at the centre the policy selects.
fn fire_targeted_area_strike(
    cx: &mut Activation<'_>,
    radius: usize,
    damage: u8,
    minimum_hp: u8,
) -> Result<(), ExecuteError> {
    let actor_team = cx.next.player(cx.seat).team.clone();
    let enemy_owners = enemy_seats(cx.next, &actor_team);
    let mut candidates: Vec<_> = cx
        .next
        .units
        .iter()
        .filter_map(|unit| match unit.location {
            Location::Board { position } if enemy_owners.contains(&unit.owner) => Some(position),
            _ => None,
        })
        .collect();
    candidates.sort_by_key(|position| (position.y, position.x));
    let mut best: Option<(i128, Pos)> = None;
    for center in candidates {
        let mut score = 0_i128;
        for unit in &cx.next.units {
            let Location::Board { position } = unit.location else {
                continue;
            };
            if center.distance(position) > radius as u64 {
                continue;
            }
            let cost = ruleset::profile(unit.kind).cost;
            let value = i128::from(unit.hp.div_ceil(10))
                .checked_mul(i128::from(cost))
                .ok_or_else(|| {
                    ExecuteError::InvalidState("targeted area-strike score overflow".into())
                })?;
            let friendly = cx
                .next
                .players
                .get(unit.owner.get())
                .is_some_and(|owner| owner.team == actor_team);
            score = if friendly {
                score.checked_sub(value)
            } else {
                score.checked_add(value)
            }
            .ok_or_else(|| {
                ExecuteError::InvalidState("targeted area-strike score overflow".into())
            })?;
        }
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, center));
        }
    }
    let Some((_, center)) = best else {
        return Ok(());
    };
    cx.events.push(Event::AreaStrikeResolved {
        strike: 0,
        policy: AreaStrikePolicy::UnitValue,
        center,
        radius,
        damage,
    });
    let mut targets: Vec<_> = cx
        .next
        .units
        .iter()
        .filter_map(|unit| match unit.location {
            Location::Board { position } if center.distance(position) <= radius as u64 => {
                Some(unit.id)
            }
            _ => None,
        })
        .collect();
    targets.sort();
    for target_id in targets {
        let target = cx
            .next
            .units
            .get_mut(target_id)
            .expect("targeted area-strike target remains present");
        let from_hp = target.hp;
        let to_hp = from_hp.saturating_sub(damage).max(minimum_hp);
        if to_hp == from_hp {
            continue;
        }
        target.hp = to_hp;
        cx.events.push(Event::UnitDamaged {
            unit: target_id,
            from_hp,
            to_hp,
            reason: KnownReason::CommanderPower.into(),
        });
    }
    Ok(())
}

/// Fire one strike that also immobilises what survives it.
fn fire_immobilizing_area_strike(
    cx: &mut Activation<'_>,
    radius: usize,
    damage: u8,
    minimum_hp: u8,
) -> Result<(), ExecuteError> {
    let actor_team = cx.next.player(cx.seat).team.clone();
    let enemy_owners = enemy_seats(cx.next, &actor_team);
    let mut priced_units = Vec::new();
    for unit in &cx.state.units {
        let Location::Board { position } = unit.location else {
            continue;
        };
        let cost = ruleset::profile(unit.kind).cost;
        let friendly = cx
            .state
            .players
            .get(unit.owner.get())
            .is_some_and(|owner| owner.team == actor_team);
        priced_units.push((unit, position, cost, friendly));
    }
    let mut best: Option<(i128, i128, Pos)> = None;
    for center in cx.state.board.positions() {
        {
            let mut score = 0_i128;
            let mut enemy_tiebreak = 0_i128;
            for (unit, position, cost, friendly) in &priced_units {
                if center.distance(*position) > radius as u64 {
                    continue;
                }
                let exact_hp = i128::from(unit.hp);
                let cost = i128::from(*cost);
                let value = if unit.hp < 10 {
                    2
                } else {
                    exact_hp.clamp(1, 30).checked_mul(cost).ok_or_else(|| {
                        ExecuteError::InvalidState("immobilizing area-strike score overflow".into())
                    })?
                };
                score = if *friendly {
                    score.checked_sub(value)
                } else {
                    score.checked_add(value)
                }
                .ok_or_else(|| {
                    ExecuteError::InvalidState("immobilizing area-strike score overflow".into())
                })?;
                if !friendly {
                    enemy_tiebreak = enemy_tiebreak
                        .checked_add(exact_hp.checked_mul(cost).ok_or_else(|| {
                            ExecuteError::InvalidState(
                                "immobilizing area-strike tiebreak overflow".into(),
                            )
                        })?)
                        .ok_or_else(|| {
                            ExecuteError::InvalidState(
                                "immobilizing area-strike tiebreak overflow".into(),
                            )
                        })?;
                }
            }
            if best.as_ref().is_none_or(|(best_score, best_tie, _)| {
                score > *best_score || (score == *best_score && enemy_tiebreak > *best_tie)
            }) {
                best = Some((score, enemy_tiebreak, center));
            }
        }
    }
    let center = best.map(|(_, _, center)| center).ok_or_else(|| {
        ExecuteError::InvalidState("immobilizing area-strike board is empty".into())
    })?;
    cx.events.push(Event::AreaStrikeResolved {
        strike: 0,
        policy: AreaStrikePolicy::UnitValue,
        center,
        radius,
        damage,
    });
    let mut targets: Vec<_> = cx
        .next
        .units
        .iter()
        .filter_map(|unit| match unit.location {
            Location::Board { position }
                if enemy_owners.contains(&unit.owner)
                    && center.distance(position) <= radius as u64 =>
            {
                Some(unit.id)
            }
            _ => None,
        })
        .collect();
    targets.sort();
    for target_id in targets {
        let target = cx
            .next
            .units
            .get_mut(target_id)
            .expect("immobilizing area-strike target remains present");
        let from_hp = target.hp;
        let to_hp = from_hp.saturating_sub(damage).max(minimum_hp);
        if to_hp != from_hp {
            target.hp = to_hp;
            cx.events.push(Event::UnitDamaged {
                unit: target_id,
                from_hp,
                to_hp,
                reason: KnownReason::CommanderPower.into(),
            });
        }
        if target.action != UnitAction::Immobilized {
            let from = target.action;
            target.action = UnitAction::Immobilized;
            cx.events.push(Event::UnitActionChanged {
                unit: target_id,
                from,
                to: UnitAction::Immobilized,
                reason: KnownReason::CommanderPower.into(),
            });
        }
    }
    Ok(())
}

/// Scale the target's funds.
fn multiply_funds_ratio(
    cx: &mut Activation<'_>,
    numerator: u64,
    denominator: u64,
) -> Result<(), ExecuteError> {
    let from = cx.next.player_mut(cx.seat).funds;
    let exact = u128::from(from)
        .checked_mul(u128::from(numerator))
        .and_then(|value| value.checked_div(u128::from(denominator)))
        .ok_or_else(|| ExecuteError::InvalidState("invalid funds multiplier ratio".into()))?;
    let to = u64::try_from(exact)
        .map_err(|_| ExecuteError::InvalidState("player funds overflow".into()))?;
    if to == from {
        return Ok(());
    }
    cx.next.player_mut(cx.seat).funds = to;
    cx.events.push(Event::FundsChanged {
        player: cx.player.clone(),
        from,
        to,
        reason: KnownReason::CommanderPower.into(),
    });
    Ok(())
}
