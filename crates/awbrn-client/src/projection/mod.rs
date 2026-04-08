use crate::core::AppState;
use crate::features::player_display::{
    PlayerDisplayFactionOverrides, display_faction_for_actual_faction,
};
use crate::features::player_roster::PlayerRosterConfig;
use awbrn_game::MapPosition;
use awbrn_game::replay::{
    ReplayKnowledgeKey, ReplayPlayerRegistry, ReplayState, ReplayTerrainKnowledge, ReplayViewpoint,
};
use awbrn_game::world::{
    Capturing, CarriedBy, Faction, FogActive, FogOfWarMap, FriendlyFactions, GraphicalHp, HasCargo,
    Hiding, TerrainTile, Unit, UnitActive,
};
use awbrn_map::Position;
use awbrn_types::{
    Faction as TerrainFaction, GraphicalTerrain, Property, PropertyKind, UnitDomain,
};
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
    pub health: Option<u8>,
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
    fog_map: Res<'w, FogOfWarMap>,
    fog_active: Res<'w, FogActive>,
    friendly: Res<'w, FriendlyFactions>,
    player_roster: Option<Res<'w, PlayerRosterConfig>>,
    display_faction_overrides: Option<Res<'w, PlayerDisplayFactionOverrides>>,
}

#[derive(SystemParam)]
pub(crate) struct TerrainProjectionResources<'w> {
    fog_map: Res<'w, FogOfWarMap>,
    fog_active: Res<'w, FogActive>,
    player_roster: Option<Res<'w, PlayerRosterConfig>>,
    display_faction_overrides: Option<Res<'w, PlayerDisplayFactionOverrides>>,
    viewpoint: Option<Res<'w, ReplayViewpoint>>,
    registry: Option<Res<'w, ReplayPlayerRegistry>>,
    replay_state: Option<Res<'w, ReplayState>>,
    knowledge: Option<Res<'w, ReplayTerrainKnowledge>>,
}

#[derive(Clone, Copy)]
struct UnitVisibilityInput {
    unit: Unit,
    faction: Faction,
    position: Option<MapPosition>,
    is_hiding: bool,
    is_carried: bool,
}

type UnitProjectionItem<'a> = (
    Entity,
    &'a Unit,
    &'a Faction,
    Option<&'a MapPosition>,
    Option<Ref<'a, UnitActive>>,
    Has<Capturing>,
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

fn projected_health(hp: Option<&GraphicalHp>) -> Option<u8> {
    hp.filter(|hp| !hp.is_full_health() && !hp.is_destroyed())
        .map(GraphicalHp::value)
}

fn unit_visible_to_viewer(resources: &UnitProjectionResources, input: UnitVisibilityInput) -> bool {
    if input.is_carried {
        return false;
    }

    if !resources.fog_active.0 || resources.friendly.0.contains(&input.faction.0) {
        return true;
    }

    let Some(position) = input.position else {
        return false;
    };

    !input.is_hiding
        && resources.fog_map.is_unit_visible(
            position.position(),
            input.unit.0.domain() == UnitDomain::Air,
        )
}

fn terrain_for_viewer(
    fog_map: &FogOfWarMap,
    fog_active: bool,
    knowledge: Option<&ReplayTerrainKnowledge>,
    knowledge_key: Option<ReplayKnowledgeKey>,
    position: Position,
    actual: GraphicalTerrain,
) -> GraphicalTerrain {
    if !fog_active || !fog_map.is_fogged(position) {
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
    for (
        entity,
        unit,
        faction,
        position,
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
            visible: unit_visible_to_viewer(
                &resources,
                UnitVisibilityInput {
                    unit: *unit,
                    faction: *faction,
                    position: position.copied(),
                    is_hiding,
                    is_carried,
                },
            ),
            active: is_active,
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
        resources.fog_active.0,
        resources.viewpoint.as_deref(),
        resources.replay_state.as_deref(),
        resources.registry.as_deref(),
    );

    for (entity, terrain_tile, position, current) in &terrain_tiles {
        let visible_terrain = terrain_for_viewer(
            resources.fog_map.as_ref(),
            resources.fog_active.0,
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
