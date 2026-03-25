use std::collections::{HashMap, HashSet};

use awbrn_core::{
    AwbwGamePlayerId, Faction as CoreFaction, GraphicalTerrain, PlayerFaction, UnitDomain, Weather,
};
use awbrn_map::Position;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::core::map::TerrainTile;
use crate::core::units::{CarriedBy, VisionRange};
use crate::core::{Faction, MapPosition, Unit};
use crate::features::fog::{FogActive, FogOfWarMap, FriendlyFactions, TerrainFogProperties};
use crate::features::weather::CurrentWeather;
use crate::modes::replay::PowerVisionBoosts;
use crate::render::map::TerrainVisualOverride;

/// Whether the underlying replay uses fog of war.
/// Derived from the game's `fog` field at bootstrap.
#[derive(Resource, Default)]
pub struct ReplayFogEnabled(pub bool);

/// Selects whose perspective the fog is computed for.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub enum ReplayViewpoint {
    #[default]
    Spectator,
    /// Follow the active player each turn.
    ActivePlayer,
    /// Locked to a specific player.
    Player(AwbwGamePlayerId),
}

#[derive(Event, Debug, Default, Clone, Copy)]
pub struct ReplayFogDirty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplayKnowledgeKey {
    Player(AwbwGamePlayerId),
    Team(u8),
}

#[derive(Resource, Default)]
pub struct ReplayTerrainKnowledge {
    by_view: HashMap<ReplayKnowledgeKey, HashMap<Position, awbrn_core::GraphicalTerrain>>,
}

impl ReplayTerrainKnowledge {
    pub fn from_map_and_registry(
        game_map: &crate::core::GameMap,
        registry: &ReplayPlayerRegistry,
    ) -> Self {
        let terrain_by_position = (0..game_map.height())
            .flat_map(|y| {
                (0..game_map.width()).filter_map(move |x| {
                    let position = Position::new(x, y);
                    game_map
                        .terrain_at(position)
                        .map(|terrain| (position, terrain))
                })
            })
            .collect::<HashMap<_, _>>();

        let by_view = registry
            .knowledge_keys()
            .into_iter()
            .map(|key| (key, terrain_by_position.clone()))
            .collect();

        Self { by_view }
    }
}

/// Info about a single player for mapping faction → player → team.
#[derive(Debug, Clone)]
struct ReplayPlayerInfo {
    game_player_id: AwbwGamePlayerId,
    faction: PlayerFaction,
    /// Team letter from AWBW (e.g., b'A', b'B'). 0 means no team (FFA).
    team: u8,
}

/// Maps factions to players and teams. Built once at bootstrap from `AwbwGame.players`.
#[derive(Resource, Default)]
pub struct ReplayPlayerRegistry {
    players: Vec<ReplayPlayerInfo>,
}

impl ReplayPlayerRegistry {
    pub fn from_players(players: &[awbw_replay::game_models::AwbwPlayer], has_teams: bool) -> Self {
        let mut players = players.to_vec();
        players.sort_by_key(|p| p.order);

        let players = players
            .into_iter()
            .map(|p| ReplayPlayerInfo {
                game_player_id: p.id,
                faction: p.faction,
                team: if has_teams {
                    p.team.as_bytes().first().copied().unwrap_or(0)
                } else {
                    0
                },
            })
            .collect();
        Self { players }
    }

    /// Get the set of factions friendly to the given player (same team, or just
    /// the player's own faction if FFA / no team).
    pub fn friendly_factions_for_player(
        &self,
        player_id: AwbwGamePlayerId,
    ) -> HashSet<PlayerFaction> {
        let Some(player) = self.players.iter().find(|p| p.game_player_id == player_id) else {
            return HashSet::new();
        };

        if player.team == 0 {
            // FFA: only this player's faction
            return HashSet::from([player.faction]);
        }

        // Team game: all players on the same team
        self.players
            .iter()
            .filter(|p| p.team == player.team)
            .map(|p| p.faction)
            .collect()
    }

    /// Get the faction for a given player ID.
    pub fn faction_for_player(&self, player_id: AwbwGamePlayerId) -> Option<PlayerFaction> {
        self.players
            .iter()
            .find(|p| p.game_player_id == player_id)
            .map(|p| p.faction)
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    pub fn knowledge_key_for_player(
        &self,
        player_id: AwbwGamePlayerId,
    ) -> Option<ReplayKnowledgeKey> {
        self.players
            .iter()
            .find(|p| p.game_player_id == player_id)
            .map(|p| {
                if p.team == 0 {
                    ReplayKnowledgeKey::Player(player_id)
                } else {
                    ReplayKnowledgeKey::Team(p.team)
                }
            })
    }

    pub fn knowledge_keys(&self) -> Vec<ReplayKnowledgeKey> {
        let mut keys = Vec::new();
        for player in &self.players {
            let key = if player.team == 0 {
                ReplayKnowledgeKey::Player(player.game_player_id)
            } else {
                ReplayKnowledgeKey::Team(player.team)
            };
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    /// Get the player ID at the given turn-order index (0-based).
    pub fn player_id_at_index(&self, index: usize) -> Option<AwbwGamePlayerId> {
        self.players.get(index).map(|p| p.game_player_id)
    }
}

fn range_modifier_for_weather(weather: Weather) -> i32 {
    if weather == Weather::Rain { -1 } else { 0 }
}

struct FriendlyUnit {
    position: Position,
    vision: i32,
    is_air: bool,
}

fn collect_friendly_units(
    units: impl Iterator<Item = (Position, u32, PlayerFaction, awbrn_core::Unit)>,
    friendly_factions: &HashSet<PlayerFaction>,
    power_vision_boosts: Option<&HashMap<PlayerFaction, i32>>,
) -> Vec<FriendlyUnit> {
    units
        .filter(|(_, _, faction, _)| friendly_factions.contains(faction))
        .map(|(position, vision, faction, unit)| {
            let vision_boost = power_vision_boosts
                .and_then(|boosts| boosts.get(&faction))
                .copied()
                .unwrap_or_default();
            FriendlyUnit {
                position,
                vision: (vision as i32 + vision_boost).max(1),
                is_air: unit.domain() == UnitDomain::Air,
            }
        })
        .collect()
}

fn rebuild_fog_map(
    game_map: &crate::core::GameMap,
    friendly_factions: &HashSet<PlayerFaction>,
    friendly_units: &[FriendlyUnit],
    range_modifier: i32,
    fog_map: &mut FogOfWarMap,
) {
    let map_width = game_map.width();
    let map_height = game_map.height();
    let default_props = TerrainFogProperties {
        sight_increase: 0,
        limit: 0,
    };
    let mut friendly_building_positions = Vec::new();
    let mut terrain_lookup = vec![default_props; map_width * map_height];

    for y in 0..map_height {
        for x in 0..map_width {
            let pos = Position::new(x, y);
            let Some(terrain) = game_map.terrain_at(pos) else {
                continue;
            };

            terrain_lookup[y * map_width + x] =
                TerrainFogProperties::from_graphical_terrain(terrain);

            if let GraphicalTerrain::Property(prop) = terrain
                && let CoreFaction::Player(faction) = prop.faction()
                && friendly_factions.contains(&faction)
            {
                friendly_building_positions.push(pos);
            }
        }
    }

    let terrain_at = |pos: Position| -> TerrainFogProperties {
        if pos.x < map_width && pos.y < map_height {
            terrain_lookup[pos.y * map_width + pos.x]
        } else {
            default_props
        }
    };

    fog_map.reset(map_width, map_height);

    for pos in &friendly_building_positions {
        fog_map.reveal(*pos);
    }

    for unit in friendly_units {
        let mut effective_vision = (unit.vision + range_modifier).max(1);
        if !unit.is_air {
            effective_vision = (effective_vision + terrain_at(unit.position).sight_increase).max(1);
        }
        fog_map.apply_unit_vision(unit.position, effective_vision, &terrain_at);
    }
}

pub fn sync_viewpoint(world: &mut World) {
    let viewpoint = world.resource::<ReplayViewpoint>().clone();
    let fog_enabled = world.resource::<ReplayFogEnabled>().0;
    let active_player_id = world
        .resource::<super::state::ReplayState>()
        .active_player_id;
    let next_view = match viewpoint {
        ReplayViewpoint::Spectator => (false, HashSet::new()),
        ReplayViewpoint::ActivePlayer => active_player_id
            .map(|active_id| {
                (
                    fog_enabled,
                    world
                        .resource::<ReplayPlayerRegistry>()
                        .friendly_factions_for_player(active_id),
                )
            })
            .unwrap_or_else(|| (false, HashSet::new())),
        ReplayViewpoint::Player(id) => (
            fog_enabled,
            world
                .resource::<ReplayPlayerRegistry>()
                .friendly_factions_for_player(id),
        ),
    };

    let fog_changed = world.resource::<FogActive>().0 != next_view.0;
    let friendly_changed = world.resource::<FriendlyFactions>().0 != next_view.1;
    if !fog_changed && !friendly_changed {
        return;
    }

    world.resource_mut::<FogActive>().0 = next_view.0;
    world.resource_mut::<FriendlyFactions>().0 = next_view.1;
    world.trigger(ReplayFogDirty);
}

pub fn trigger_fog_recompute_on_weather_change(world: &mut World) {
    world.trigger(ReplayFogDirty);
}

#[derive(SystemParam)]
pub struct ReplayFogResources<'w, 's> {
    fog_active: Res<'w, FogActive>,
    friendly_factions: Res<'w, FriendlyFactions>,
    power_vision_boosts: Option<Res<'w, PowerVisionBoosts>>,
    weather: Option<Res<'w, CurrentWeather>>,
    game_map: Option<Res<'w, crate::core::GameMap>>,
    fog_map: ResMut<'w, FogOfWarMap>,
    viewpoint: Option<Res<'w, ReplayViewpoint>>,
    registry: Option<Res<'w, ReplayPlayerRegistry>>,
    replay_state: Option<Res<'w, super::state::ReplayState>>,
    knowledge: Option<ResMut<'w, ReplayTerrainKnowledge>>,
    marker: std::marker::PhantomData<&'s ()>,
}

#[derive(SystemParam)]
pub struct ReplayFogQueries<'w, 's> {
    unit_query: Query<
        'w,
        's,
        (
            &'static MapPosition,
            &'static VisionRange,
            &'static Faction,
            &'static Unit,
        ),
        Without<CarriedBy>,
    >,
    terrain_query: Query<
        'w,
        's,
        (
            Entity,
            &'static MapPosition,
            Option<&'static TerrainVisualOverride>,
        ),
        With<TerrainTile>,
    >,
}

pub fn on_replay_fog_dirty(
    _: On<ReplayFogDirty>,
    mut commands: Commands,
    mut resources: ReplayFogResources,
    queries: ReplayFogQueries,
) {
    let Some(game_map) = resources.game_map.as_deref() else {
        return;
    };
    let terrain_entities: Vec<(Entity, Position, Option<TerrainVisualOverride>)> = queries
        .terrain_query
        .iter()
        .map(|(entity, position, current_override)| {
            (entity, position.position(), current_override.copied())
        })
        .collect();

    if !resources.fog_active.0 {
        if let Some(mut knowledge) = resources.knowledge {
            refresh_terrain_knowledge_from_state(
                &mut commands,
                &terrain_entities,
                None,
                game_map,
                &resources.fog_map,
                &mut knowledge,
            );
        }
        return;
    }

    let range_modifier = range_modifier_for_weather(
        resources
            .weather
            .as_deref()
            .map_or(Weather::Clear, CurrentWeather::weather),
    );
    let friendly_units = collect_friendly_units(
        queries
            .unit_query
            .iter()
            .map(|(position, vision, faction, unit)| {
                (position.position(), vision.0, faction.0, unit.0)
            }),
        &resources.friendly_factions.0,
        resources
            .power_vision_boosts
            .as_deref()
            .map(|power_vision_boosts| &power_vision_boosts.0),
    );

    rebuild_fog_map(
        game_map,
        &resources.friendly_factions.0,
        &friendly_units,
        range_modifier,
        &mut resources.fog_map,
    );

    if let (Some(viewpoint), Some(registry), Some(replay_state), Some(mut knowledge)) = (
        resources.viewpoint,
        resources.registry,
        resources.replay_state,
        resources.knowledge,
    ) {
        let current_key = current_knowledge_key_from_state(
            resources.fog_active.0,
            &viewpoint,
            &replay_state,
            &registry,
        );
        refresh_terrain_knowledge_from_state(
            &mut commands,
            &terrain_entities,
            current_key,
            game_map,
            &resources.fog_map,
            &mut knowledge,
        );
    }
}

fn current_knowledge_key_from_state(
    fog_active: bool,
    viewpoint: &ReplayViewpoint,
    replay_state: &super::state::ReplayState,
    registry: &ReplayPlayerRegistry,
) -> Option<ReplayKnowledgeKey> {
    if !fog_active {
        return None;
    }

    match viewpoint {
        ReplayViewpoint::Spectator => None,
        ReplayViewpoint::ActivePlayer => replay_state
            .active_player_id
            .and_then(|id| registry.knowledge_key_for_player(id)),
        ReplayViewpoint::Player(id) => registry.knowledge_key_for_player(*id),
    }
}

fn refresh_terrain_knowledge_from_state(
    commands: &mut Commands,
    terrain_entities: &[(Entity, Position, Option<TerrainVisualOverride>)],
    current_key: Option<ReplayKnowledgeKey>,
    game_map: &crate::core::GameMap,
    fog_map: &FogOfWarMap,
    knowledge: &mut ReplayTerrainKnowledge,
) {
    if current_key.is_none() {
        let desired = TerrainVisualOverride(None);
        for (entity, _, current) in terrain_entities {
            if *current != Some(desired) {
                commands.entity(*entity).insert(desired);
            }
        }
        return;
    }

    let current_key = current_key.unwrap();
    let Some(known_terrain) = knowledge.by_view.get_mut(&current_key) else {
        return;
    };

    // First pass: update knowledge for revealed tiles
    for (_, position, _) in terrain_entities {
        if !fog_map.is_fogged(*position)
            && let Some(actual) = game_map.terrain_at(*position)
        {
            known_terrain.insert(*position, actual);
        }
    }

    // Second pass: set visual overrides only when changed
    for (entity, position, current_override) in terrain_entities {
        let is_fogged = fog_map.is_fogged(*position);
        let visual_override = if is_fogged {
            let actual_terrain = game_map.terrain_at(*position);
            match (known_terrain.get(position).copied(), actual_terrain) {
                (Some(known), Some(actual)) if known != actual => Some(known),
                _ => None,
            }
        } else {
            None
        };
        let desired = TerrainVisualOverride(visual_override);
        if *current_override != Some(desired) {
            commands.entity(*entity).insert(desired);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::map::TerrainTile;
    use crate::core::units::VisionRange;
    use crate::core::{Faction, GameMap, MapPosition, Unit};
    use crate::features::weather::CurrentWeather;
    use crate::modes::replay::state::ReplayState;
    use crate::render::map::TerrainVisualOverride;
    use awbrn_core::{Faction as TerrainFaction, Property, Weather};
    use awbrn_map::AwbrnMap;
    use bevy::prelude::App;

    fn test_player(
        id: u32,
        faction: PlayerFaction,
        order: u32,
        team: &str,
    ) -> awbw_replay::game_models::AwbwPlayer {
        awbw_replay::game_models::AwbwPlayer {
            id: AwbwGamePlayerId::new(id),
            users_id: awbrn_core::AwbwPlayerId::new(id * 100),
            games_id: awbrn_core::AwbwGameId::new(1),
            faction,
            co_id: 1,
            funds: 0,
            turn: None,
            email: None,
            uniq_id: None,
            eliminated: false,
            last_read: String::new(),
            last_read_broadcasts: None,
            emailpress: None,
            signature: None,
            co_power: 0,
            co_power_on: awbw_replay::game_models::CoPower::None,
            order,
            accept_draw: false,
            co_max_power: 0,
            co_max_spower: 0,
            co_image: None,
            team: team.to_string(),
            aet_count: 0,
            turn_start: String::new(),
            turn_clock: 0,
            tags_co_id: None,
            tags_co_power: None,
            tags_co_max_power: None,
            tags_co_max_spower: None,
            interface: false,
        }
    }

    #[test]
    fn registry_ffa_returns_single_faction() {
        let players = vec![
            test_player(1, PlayerFaction::OrangeStar, 0, ""),
            test_player(2, PlayerFaction::BlueMoon, 1, ""),
        ];

        let registry = ReplayPlayerRegistry::from_players(&players, false);
        let friendly = registry.friendly_factions_for_player(AwbwGamePlayerId::new(1));
        assert_eq!(friendly, HashSet::from([PlayerFaction::OrangeStar]));
        assert!(!friendly.contains(&PlayerFaction::BlueMoon));
    }

    #[test]
    fn registry_team_returns_all_team_factions() {
        let players = vec![
            test_player(1, PlayerFaction::OrangeStar, 0, "A"),
            test_player(2, PlayerFaction::GreenEarth, 1, "A"),
            test_player(3, PlayerFaction::BlueMoon, 2, "B"),
        ];

        let registry = ReplayPlayerRegistry::from_players(&players, true);
        let friendly = registry.friendly_factions_for_player(AwbwGamePlayerId::new(1));
        assert!(friendly.contains(&PlayerFaction::OrangeStar));
        assert!(friendly.contains(&PlayerFaction::GreenEarth));
        assert!(!friendly.contains(&PlayerFaction::BlueMoon));
    }

    #[test]
    fn registry_player_indices_follow_turn_order() {
        let players = vec![
            test_player(10, PlayerFaction::OrangeStar, 2, ""),
            test_player(20, PlayerFaction::BlueMoon, 0, ""),
            test_player(30, PlayerFaction::GreenEarth, 1, ""),
        ];

        let registry = ReplayPlayerRegistry::from_players(&players, false);
        assert_eq!(
            registry.player_id_at_index(0),
            Some(AwbwGamePlayerId::new(20))
        );
        assert_eq!(
            registry.player_id_at_index(1),
            Some(AwbwGamePlayerId::new(30))
        );
        assert_eq!(
            registry.player_id_at_index(2),
            Some(AwbwGamePlayerId::new(10))
        );
    }

    #[test]
    fn non_team_games_ignore_numeric_team_strings() {
        let players = vec![
            test_player(3252378, PlayerFaction::OrangeStar, 6, "3252378"),
            test_player(3252473, PlayerFaction::BlueMoon, 20, "3252473"),
        ];

        let registry = ReplayPlayerRegistry::from_players(&players, false);
        assert_eq!(
            registry.friendly_factions_for_player(AwbwGamePlayerId::new(3252378)),
            HashSet::from([PlayerFaction::OrangeStar])
        );
        assert_eq!(
            registry.friendly_factions_for_player(AwbwGamePlayerId::new(3252473)),
            HashSet::from([PlayerFaction::BlueMoon])
        );
    }

    #[test]
    fn registry_unknown_player_returns_empty() {
        let registry = ReplayPlayerRegistry::default();
        let friendly = registry.friendly_factions_for_player(AwbwGamePlayerId::new(999));
        assert!(friendly.is_empty());
    }

    #[test]
    fn weather_changes_recompute_replay_fog() {
        let mut app = fog_test_app();
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::new(5, 1, GraphicalTerrain::Plain));
        app.world_mut().resource_mut::<FogActive>().0 = true;
        app.world_mut().resource_mut::<FriendlyFactions>().0 =
            HashSet::from([PlayerFaction::OrangeStar]);
        app.world_mut().spawn((
            MapPosition::new(0, 0),
            Faction(PlayerFaction::OrangeStar),
            Unit(awbrn_core::Unit::Infantry),
            VisionRange(2),
        ));

        app.world_mut().trigger(ReplayFogDirty);
        app.update();
        assert!(
            !app.world()
                .resource::<crate::features::fog::FogOfWarMap>()
                .is_fogged(Position::new(2, 0)),
            "clear weather should keep base vision"
        );

        app.world_mut()
            .resource_mut::<CurrentWeather>()
            .set(Weather::Rain);
        app.world_mut().trigger(ReplayFogDirty);
        app.update();

        let fog_map = app.world().resource::<crate::features::fog::FogOfWarMap>();
        assert!(
            fog_map.is_fogged(Position::new(2, 0)),
            "rain should reduce the visible range by one tile"
        );
        assert!(!fog_map.is_fogged(Position::new(1, 0)));
    }

    #[test]
    fn temporary_power_vision_boosts_extend_replay_fog_until_turn_end() {
        let mut app = fog_test_app();
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::new(5, 1, GraphicalTerrain::Plain));
        app.world_mut().resource_mut::<FogActive>().0 = true;
        app.world_mut().resource_mut::<FriendlyFactions>().0 =
            HashSet::from([PlayerFaction::OrangeStar]);
        app.world_mut().spawn((
            MapPosition::new(0, 0),
            Faction(PlayerFaction::OrangeStar),
            Unit(awbrn_core::Unit::Infantry),
            VisionRange(1),
        ));
        app.world_mut()
            .resource_mut::<PowerVisionBoosts>()
            .0
            .insert(PlayerFaction::OrangeStar, 1);

        app.world_mut().trigger(ReplayFogDirty);

        assert!(
            !app.world()
                .resource::<crate::features::fog::FogOfWarMap>()
                .is_fogged(Position::new(2, 0)),
            "temporary vision boosts should be applied while the power is active"
        );

        app.world_mut()
            .resource_mut::<PowerVisionBoosts>()
            .0
            .clear();
        app.world_mut().trigger(ReplayFogDirty);

        assert!(
            app.world()
                .resource::<crate::features::fog::FogOfWarMap>()
                .is_fogged(Position::new(2, 0)),
            "clearing the temporary boost should restore base vision"
        );
    }

    #[test]
    fn hidden_buildings_keep_last_known_owner_until_visible() {
        let mut app = fog_test_app();
        let player_id = AwbwGamePlayerId::new(1);
        let actual_terrain = GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::BlueMoon,
        )));
        let known_terrain = GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::OrangeStar,
        )));
        let players = vec![test_player(1, PlayerFaction::OrangeStar, 0, "")];
        let registry = ReplayPlayerRegistry::from_players(&players, false);

        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::new(1, 1, known_terrain));
        let terrain_knowledge = {
            let game_map = app.world().resource::<GameMap>();
            ReplayTerrainKnowledge::from_map_and_registry(game_map, &registry)
        };

        app.world_mut().insert_resource(registry);
        app.world_mut().insert_resource(terrain_knowledge);
        app.world_mut()
            .insert_resource(ReplayViewpoint::Player(player_id));
        app.world_mut().insert_resource(ReplayState {
            active_player_id: Some(player_id),
            ..ReplayState::default()
        });
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::new(1, 1, actual_terrain));
        app.world_mut().resource_mut::<FogActive>().0 = true;
        app.world_mut().resource_mut::<FriendlyFactions>().0 =
            HashSet::from([PlayerFaction::OrangeStar]);

        let entity = app
            .world_mut()
            .spawn((
                MapPosition::new(0, 0),
                TerrainTile {
                    terrain: actual_terrain,
                },
                TerrainVisualOverride(None),
            ))
            .id();

        app.world_mut().trigger(ReplayFogDirty);
        app.update();

        assert_eq!(
            app.world().entity(entity).get::<TerrainVisualOverride>(),
            Some(&TerrainVisualOverride(Some(known_terrain)))
        );

        app.world_mut().spawn((
            MapPosition::new(0, 0),
            Faction(PlayerFaction::OrangeStar),
            Unit(awbrn_core::Unit::Infantry),
            VisionRange(1),
        ));
        app.world_mut().trigger(ReplayFogDirty);
        app.update();

        assert_eq!(
            app.world().entity(entity).get::<TerrainVisualOverride>(),
            Some(&TerrainVisualOverride(None))
        );
    }

    #[test]
    fn air_unit_classification() {
        assert_eq!(awbrn_core::Unit::BCopter.domain(), UnitDomain::Air);
        assert_eq!(awbrn_core::Unit::Bomber.domain(), UnitDomain::Air);
        assert_eq!(awbrn_core::Unit::Fighter.domain(), UnitDomain::Air);
        assert_eq!(awbrn_core::Unit::TCopter.domain(), UnitDomain::Air);
        assert_eq!(awbrn_core::Unit::BlackBomb.domain(), UnitDomain::Air);
        assert_eq!(awbrn_core::Unit::Stealth.domain(), UnitDomain::Air);

        assert_eq!(awbrn_core::Unit::Infantry.domain(), UnitDomain::Ground);
        assert_eq!(awbrn_core::Unit::Tank.domain(), UnitDomain::Ground);
        assert_eq!(awbrn_core::Unit::Sub.domain(), UnitDomain::Sea);
        assert_eq!(awbrn_core::Unit::Battleship.domain(), UnitDomain::Sea);
    }

    fn fog_test_app() -> App {
        let mut app = App::new();
        app.init_resource::<GameMap>();
        app.init_resource::<crate::features::fog::FogOfWarMap>();
        app.init_resource::<FogActive>();
        app.init_resource::<FriendlyFactions>();
        app.init_resource::<PowerVisionBoosts>();
        app.init_resource::<ReplayTerrainKnowledge>();
        app.init_resource::<ReplayViewpoint>();
        app.init_resource::<ReplayPlayerRegistry>();
        app.insert_resource(ReplayState::default());
        app.insert_resource(CurrentWeather::default());
        app.add_observer(on_replay_fog_dirty);
        app
    }
}
