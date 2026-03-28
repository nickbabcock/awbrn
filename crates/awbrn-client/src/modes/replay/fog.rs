use awbrn_game::MapPosition;
pub use awbrn_game::replay::{
    FriendlyUnit, ReplayFogDirty, ReplayFogEnabled, ReplayKnowledgeKey, ReplayPlayerRegistry,
    ReplayTerrainKnowledge, ReplayViewpoint, collect_friendly_units, range_modifier_for_weather,
    rebuild_fog_map,
};
use awbrn_game::replay::{PowerVisionBoosts, ReplayState};
use awbrn_game::world::{
    CarriedBy, Faction, FogActive, FogOfWarMap, FriendlyFactions, GameMap, TerrainTile, Unit,
    VisionRange,
};
use awbrn_map::Position;
use awbrn_types::Weather;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::features::weather::CurrentWeather;
use crate::render::map::TerrainVisualOverride;

#[derive(SystemParam)]
pub struct ReplayFogResources<'w, 's> {
    fog_active: Res<'w, FogActive>,
    friendly_factions: Res<'w, FriendlyFactions>,
    power_vision_boosts: Option<Res<'w, PowerVisionBoosts>>,
    weather: Option<Res<'w, CurrentWeather>>,
    game_map: Option<Res<'w, GameMap>>,
    fog_map: ResMut<'w, FogOfWarMap>,
    viewpoint: Option<Res<'w, ReplayViewpoint>>,
    registry: Option<Res<'w, ReplayPlayerRegistry>>,
    replay_state: Option<Res<'w, ReplayState>>,
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
    replay_state: &ReplayState,
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
    game_map: &GameMap,
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

    for (_, position, _) in terrain_entities {
        if !fog_map.is_fogged(*position)
            && let Some(actual) = game_map.terrain_at(*position)
        {
            known_terrain.insert(*position, actual);
        }
    }

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
    use crate::features::weather::CurrentWeather;
    use crate::render::map::TerrainVisualOverride;
    use awbrn_game::MapPosition;
    use awbrn_game::world::{Faction, GameMap, TerrainTile, Unit, VisionRange};
    use awbrn_map::AwbrnMap;
    use awbrn_types::{AwbwGamePlayerId, PlayerFaction, Property, UnitDomain, Weather};
    use bevy::prelude::App;

    #[test]
    fn weather_changes_recompute_replay_fog() {
        let mut app = fog_test_app();
        app.world_mut().resource_mut::<GameMap>().set(AwbrnMap::new(
            5,
            1,
            awbrn_types::GraphicalTerrain::Plain,
        ));
        app.world_mut().resource_mut::<FogActive>().0 = true;
        app.world_mut().resource_mut::<FriendlyFactions>().0 =
            std::collections::HashSet::from([PlayerFaction::OrangeStar]);
        app.world_mut().spawn((
            MapPosition::new(0, 0),
            Faction(PlayerFaction::OrangeStar),
            Unit(awbrn_types::Unit::Infantry),
            VisionRange(2),
        ));

        app.world_mut().trigger(ReplayFogDirty);
        app.update();
        assert!(
            !app.world()
                .resource::<FogOfWarMap>()
                .is_fogged(awbrn_map::Position::new(2, 0)),
            "clear weather should keep base vision"
        );

        app.world_mut()
            .resource_mut::<CurrentWeather>()
            .set(Weather::Rain);
        app.world_mut().trigger(ReplayFogDirty);
        app.update();

        let fog_map = app.world().resource::<FogOfWarMap>();
        assert!(
            fog_map.is_fogged(awbrn_map::Position::new(2, 0)),
            "rain should reduce the visible range by one tile"
        );
        assert!(!fog_map.is_fogged(awbrn_map::Position::new(1, 0)));
    }

    #[test]
    fn temporary_power_vision_boosts_extend_replay_fog_until_turn_end() {
        let mut app = fog_test_app();
        app.world_mut().resource_mut::<GameMap>().set(AwbrnMap::new(
            5,
            1,
            awbrn_types::GraphicalTerrain::Plain,
        ));
        app.world_mut().resource_mut::<FogActive>().0 = true;
        app.world_mut().resource_mut::<FriendlyFactions>().0 =
            std::collections::HashSet::from([PlayerFaction::OrangeStar]);
        app.world_mut().spawn((
            MapPosition::new(0, 0),
            Faction(PlayerFaction::OrangeStar),
            Unit(awbrn_types::Unit::Infantry),
            VisionRange(1),
        ));
        app.world_mut()
            .resource_mut::<PowerVisionBoosts>()
            .0
            .insert(PlayerFaction::OrangeStar, 1);

        app.world_mut().trigger(ReplayFogDirty);

        assert!(
            !app.world()
                .resource::<FogOfWarMap>()
                .is_fogged(awbrn_map::Position::new(2, 0)),
            "temporary vision boosts should be applied while the power is active"
        );

        app.world_mut()
            .resource_mut::<PowerVisionBoosts>()
            .0
            .clear();
        app.world_mut().trigger(ReplayFogDirty);

        assert!(
            app.world()
                .resource::<FogOfWarMap>()
                .is_fogged(awbrn_map::Position::new(2, 0)),
            "clearing the temporary boost should restore base vision"
        );
    }

    #[test]
    fn hidden_buildings_keep_last_known_owner_until_visible() {
        let mut app = fog_test_app();
        let player_id = AwbwGamePlayerId::new(1);
        let actual_terrain = awbrn_types::GraphicalTerrain::Property(Property::City(
            awbrn_types::Faction::Player(PlayerFaction::BlueMoon),
        ));
        let known_terrain = awbrn_types::GraphicalTerrain::Property(Property::City(
            awbrn_types::Faction::Player(PlayerFaction::OrangeStar),
        ));
        let mut registry = ReplayPlayerRegistry::default();
        registry.add_player(player_id, PlayerFaction::OrangeStar, 0);

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
            std::collections::HashSet::from([PlayerFaction::OrangeStar]);

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
            Unit(awbrn_types::Unit::Infantry),
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
        assert_eq!(awbrn_types::Unit::BCopter.domain(), UnitDomain::Air);
        assert_eq!(awbrn_types::Unit::Bomber.domain(), UnitDomain::Air);
        assert_eq!(awbrn_types::Unit::Infantry.domain(), UnitDomain::Ground);
        assert_eq!(awbrn_types::Unit::Sub.domain(), UnitDomain::Sea);
    }

    fn fog_test_app() -> App {
        let mut app = App::new();
        app.init_resource::<GameMap>();
        app.init_resource::<FogOfWarMap>();
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
