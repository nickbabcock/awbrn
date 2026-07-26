//! Combat: choosing a target, resolving the exchange, and its side effects.
//!
//! Normative source:
//! * `spec/semantics/combat.md`

use super::*;
use crate::combat::{self, Side};
use crate::commander::{self, CombatContext, Combatant, Strike};
use crate::event::{AttackTarget, Event};
use crate::random::{Luck, RandomTape, RandomToken};
use crate::ruleset::{self, FireMode, TerrainTrait};
use crate::semantic::{
    AwbwVisibility, Concealment, Location, PlayerId, Pos, PowerState, ReasonId, State, TerrainId,
    Unit, UnitAction, UnitId, UnitKindId, Visibility,
};
use crate::violation::{Action, Violation};
use std::collections::HashSet;

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_strike_funds(
    state: &State,
    next: &mut State,
    events: &mut Vec<Event>,
    striker: &str,
    target_owner: &str,
    target_kind: UnitKindId,
    from_hp: u8,
    to_hp: u8,
) -> Result<(), ExecuteError> {
    let base_value = ruleset::profile(target_kind).cost;
    let target_value = commander::effective_build_cost(state, target_owner, base_value)
        .ok_or_else(|| ExecuteError::InvalidState("strike target value overflow".into()))?;
    let gain =
        commander::strike_funds_gain(state, striker, target_owner, from_hp, to_hp, target_value)
            .ok_or_else(|| {
                ExecuteError::InvalidState("strike funds profile or arithmetic is invalid".into())
            })?;
    if gain == 0 {
        return Ok(());
    }
    let player = next
        .find_player_mut(striker)
        .ok_or_else(|| ExecuteError::InvalidState("strike owner is absent".into()))?;
    let from = player.funds;
    let to = from
        .checked_add(gain)
        .ok_or_else(|| ExecuteError::InvalidState("strike funds overflow".into()))?;
    player.funds = to;
    events.push(Event::FundsChanged {
        player: PlayerId::from(striker),
        from,
        to,
        reason: ReasonId::from("commander-power"),
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_strike_power_charge(
    state: &State,
    next: &mut State,
    events: &mut Vec<Event>,
    striker: &str,
    target_owner: &str,
    target_kind: UnitKindId,
    from_hp: u8,
    to_hp: u8,
    reason: &str,
) -> Result<(), ExecuteError> {
    let visual_damage = u64::from(from_hp.div_ceil(10).saturating_sub(to_hp.div_ceil(10)));
    if visual_damage == 0 {
        return Ok(());
    }
    let base_value = ruleset::profile(target_kind).cost;
    let target_value = commander::effective_build_cost(state, target_owner, base_value)
        .ok_or_else(|| ExecuteError::InvalidState("power charge unit value overflow".into()))?;
    let dealt_gain = target_value
        .checked_mul(visual_damage)
        .and_then(|value| value.checked_div(20))
        .ok_or_else(|| ExecuteError::InvalidState("dealt power charge overflow".into()))?;
    let received_gain = target_value
        .checked_mul(visual_damage)
        .and_then(|value| value.checked_div(10))
        .ok_or_else(|| ExecuteError::InvalidState("received power charge overflow".into()))?;
    for (player_id, gain) in [(striker, dealt_gain), (target_owner, received_gain)] {
        if gain == 0 {
            continue;
        }
        let player_index = next
            .player_index(player_id)
            .ok_or_else(|| ExecuteError::InvalidState("combat owner is absent".into()))?;
        if !matches!(next.player_mut(player_index).power_state, PowerState::None) {
            continue;
        }
        let active_slot = next
            .player_mut(player_index)
            .commanders
            .iter()
            .position(|commander| commander.active)
            .ok_or_else(|| ExecuteError::InvalidState("active commander is absent".into()))?;
        let commander_slots = if state.settings.tags {
            if next.player_mut(player_index).commanders.len() != 2 {
                return Err(ExecuteError::InvalidState(
                    "tag player does not have two commander slots".into(),
                ));
            }
            vec![active_slot, 1 - active_slot]
        } else {
            vec![active_slot]
        };
        for commander_slot in commander_slots {
            let slot_gain = if commander_slot == active_slot {
                gain
            } else {
                gain / 2
            };
            if slot_gain == 0 {
                continue;
            }
            let commander = &next.player_mut(player_index).commanders[commander_slot];
            let Some(maximum) = commander::maximum_power_charge(commander.id, commander.power_uses)
                .map_err(|_| ExecuteError::InvalidState("maximum power charge overflow".into()))?
            else {
                continue;
            };
            let from = commander.power_charge;
            if from >= maximum {
                continue;
            }
            let to = from
                .checked_add(slot_gain)
                .ok_or_else(|| ExecuteError::InvalidState("power charge overflow".into()))?
                .min(maximum);
            if to == from {
                continue;
            }
            next.player_mut(player_index).commanders[commander_slot].power_charge = to;
            events.push(Event::PowerChargeChanged {
                player: PlayerId::from(player_id),
                commander_slot,
                from,
                to,
                reason: ReasonId::from(reason),
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_tile_attack(
    state: &State,
    player: &str,
    unit_id: UnitId,
    attacker_index: usize,
    attacker: &Unit,
    origin: Pos,
    position: Pos,
) -> Result<Execution, ExecuteError> {
    let tile = state.board.get(position).ok_or_else(|| {
        violation(Violation::InvalidTarget {
            target: Some(position.into()),
        })
    })?;
    if state
        .units
        .iter()
        .any(|unit| board_position(unit) == Some(position))
    {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }

    let Some(destructible) = ruleset::terrain(tile.terrain).destructible else {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    };
    let from_hp = tile
        .destructible_hp
        .ok_or_else(|| ExecuteError::InvalidState("destructible tile has no HP".into()))?;
    if from_hp > destructible.maximum_hp {
        return Err(ExecuteError::InvalidState(
            "destructible tile HP exceeds its maximum".into(),
        ));
    }
    let from_hp = u8::try_from(from_hp)
        .map_err(|_| ExecuteError::InvalidState("destructible tile HP overflow".into()))?;
    let target_kind = destructible.target_kind;
    let destruction_replacement = destructible.destruction_replacement;

    let actor_team = state
        .find_player(player)
        .map(|candidate| candidate.team.as_str())
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    if state.settings.fog && !AwbwVisibility.visible_position(state, actor_team, position) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }

    let profile = ruleset::profile(attacker.kind);
    let fire_mode = profile.fire_mode;
    if fire_mode == FireMode::None {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::Attack,
        }));
    }
    let distance = origin.distance(position);
    if let Some(range) = profile.indirect_range {
        let minimum = range.minimum;
        let maximum = commander::effective_attack_range(
            state,
            attacker,
            range.maximum,
            profile.domain.as_str(),
            "indirect",
        );
        if distance < minimum || distance > maximum {
            return Err(violation(Violation::TargetOutOfRange {
                target: Some(position.into()),
            }));
        }
    } else if distance != 1 {
        return Err(violation(Violation::TargetOutOfRange {
            target: Some(position.into()),
        }));
    }
    if combat::select_weapon(attacker.kind, target_kind, attacker.ammo).is_none() {
        return Err(violation(Violation::InvalidTarget {
            target: Some(position.into()),
        }));
    }

    let unit_domain = combat_domain(profile);
    let tower_count = state
        .board
        .tiles()
        .filter(|candidate| {
            candidate
                .owner
                .player()
                .is_some_and(|owner| owner == player)
                && ruleset::terrain_has(candidate.terrain, TerrainTrait::CommunicationBonus)
        })
        .count() as i64;
    let owned_properties = state
        .board
        .tiles()
        .filter(|candidate| {
            candidate
                .owner
                .player()
                .is_some_and(|owner| owner == player)
                && ruleset::terrain_has(candidate.terrain, TerrainTrait::Capturable)
        })
        .count() as u64;
    let attacker_terrain = state.board.tile(origin).terrain;
    let attacker_stars = ruleset::defense_stars(attacker_terrain);
    let combat_weather = state.weather.kind;
    let no_capabilities = HashSet::new();
    let context = Combatant {
        kind: attacker.kind,
        domain: unit_domain,
        fire_mode: fire_mode.as_str(),
        terrain: attacker_terrain,
        weather: combat_weather,
        property: ruleset::terrain_has(attacker_terrain, TerrainTrait::Capturable),
        capabilities: &no_capabilities,
    };
    let (attack, _, _, _, _) = commander::effective_combat(
        state,
        player,
        context,
        Strike::Initial,
        CombatContext {
            tower_count,
            funds: state.players[attacker_index].funds,
            owned_properties,
            base_terrain_stars: i64::from(attacker_stars),
        },
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let hit = combat::damage(
        Side {
            kind: attacker.kind,
            hp: attacker.hp,
            ammo: attacker.ammo,
            attack,
            defense: 100,
            terrain_stars: attacker_stars,
        },
        Side {
            kind: target_kind,
            hp: from_hp,
            ammo: 0,
            attack: 100,
            defense: 100,
            terrain_stars: 0,
        },
        0,
    )
    .expect("tile weapon was validated");
    let to_hp = from_hp.saturating_sub(hit.damage);
    let mut next = state.clone();
    let mut events = Vec::new();
    if hit.weapon.ammo_cost > 0 {
        let before = next.units[attacker_index].ammo;
        next.units[attacker_index].ammo -= hit.weapon.ammo_cost;
        events.push(Event::UnitResourced {
            unit: unit_id,
            fuel_before: attacker.fuel,
            fuel_after: attacker.fuel,
            ammo_before: before,
            ammo_after: next.units[attacker_index].ammo,
            reason: ReasonId::from("combat"),
        });
    }
    events.push(Event::AttackResolved {
        attacker: unit_id,
        weapon: hit.weapon.weapon,
        target: AttackTarget::Tile { position },
    });
    events.push(Event::DestructibleDamaged {
        position,
        from_hp,
        to_hp,
    });
    if to_hp == 0 {
        next.board.tile_mut(position).terrain = destruction_replacement;
        next.board.tile_mut(position).destructible_hp = None;
        events.push(Event::TileTerrainChanged {
            position,
            from: tile.terrain,
            to: destruction_replacement,
            reason: ReasonId::from("combat"),
        });
    } else {
        next.board.tile_mut(position).destructible_hp = Some(u64::from(to_hp));
    }
    next.units[attacker_index].action = UnitAction::Spent;
    events.push(Event::UnitActionChanged {
        unit: unit_id,
        from: UnitAction::Ready,
        to: UnitAction::Spent,
        reason: ReasonId::from("attack"),
    });
    Ok(Execution {
        state: next,
        events,
        random_consumed: 0,
    })
}

pub(crate) fn execute_move_attack(
    state: &State,
    player: &str,
    unit_id: UnitId,
    path: Vec<Pos>,
    target: AttackTarget,
    random: &[RandomToken],
) -> Result<Execution, ExecuteError> {
    let turn = ActiveTurn::open(state, player)?;
    let plan = turn.plan_move(unit_id, path)?;
    let ai = plan.unit_index();
    let attacker = &state.units[ai];
    let origin = plan.origin();

    if plan.path().len() > 1 {
        match ruleset::profile(attacker.kind).fire_mode {
            FireMode::Indirect => {
                return Err(violation(Violation::ActionNotSupported {
                    action: Action::MoveAndFire,
                }));
            }
            FireMode::None => {
                return Err(violation(Violation::ActionNotSupported {
                    action: Action::Attack,
                }));
            }
            FireMode::Direct => {}
        }

        let destination = plan.destination();
        let visibility = AwbwVisibility;
        if state.units.iter().any(|other| {
            other.id != unit_id
                && board_position(other) == Some(destination)
                && occupancy_is_disclosed(&visibility, state, plan.actor_team(), other)
        }) {
            return Err(violation(Violation::DestinationOccupied {
                position: destination,
            }));
        }

        let mut movement = execute_planned_movement(state, unit_id, &plan);
        if movement.trapped {
            return Ok(Execution {
                state: movement.state,
                events: movement.events,
                random_consumed: 0,
            });
        }

        // Movement spends the unit for movement-only actions. Restore readiness
        // internally so the atomic follow-up can resolve and emit the single
        // attack action transition.
        movement.state.units[plan.unit_index()].action = UnitAction::Ready;
        let mut combat = execute_move_attack(
            &movement.state,
            player,
            unit_id,
            vec![destination],
            target,
            random,
        )?;
        movement.events.append(&mut combat.events);
        combat.events = movement.events;
        return Ok(combat);
    }
    let target_id = match target {
        AttackTarget::Unit { unit } => unit,
        AttackTarget::Tile { position } => {
            return execute_tile_attack(state, player, unit_id, ai, attacker, origin, position);
        }
    };
    let di = state.units.index_of(target_id).ok_or_else(|| {
        violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        })
    })?;
    let defender = &state.units[di];
    let defender_owner = defender.owner.clone();
    let Location::Board { position: dp } = defender.location else {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    };
    if defender.owner == player {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let actor_team = state
        .find_player(player)
        .map(|candidate| candidate.team.as_str())
        .ok_or_else(|| ExecuteError::InvalidState("active player is absent".into()))?;
    if !AwbwVisibility.visible_unit(state, actor_team, defender) {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let concealed_target_compatible = match (
        defender.concealment,
        defender.kind.as_str(),
        attacker.kind.as_str(),
    ) {
        (Concealment::Hidden, "sub", "sub" | "cruiser")
        | (Concealment::Hidden, "stealth", "fighter" | "stealth") => true,
        (Concealment::Hidden, "sub" | "stealth", _) => false,
        _ => true,
    };
    if !concealed_target_compatible {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let profile = ruleset::profile(attacker.kind);
    if profile.fire_mode == FireMode::None {
        return Err(violation(Violation::ActionNotSupported {
            action: Action::Attack,
        }));
    }
    let distance = origin.distance(dp);
    if let Some(range) = profile.indirect_range {
        let min = range.minimum;
        let max = commander::effective_attack_range(
            state,
            attacker,
            range.maximum,
            profile.domain.as_str(),
            "indirect",
        );
        if distance < min || distance > max {
            return Err(violation(Violation::TargetOutOfRange {
                target: Some(target_id.into()),
            }));
        }
    } else if distance != 1 {
        return Err(violation(Violation::TargetOutOfRange {
            target: Some(target_id.into()),
        }));
    }
    if combat::select_weapon(attacker.kind, defender.kind, attacker.ammo).is_none() {
        return Err(violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        }));
    }
    let mut tape = RandomTape::new(random);
    let stars = |p: Pos| ruleset::defense_stars(state.board.tile(p).terrain);
    let unit_domain = |kind: UnitKindId| combat_domain(ruleset::profile(kind));
    let fire_mode = |kind: UnitKindId| ruleset::profile(kind).fire_mode.as_str();
    let tower_count = |owner: &str| {
        state
            .board
            .tiles()
            .filter(|tile| {
                tile.owner.player().is_some_and(|value| value == owner)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::CommunicationBonus)
            })
            .count() as i64
    };
    let is_property = |terrain: TerrainId| ruleset::terrain_has(terrain, TerrainTrait::Capturable);
    let combat_context = |owner: &str, position: Pos| CombatContext {
        tower_count: tower_count(owner),
        funds: state.find_player(owner).map_or(0, |player| player.funds),
        owned_properties: state
            .board
            .tiles()
            .filter(|tile| {
                tile.owner.player().is_some_and(|value| value == owner) && is_property(tile.terrain)
            })
            .count() as u64,
        base_terrain_stars: i64::from(stars(position)),
    };
    let no_capabilities = HashSet::new();
    let combat_weather = state.weather.kind;
    let attacker_context = Combatant {
        kind: attacker.kind,
        domain: unit_domain(attacker.kind),
        fire_mode: fire_mode(attacker.kind),
        terrain: state.board.tile(origin).terrain,
        weather: combat_weather,
        property: is_property(state.board.tile(origin).terrain),
        capabilities: &no_capabilities,
    };
    let defender_context = Combatant {
        kind: defender.kind,
        domain: unit_domain(defender.kind),
        fire_mode: fire_mode(attacker.kind),
        terrain: state.board.tile(dp).terrain,
        weather: combat_weather,
        property: is_property(state.board.tile(dp).terrain),
        capabilities: &no_capabilities,
    };
    let (attacker_attack, _, _, attacker_good, attacker_bad) = commander::effective_combat(
        state,
        &attacker.owner,
        attacker_context,
        Strike::Initial,
        combat_context(&attacker.owner, origin),
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let (_, defender_defense, defender_stars, _, _) = commander::effective_combat(
        state,
        &defender.owner,
        defender_context,
        Strike::Initial,
        combat_context(&defender.owner, dp),
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let defender_stars = commander::effective_enemy_terrain_stars(
        state,
        &attacker.owner,
        attacker_context,
        Strike::Initial,
        defender_stars,
    )
    .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
    let defender_stars = u8::try_from(defender_stars)
        .map_err(|_| ExecuteError::InvalidState("terrain stars overflow".into()))?;
    let attacker_side = Side {
        kind: attacker.kind,
        hp: attacker.hp,
        ammo: attacker.ammo,
        attack: attacker_attack,
        defense: 100,
        terrain_stars: stars(origin),
    };
    let defender_side = Side {
        kind: defender.kind,
        hp: defender.hp,
        ammo: defender.ammo,
        attack: 100,
        defense: defender_defense,
        terrain_stars: defender_stars,
    };
    let defender_direct = ruleset::profile(defender.kind).fire_mode == FireMode::Direct;
    let counter_first_context = Combatant {
        kind: defender.kind,
        domain: unit_domain(defender.kind),
        fire_mode: fire_mode(defender.kind),
        terrain: state.board.tile(dp).terrain,
        weather: combat_weather,
        property: is_property(state.board.tile(dp).terrain),
        capabilities: &no_capabilities,
    };
    let counter_first = distance == 1
        && defender_direct
        && combat::select_weapon(defender.kind, attacker.kind, defender.ammo).is_some()
        && commander::counter_first(
            state,
            &defender.owner,
            counter_first_context,
            Strike::Counter,
        );
    if counter_first {
        let countered_context = Combatant {
            kind: attacker.kind,
            domain: unit_domain(attacker.kind),
            fire_mode: fire_mode(defender.kind),
            terrain: state.board.tile(origin).terrain,
            weather: combat_weather,
            property: is_property(state.board.tile(origin).terrain),
            capabilities: &no_capabilities,
        };
        let (counter_attack, _, _, counter_good, counter_bad) = commander::effective_combat(
            state,
            &defender.owner,
            counter_first_context,
            Strike::Counter,
            combat_context(&defender.owner, dp),
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let (_, countered_defense, countered_stars, _, _) = commander::effective_combat(
            state,
            &attacker.owner,
            countered_context,
            Strike::Counter,
            combat_context(&attacker.owner, origin),
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let countered_stars = commander::effective_enemy_terrain_stars(
            state,
            &defender.owner,
            counter_first_context,
            Strike::Counter,
            countered_stars,
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let countered_stars = u8::try_from(countered_stars)
            .map_err(|_| ExecuteError::InvalidState("terrain stars overflow".into()))?;
        let counter_luck =
            draw(&mut tape, Luck::Good, counter_good)? - draw(&mut tape, Luck::Bad, counter_bad)?;
        let preemptive = combat::damage(
            Side {
                attack: counter_attack,
                ..defender_side
            },
            Side {
                defense: countered_defense,
                terrain_stars: countered_stars,
                ..attacker_side
            },
            counter_luck,
        )
        .expect("counter-first eligibility selected a weapon");
        let attacker_remaining = attacker.hp.saturating_sub(preemptive.damage);
        let initiating = if attacker_remaining > 0 {
            let attack_luck = draw(&mut tape, Luck::Good, attacker_good)?
                - draw(&mut tape, Luck::Bad, attacker_bad)?;
            Some(
                combat::damage(
                    Side {
                        hp: attacker_remaining,
                        ..attacker_side
                    },
                    defender_side,
                    attack_luck,
                )
                .expect("initiating weapon was validated"),
            )
        } else {
            None
        };

        let mut next = state.clone();
        let mut events = Vec::new();
        if preemptive.weapon.ammo_cost > 0 {
            let index = next
                .units
                .index_of(target_id)
                .expect("counter-first defender remains present");
            let before = next.units[index].ammo;
            next.units[index].ammo -= preemptive.weapon.ammo_cost;
            events.push(Event::UnitResourced {
                unit: target_id,
                fuel_before: defender.fuel,
                fuel_after: defender.fuel,
                ammo_before: before,
                ammo_after: next.units[index].ammo,
                reason: ReasonId::from("combat-counter"),
            });
        }
        events.push(Event::AttackResolved {
            attacker: target_id,
            weapon: preemptive.weapon.weapon,
            target: AttackTarget::Unit { unit: unit_id },
        });
        events.push(Event::UnitDamaged {
            unit: unit_id,
            from_hp: attacker.hp,
            to_hp: attacker_remaining,
            reason: ReasonId::from("combat-counter"),
        });
        apply_strike_funds(
            state,
            &mut next,
            &mut events,
            &defender.owner,
            &attacker.owner,
            attacker.kind,
            attacker.hp,
            attacker_remaining,
        )?;
        apply_strike_power_charge(
            state,
            &mut next,
            &mut events,
            &defender.owner,
            &attacker.owner,
            attacker.kind,
            attacker.hp,
            attacker_remaining,
            "combat-counter",
        )?;
        if attacker_remaining == 0 {
            events.push(Event::UnitRemoved {
                unit: unit_id,
                reason: ReasonId::from("combat-counter"),
            });
            next.units.remove(ai);
            if !next.units.iter().any(|unit| unit.owner == attacker.owner) {
                eliminate_player(&mut next, &attacker.owner, "rout", None, None, &mut events)?;
            }
            return Ok(Execution {
                state: next,
                events,
                random_consumed: tape.consumed(),
            });
        }
        next.units[ai].hp = attacker_remaining;

        let hit = initiating.expect("surviving attacker performs initiating strike");
        let next_ai = next
            .units
            .index_of(unit_id)
            .expect("surviving attacker remains present");
        if hit.weapon.ammo_cost > 0 {
            let before = next.units[next_ai].ammo;
            next.units[next_ai].ammo -= hit.weapon.ammo_cost;
            events.push(Event::UnitResourced {
                unit: unit_id,
                fuel_before: attacker.fuel,
                fuel_after: attacker.fuel,
                ammo_before: before,
                ammo_after: next.units[next_ai].ammo,
                reason: ReasonId::from("combat"),
            });
        }
        let defender_remaining = defender.hp.saturating_sub(hit.damage);
        events.push(Event::AttackResolved {
            attacker: unit_id,
            weapon: hit.weapon.weapon,
            target: AttackTarget::Unit { unit: target_id },
        });
        events.push(Event::UnitDamaged {
            unit: target_id,
            from_hp: defender.hp,
            to_hp: defender_remaining,
            reason: ReasonId::from("combat"),
        });
        apply_strike_funds(
            state,
            &mut next,
            &mut events,
            &attacker.owner,
            &defender.owner,
            defender.kind,
            defender.hp,
            defender_remaining,
        )?;
        apply_strike_power_charge(
            state,
            &mut next,
            &mut events,
            &attacker.owner,
            &defender.owner,
            defender.kind,
            defender.hp,
            defender_remaining,
            "combat",
        )?;
        if defender_remaining == 0 {
            events.push(Event::UnitRemoved {
                unit: target_id,
                reason: ReasonId::from("combat"),
            });
            let next_di = next
                .units
                .index_of(target_id)
                .expect("lethal target remains until removal");
            next.units.remove(next_di);
        } else {
            let next_di = next
                .units
                .index_of(target_id)
                .expect("surviving target remains present");
            next.units[next_di].hp = defender_remaining;
        }
        let next_ai = next
            .units
            .index_of(unit_id)
            .expect("acting attacker survives counter-first engagement");
        next.units[next_ai].action = UnitAction::Spent;
        events.push(Event::UnitActionChanged {
            unit: unit_id,
            from: UnitAction::Ready,
            to: UnitAction::Spent,
            reason: ReasonId::from("attack"),
        });
        if defender_remaining == 0 && !next.units.iter().any(|unit| unit.owner == defender_owner) {
            eliminate_player(&mut next, &defender_owner, "rout", None, None, &mut events)?;
        }
        return Ok(Execution {
            state: next,
            events,
            random_consumed: tape.consumed(),
        });
    }
    let attack_luck =
        draw(&mut tape, Luck::Good, attacker_good)? - draw(&mut tape, Luck::Bad, attacker_bad)?;
    let first = combat::damage(attacker_side, defender_side, attack_luck).ok_or_else(|| {
        violation(Violation::InvalidTarget {
            target: Some(target_id.into()),
        })
    })?;
    let remaining = defender.hp.saturating_sub(first.damage);
    let counter = if remaining > 0
        && distance == 1
        && defender_direct
        && combat::select_weapon(defender.kind, attacker.kind, defender.ammo).is_some()
    {
        let counter_context = Combatant {
            kind: defender.kind,
            domain: unit_domain(defender.kind),
            fire_mode: fire_mode(defender.kind),
            terrain: state.board.tile(dp).terrain,
            weather: combat_weather,
            property: is_property(state.board.tile(dp).terrain),
            capabilities: &no_capabilities,
        };
        let countered_context = Combatant {
            kind: attacker.kind,
            domain: unit_domain(attacker.kind),
            fire_mode: fire_mode(defender.kind),
            terrain: state.board.tile(origin).terrain,
            weather: combat_weather,
            property: is_property(state.board.tile(origin).terrain),
            capabilities: &no_capabilities,
        };
        let (counter_attack, _, _, counter_good, counter_bad) = commander::effective_combat(
            state,
            &defender.owner,
            counter_context,
            Strike::Counter,
            combat_context(&defender.owner, dp),
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let (_, countered_defense, countered_stars, _, _) = commander::effective_combat(
            state,
            &attacker.owner,
            countered_context,
            Strike::Counter,
            combat_context(&attacker.owner, origin),
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let countered_stars = commander::effective_enemy_terrain_stars(
            state,
            &defender.owner,
            counter_context,
            Strike::Counter,
            countered_stars,
        )
        .ok_or_else(|| ExecuteError::InvalidState("commander combat overflow".into()))?;
        let countered_stars = u8::try_from(countered_stars)
            .map_err(|_| ExecuteError::InvalidState("terrain stars overflow".into()))?;
        let luck =
            draw(&mut tape, Luck::Good, counter_good)? - draw(&mut tape, Luck::Bad, counter_bad)?;
        combat::damage(
            Side {
                hp: remaining,
                attack: counter_attack,
                ..defender_side
            },
            Side {
                defense: countered_defense,
                terrain_stars: countered_stars,
                ..attacker_side
            },
            luck,
        )
    } else {
        None
    };
    let mut next = state.clone();
    let mut events = Vec::new();
    if first.weapon.ammo_cost > 0 {
        let before = next.units[ai].ammo;
        next.units[ai].ammo -= first.weapon.ammo_cost;
        events.push(Event::UnitResourced {
            unit: unit_id,
            fuel_before: attacker.fuel,
            fuel_after: attacker.fuel,
            ammo_before: before,
            ammo_after: next.units[ai].ammo,
            reason: ReasonId::from("combat"),
        });
    }
    events.push(Event::AttackResolved {
        attacker: unit_id,
        weapon: first.weapon.weapon,
        target: AttackTarget::Unit { unit: target_id },
    });
    events.push(Event::UnitDamaged {
        unit: target_id,
        from_hp: defender.hp,
        to_hp: remaining,
        reason: ReasonId::from("combat"),
    });
    apply_strike_funds(
        state,
        &mut next,
        &mut events,
        &attacker.owner,
        &defender.owner,
        defender.kind,
        defender.hp,
        remaining,
    )?;
    apply_strike_power_charge(
        state,
        &mut next,
        &mut events,
        &attacker.owner,
        &defender.owner,
        defender.kind,
        defender.hp,
        remaining,
        "combat",
    )?;
    if remaining > 0 {
        next.units[di].hp = remaining;
    }
    if let Some(hit) = counter {
        let before = next.units[di].ammo;
        if hit.weapon.ammo_cost > 0 {
            next.units[di].ammo -= hit.weapon.ammo_cost;
            events.push(Event::UnitResourced {
                unit: target_id,
                fuel_before: defender.fuel,
                fuel_after: defender.fuel,
                ammo_before: before,
                ammo_after: next.units[di].ammo,
                reason: ReasonId::from("combat-counter"),
            });
        }
        let ahp = attacker.hp.saturating_sub(hit.damage);
        events.push(Event::AttackResolved {
            attacker: target_id,
            weapon: hit.weapon.weapon,
            target: AttackTarget::Unit { unit: unit_id },
        });
        events.push(Event::UnitDamaged {
            unit: unit_id,
            from_hp: attacker.hp,
            to_hp: ahp,
            reason: ReasonId::from("combat-counter"),
        });
        apply_strike_funds(
            state,
            &mut next,
            &mut events,
            &defender.owner,
            &attacker.owner,
            attacker.kind,
            attacker.hp,
            ahp,
        )?;
        apply_strike_power_charge(
            state,
            &mut next,
            &mut events,
            &defender.owner,
            &attacker.owner,
            attacker.kind,
            attacker.hp,
            ahp,
            "combat-counter",
        )?;
        next.units[ai].hp = ahp;
    }
    if remaining == 0 {
        events.push(Event::UnitRemoved {
            unit: target_id,
            reason: ReasonId::from("combat"),
        });
        next.units.remove(di);
    }
    let next_ai = next
        .units
        .index_of(unit_id)
        .expect("attacker survives this slice");
    next.units[next_ai].action = UnitAction::Spent;
    events.push(Event::UnitActionChanged {
        unit: unit_id,
        from: UnitAction::Ready,
        to: UnitAction::Spent,
        reason: ReasonId::from("attack"),
    });
    if remaining == 0 && !next.units.iter().any(|unit| unit.owner == defender_owner) {
        eliminate_player(&mut next, &defender_owner, "rout", None, None, &mut events)?;
    }
    Ok(Execution {
        state: next,
        events,
        random_consumed: tape.consumed(),
    })
}
