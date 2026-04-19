use bevy::prelude::*;

use crate::adjacency::adjacent_positions;
use crate::command::{GameCommand, PostMoveAction};
use crate::damage::{CombatInput, CombatOutcome, CombatSide, LuckCap, PercentMod, TerrainStars};
use crate::player::PlayerRegistry;
use crate::replay::ReplayEventError;
use crate::server::spawn_unit_entity;
use crate::setup::GameRng;
use crate::state::{ServerGameState, TurnPhase};
use crate::unit_id::ServerUnitId;
use awbrn_game::MapPosition;
use awbrn_game::world::{
    Ammo, BoardIndex, CaptureAction, CaptureActionOutcome, CaptureProgress, CaptureProgressInput,
    CarriedBy, Fuel, GameMap, GraphicalHp, Hiding, StrongIdMap, UnitActive, UnitHp,
};
use awbrn_game::world::{Faction, Unit};
use awbrn_types::{DamagePts, ExactHp, PlayerFaction};

/// The set of world mutations that occurred from applying a command.
/// Used by the view layer to build per-player updates.
pub(crate) enum ApplyOutcome {
    UnitMoved {
        unit_id: ServerUnitId,
        entity: Entity,
        from: awbrn_map::Position,
        to: awbrn_map::Position,
        path: Vec<awbrn_map::Position>,
        faction: PlayerFaction,
    },
    TurnEnded {
        new_active_player: crate::player::PlayerId,
        new_day: Option<u32>,
    },
    UnitAttacked {
        // Movement data (same fields as UnitMoved so fog diff can reuse the same logic)
        attacker_id: ServerUnitId,
        attacker_entity: Entity,
        from: awbrn_map::Position,
        to: awbrn_map::Position,
        path: Vec<awbrn_map::Position>,
        attacker_faction: PlayerFaction,
        // Attack outcome data
        defender_id: ServerUnitId,
        defender_entity: Entity,
        defender_position: awbrn_map::Position,
        defender_faction: PlayerFaction,
        /// Post-combat visual HP. `GraphicalHp(0)` means the unit was destroyed.
        /// Captured before any entity despawn so the values remain accessible.
        attacker_hp_after: GraphicalHp,
        defender_hp_after: GraphicalHp,
        combat_outcome: CombatOutcome,
    },
    PropertyCaptured {
        unit_id: ServerUnitId,
        entity: Entity,
        from: awbrn_map::Position,
        to: awbrn_map::Position,
        path: Vec<awbrn_map::Position>,
        faction: PlayerFaction,
        tile: awbrn_map::Position,
        new_faction: PlayerFaction,
    },
    CaptureContinued {
        unit_id: ServerUnitId,
        entity: Entity,
        from: awbrn_map::Position,
        to: awbrn_map::Position,
        path: Vec<awbrn_map::Position>,
        faction: PlayerFaction,
        tile: awbrn_map::Position,
        progress: u8,
    },
    UnitBuilt {
        tile: awbrn_map::Position,
        unit_type: awbrn_types::Unit,
        unit_id: ServerUnitId,
    },
    UnitSupplied {
        supplier_id: ServerUnitId,
        resupplied_unit_ids: Vec<ServerUnitId>,
    },
    UnitLoaded {
        cargo_id: ServerUnitId,
        cargo_entity: Entity,
        transport_id: ServerUnitId,
        transport_entity: Entity,
        from: awbrn_map::Position,
        to: awbrn_map::Position,
        path: Vec<awbrn_map::Position>,
        cargo_faction: PlayerFaction,
    },
    UnitUnloaded {
        cargo_id: ServerUnitId,
        cargo_entity: Entity,
        transport_id: ServerUnitId,
        transport_entity: Entity,
        destination_tile: awbrn_map::Position,
    },
    UnitHidden {
        unit_id: ServerUnitId,
        entity: Entity,
        position: awbrn_map::Position,
        faction: PlayerFaction,
    },
    UnitUnhidden {
        unit_id: ServerUnitId,
        entity: Entity,
        position: awbrn_map::Position,
        faction: PlayerFaction,
    },
    UnitJoined {
        source_id: ServerUnitId,
        source_entity: Entity,
        source_unit_type: awbrn_types::Unit,
        target_id: ServerUnitId,
        target_entity: Entity,
        from: awbrn_map::Position,
        to: awbrn_map::Position,
        path: Vec<awbrn_map::Position>,
        source_faction: PlayerFaction,
        funds_refund: u32,
    },
}

/// Apply a validated command to the world. Returns the outcome for view generation.
pub(crate) fn apply_command(world: &mut World, command: &GameCommand) -> ApplyOutcome {
    apply_command_with_combat(world, command, CombatResolution::Roll)
        .expect("validated command must apply")
}

/// Replay contract: attack commands require a stored outcome; non-attacks must not provide one.
pub(crate) fn apply_command_with_stored_combat(
    world: &mut World,
    command: &GameCommand,
    combat_outcome: Option<&CombatOutcome>,
) -> Result<ApplyOutcome, ReplayEventError> {
    if crate::replay::command_is_attack(command) {
        let combat_outcome = combat_outcome.ok_or(ReplayEventError::MissingCombatOutcome)?;
        apply_command_with_combat(world, command, CombatResolution::Stored(combat_outcome))
    } else if combat_outcome.is_some() {
        Err(ReplayEventError::UnexpectedCombatOutcome)
    } else {
        apply_command_with_combat(world, command, CombatResolution::Roll)
    }
}

#[derive(Clone, Copy)]
enum CombatResolution<'a> {
    Roll,
    Stored(&'a CombatOutcome),
}

fn apply_command_with_combat(
    world: &mut World,
    command: &GameCommand,
    combat_resolution: CombatResolution<'_>,
) -> Result<ApplyOutcome, ReplayEventError> {
    match command {
        GameCommand::MoveUnit {
            unit_id,
            path,
            action,
        } => apply_move_unit(world, *unit_id, path, action.as_ref(), combat_resolution),
        GameCommand::Build {
            position,
            unit_type,
        } => Ok(apply_build(world, *position, *unit_type)),
        GameCommand::EndTurn => Ok(apply_end_turn(world)),
    }
}

fn apply_build(
    world: &mut World,
    tile: awbrn_map::Position,
    unit_type: awbrn_types::Unit,
) -> ApplyOutcome {
    let player = world.resource::<ServerGameState>().active_player;
    let faction = world
        .resource::<PlayerRegistry>()
        .faction_for_player(player)
        .expect("validated build player must have a faction");
    let cost = unit_type.base_cost();

    world
        .resource_mut::<PlayerRegistry>()
        .get_mut(player)
        .expect("validated build player must exist")
        .funds -= cost;

    let unit_id = spawn_unit_entity(world, tile, unit_type, faction, false);

    ApplyOutcome::UnitBuilt {
        tile,
        unit_type,
        unit_id,
    }
}

fn apply_move_unit(
    world: &mut World,
    unit_id: ServerUnitId,
    path: &[awbrn_map::Position],
    action: Option<&PostMoveAction>,
    combat_resolution: CombatResolution<'_>,
) -> Result<ApplyOutcome, ReplayEventError> {
    let entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&unit_id)
        .expect("validated unit must exist");

    let from = world
        .entity(entity)
        .get::<MapPosition>()
        .expect("unit must have position")
        .position();

    let faction = world
        .entity(entity)
        .get::<Faction>()
        .expect("unit must have faction")
        .0;

    let to = *path.last().expect("validated path is non-empty");

    // Consume fuel: one per tile moved (path includes start, so subtract 1).
    let tiles_moved = path.len().saturating_sub(1) as u32;
    if tiles_moved > 0 {
        let current_fuel = world.entity(entity).get::<Fuel>().map(|f| f.0).unwrap_or(0);
        let new_fuel = current_fuel.saturating_sub(tiles_moved);
        world.entity_mut(entity).insert(Fuel(new_fuel));
    }

    let destination_is_occupied_action = matches!(
        action,
        Some(PostMoveAction::Load { .. } | PostMoveAction::Join { .. })
    );

    // Update position if it changed. Load and Join intentionally end on an
    // occupied tile, so the acting unit must not be inserted into BoardIndex
    // at the destination before being loaded/despawned.
    if from != to {
        // Capture progress is tied to the unit staying on the same property.
        world.entity_mut(entity).remove::<CaptureProgress>();
        if !destination_is_occupied_action {
            world.entity_mut(entity).insert(MapPosition::from(to));
        }
    }

    // Deactivate the unit (it has acted this turn).
    world.entity_mut(entity).remove::<UnitActive>();

    match action {
        Some(PostMoveAction::Attack { target }) => apply_attack(
            world,
            unit_id,
            entity,
            from,
            to,
            path,
            faction,
            *target,
            combat_resolution,
        ),
        Some(PostMoveAction::Capture) => Ok(apply_capture(
            world, unit_id, entity, from, to, path, faction,
        )),
        Some(PostMoveAction::Load { transport_id }) => Ok(apply_load(
            world,
            unit_id,
            entity,
            *transport_id,
            from,
            to,
            path,
            faction,
        )),
        Some(PostMoveAction::Unload { cargo_id, position }) => {
            Ok(apply_unload(world, unit_id, entity, *cargo_id, *position))
        }
        Some(PostMoveAction::Supply) => Ok(apply_supply(world, unit_id, to, faction)),
        Some(PostMoveAction::Hide) => Ok(apply_hide(world, unit_id, entity, to, faction)),
        Some(PostMoveAction::Unhide) => Ok(apply_unhide(world, unit_id, entity, to, faction)),
        Some(PostMoveAction::Join { target_id }) => Ok(apply_join(
            world, unit_id, entity, *target_id, from, to, path, faction,
        )),
        _ => Ok(ApplyOutcome::UnitMoved {
            unit_id,
            entity,
            from,
            to,
            path: path.to_vec(),
            faction,
        }),
    }
}

fn apply_supply(
    world: &mut World,
    supplier_id: ServerUnitId,
    position: awbrn_map::Position,
    faction: PlayerFaction,
) -> ApplyOutcome {
    let mut resupplied_unit_ids = Vec::with_capacity(4);

    for position in adjacent_positions(position).into_iter().flatten() {
        let Some(entity) = world
            .resource::<BoardIndex>()
            .unit_entity(position)
            .ok()
            .flatten()
        else {
            continue;
        };

        let Some((unit_type, unit_id, current_fuel, current_ammo)) = ({
            let entity_ref = world.entity(entity);
            let is_self_owned = entity_ref
                .get::<Faction>()
                .is_some_and(|unit_faction| unit_faction.0 == faction);
            if !is_self_owned {
                None
            } else {
                let unit_type = entity_ref
                    .get::<Unit>()
                    .copied()
                    .expect("resupplied unit must have Unit component")
                    .0;
                let unit_id = entity_ref
                    .get::<ServerUnitId>()
                    .copied()
                    .expect("resupplied unit must have ServerUnitId");
                let current_fuel = entity_ref.get::<Fuel>().map(|fuel| fuel.0);
                let current_ammo = entity_ref.get::<Ammo>().map(|ammo| ammo.0);
                Some((unit_type, unit_id, current_fuel, current_ammo))
            }
        }) else {
            continue;
        };

        let max_fuel = unit_type.max_fuel();
        let max_ammo = unit_type.max_ammo();
        let needs_fuel = current_fuel.is_some_and(|fuel| fuel < max_fuel);
        let needs_ammo = current_ammo.is_some_and(|ammo| ammo < max_ammo);

        if needs_fuel || needs_ammo {
            world
                .entity_mut(entity)
                .insert((Fuel(max_fuel), Ammo(max_ammo)));
            resupplied_unit_ids.push(unit_id);
        }
    }

    ApplyOutcome::UnitSupplied {
        supplier_id,
        resupplied_unit_ids,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_load(
    world: &mut World,
    cargo_id: ServerUnitId,
    cargo_entity: Entity,
    transport_id: ServerUnitId,
    from: awbrn_map::Position,
    to: awbrn_map::Position,
    path: &[awbrn_map::Position],
    cargo_faction: PlayerFaction,
) -> ApplyOutcome {
    let transport_entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&transport_id)
        .expect("validated transport must exist");

    world
        .entity_mut(cargo_entity)
        .insert(CarriedBy(transport_entity))
        .remove::<MapPosition>();

    ApplyOutcome::UnitLoaded {
        cargo_id,
        cargo_entity,
        transport_id,
        transport_entity,
        from,
        to,
        path: path.to_vec(),
        cargo_faction,
    }
}

fn apply_unload(
    world: &mut World,
    transport_id: ServerUnitId,
    transport_entity: Entity,
    cargo_id: ServerUnitId,
    destination_tile: awbrn_map::Position,
) -> ApplyOutcome {
    let cargo_entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&cargo_id)
        .expect("validated cargo must exist");

    world
        .entity_mut(cargo_entity)
        .insert(MapPosition::from(destination_tile))
        .remove::<CarriedBy>();

    ApplyOutcome::UnitUnloaded {
        cargo_id,
        cargo_entity,
        transport_id,
        transport_entity,
        destination_tile,
    }
}

fn apply_hide(
    world: &mut World,
    unit_id: ServerUnitId,
    entity: Entity,
    position: awbrn_map::Position,
    faction: PlayerFaction,
) -> ApplyOutcome {
    world.entity_mut(entity).insert(Hiding);
    ApplyOutcome::UnitHidden {
        unit_id,
        entity,
        position,
        faction,
    }
}

fn apply_unhide(
    world: &mut World,
    unit_id: ServerUnitId,
    entity: Entity,
    position: awbrn_map::Position,
    faction: PlayerFaction,
) -> ApplyOutcome {
    world.entity_mut(entity).remove::<Hiding>();
    ApplyOutcome::UnitUnhidden {
        unit_id,
        entity,
        position,
        faction,
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_join(
    world: &mut World,
    source_id: ServerUnitId,
    source_entity: Entity,
    target_id: ServerUnitId,
    from: awbrn_map::Position,
    to: awbrn_map::Position,
    path: &[awbrn_map::Position],
    source_faction: PlayerFaction,
) -> ApplyOutcome {
    let target_entity = world
        .resource::<StrongIdMap<ServerUnitId>>()
        .get(&target_id)
        .expect("validated join target must exist");

    let (unit_type, source_hp, source_fuel, source_ammo, target_hp, target_fuel, target_ammo) = {
        let source = world.entity(source_entity);
        let target = world.entity(target_entity);
        let unit_type = source
            .get::<Unit>()
            .copied()
            .expect("source must have Unit component")
            .0;
        let source_hp = source
            .get::<UnitHp>()
            .copied()
            .expect("source must have UnitHp")
            .0;
        let source_fuel = source.get::<Fuel>().map(|fuel| fuel.0).unwrap_or(0);
        let source_ammo = source.get::<Ammo>().map(|ammo| ammo.0).unwrap_or(0);
        let target_hp = target
            .get::<UnitHp>()
            .copied()
            .expect("target must have UnitHp")
            .0;
        let target_fuel = target.get::<Fuel>().map(|fuel| fuel.0).unwrap_or(0);
        let target_ammo = target.get::<Ammo>().map(|ammo| ammo.0).unwrap_or(0);

        (
            unit_type,
            source_hp,
            source_fuel,
            source_ammo,
            target_hp,
            target_fuel,
            target_ammo,
        )
    };

    let combined_hp = u16::from(source_hp.get()) + u16::from(target_hp.get());
    let overflow_hp = combined_hp.saturating_sub(100);
    let joined_hp = combined_hp.min(100) as u8;
    let funds_refund = if overflow_hp > 0 {
        (unit_type.base_cost() / 100) * u32::from(overflow_hp)
    } else {
        0
    };

    if funds_refund > 0
        && let Some(owner) = world
            .resource::<PlayerRegistry>()
            .player_for_faction(source_faction)
    {
        world
            .resource_mut::<PlayerRegistry>()
            .get_mut(owner)
            .expect("join source owner must exist")
            .funds += funds_refund;
    }

    let joined_fuel = source_fuel
        .saturating_add(target_fuel)
        .min(unit_type.max_fuel());
    let joined_ammo = source_ammo
        .saturating_add(target_ammo)
        .min(unit_type.max_ammo());

    world.entity_mut(target_entity).insert((
        UnitHp(ExactHp::new(joined_hp)),
        Fuel(joined_fuel),
        Ammo(joined_ammo),
    ));
    world.entity_mut(source_entity).despawn();

    ApplyOutcome::UnitJoined {
        source_id,
        source_entity,
        source_unit_type: unit_type,
        target_id,
        target_entity,
        from,
        to,
        path: path.to_vec(),
        source_faction,
        funds_refund,
    }
}

fn apply_capture(
    world: &mut World,
    unit_id: ServerUnitId,
    entity: Entity,
    from: awbrn_map::Position,
    to: awbrn_map::Position,
    path: &[awbrn_map::Position],
    faction: PlayerFaction,
) -> ApplyOutcome {
    let outcome = CaptureAction {
        unit_entity: entity,
        progress_input: CaptureProgressInput::AddCurrentVisualHp,
    }
    .apply(world)
    .expect("validated capture action must apply");

    match outcome {
        CaptureActionOutcome::Continued { tile, progress, .. } => ApplyOutcome::CaptureContinued {
            unit_id,
            entity,
            from,
            to,
            path: path.to_vec(),
            faction,
            tile,
            progress: progress.value(),
        },
        CaptureActionOutcome::Completed {
            tile, new_faction, ..
        } => ApplyOutcome::PropertyCaptured {
            unit_id,
            entity,
            from,
            to,
            path: path.to_vec(),
            faction,
            tile,
            new_faction,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_attack(
    world: &mut World,
    attacker_id: ServerUnitId,
    attacker_entity: Entity,
    from: awbrn_map::Position,
    to: awbrn_map::Position,
    path: &[awbrn_map::Position],
    attacker_faction: PlayerFaction,
    target: awbrn_map::Position,
    combat_resolution: CombatResolution<'_>,
) -> Result<ApplyOutcome, ReplayEventError> {
    // Look up the defender entity at the target position.
    let defender_entity = world
        .resource::<BoardIndex>()
        .unit_entity(target)
        .ok()
        .flatten()
        .expect("validated target must have a unit");

    // Read attacker and defender components in a single block, then drop the refs
    // before any mutable world access.
    let (
        attacker_unit,
        attacker_exact_hp,
        attacker_ammo,
        defender_unit,
        defender_exact_hp,
        defender_ammo,
        defender_faction,
        defender_id,
    ) = {
        let attacker = world.entity(attacker_entity);
        let attacker_unit = attacker
            .get::<Unit>()
            .copied()
            .expect("attacker must have Unit");
        let attacker_exact_hp = attacker
            .get::<UnitHp>()
            .map(|h| h.0)
            .expect("attacker must have UnitHp");
        let attacker_ammo = attacker.get::<Ammo>().map(|a| a.0).unwrap_or(0);

        let defender = world.entity(defender_entity);
        let defender_unit = defender
            .get::<Unit>()
            .copied()
            .expect("defender must have Unit");
        let defender_exact_hp = defender
            .get::<UnitHp>()
            .map(|h| h.0)
            .expect("defender must have UnitHp");
        let defender_ammo = defender.get::<Ammo>().map(|a| a.0).unwrap_or(0);
        let defender_faction = defender
            .get::<Faction>()
            .copied()
            .expect("defender must have Faction")
            .0;
        let defender_id = defender
            .get::<ServerUnitId>()
            .copied()
            .expect("defender must have ServerUnitId");

        (
            attacker_unit,
            attacker_exact_hp,
            attacker_ammo,
            defender_unit,
            defender_exact_hp,
            defender_ammo,
            defender_faction,
            defender_id,
        )
    };

    // Look up CO stats for both sides.
    let attacker_co_stats = world
        .resource::<PlayerRegistry>()
        .player_for_faction(attacker_faction)
        .and_then(|pid| world.resource::<PlayerRegistry>().co_stats_for_player(pid))
        .unwrap_or_default();
    let defender_co_stats = world
        .resource::<PlayerRegistry>()
        .player_for_faction(defender_faction)
        .and_then(|pid| world.resource::<PlayerRegistry>().co_stats_for_player(pid))
        .unwrap_or_default();

    // Look up terrain defense stars for both combatants.
    // Attacker's terrain stars protect them from the counterattack.
    // Defender's terrain stars protect them from the initial attack.
    let game_map = world.resource::<GameMap>();
    let attacker_terrain_stars = game_map
        .terrain_at(to)
        .map(|t| t.defense_stars())
        .unwrap_or(0);
    let defender_terrain_stars = game_map
        .terrain_at(target)
        .map(|t| t.defense_stars())
        .unwrap_or(0);

    // Build the combat input.
    let input = CombatInput {
        attacker: CombatSide {
            unit_type: attacker_unit.0,
            exact_hp: attacker_exact_hp,
            attack_mod: PercentMod::new(100 + attacker_co_stats.attack_bonus),
            defense_mod: PercentMod::new(100 + attacker_co_stats.defense_bonus),
            max_good_luck: LuckCap::new(attacker_co_stats.max_good_luck),
            max_bad_luck: LuckCap::new(attacker_co_stats.max_bad_luck),
            ammo: attacker_ammo,
            terrain_stars: TerrainStars::new(attacker_terrain_stars),
        },
        defender: CombatSide {
            unit_type: defender_unit.0,
            exact_hp: defender_exact_hp,
            attack_mod: PercentMod::new(100 + defender_co_stats.attack_bonus),
            defense_mod: PercentMod::new(100 + defender_co_stats.defense_bonus),
            max_good_luck: LuckCap::new(defender_co_stats.max_good_luck),
            max_bad_luck: LuckCap::new(defender_co_stats.max_bad_luck),
            ammo: defender_ammo,
            terrain_stars: TerrainStars::new(defender_terrain_stars),
        },
        is_direct_combat: !attacker_unit.0.is_indirect(),
    };

    let outcome = match combat_resolution {
        CombatResolution::Roll => {
            crate::damage::calculate_combat_rng(&input, &mut world.resource_mut::<GameRng>())
                .expect("validated weapon must produce a combat outcome")
        }
        CombatResolution::Stored(outcome) => {
            validate_stored_combat_outcome(
                outcome,
                attacker_exact_hp,
                defender_exact_hp,
                input.is_direct_combat,
            )?;
            *outcome
        }
    };

    // Apply damage to defender.
    let defender_new_exact =
        defender_exact_hp.saturating_sub(DamagePts::new(outcome.attacker_damage_pts));
    let defender_hp_after = GraphicalHp(defender_new_exact.visual().get());

    if defender_hp_after.is_destroyed() {
        world.entity_mut(defender_entity).despawn();
    } else {
        let mut defender_mut = world.entity_mut(defender_entity);
        defender_mut.insert(UnitHp(defender_new_exact));
        // Consume defender's ammo if they counterattacked with their primary weapon.
        if outcome.defender_damage_pts.is_some()
            && crate::damage::uses_primary_weapon(defender_unit.0, attacker_unit.0, defender_ammo)
        {
            defender_mut.insert(Ammo(defender_ammo - 1));
        }
    }

    // Apply counterattack damage to attacker (if any).
    let attacker_hp_after = if let Some(counter_dmg) = outcome.defender_damage_pts {
        let attacker_new_exact = attacker_exact_hp.saturating_sub(DamagePts::new(counter_dmg));
        let hp_after = GraphicalHp(attacker_new_exact.visual().get());

        if hp_after.is_destroyed() {
            world.entity_mut(attacker_entity).despawn();
        } else {
            world
                .entity_mut(attacker_entity)
                .insert(UnitHp(attacker_new_exact));
        }

        hp_after
    } else {
        // No counterattack — attacker HP unchanged.
        GraphicalHp(attacker_exact_hp.visual().get())
    };

    // Consume attacker's ammo if primary weapon was used.
    if !attacker_hp_after.is_destroyed()
        && crate::damage::uses_primary_weapon(attacker_unit.0, defender_unit.0, attacker_ammo)
    {
        world
            .entity_mut(attacker_entity)
            .insert(Ammo(attacker_ammo - 1));
    }

    Ok(ApplyOutcome::UnitAttacked {
        attacker_id,
        attacker_entity,
        from,
        to,
        path: path.to_vec(),
        attacker_faction,
        defender_id,
        defender_entity,
        defender_position: target,
        defender_faction,
        attacker_hp_after,
        defender_hp_after,
        combat_outcome: outcome,
    })
}

/// Validate only stored-outcome structure and bounds, not formula achievability.
///
/// This rejects impossible deltas such as damage greater than current HP,
/// indirect counterattacks, and counters from destroyed defenders. It does not
/// re-derive whether the damage could have been produced by the current
/// CO/terrain/luck formula. That tradeoff keeps replay independent from RNG
/// state and future formula changes; trust-boundary callers that need stronger
/// guarantees must add separate formula-level or cryptographic validation.
fn validate_stored_combat_outcome(
    outcome: &CombatOutcome,
    attacker_exact_hp: ExactHp,
    defender_exact_hp: ExactHp,
    is_direct_combat: bool,
) -> Result<(), ReplayEventError> {
    if outcome.attacker_damage_pts > defender_exact_hp.get() {
        return Err(ReplayEventError::InvalidCombatOutcome {
            reason: format!(
                "attacker damage {} exceeds defender HP {}",
                outcome.attacker_damage_pts,
                defender_exact_hp.get()
            ),
        });
    }

    let Some(counter_damage) = outcome.defender_damage_pts else {
        return Ok(());
    };

    if !is_direct_combat {
        return Err(ReplayEventError::InvalidCombatOutcome {
            reason: "indirect attacks cannot receive counterattack damage".into(),
        });
    }

    if outcome.attacker_damage_pts >= defender_exact_hp.get() {
        return Err(ReplayEventError::InvalidCombatOutcome {
            reason: "destroyed defenders cannot counterattack".into(),
        });
    }

    if counter_damage > attacker_exact_hp.get() {
        return Err(ReplayEventError::InvalidCombatOutcome {
            reason: format!(
                "counterattack damage {} exceeds attacker HP {}",
                counter_damage,
                attacker_exact_hp.get()
            ),
        });
    }

    Ok(())
}

fn apply_end_turn(world: &mut World) -> ApplyOutcome {
    let current_player = world.resource::<ServerGameState>().active_player;

    // Compute next player and both indices before any mutations to avoid borrow conflicts.
    let (next_player_opt, current_idx, next_idx) = {
        let registry = world.resource::<PlayerRegistry>();
        let next = registry.next_active_player_after(current_player);
        let current_idx = registry.player_index(current_player).unwrap_or(0);
        let next_idx = next.and_then(|p| registry.player_index(p)).unwrap_or(0);
        (next, current_idx, next_idx)
    };

    let Some(next_player) = next_player_opt else {
        // No active players remain -- game over.
        world.resource_mut::<ServerGameState>().phase = TurnPhase::GameOver { winner: None };
        return ApplyOutcome::TurnEnded {
            new_active_player: current_player,
            new_day: None,
        };
    };

    // A new day begins when the turn order wraps back to an earlier index.
    let new_day = if next_idx <= current_idx {
        let mut state = world.resource_mut::<ServerGameState>();
        state.day += 1;
        Some(state.day)
    } else {
        None
    };

    world.resource_mut::<ServerGameState>().active_player = next_player;

    // Reactivate all units belonging to the new active player.
    let new_faction = world
        .resource::<PlayerRegistry>()
        .faction_for_player(next_player)
        .expect("next player must have a faction");

    activate_player_units(world, new_faction);

    ApplyOutcome::TurnEnded {
        new_active_player: next_player,
        new_day,
    }
}

/// Mark all units belonging to the given faction as active.
fn activate_player_units(world: &mut World, faction: PlayerFaction) {
    let batch: Vec<(Entity, UnitActive)> = world
        .query_filtered::<(Entity, &awbrn_game::world::Faction), Without<UnitActive>>()
        .iter(world)
        .filter(|(_, f)| f.0 == faction)
        .map(|(e, _)| (e, UnitActive))
        .collect();

    world.insert_batch(batch);
}
