use std::collections::{HashMap, HashSet};

use crate::apply::ApplyOutcome;
use crate::damage::CombatOutcome;
use crate::player::{PlayerId, PlayerRegistry};
use crate::state::{ServerGameState, TurnPhase};
use crate::unit_id::ServerUnitId;
use awbrn_game::MapPosition;
use awbrn_game::replay::{PowerVisionBoosts, range_modifier_for_weather};
use awbrn_game::world::{
    Ammo, CaptureProgress, CarriedBy, CurrentWeather, Faction, FogOfWarMap, Fuel, GameMap,
    GraphicalHp, Hiding, StrongIdMap, Unit, VisionRange, collect_friendly_units, rebuild_fog_map,
};
use awbrn_map::Position;
use awbrn_types::PlayerFaction;
use bevy::prelude::*;

/// Header with the current game state, included in every response.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GameStateHeader {
    pub day: u32,
    pub active_player: PlayerId,
    pub phase: TurnPhase,
}

/// A unit as visible to a specific player.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VisibleUnit {
    pub id: ServerUnitId,
    pub unit_type: awbrn_types::Unit,
    pub faction: PlayerFaction,
    pub position: Position,
    pub hp: u8,
    /// Included for units owned by the viewing player or an allied teammate.
    pub fuel: Option<u32>,
    /// Included for units owned by the viewing player or an allied teammate.
    pub ammo: Option<u32>,
    pub capturing: bool,
    pub capture_progress: Option<u8>,
    pub hiding: bool,
}

/// A terrain tile as visible to a specific player.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VisibleTerrain {
    pub position: Position,
    pub terrain: awbrn_types::GraphicalTerrain,
}

/// Full snapshot of what a player can see (for initial load / reconnection).
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerView {
    pub state: GameStateHeader,
    pub my_funds: u32,
    pub players: Vec<PublicPlayerState>,
    pub units: Vec<VisibleUnit>,
    pub terrain: Vec<VisibleTerrain>,
}

/// Public per-player state visible in full-state snapshots.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicPlayerState {
    pub slot_index: u8,
    pub funds: u32,
}

/// Full public snapshot for non-fog spectators.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpectatorView {
    pub state: GameStateHeader,
    pub players: Vec<PublicPlayerState>,
    pub units: Vec<VisibleUnit>,
    pub terrain: Vec<VisibleTerrain>,
}

/// Information about a unit that moved.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UnitMoved {
    pub id: ServerUnitId,
    pub path: Vec<Position>,
    pub from: Position,
    pub to: Position,
}

/// Information about a turn change.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TurnChange {
    pub new_active_player: PlayerId,
    pub new_day: Option<u32>,
}

/// Combat event visible to a player after an attack.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UnitCombatEvent {
    pub attacker_id: ServerUnitId,
    pub defender_id: ServerUnitId,
    /// Post-combat HP for the attacker. `0` means destroyed.
    pub attacker_hp_after: GraphicalHp,
    /// Post-combat HP for the defender. `0` means destroyed.
    pub defender_hp_after: GraphicalHp,
}

/// Capture event visible to a player after a capture action.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CaptureEvent {
    CaptureContinued {
        tile: Position,
        unit_id: ServerUnitId,
        progress: u8,
    },
    PropertyCaptured {
        tile: Position,
        new_faction: PlayerFaction,
    },
}

/// Incremental update for a specific player after a command.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlayerUpdate {
    /// Units newly revealed to this player.
    pub units_revealed: Vec<VisibleUnit>,
    /// Units that moved within this player's vision.
    pub units_moved: Vec<UnitMoved>,
    /// Units that disappeared from this player's vision.
    pub units_removed: Vec<ServerUnitId>,
    /// Terrain tiles newly revealed to this player (fog lifted).
    pub terrain_revealed: Vec<VisibleTerrain>,
    /// Terrain tiles whose visible terrain changed while known to this player.
    pub terrain_changed: Vec<VisibleTerrain>,
    /// Turn change information (if EndTurn was the command).
    pub turn_change: Option<TurnChange>,
    /// Combat event (if an attack was the command).
    pub combat_event: Option<UnitCombatEvent>,
    /// Capture event (if a capture action was visible to this player).
    pub capture_event: Option<CaptureEvent>,
    /// Updated funds for this player, when the command changed their balance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_funds: Option<u32>,
    /// Current game state.
    pub state: GameStateHeader,
}

/// The result of applying a command, containing per-player updates.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommandResult {
    pub updates: Vec<(PlayerId, PlayerUpdate)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_outcome: Option<CombatOutcome>,
}

fn game_state_header(world: &World) -> GameStateHeader {
    let state = world.resource::<ServerGameState>();
    GameStateHeader {
        day: state.day,
        active_player: state.active_player,
        phase: state.phase,
    }
}

/// Build the full visible state for a player.
pub(crate) fn build_player_view(world: &mut World, player: PlayerId) -> PlayerView {
    let registry = world.resource::<PlayerRegistry>();
    let funds = registry.get(player).expect("player must exist").funds;
    let friendly_factions = registry.friendly_factions_for_player(player);
    let players = public_player_states(registry);
    let fog_active = world.resource::<awbrn_game::world::FogActive>().0;

    let fog_map = if fog_active {
        Some(compute_fog_for_factions(world, &friendly_factions))
    } else {
        None
    };

    let units = collect_visible_units(world, &friendly_factions, fog_map.as_ref());
    let terrain = collect_visible_terrain(world, fog_map.as_ref());

    PlayerView {
        state: game_state_header(world),
        my_funds: funds,
        players,
        units,
        terrain,
    }
}

/// Build the full public state for a spectator in a non-fog match.
pub(crate) fn build_spectator_view(world: &mut World) -> SpectatorView {
    let registry = world.resource::<PlayerRegistry>();
    let players = public_player_states(registry);
    SpectatorView {
        state: game_state_header(world),
        players,
        units: collect_spectator_units(world),
        terrain: collect_visible_terrain(world, None),
    }
}

/// Build per-player updates after applying a command.
pub(crate) fn build_command_result(
    world: &mut World,
    outcome: &ApplyOutcome,
    pre_fog: &[(PlayerId, FogOfWarMap)],
) -> CommandResult {
    let registry = world.resource::<PlayerRegistry>();
    let header = game_state_header(world);
    let fog_active = world.resource::<awbrn_game::world::FogActive>().0;
    let player_ids: Vec<_> = registry.players().iter().map(|s| s.id).collect();

    let mut updates = Vec::new();

    for player in player_ids {
        let friendly_factions = world
            .resource::<PlayerRegistry>()
            .friendly_factions_for_player(player);

        let post_fog = if fog_active {
            Some(compute_fog_for_factions(world, &friendly_factions))
        } else {
            None
        };

        let pre = pre_fog.iter().find(|(id, _)| *id == player).map(|(_, f)| f);

        let update = build_player_update(world, player, outcome, pre, post_fog.as_ref(), &header);
        updates.push((player, update));
    }

    CommandResult {
        updates,
        combat_outcome: match outcome {
            ApplyOutcome::UnitAttacked { combat_outcome, .. } => Some(*combat_outcome),
            _ => None,
        },
    }
}

fn build_player_update(
    world: &mut World,
    player: PlayerId,
    outcome: &ApplyOutcome,
    pre_fog: Option<&FogOfWarMap>,
    post_fog: Option<&FogOfWarMap>,
    header: &GameStateHeader,
) -> PlayerUpdate {
    let registry = world.resource::<PlayerRegistry>();
    let friendly_factions = registry.friendly_factions_for_player(player);

    match outcome {
        ApplyOutcome::UnitMoved {
            unit_id,
            entity,
            from,
            to,
            path,
            faction,
        } => {
            let is_friendly_unit = friendly_factions.contains(faction);
            let mut units_moved = Vec::new();
            let mut units_revealed = Vec::new();
            let mut units_removed = Vec::new();
            let mut terrain_revealed = Vec::new();

            if is_friendly_unit {
                // Friendly viewers always see allied moves.
                units_moved.push(UnitMoved {
                    id: *unit_id,
                    path: path.clone(),
                    from: *from,
                    to: *to,
                });

                // Moving a friendly unit changes shared vision — diff the full enemy visible set.
                let pre_enemies = visible_enemy_units(world, &friendly_factions, pre_fog);
                let post_enemies = visible_enemy_units(world, &friendly_factions, post_fog);

                for (uid, ent) in &post_enemies {
                    if !pre_enemies.contains_key(uid)
                        && let Some(visible) =
                            entity_to_visible_unit(world, *ent, &friendly_factions)
                    {
                        units_revealed.push(visible);
                    }
                }
                for uid in pre_enemies.keys() {
                    if !post_enemies.contains_key(uid) {
                        units_removed.push(*uid);
                    }
                }

                terrain_revealed = terrain_diff(world, pre_fog, post_fog);
            } else {
                // Enemy unit moved — our fog map didn't change, only this unit's position did.
                let unit_type = world.entity(*entity).get::<Unit>().map(|u| u.0);
                let is_air = unit_type
                    .map(|u| u.domain() == awbrn_types::UnitDomain::Air)
                    .unwrap_or(false);

                let from_was_visible = pre_fog
                    .map(|f| f.is_unit_visible(*from, is_air))
                    .unwrap_or(true);
                let to_is_visible = post_fog
                    .map(|f| f.is_unit_visible(*to, is_air))
                    .unwrap_or(true);

                if from_was_visible && to_is_visible {
                    // Enemy unit moved within our vision.
                    units_moved.push(UnitMoved {
                        id: *unit_id,
                        path: path.clone(),
                        from: *from,
                        to: *to,
                    });
                } else if !from_was_visible && to_is_visible {
                    // Enemy unit appeared in our vision.
                    if let Some(visible) =
                        entity_to_visible_unit(world, *entity, &friendly_factions)
                    {
                        units_revealed.push(visible);
                    }
                } else if from_was_visible && !to_is_visible {
                    // Enemy unit left our vision.
                    units_removed.push(*unit_id);
                }
                // If neither was visible, this player sees nothing.
            }

            PlayerUpdate {
                units_revealed,
                units_moved,
                units_removed,
                terrain_revealed,
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event: None,
                my_funds: None,
                state: header.clone(),
            }
        }
        ApplyOutcome::TurnEnded {
            new_active_player,
            new_day,
        } => PlayerUpdate {
            units_revealed: Vec::new(),
            units_moved: Vec::new(),
            units_removed: Vec::new(),
            terrain_revealed: Vec::new(),
            terrain_changed: Vec::new(),
            turn_change: Some(TurnChange {
                new_active_player: *new_active_player,
                new_day: *new_day,
            }),
            combat_event: None,
            capture_event: None,
            my_funds: None,
            state: header.clone(),
        },
        ApplyOutcome::UnitBuilt {
            tile,
            unit_type,
            unit_id,
        } => {
            let entity = world
                .resource::<StrongIdMap<ServerUnitId>>()
                .get(unit_id)
                .expect("built unit must exist");
            let faction = world
                .entity(entity)
                .get::<Faction>()
                .expect("built unit must have faction")
                .0;

            let mut units_revealed = Vec::new();
            let mut units_removed = Vec::new();
            let mut terrain_revealed = Vec::new();
            let is_friendly_unit = friendly_factions.contains(&faction);

            if is_friendly_unit {
                if let Some(visible) = entity_to_visible_unit(world, entity, &friendly_factions) {
                    units_revealed.push(visible);
                }

                let pre_enemies = visible_enemy_units(world, &friendly_factions, pre_fog);
                let post_enemies = visible_enemy_units(world, &friendly_factions, post_fog);

                for (uid, ent) in &post_enemies {
                    if !pre_enemies.contains_key(uid)
                        && let Some(visible) =
                            entity_to_visible_unit(world, *ent, &friendly_factions)
                    {
                        units_revealed.push(visible);
                    }
                }
                for uid in pre_enemies.keys() {
                    if !post_enemies.contains_key(uid) {
                        units_removed.push(*uid);
                    }
                }

                terrain_revealed = terrain_diff(world, pre_fog, post_fog);
            } else {
                let is_air = unit_type.domain() == awbrn_types::UnitDomain::Air;
                let tile_is_visible = post_fog
                    .map(|fog| fog.is_unit_visible(*tile, is_air))
                    .unwrap_or(true);

                if tile_is_visible
                    && let Some(visible) = entity_to_visible_unit(world, entity, &friendly_factions)
                {
                    units_revealed.push(visible);
                }
            }

            let my_funds = world
                .resource::<PlayerRegistry>()
                .player_for_faction(faction)
                .filter(|owner| *owner == player)
                .and_then(|owner| {
                    world
                        .resource::<PlayerRegistry>()
                        .get(owner)
                        .map(|slot| slot.funds)
                });

            PlayerUpdate {
                units_revealed,
                units_moved: Vec::new(),
                units_removed,
                terrain_revealed,
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event: None,
                my_funds,
                state: header.clone(),
            }
        }
        ApplyOutcome::UnitSupplied {
            supplier_id,
            resupplied_unit_ids,
        } => {
            let mut units_revealed = Vec::new();
            let mut seen = HashSet::new();

            for unit_id in std::iter::once(supplier_id).chain(resupplied_unit_ids.iter()) {
                if !seen.insert(*unit_id) {
                    continue;
                }
                let Some(entity) = world.resource::<StrongIdMap<ServerUnitId>>().get(unit_id)
                else {
                    continue;
                };
                if entity_visible_to_factions(world, entity, &friendly_factions, post_fog)
                    && let Some(visible) = entity_to_visible_unit(world, entity, &friendly_factions)
                {
                    units_revealed.push(visible);
                }
            }

            PlayerUpdate {
                units_revealed,
                units_moved: Vec::new(),
                units_removed: Vec::new(),
                terrain_revealed: Vec::new(),
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event: None,
                my_funds: None,
                state: header.clone(),
            }
        }
        ApplyOutcome::UnitLoaded {
            cargo_id,
            cargo_entity,
            transport_id,
            transport_entity,
            from,
            to,
            path,
            cargo_faction,
        } => {
            let cargo_unit_type = world
                .entity(*cargo_entity)
                .get::<Unit>()
                .map(|unit| unit.0)
                .unwrap_or(awbrn_types::Unit::Infantry);
            let detectors = detector_positions(world, &friendly_factions);
            let from_was_visible = unit_would_be_visible_to_factions(
                *from,
                cargo_unit_type,
                *cargo_faction,
                false,
                &friendly_factions,
                pre_fog,
                &detectors,
            );
            let to_is_visible = unit_would_be_visible_to_factions(
                *to,
                cargo_unit_type,
                *cargo_faction,
                false,
                &friendly_factions,
                post_fog,
                &detectors,
            );
            let mut units_moved = Vec::new();
            let mut units_removed = Vec::new();

            if from_was_visible && to_is_visible {
                units_moved.push(UnitMoved {
                    id: *cargo_id,
                    path: path.clone(),
                    from: *from,
                    to: *to,
                });
                units_removed.push(*cargo_id);
            } else if from_was_visible && !to_is_visible {
                units_removed.push(*cargo_id);
            }

            let mut units_revealed = Vec::new();
            if entity_visible_to_factions_with_detectors(
                world,
                *transport_entity,
                &friendly_factions,
                post_fog,
                &detectors,
            ) && let Some(transport) =
                entity_to_visible_unit(world, *transport_entity, &friendly_factions)
            {
                units_revealed.push(transport);
            }

            let _ = transport_id;

            PlayerUpdate {
                units_revealed,
                units_moved,
                units_removed,
                terrain_revealed: Vec::new(),
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event: None,
                my_funds: None,
                state: header.clone(),
            }
        }
        ApplyOutcome::UnitUnloaded {
            cargo_id,
            cargo_entity,
            transport_id,
            transport_entity,
            destination_tile,
        } => {
            let mut units_revealed = Vec::new();
            let detectors = detector_positions(world, &friendly_factions);
            if entity_visible_to_factions_with_detectors(
                world,
                *cargo_entity,
                &friendly_factions,
                post_fog,
                &detectors,
            ) && let Some(visible) =
                entity_to_visible_unit(world, *cargo_entity, &friendly_factions)
            {
                units_revealed.push(visible);
            }
            if entity_visible_to_factions_with_detectors(
                world,
                *transport_entity,
                &friendly_factions,
                post_fog,
                &detectors,
            ) && let Some(transport) =
                entity_to_visible_unit(world, *transport_entity, &friendly_factions)
            {
                units_revealed.push(transport);
            }

            let _ = (cargo_id, transport_id, destination_tile);

            PlayerUpdate {
                units_revealed,
                units_moved: Vec::new(),
                units_removed: Vec::new(),
                terrain_revealed: Vec::new(),
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event: None,
                my_funds: None,
                state: header.clone(),
            }
        }
        ApplyOutcome::UnitHidden {
            unit_id,
            entity,
            position,
            faction,
        } => {
            let unit_type = world
                .entity(*entity)
                .get::<Unit>()
                .map(|unit| unit.0)
                .unwrap_or(awbrn_types::Unit::Infantry);
            let detectors = detector_positions(world, &friendly_factions);
            let was_visible = unit_would_be_visible_to_factions(
                *position,
                unit_type,
                *faction,
                false,
                &friendly_factions,
                pre_fog,
                &detectors,
            );
            let is_visible = unit_would_be_visible_to_factions(
                *position,
                unit_type,
                *faction,
                true,
                &friendly_factions,
                post_fog,
                &detectors,
            );

            let mut units_revealed = Vec::new();
            if (friendly_factions.contains(faction) || (was_visible && is_visible))
                && let Some(visible) = entity_to_visible_unit(world, *entity, &friendly_factions)
            {
                units_revealed.push(visible);
            }

            PlayerUpdate {
                units_revealed,
                units_moved: Vec::new(),
                units_removed: if was_visible && !is_visible {
                    vec![*unit_id]
                } else {
                    Vec::new()
                },
                terrain_revealed: Vec::new(),
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event: None,
                my_funds: None,
                state: header.clone(),
            }
        }
        ApplyOutcome::UnitUnhidden {
            unit_id,
            entity,
            position,
            faction,
        } => {
            let unit_type = world
                .entity(*entity)
                .get::<Unit>()
                .map(|unit| unit.0)
                .unwrap_or(awbrn_types::Unit::Infantry);
            let detectors = detector_positions(world, &friendly_factions);
            let is_visible = unit_would_be_visible_to_factions(
                *position,
                unit_type,
                *faction,
                false,
                &friendly_factions,
                post_fog,
                &detectors,
            );

            let mut units_revealed = Vec::new();
            if (friendly_factions.contains(faction) || is_visible)
                && let Some(visible) = entity_to_visible_unit(world, *entity, &friendly_factions)
            {
                units_revealed.push(visible);
            }

            let _ = unit_id;

            PlayerUpdate {
                units_revealed,
                units_moved: Vec::new(),
                units_removed: Vec::new(),
                terrain_revealed: Vec::new(),
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event: None,
                my_funds: None,
                state: header.clone(),
            }
        }
        ApplyOutcome::UnitJoined {
            source_id,
            source_entity,
            source_unit_type,
            target_id,
            target_entity,
            from,
            to,
            path,
            source_faction,
            funds_refund,
        } => {
            let detectors = detector_positions(world, &friendly_factions);
            let source_was_visible = unit_would_be_visible_to_factions(
                *from,
                *source_unit_type,
                *source_faction,
                false,
                &friendly_factions,
                pre_fog,
                &detectors,
            );
            let target_is_visible = entity_visible_to_factions_with_detectors(
                world,
                *target_entity,
                &friendly_factions,
                post_fog,
                &detectors,
            );
            let mut units_moved = Vec::new();
            let mut units_removed = Vec::new();
            if source_was_visible && target_is_visible {
                units_moved.push(UnitMoved {
                    id: *source_id,
                    path: path.clone(),
                    from: *from,
                    to: *to,
                });
                units_removed.push(*source_id);
            } else if source_was_visible && !target_is_visible {
                units_removed.push(*source_id);
            }

            let mut units_revealed = Vec::new();
            if target_is_visible
                && let Some(visible) =
                    entity_to_visible_unit(world, *target_entity, &friendly_factions)
            {
                units_revealed.push(visible);
            }

            let _ = (source_entity, target_id);
            let my_funds = if *funds_refund > 0 {
                world
                    .resource::<PlayerRegistry>()
                    .player_for_faction(*source_faction)
                    .filter(|owner| *owner == player)
                    .and_then(|owner| {
                        world
                            .resource::<PlayerRegistry>()
                            .get(owner)
                            .map(|slot| slot.funds)
                    })
            } else {
                None
            };

            PlayerUpdate {
                units_revealed,
                units_moved,
                units_removed,
                terrain_revealed: Vec::new(),
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event: None,
                my_funds,
                state: header.clone(),
            }
        }
        ApplyOutcome::UnitAttacked {
            attacker_id,
            attacker_entity,
            from,
            to,
            path,
            attacker_faction,
            defender_id,
            defender_entity,
            defender_position,
            defender_faction,
            attacker_hp_after,
            defender_hp_after,
            combat_outcome: _,
        } => {
            let is_friendly_unit = friendly_factions.contains(attacker_faction);
            let mut units_moved = Vec::new();
            let mut units_revealed = Vec::new();
            let mut units_removed = Vec::new();
            let mut terrain_revealed = Vec::new();

            if is_friendly_unit {
                // Friendly viewers always see allied moves.
                units_moved.push(UnitMoved {
                    id: *attacker_id,
                    path: path.clone(),
                    from: *from,
                    to: *to,
                });

                // Moving a friendly unit changes shared vision — diff the full enemy visible set.
                let pre_enemies = visible_enemy_units(world, &friendly_factions, pre_fog);
                let post_enemies = visible_enemy_units(world, &friendly_factions, post_fog);

                for (uid, ent) in &post_enemies {
                    if !pre_enemies.contains_key(uid)
                        && let Some(visible) =
                            entity_to_visible_unit(world, *ent, &friendly_factions)
                    {
                        units_revealed.push(visible);
                    }
                }
                for uid in pre_enemies.keys() {
                    if !post_enemies.contains_key(uid) {
                        units_removed.push(*uid);
                    }
                }

                terrain_revealed = terrain_diff(world, pre_fog, post_fog);
            } else {
                // Enemy attacker — handle attacker visibility the same as UnitMoved enemy branch.
                // Use get_entity in case the attacker was despawned (destroyed by counterattack).
                let attacker_unit_type = world
                    .get_entity(*attacker_entity)
                    .ok()
                    .and_then(|e| e.get::<awbrn_game::world::Unit>().map(|u| u.0));
                let is_air = attacker_unit_type
                    .map(|u| u.domain() == awbrn_types::UnitDomain::Air)
                    .unwrap_or(false);

                let from_was_visible = pre_fog
                    .map(|f| f.is_unit_visible(*from, is_air))
                    .unwrap_or(true);
                let to_is_visible = post_fog
                    .map(|f| f.is_unit_visible(*to, is_air))
                    .unwrap_or(true);

                if from_was_visible && to_is_visible {
                    units_moved.push(UnitMoved {
                        id: *attacker_id,
                        path: path.clone(),
                        from: *from,
                        to: *to,
                    });
                } else if !from_was_visible && to_is_visible {
                    if let Some(visible) =
                        entity_to_visible_unit(world, *attacker_entity, &friendly_factions)
                    {
                        units_revealed.push(visible);
                    }
                } else if from_was_visible && !to_is_visible {
                    units_removed.push(*attacker_id);
                }
            }

            // Determine if the attacker's destination is visible to this player.
            // Use get_entity in case the attacker was despawned (killed by counterattack).
            let attacker_unit_type_for_vis = world
                .get_entity(*attacker_entity)
                .ok()
                .and_then(|e| e.get::<awbrn_game::world::Unit>().map(|u| u.0));
            let attacker_is_air = attacker_unit_type_for_vis
                .map(|u| u.domain() == awbrn_types::UnitDomain::Air)
                .unwrap_or(false);
            let attacker_visible = is_friendly_unit
                || post_fog
                    .map(|f| f.is_unit_visible(*to, attacker_is_air))
                    .unwrap_or(true);

            // Determine if the defender's position is visible to this player after the command.
            // Use get_entity in case the defender was despawned (destroyed) during apply.
            let defender_unit_type = world
                .get_entity(*defender_entity)
                .ok()
                .and_then(|e| e.get::<awbrn_game::world::Unit>().map(|u| u.0));
            let defender_is_air = defender_unit_type
                .map(|u| u.domain() == awbrn_types::UnitDomain::Air)
                .unwrap_or(false);
            let defender_is_friendly = friendly_factions.contains(defender_faction);
            let defender_visible = defender_is_friendly
                || post_fog
                    .map(|f| f.is_unit_visible(*defender_position, defender_is_air))
                    .unwrap_or(true);

            // Build the combat event if either combatant is visible to this player.
            let combat_event = if attacker_visible || defender_visible {
                Some(UnitCombatEvent {
                    attacker_id: *attacker_id,
                    defender_id: *defender_id,
                    attacker_hp_after: *attacker_hp_after,
                    defender_hp_after: *defender_hp_after,
                })
            } else {
                None
            };

            // Handle units destroyed during combat.
            if defender_hp_after.is_destroyed() && defender_visible {
                units_removed.push(*defender_id);
            }
            if attacker_hp_after.is_destroyed() && attacker_visible {
                units_removed.push(*attacker_id);
            }

            PlayerUpdate {
                units_revealed,
                units_moved,
                units_removed,
                terrain_revealed,
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event,
                capture_event: None,
                my_funds: None,
                state: header.clone(),
            }
        }
        ApplyOutcome::CaptureContinued {
            unit_id,
            entity,
            from,
            to,
            path,
            faction,
            tile,
            progress,
        } => {
            let movement = movement_visibility_update(
                world,
                &friendly_factions,
                pre_fog,
                post_fog,
                *unit_id,
                *entity,
                *from,
                *to,
                path,
                *faction,
            );
            let capture_event = if tile_visible(post_fog, *tile) {
                Some(CaptureEvent::CaptureContinued {
                    tile: *tile,
                    unit_id: *unit_id,
                    progress: *progress,
                })
            } else {
                None
            };

            PlayerUpdate {
                units_revealed: movement.units_revealed,
                units_moved: movement.units_moved,
                units_removed: movement.units_removed,
                terrain_revealed: movement.terrain_revealed,
                terrain_changed: Vec::new(),
                turn_change: None,
                combat_event: None,
                capture_event,
                my_funds: None,
                state: header.clone(),
            }
        }
        ApplyOutcome::PropertyCaptured {
            unit_id,
            entity,
            from,
            to,
            path,
            faction,
            tile,
            new_faction,
        } => {
            let movement = movement_visibility_update(
                world,
                &friendly_factions,
                pre_fog,
                post_fog,
                *unit_id,
                *entity,
                *from,
                *to,
                path,
                *faction,
            );
            let was_visible = tile_visible(pre_fog, *tile);
            let capture_visible = was_visible || tile_visible(post_fog, *tile);
            let capture_event = if capture_visible {
                Some(CaptureEvent::PropertyCaptured {
                    tile: *tile,
                    new_faction: *new_faction,
                })
            } else {
                None
            };
            let terrain_changed = if was_visible {
                world
                    .resource::<GameMap>()
                    .terrain_at(*tile)
                    .map(|terrain| {
                        vec![VisibleTerrain {
                            position: *tile,
                            terrain,
                        }]
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            PlayerUpdate {
                units_revealed: movement.units_revealed,
                units_moved: movement.units_moved,
                units_removed: movement.units_removed,
                terrain_revealed: movement.terrain_revealed,
                terrain_changed,
                turn_change: None,
                combat_event: None,
                capture_event,
                my_funds: None,
                state: header.clone(),
            }
        }
    }
}

#[derive(Default)]
struct MovementVisibilityUpdate {
    units_revealed: Vec<VisibleUnit>,
    units_moved: Vec<UnitMoved>,
    units_removed: Vec<ServerUnitId>,
    terrain_revealed: Vec<VisibleTerrain>,
}

#[allow(clippy::too_many_arguments)]
fn movement_visibility_update(
    world: &mut World,
    friendly_factions: &HashSet<PlayerFaction>,
    pre_fog: Option<&FogOfWarMap>,
    post_fog: Option<&FogOfWarMap>,
    unit_id: ServerUnitId,
    entity: Entity,
    from: Position,
    to: Position,
    path: &[Position],
    faction: PlayerFaction,
) -> MovementVisibilityUpdate {
    let is_friendly_unit = friendly_factions.contains(&faction);
    let mut update = MovementVisibilityUpdate::default();

    if is_friendly_unit {
        update.units_moved.push(UnitMoved {
            id: unit_id,
            path: path.to_vec(),
            from,
            to,
        });

        let pre_enemies = visible_enemy_units(world, friendly_factions, pre_fog);
        let post_enemies = visible_enemy_units(world, friendly_factions, post_fog);

        for (uid, ent) in &post_enemies {
            if !pre_enemies.contains_key(uid)
                && let Some(visible) = entity_to_visible_unit(world, *ent, friendly_factions)
            {
                update.units_revealed.push(visible);
            }
        }
        for uid in pre_enemies.keys() {
            if !post_enemies.contains_key(uid) {
                update.units_removed.push(*uid);
            }
        }

        update.terrain_revealed = terrain_diff(world, pre_fog, post_fog);
    } else {
        let unit_type = world.entity(entity).get::<Unit>().map(|u| u.0);
        let is_air = unit_type
            .map(|u| u.domain() == awbrn_types::UnitDomain::Air)
            .unwrap_or(false);

        let from_was_visible = pre_fog
            .map(|f| f.is_unit_visible(from, is_air))
            .unwrap_or(true);
        let to_is_visible = post_fog
            .map(|f| f.is_unit_visible(to, is_air))
            .unwrap_or(true);

        if from_was_visible && to_is_visible {
            update.units_moved.push(UnitMoved {
                id: unit_id,
                path: path.to_vec(),
                from,
                to,
            });
        } else if !from_was_visible && to_is_visible {
            if let Some(visible) = entity_to_visible_unit(world, entity, friendly_factions) {
                update.units_revealed.push(visible);
            }
        } else if from_was_visible && !to_is_visible {
            update.units_removed.push(unit_id);
        }
    }

    update
}

/// Compute a fog of war map for the given set of friendly factions.
pub(crate) fn compute_fog_for_factions(
    world: &mut World,
    friendly_factions: &HashSet<PlayerFaction>,
) -> FogOfWarMap {
    let game_map = world.resource::<GameMap>();
    let width = game_map.width();
    let height = game_map.height();
    let range_modifier = range_modifier_for_weather(world.resource::<CurrentWeather>().weather());

    let mut unit_query = world.query::<(&MapPosition, &VisionRange, &Faction, &Unit)>();
    let friendly_units = collect_friendly_units(
        unit_query
            .iter(world)
            .map(|(pos, vis, fac, unit)| (pos.position(), vis.0, fac.0, unit.0)),
        friendly_factions,
        Some(&world.resource::<PowerVisionBoosts>().0),
    );

    let game_map = world.resource::<GameMap>();
    let mut fog_map = FogOfWarMap::new(width, height);
    rebuild_fog_map(
        game_map,
        friendly_factions,
        &friendly_units,
        range_modifier,
        &mut fog_map,
    );
    fog_map
}

/// Snapshot the fog state for all players (used before applying a command).
pub(crate) fn snapshot_pre_fog(world: &mut World) -> Vec<(PlayerId, FogOfWarMap)> {
    let fog_active = world.resource::<awbrn_game::world::FogActive>().0;
    if !fog_active {
        return Vec::new();
    }

    let registry = world.resource::<PlayerRegistry>();
    let players: Vec<_> = registry.players().iter().map(|s| s.id).collect();

    players
        .into_iter()
        .map(|player| {
            let friendly = world
                .resource::<PlayerRegistry>()
                .friendly_factions_for_player(player);
            let fog = compute_fog_for_factions(world, &friendly);
            (player, fog)
        })
        .collect()
}

fn collect_visible_units(
    world: &mut World,
    friendly_factions: &HashSet<PlayerFaction>,
    fog_map: Option<&FogOfWarMap>,
) -> Vec<VisibleUnit> {
    let detectors = detector_positions(world, friendly_factions);
    // Include ServerUnitId, Fuel, Ammo, CaptureProgress in the query to avoid separate entity lookups.
    let mut query = world.query_filtered::<(
        Entity,
        &MapPosition,
        &Unit,
        &Faction,
        &GraphicalHp,
        &ServerUnitId,
        Option<&Fuel>,
        Option<&Ammo>,
        Option<&CaptureProgress>,
        Option<&Hiding>,
    ), Without<CarriedBy>>();

    query
        .iter(world)
        .filter(|(_, pos, unit, faction, _, _, _, _, _, hiding)| {
            unit_would_be_visible_to_factions(
                pos.position(),
                unit.0,
                faction.0,
                hiding.is_some(),
                friendly_factions,
                fog_map,
                &detectors,
            )
        })
        .map(
            |(_, pos, unit, faction, hp, unit_id, fuel, ammo, capture_progress, hiding)| {
                let include_private_stats = friendly_factions.contains(&faction.0);
                let capture_progress = capture_progress.map(|progress| progress.value());
                VisibleUnit {
                    id: *unit_id,
                    unit_type: unit.0,
                    faction: faction.0,
                    position: pos.position(),
                    hp: hp.0,
                    fuel: if include_private_stats {
                        fuel.map(|f| f.0)
                    } else {
                        None
                    },
                    ammo: if include_private_stats {
                        ammo.map(|a| a.0)
                    } else {
                        None
                    },
                    capturing: capture_progress.is_some(),
                    capture_progress,
                    hiding: hiding.is_some(),
                }
            },
        )
        .collect()
}

fn collect_spectator_units(world: &mut World) -> Vec<VisibleUnit> {
    let mut query = world.query_filtered::<(
        &MapPosition,
        &Unit,
        &Faction,
        &GraphicalHp,
        &ServerUnitId,
        Option<&Fuel>,
        Option<&Ammo>,
        Option<&CaptureProgress>,
        Option<&Hiding>,
    ), Without<CarriedBy>>();

    query
        .iter(world)
        .map(
            |(pos, unit, faction, hp, unit_id, fuel, ammo, capture_progress, hiding)| {
                let capture_progress = capture_progress.map(|progress| progress.value());
                VisibleUnit {
                    id: *unit_id,
                    unit_type: unit.0,
                    faction: faction.0,
                    position: pos.position(),
                    hp: hp.0,
                    fuel: fuel.map(|f| f.0),
                    ammo: ammo.map(|a| a.0),
                    capturing: capture_progress.is_some(),
                    capture_progress,
                    hiding: hiding.is_some(),
                }
            },
        )
        .collect()
}

fn collect_visible_terrain(world: &World, fog_map: Option<&FogOfWarMap>) -> Vec<VisibleTerrain> {
    let game_map = world.resource::<GameMap>();
    let mut terrain = Vec::new();

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let pos = Position::new(x, y);
            if let Some(fog) = fog_map
                && fog.is_fogged(pos)
            {
                continue;
            }
            if let Some(t) = game_map.terrain_at(pos) {
                terrain.push(VisibleTerrain {
                    position: pos,
                    terrain: t,
                });
            }
        }
    }

    terrain
}

fn public_player_states(registry: &PlayerRegistry) -> Vec<PublicPlayerState> {
    registry
        .players()
        .iter()
        .map(|slot| PublicPlayerState {
            slot_index: slot.id.0,
            funds: slot.funds,
        })
        .collect()
}

/// Returns a map of all enemy units visible under the given fog map.
fn visible_enemy_units(
    world: &mut World,
    friendly_factions: &HashSet<PlayerFaction>,
    fog_map: Option<&FogOfWarMap>,
) -> HashMap<ServerUnitId, Entity> {
    let detectors = detector_positions(world, friendly_factions);
    let mut query = world.query_filtered::<(
        Entity,
        &MapPosition,
        &Faction,
        &Unit,
        &ServerUnitId,
        Option<&Hiding>,
    ), Without<CarriedBy>>();

    query
        .iter(world)
        .filter(|(_, _, faction, _, _, _)| !friendly_factions.contains(&faction.0))
        .filter(|(_, pos, faction, unit, _, hiding)| {
            unit_would_be_visible_to_factions(
                pos.position(),
                unit.0,
                faction.0,
                hiding.is_some(),
                friendly_factions,
                fog_map,
                &detectors,
            )
        })
        .map(|(entity, _, _, _, uid, _)| (*uid, entity))
        .collect()
}

fn entity_visible_to_factions(
    world: &mut World,
    entity: Entity,
    friendly_factions: &HashSet<PlayerFaction>,
    fog_map: Option<&FogOfWarMap>,
) -> bool {
    let detectors = detector_positions(world, friendly_factions);
    entity_visible_to_factions_with_detectors(world, entity, friendly_factions, fog_map, &detectors)
}

fn entity_visible_to_factions_with_detectors(
    world: &World,
    entity: Entity,
    friendly_factions: &HashSet<PlayerFaction>,
    fog_map: Option<&FogOfWarMap>,
    detectors: &[Position],
) -> bool {
    let entity_ref = world.entity(entity);
    if entity_ref.contains::<CarriedBy>() {
        return false;
    }
    let Some(pos) = entity_ref.get::<MapPosition>() else {
        return false;
    };
    let Some(unit) = entity_ref.get::<Unit>() else {
        return false;
    };
    let Some(faction) = entity_ref.get::<Faction>() else {
        return false;
    };

    unit_would_be_visible_to_factions(
        pos.position(),
        unit.0,
        faction.0,
        entity_ref.contains::<Hiding>(),
        friendly_factions,
        fog_map,
        detectors,
    )
}

fn detector_positions(
    world: &mut World,
    friendly_factions: &HashSet<PlayerFaction>,
) -> Vec<Position> {
    let mut query =
        world.query_filtered::<(&MapPosition, &Faction), (With<Unit>, Without<CarriedBy>)>();

    query
        .iter(world)
        .filter_map(|(pos, faction)| {
            friendly_factions
                .contains(&faction.0)
                .then_some(pos.position())
        })
        .collect()
}

fn unit_would_be_visible_to_factions(
    position: Position,
    unit_type: awbrn_types::Unit,
    faction: PlayerFaction,
    is_hiding: bool,
    friendly_factions: &HashSet<PlayerFaction>,
    fog_map: Option<&FogOfWarMap>,
    detector_positions: &[Position],
) -> bool {
    if friendly_factions.contains(&faction) {
        return true;
    }

    let is_air = unit_type.domain() == awbrn_types::UnitDomain::Air;
    let fog_visible = fog_map
        .map(|fog| fog.is_unit_visible(position, is_air))
        .unwrap_or(true);
    if !fog_visible {
        return false;
    }

    !is_hiding
        || detector_positions
            .iter()
            .any(|detector| detector.manhattan(&position) == 1)
}

/// Returns terrain tiles that transitioned from fogged to visible between pre and post fog.
fn terrain_diff(
    world: &World,
    pre_fog: Option<&FogOfWarMap>,
    post_fog: Option<&FogOfWarMap>,
) -> Vec<VisibleTerrain> {
    let game_map = world.resource::<GameMap>();
    let mut revealed = Vec::new();

    for y in 0..game_map.height() {
        for x in 0..game_map.width() {
            let pos = Position::new(x, y);
            let was_fogged = pre_fog.map(|f| f.is_fogged(pos)).unwrap_or(false);
            let now_visible = post_fog.map(|f| !f.is_fogged(pos)).unwrap_or(true);

            if was_fogged
                && now_visible
                && let Some(t) = game_map.terrain_at(pos)
            {
                revealed.push(VisibleTerrain {
                    position: pos,
                    terrain: t,
                });
            }
        }
    }

    revealed
}

fn tile_visible(fog_map: Option<&FogOfWarMap>, position: Position) -> bool {
    fog_map.map(|fog| !fog.is_fogged(position)).unwrap_or(true)
}

fn entity_to_visible_unit(
    world: &World,
    entity: Entity,
    friendly_factions: &HashSet<PlayerFaction>,
) -> Option<VisibleUnit> {
    let entity_ref = world.entity(entity);
    let pos = entity_ref.get::<MapPosition>()?;
    let unit = entity_ref.get::<Unit>()?;
    let faction = entity_ref.get::<Faction>()?;
    let hp = entity_ref.get::<GraphicalHp>()?;
    let unit_id = entity_ref.get::<ServerUnitId>().copied()?;

    let include_private_stats = friendly_factions.contains(&faction.0);

    Some(VisibleUnit {
        id: unit_id,
        unit_type: unit.0,
        faction: faction.0,
        position: pos.position(),
        hp: hp.0,
        fuel: if include_private_stats {
            entity_ref.get::<Fuel>().map(|f| f.0)
        } else {
            None
        },
        ammo: if include_private_stats {
            entity_ref.get::<Ammo>().map(|a| a.0)
        } else {
            None
        },
        capturing: entity_ref.contains::<CaptureProgress>(),
        capture_progress: entity_ref
            .get::<CaptureProgress>()
            .map(|progress| progress.value()),
        hiding: entity_ref.contains::<Hiding>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{GameSetup, PlayerSetup, initialize_server_world};
    use awbrn_map::AwbrnMap;
    use awbrn_types::{Co, Faction as TerrainFaction, GraphicalTerrain, Property, Weather};

    fn single_player_setup(width: usize, height: usize) -> GameSetup {
        GameSetup {
            map: AwbrnMap::new(width, height, GraphicalTerrain::Plain),
            players: vec![PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: None,
                starting_funds: 1000,
                co: Co::Andy,
            }],
            fog_enabled: true,
            rng_seed: 0,
        }
    }

    #[test]
    fn compute_fog_for_factions_uses_weather_and_power_boosts() {
        let mut world = initialize_server_world(single_player_setup(5, 1)).unwrap();
        world.spawn((
            MapPosition::new(0, 0),
            Faction(PlayerFaction::OrangeStar),
            Unit(awbrn_types::Unit::Infantry),
            VisionRange(2),
        ));

        let friendly = HashSet::from([PlayerFaction::OrangeStar]);

        let clear_fog = compute_fog_for_factions(&mut world, &friendly);
        assert!(
            !clear_fog.is_fogged(Position::new(2, 0)),
            "clear weather should keep base vision"
        );

        world.resource_mut::<CurrentWeather>().set(Weather::Rain);
        let rain_fog = compute_fog_for_factions(&mut world, &friendly);
        assert!(
            rain_fog.is_fogged(Position::new(2, 0)),
            "rain should reduce vision by one tile"
        );

        world
            .resource_mut::<PowerVisionBoosts>()
            .0
            .insert(PlayerFaction::OrangeStar, 1);
        let boosted_fog = compute_fog_for_factions(&mut world, &friendly);
        assert!(
            !boosted_fog.is_fogged(Position::new(2, 0)),
            "temporary power vision boosts should offset the weather penalty"
        );
    }

    #[test]
    fn captured_property_newly_revealed_by_move_is_not_terrain_changed() {
        let mut world = initialize_server_world(single_player_setup(3, 1)).unwrap();
        let from = Position::new(0, 0);
        let tile = Position::new(2, 0);
        let captured = GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::OrangeStar,
        )));
        world.resource_mut::<GameMap>().set_terrain(tile, captured);

        let unit_id = ServerUnitId(1);
        let entity = world
            .spawn((
                MapPosition::from(tile),
                Faction(PlayerFaction::OrangeStar),
                Unit(awbrn_types::Unit::Infantry),
                GraphicalHp(10),
                VisionRange(2),
                unit_id,
            ))
            .id();

        let pre_fog = FogOfWarMap::new(3, 1);
        let mut post_fog = FogOfWarMap::new(3, 1);
        post_fog.reveal(tile);
        let header = game_state_header(&world);

        let update = build_player_update(
            &mut world,
            PlayerId(0),
            &ApplyOutcome::PropertyCaptured {
                unit_id,
                entity,
                from,
                to: tile,
                path: vec![from, tile],
                faction: PlayerFaction::OrangeStar,
                tile,
                new_faction: PlayerFaction::OrangeStar,
            },
            Some(&pre_fog),
            Some(&post_fog),
            &header,
        );

        assert!(matches!(
            update.capture_event,
            Some(CaptureEvent::PropertyCaptured {
                tile: event_tile,
                new_faction: PlayerFaction::OrangeStar,
            }) if event_tile == tile
        ));
        assert!(update.terrain_changed.is_empty());
        assert_eq!(
            update
                .terrain_revealed
                .iter()
                .find(|terrain| terrain.position == tile)
                .map(|terrain| terrain.terrain),
            Some(captured)
        );
    }
}
