use crate::core::AppState;
use crate::features::player_display::{
    PlayerDisplayFactionOverrides, display_faction_for_actual_faction,
};
use crate::features::player_roster::PlayerRosterConfig;
use awbrn_game::MapPosition;
use awbrn_game::replay::{
    AwbwUnitId, ReplayKnowledgeKey, ReplayPlayerRegistry, ReplayState, ReplayTerrainKnowledge,
    ReplayViewpoint,
};
use awbrn_game::world::{
    CaptureProgress, CarriedBy, Faction, GraphicalHp, HasCargo, Hiding, TerrainTile, Unit,
    UnitActive, ViewerVisibility,
};
use awbrn_map::Position;
use awbrn_types::{Faction as TerrainFaction, GraphicalTerrain, Property, PropertyKind};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ClientProjectionSet {
    RebuildKnowledge,
    DeriveVisibility,
    DerivePresentation,
    SyncRender,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectedUnitOverlayFlags {
    pub health: Option<GraphicalHp>,
    pub capturing: bool,
    pub cargo: bool,
    pub dive: bool,
    pub low_ammo: bool,
    pub low_fuel: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectedUnitRenderState {
    pub unit: Unit,
    pub faction: Faction,
    pub visible: bool,
    pub active: bool,
    pub overlays: ProjectedUnitOverlayFlags,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectedTerrainRenderState(pub GraphicalTerrain);

#[derive(SystemParam)]
pub(crate) struct UnitProjectionResources<'w> {
    visibility: Res<'w, ViewerVisibility>,
    player_roster: Option<Res<'w, PlayerRosterConfig>>,
    display_faction_overrides: Option<Res<'w, PlayerDisplayFactionOverrides>>,
    registry: Option<Res<'w, ReplayPlayerRegistry>>,
    replay_state: Option<Res<'w, ReplayState>>,
}

#[derive(SystemParam)]
pub(crate) struct TerrainProjectionResources<'w> {
    visibility: Res<'w, ViewerVisibility>,
    player_roster: Option<Res<'w, PlayerRosterConfig>>,
    display_faction_overrides: Option<Res<'w, PlayerDisplayFactionOverrides>>,
    viewpoint: Option<Res<'w, ReplayViewpoint>>,
    registry: Option<Res<'w, ReplayPlayerRegistry>>,
    replay_state: Option<Res<'w, ReplayState>>,
    knowledge: Option<Res<'w, ReplayTerrainKnowledge>>,
}

type UnitProjectionItem<'a> = (
    Entity,
    &'a Unit,
    &'a Faction,
    Option<&'a AwbwUnitId>,
    Option<Ref<'a, UnitActive>>,
    Has<CaptureProgress>,
    Has<HasCargo>,
    Has<Hiding>,
    Option<&'a GraphicalHp>,
    Has<CarriedBy>,
    Option<&'a ProjectedUnitRenderState>,
);

type TerrainProjectionItem<'a> = (
    Entity,
    &'a TerrainTile,
    &'a MapPosition,
    Option<&'a ProjectedTerrainRenderState>,
);

fn current_knowledge_key(
    fog_active: bool,
    viewpoint: Option<&ReplayViewpoint>,
    replay_state: Option<&ReplayState>,
    registry: Option<&ReplayPlayerRegistry>,
) -> Option<ReplayKnowledgeKey> {
    if !fog_active {
        return None;
    }

    let (Some(viewpoint), Some(replay_state), Some(registry)) = (viewpoint, replay_state, registry)
    else {
        return None;
    };

    match viewpoint {
        ReplayViewpoint::Spectator => None,
        ReplayViewpoint::ActivePlayer => replay_state
            .active_player_id
            .and_then(|id| registry.knowledge_key_for_player(id)),
        ReplayViewpoint::Player(id) => registry.knowledge_key_for_player(*id),
    }
}

/// The faction whose turn it is, if the presentation knows one.
fn active_faction(
    replay_state: Option<&ReplayState>,
    registry: Option<&ReplayPlayerRegistry>,
) -> Option<awbrn_types::PlayerFaction> {
    let player = replay_state?.active_player_id?;
    registry?.faction_for_player(player)
}

/// Whether a unit is drawn ready rather than greyed out.
///
/// Only the player whose turn it is can spend units, so a waiting unit of any
/// other player says nothing to the viewer. AWBW greys out the current
/// player's spent units alone, and the presentation follows it. Without a
/// known active faction every unit falls back to its own `UnitActive`.
fn unit_is_active(
    is_active: bool,
    faction: Faction,
    active_faction: Option<awbrn_types::PlayerFaction>,
) -> bool {
    match active_faction {
        Some(active) => is_active || faction.0 != active,
        None => is_active,
    }
}

fn projected_health(hp: Option<&GraphicalHp>) -> Option<GraphicalHp> {
    hp.copied()
        .filter(|hp| !hp.is_full_health() && !hp.is_destroyed())
}

/// Whether the viewer may see this unit.
///
/// The selected recipient's projection already applied every rule in
/// `spec/semantics/fog.md` — range, terrain, weather, detection and
/// concealment — so this is a lookup, not a second decision. Carried units are
/// never drawn, and a unit the presentation spawned without a semantic id
/// cannot be named by a projection.
fn unit_visible_to_viewer(
    resources: &UnitProjectionResources,
    unit_id: Option<&AwbwUnitId>,
    is_carried: bool,
) -> bool {
    if is_carried {
        return false;
    }

    match unit_id {
        Some(unit_id) => resources.visibility.unit_visible(unit_id.0),
        None => !resources.visibility.fog_active(),
    }
}

fn terrain_for_viewer(
    visibility: &ViewerVisibility,
    knowledge: Option<&ReplayTerrainKnowledge>,
    knowledge_key: Option<ReplayKnowledgeKey>,
    position: Position,
    actual: GraphicalTerrain,
) -> GraphicalTerrain {
    if !visibility.is_fogged(position) {
        return actual;
    }

    let Some(knowledge_key) = knowledge_key else {
        return actual;
    };

    knowledge
        .and_then(|knowledge| knowledge.by_view.get(&knowledge_key))
        .and_then(|known| known.get(&position).copied())
        .unwrap_or(actual)
}

fn property_with_display_faction(
    property: Property,
    display_faction: awbrn_types::PlayerFaction,
) -> Property {
    match property.kind() {
        PropertyKind::Airport => Property::Airport(TerrainFaction::Player(display_faction)),
        PropertyKind::Base => Property::Base(TerrainFaction::Player(display_faction)),
        PropertyKind::City => Property::City(TerrainFaction::Player(display_faction)),
        PropertyKind::ComTower => Property::ComTower(TerrainFaction::Player(display_faction)),
        PropertyKind::HQ => Property::HQ(display_faction),
        PropertyKind::Lab => Property::Lab(TerrainFaction::Player(display_faction)),
        PropertyKind::Port => Property::Port(TerrainFaction::Player(display_faction)),
    }
}

fn terrain_with_display_faction(
    terrain: GraphicalTerrain,
    player_roster: Option<&PlayerRosterConfig>,
    display_faction_overrides: Option<&PlayerDisplayFactionOverrides>,
) -> GraphicalTerrain {
    let GraphicalTerrain::Property(property) = terrain else {
        return terrain;
    };
    let TerrainFaction::Player(actual_faction) = property.faction() else {
        return terrain;
    };

    let display_faction = display_faction_for_actual_faction(
        player_roster,
        display_faction_overrides,
        actual_faction,
    );
    if display_faction == actual_faction {
        return terrain;
    }

    GraphicalTerrain::Property(property_with_display_faction(property, display_faction))
}

pub(crate) fn project_unit_render_state(
    mut commands: Commands,
    resources: UnitProjectionResources,
    units: Query<UnitProjectionItem<'_>, With<Unit>>,
) {
    let active_faction = active_faction(
        resources.replay_state.as_deref(),
        resources.registry.as_deref(),
    );

    for (
        entity,
        unit,
        faction,
        unit_id,
        unit_active,
        is_capturing,
        has_cargo,
        is_hiding,
        hp,
        is_carried,
        current,
    ) in &units
    {
        let is_active = unit_active.is_some();
        let force_refresh = unit_active
            .as_ref()
            .is_some_and(|unit_active| unit_active.is_changed());
        let display_faction = display_faction_for_actual_faction(
            resources.player_roster.as_deref(),
            resources.display_faction_overrides.as_deref(),
            faction.0,
        );
        let next = ProjectedUnitRenderState {
            unit: *unit,
            faction: Faction(display_faction),
            visible: unit_visible_to_viewer(&resources, unit_id, is_carried),
            active: unit_is_active(is_active, *faction, active_faction),
            overlays: ProjectedUnitOverlayFlags {
                health: projected_health(hp),
                capturing: is_capturing,
                cargo: has_cargo,
                dive: is_hiding,
                low_ammo: false,
                low_fuel: false,
            },
        };

        if force_refresh || current.copied() != Some(next) {
            commands.entity(entity).insert(next);
        }
    }
}

pub(crate) fn project_terrain_render_state(
    mut commands: Commands,
    resources: TerrainProjectionResources,
    terrain_tiles: Query<TerrainProjectionItem<'_>, With<TerrainTile>>,
) {
    let knowledge_key = current_knowledge_key(
        resources.visibility.fog_active(),
        resources.viewpoint.as_deref(),
        resources.replay_state.as_deref(),
        resources.registry.as_deref(),
    );

    for (entity, terrain_tile, position, current) in &terrain_tiles {
        let visible_terrain = terrain_for_viewer(
            resources.visibility.as_ref(),
            resources.knowledge.as_deref(),
            knowledge_key,
            position.position(),
            terrain_tile.terrain,
        );

        let next = ProjectedTerrainRenderState(terrain_with_display_faction(
            visible_terrain,
            resources.player_roster.as_deref(),
            resources.display_faction_overrides.as_deref(),
        ));

        if current.copied() != Some(next) {
            commands.entity(entity).insert(next);
        }
    }
}

pub struct ClientProjectionPlugin;

impl Plugin for ClientProjectionPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (
                ClientProjectionSet::RebuildKnowledge,
                ClientProjectionSet::DeriveVisibility,
                ClientProjectionSet::DerivePresentation,
                ClientProjectionSet::SyncRender,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            project_unit_render_state.in_set(ClientProjectionSet::DeriveVisibility),
        )
        .add_systems(
            Update,
            project_terrain_render_state.in_set(ClientProjectionSet::DerivePresentation),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use awbrn_game::GameWorldPlugin;
    use awbrn_game::world::GameMap;
    use awbrn_map::AwbrnMap;
    use awbrn_types::{AwbwGamePlayerId, PlayerFaction, Property};

    /// A fogged property keeps the owner the viewer last saw on it.
    ///
    /// A projection reports a fogged tile's terrain but not its owner, so the
    /// sprite has to come from `ReplayTerrainKnowledge`. Once the tile is
    /// visible again the actual owner takes over.
    #[test]
    fn a_fogged_property_draws_the_owner_the_viewer_last_saw() {
        let player = AwbwGamePlayerId::new(1);
        let remembered = GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::OrangeStar,
        )));
        let actual = GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::BlueMoon,
        )));

        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app.add_systems(Update, project_terrain_render_state);

        let mut registry = ReplayPlayerRegistry::default();
        registry.add_player(player, PlayerFaction::OrangeStar, 0);
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::new(1, 1, remembered));
        let knowledge = {
            let game_map = app.world().resource::<GameMap>();
            ReplayTerrainKnowledge::from_map_and_registry(game_map, &registry)
        };
        app.world_mut().insert_resource(registry);
        app.world_mut().insert_resource(knowledge);
        app.world_mut()
            .insert_resource(ReplayViewpoint::Player(player));
        app.world_mut().insert_resource(ReplayState {
            active_player_id: Some(player),
            ..ReplayState::default()
        });
        app.world_mut()
            .resource_mut::<GameMap>()
            .set(AwbrnMap::new(1, 1, actual));
        let entity = app
            .world_mut()
            .spawn((MapPosition::new(0, 0), TerrainTile { terrain: actual }))
            .id();

        app.world_mut()
            .resource_mut::<ViewerVisibility>()
            .reset(true, 1, 1);
        app.update();
        assert_eq!(
            app.world()
                .entity(entity)
                .get::<ProjectedTerrainRenderState>(),
            Some(&ProjectedTerrainRenderState(remembered))
        );

        app.world_mut()
            .resource_mut::<ViewerVisibility>()
            .set_tile_visible(Position::new(0, 0));
        app.update();
        assert_eq!(
            app.world()
                .entity(entity)
                .get::<ProjectedTerrainRenderState>(),
            Some(&ProjectedTerrainRenderState(actual))
        );
    }

    /// Unit visibility is a lookup into the selected projection.
    #[test]
    fn a_unit_is_drawn_only_when_the_projection_named_it() {
        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app.add_systems(Update, project_unit_render_state);

        let seen = awbrn_types::AwbwUnitId::new(1);
        let unseen = awbrn_types::AwbwUnitId::new(2);
        for id in [seen, unseen] {
            app.world_mut().spawn((
                AwbwUnitId(id),
                Unit(awbrn_types::Unit::Infantry),
                Faction(PlayerFaction::OrangeStar),
                MapPosition::new(0, 0),
            ));
        }
        {
            let mut visibility = app.world_mut().resource_mut::<ViewerVisibility>();
            visibility.reset(true, 1, 1);
            visibility.set_unit_visible(seen);
        }
        app.update();

        let mut query = app
            .world_mut()
            .query::<(&AwbwUnitId, &ProjectedUnitRenderState)>();
        let mut drawn = query
            .iter(app.world())
            .filter(|(_, state)| state.visible)
            .map(|(id, _)| id.0)
            .collect::<Vec<_>>();
        drawn.sort_unstable();
        assert_eq!(drawn, vec![seen]);
    }

    /// Only the current player has spent units to grey out.
    #[test]
    fn a_waiting_unit_of_another_player_is_still_drawn_ready() {
        let current = AwbwGamePlayerId::new(1);
        let other = AwbwGamePlayerId::new(2);

        let mut app = App::new();
        app.add_plugins(GameWorldPlugin);
        app.add_systems(Update, project_unit_render_state);

        let mut registry = ReplayPlayerRegistry::default();
        registry.add_player(current, PlayerFaction::OrangeStar, 0);
        registry.add_player(other, PlayerFaction::BlueMoon, 0);
        app.world_mut().insert_resource(registry);
        app.world_mut().insert_resource(ReplayState {
            active_player_id: Some(current),
            ..ReplayState::default()
        });

        // Neither unit can act: one has spent its turn, the other is waiting
        // for a turn it does not have yet.
        let spent = app
            .world_mut()
            .spawn((
                Unit(awbrn_types::Unit::Infantry),
                Faction(PlayerFaction::OrangeStar),
                MapPosition::new(0, 0),
            ))
            .id();
        let waiting = app
            .world_mut()
            .spawn((
                Unit(awbrn_types::Unit::Infantry),
                Faction(PlayerFaction::BlueMoon),
                MapPosition::new(1, 0),
            ))
            .id();

        app.update();

        assert!(
            !app.world()
                .entity(spent)
                .get::<ProjectedUnitRenderState>()
                .unwrap()
                .active,
            "the current player's spent unit is greyed out"
        );
        assert!(
            app.world()
                .entity(waiting)
                .get::<ProjectedUnitRenderState>()
                .unwrap()
                .active,
            "another player's unit is drawn ready"
        );
    }
}
