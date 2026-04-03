use std::collections::{HashMap, HashSet};

use crate::apply::ApplyOutcome;
use crate::player::{PlayerId, PlayerRegistry};
use crate::state::{ServerGameState, TurnPhase};
use crate::unit_id::ServerUnitId;
use awbrn_game::MapPosition;
use awbrn_game::replay::{PowerVisionBoosts, range_modifier_for_weather};
use awbrn_game::world::{
    Ammo, Capturing, CarriedBy, CurrentWeather, Faction, FogOfWarMap, Fuel, GameMap, GraphicalHp,
    Unit, VisionRange, collect_friendly_units, rebuild_fog_map,
};
use awbrn_map::Position;
use awbrn_types::PlayerFaction;
use bevy::prelude::*;

/// Header with the current game state, included in every response.
#[derive(Debug, Clone)]
pub struct GameStateHeader {
    pub day: u32,
    pub active_player: PlayerId,
    pub phase: TurnPhase,
}

/// A unit as visible to a specific player.
#[derive(Debug, Clone)]
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
}

/// A terrain tile as visible to a specific player.
#[derive(Debug, Clone)]
pub struct VisibleTerrain {
    pub position: Position,
    pub terrain: awbrn_types::GraphicalTerrain,
}

/// Full snapshot of what a player can see (for initial load / reconnection).
#[derive(Debug, Clone)]
pub struct PlayerView {
    pub state: GameStateHeader,
    pub my_funds: u32,
    pub units: Vec<VisibleUnit>,
    pub terrain: Vec<VisibleTerrain>,
}

/// Information about a unit that moved.
#[derive(Debug, Clone)]
pub struct UnitMoved {
    pub id: ServerUnitId,
    pub path: Vec<Position>,
    pub from: Position,
    pub to: Position,
}

/// Information about a turn change.
#[derive(Debug, Clone)]
pub struct TurnChange {
    pub new_active_player: PlayerId,
    pub new_day: Option<u32>,
}

/// Incremental update for a specific player after a command.
#[derive(Debug, Clone)]
pub struct PlayerUpdate {
    /// Units newly revealed to this player.
    pub units_revealed: Vec<VisibleUnit>,
    /// Units that moved within this player's vision.
    pub units_moved: Vec<UnitMoved>,
    /// Units that disappeared from this player's vision.
    pub units_removed: Vec<ServerUnitId>,
    /// Terrain tiles newly revealed to this player (fog lifted).
    pub terrain_revealed: Vec<VisibleTerrain>,
    /// Turn change information (if EndTurn was the command).
    pub turn_change: Option<TurnChange>,
    /// Current game state.
    pub state: GameStateHeader,
}

/// The result of applying a command, containing per-player updates.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub updates: Vec<(PlayerId, PlayerUpdate)>,
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
        units,
        terrain,
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

    CommandResult { updates }
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
                turn_change: None,
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
            turn_change: Some(TurnChange {
                new_active_player: *new_active_player,
                new_day: *new_day,
            }),
            state: header.clone(),
        },
    }
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
    // Include ServerUnitId, Fuel, Ammo, Capturing in the query to avoid separate entity lookups.
    let mut query = world.query_filtered::<(
        Entity,
        &MapPosition,
        &Unit,
        &Faction,
        &GraphicalHp,
        &ServerUnitId,
        Option<&Fuel>,
        Option<&Ammo>,
        Has<Capturing>,
    ), Without<CarriedBy>>();

    query
        .iter(world)
        .filter(|(_, pos, unit, _, _, _, _, _, _)| {
            if let Some(fog) = fog_map {
                let is_air = unit.0.domain() == awbrn_types::UnitDomain::Air;
                fog.is_unit_visible(pos.position(), is_air)
            } else {
                true
            }
        })
        .map(
            |(_, pos, unit, faction, hp, unit_id, fuel, ammo, capturing)| {
                let include_private_stats = friendly_factions.contains(&faction.0);
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
                    capturing,
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

/// Returns a map of all enemy units visible under the given fog map.
fn visible_enemy_units(
    world: &mut World,
    friendly_factions: &HashSet<PlayerFaction>,
    fog_map: Option<&FogOfWarMap>,
) -> HashMap<ServerUnitId, Entity> {
    let mut query = world.query_filtered::<
        (Entity, &MapPosition, &Faction, &Unit, &ServerUnitId),
        Without<CarriedBy>,
    >();

    query
        .iter(world)
        .filter(|(_, _, faction, _, _)| !friendly_factions.contains(&faction.0))
        .filter(|(_, pos, _, unit, _)| {
            if let Some(fog) = fog_map {
                let is_air = unit.0.domain() == awbrn_types::UnitDomain::Air;
                fog.is_unit_visible(pos.position(), is_air)
            } else {
                true
            }
        })
        .map(|(entity, _, _, _, uid)| (*uid, entity))
        .collect()
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
        capturing: entity_ref.contains::<Capturing>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{GameSetup, PlayerSetup, initialize_server_world};
    use awbrn_map::AwbrnMap;
    use awbrn_types::{GraphicalTerrain, Weather};

    fn single_player_setup(width: usize, height: usize) -> GameSetup {
        GameSetup {
            map: AwbrnMap::new(width, height, GraphicalTerrain::Plain),
            players: vec![PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: None,
                starting_funds: 1000,
                co_id: None,
            }],
            fog_enabled: true,
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
}
