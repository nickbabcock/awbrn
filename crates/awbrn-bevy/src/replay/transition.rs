//! Typed AWVM transition application for the headless presentation ECS.
//!
//! The ECS is a cache of recipient-safe presentation facts, not another rules
//! engine. Native games and fixtures can obtain these values from AWVM
//! execution; archived AWBW games obtain the same values from the
//! recorded-outcome adapter. This module deliberately cannot tell which source
//! produced them.

use std::collections::{BTreeMap, HashMap};

use awbrn_map::Pos;
use awbrn_types::{AwbwGamePlayerId, AwbwUnitId as RawAwbwUnitId, Faction as MapFaction};
use awvm::semantic::{
    Location, Observation, ObservedEvent, ObservedTileOwner, ObservedTransition, ObservedUnit,
    ObservedUnitRef, PlayerId, UnitAction,
};
use bevy::prelude::*;

use crate::MapPosition;
use crate::replay::{
    AwbwUnitId, RecipientObservations, ReplayPlayerRegistry, ReplayState, refresh_viewer_visibility,
};
use crate::world::{
    Ammo, BoardIndex, BoardOf, CaptureProgress, CarriedBy, CurrentWeather, Faction, Fuel, GameMap,
    GraphicalHp, Hiding, StrongIdMap, TerrainHp, TerrainTile, Unit, UnitActive, VisionRange,
    board_root,
};

#[derive(Component)]
struct RecipientEnemy;

#[derive(Resource)]
struct RecipientEnemyIds {
    next: u32,
}

impl Default for RecipientEnemyIds {
    fn default() -> Self {
        Self { next: u32::MAX }
    }
}

/// Reconcile a complete set of recipient projections into the presentation ECS.
///
/// One transition per player makes every authoritative unit available through
/// its owner's `Friendly` reference, even when fog hides it from opponents.
/// Repeated board and public facts must agree.
pub fn apply_observed_transitions(
    world: &mut World,
    transitions: &[ObservedTransition],
) -> Result<(), TransitionApplyError> {
    let first = transitions
        .first()
        .ok_or(TransitionApplyError::EmptyTransitionSet)?;
    let post = &first.post;
    for transition in transitions {
        ensure_public_post_agrees(post, &transition.post)?;
        consume_events(&transition.events);
    }
    ensure_every_player_is_represented(transitions, post)?;

    let units = collect_owned_units(transitions)?;
    ensure_players_are_known(world, transitions)?;
    sync_replay_state(world, post)?;
    sync_weather(world, post);
    let capture_points = sync_tiles(world, transitions)?;
    sync_units(world, &units, &capture_points)?;
    store_observations(world, transitions);
    refresh_viewer_visibility(world);
    Ok(())
}

/// Reconcile one recipient-safe transition into the presentation ECS.
///
/// Unlike [`apply_observed_transitions`], this mode never requires or infers
/// other players' private projections. Friendly units retain their semantic
/// IDs. Visible enemies receive presentation-local IDs and are matched by the
/// positions and movement facts the recipient is entitled to observe.
pub fn apply_observed_transition(
    world: &mut World,
    transition: &ObservedTransition,
) -> Result<(), TransitionApplyError> {
    consume_events(&transition.events);
    let units = collect_recipient_units(world, transition)?;
    ensure_players_are_known(world, std::slice::from_ref(transition))?;
    sync_replay_state(world, &transition.post)?;
    sync_weather(world, &transition.post);
    let capture_points = sync_tiles(world, std::slice::from_ref(transition))?;
    sync_units(world, &units, &capture_points)?;
    store_observations(world, std::slice::from_ref(transition));
    refresh_viewer_visibility(world);
    Ok(())
}

fn ensure_public_post_agrees(
    expected: &Observation,
    candidate: &Observation,
) -> Result<(), TransitionApplyError> {
    if expected.ruleset != candidate.ruleset
        || expected.settings != candidate.settings
        || expected.teams != candidate.teams
        || expected.turn != candidate.turn
        || expected.weather != candidate.weather
        || expected.match_state != candidate.match_state
    {
        return Err(TransitionApplyError::InconsistentPostObservations);
    }
    if expected.board.width() != candidate.board.width()
        || expected.board.height() != candidate.board.height()
    {
        return Err(TransitionApplyError::InconsistentPostObservations);
    }
    Ok(())
}

/// Units are only authoritative in their owner's projection, so a set missing a
/// player would despawn every unit that player owns. Reject it instead.
fn ensure_every_player_is_represented(
    transitions: &[ObservedTransition],
    post: &Observation,
) -> Result<(), TransitionApplyError> {
    for player in &post.players {
        let id = match player {
            awvm::semantic::ObservedPlayer::Private { id, .. }
            | awvm::semantic::ObservedPlayer::Public { id, .. } => id,
        };
        if !transitions
            .iter()
            .any(|transition| transition.post.recipient == *id)
        {
            return Err(TransitionApplyError::MissingPlayerTransition(
                id.to_string(),
            ));
        }
    }
    Ok(())
}

/// Reject a projection that names a player the registry does not hold, before
/// the world changes. A faction lookup that fails halfway through leaves the
/// ECS between two states, so make reconciliation all-or-nothing.
fn ensure_players_are_known(
    world: &World,
    transitions: &[ObservedTransition],
) -> Result<(), TransitionApplyError> {
    // A world with no registry does no faction lookups either, so it has
    // nothing to check.
    let Some(registry) = world.get_resource::<ReplayPlayerRegistry>() else {
        return Ok(());
    };
    for transition in transitions {
        for player in &transition.post.players {
            let id = match player {
                awvm::semantic::ObservedPlayer::Private { id, .. }
                | awvm::semantic::ObservedPlayer::Public { id, .. } => id,
            };
            let player = parse_player_id(id)?;
            if registry.faction_for_player(player).is_none() {
                return Err(TransitionApplyError::UnknownPlayer(player.as_u32()));
            }
        }
    }
    Ok(())
}

fn consume_events(events: &[ObservedEvent]) {
    for event in events {
        match event {
            ObservedEvent::UnitMoved { .. }
            | ObservedEvent::UnitAppeared { .. }
            | ObservedEvent::UnitDisappeared { .. }
            | ObservedEvent::MovementStopped { .. }
            | ObservedEvent::CombatEngaged { .. }
            | ObservedEvent::UnitChanged { .. }
            | ObservedEvent::UnitRemoved { .. }
            | ObservedEvent::TileChanged { .. }
            | ObservedEvent::PlayerChanged { .. }
            | ObservedEvent::AreaStrikeResolved { .. }
            | ObservedEvent::PublicEvent { .. } => {}
        }
    }
}

fn collect_owned_units(
    transitions: &[ObservedTransition],
) -> Result<BTreeMap<RawAwbwUnitId, ObservedUnit>, TransitionApplyError> {
    let mut units = BTreeMap::new();
    for transition in transitions {
        for unit in &transition.post.units {
            let ObservedUnitRef::Friendly { unit: id } = &unit.reference else {
                continue;
            };
            if unit.owner != transition.post.recipient {
                continue;
            }
            let id = RawAwbwUnitId::new(id.get());
            if let Some(previous) = units.insert(id, unit.clone())
                && previous != *unit
            {
                return Err(TransitionApplyError::InconsistentUnit(id.as_u32()));
            }
        }
    }
    Ok(units)
}

fn collect_recipient_units(
    world: &mut World,
    transition: &ObservedTransition,
) -> Result<BTreeMap<RawAwbwUnitId, ObservedUnit>, TransitionApplyError> {
    world.init_resource::<RecipientEnemyIds>();
    let moved_enemies = transition
        .events
        .iter()
        .filter_map(|event| match event {
            ObservedEvent::UnitMoved {
                unit: ObservedUnitRef::Enemy { .. },
                from,
                to,
                ..
            } => Some((*from, *to)),
            ObservedEvent::UnitMoved { .. }
            | ObservedEvent::UnitAppeared { .. }
            | ObservedEvent::UnitDisappeared { .. }
            | ObservedEvent::MovementStopped { .. }
            | ObservedEvent::CombatEngaged { .. }
            | ObservedEvent::UnitChanged { .. }
            | ObservedEvent::UnitRemoved { .. }
            | ObservedEvent::TileChanged { .. }
            | ObservedEvent::PlayerChanged { .. }
            | ObservedEvent::AreaStrikeResolved { .. }
            | ObservedEvent::PublicEvent { .. } => None,
        })
        .collect::<Vec<_>>();
    let mut used = std::collections::HashSet::new();
    let mut units = BTreeMap::new();
    for unit in &transition.post.units {
        let id = match unit.reference {
            ObservedUnitRef::Friendly { unit } => RawAwbwUnitId::new(unit.get()),
            ObservedUnitRef::Enemy { position } => {
                let post = position;
                let moved_from = moved_enemies
                    .iter()
                    .find_map(|(from, to)| (*to == position).then_some(*from));
                let existing = moved_from
                    .and_then(|position| recipient_enemy_at(world, position))
                    .or_else(|| recipient_enemy_at(world, post))
                    .filter(|id| !used.contains(id));
                existing.unwrap_or_else(|| spawn_recipient_enemy(world, unit, post))
            }
        };
        used.insert(id);
        units.insert(id, unit.clone());
    }
    Ok(units)
}

fn recipient_enemy_at(world: &World, position: Pos) -> Option<RawAwbwUnitId> {
    let entity = world
        .resource::<BoardIndex>()
        .unit_entity(position)
        .ok()
        .flatten()?;
    world.get::<RecipientEnemy>(entity)?;
    world.get::<AwbwUnitId>(entity).map(|id| id.0)
}

fn spawn_recipient_enemy(world: &mut World, unit: &ObservedUnit, position: Pos) -> RawAwbwUnitId {
    let id = loop {
        let candidate = {
            let mut ids = world.resource_mut::<RecipientEnemyIds>();
            let candidate = ids.next;
            ids.next = ids.next.saturating_sub(1);
            RawAwbwUnitId::new(candidate)
        };
        if world
            .resource::<StrongIdMap<AwbwUnitId>>()
            .get(&AwbwUnitId(candidate))
            .is_none()
        {
            break candidate;
        }
    };
    let root = board_root(world);
    let mut spawned = world.spawn((
        AwbwUnitId(id),
        Unit(unit.kind),
        MapPosition::from(position),
        RecipientEnemy,
    ));
    if let Some(root) = root {
        spawned.insert(BoardOf(root));
    }
    id
}

/// Keep the projections the ECS was reconciled from.
///
/// Presentation visibility is a restatement of one of these, so a viewpoint
/// change can re-select without asking the rules engine again.
fn store_observations(world: &mut World, transitions: &[ObservedTransition]) {
    world.init_resource::<RecipientObservations>();
    world.resource_mut::<RecipientObservations>().set(
        transitions
            .iter()
            .map(|transition| transition.post.clone())
            .collect(),
    );
}

fn sync_replay_state(world: &mut World, post: &Observation) -> Result<(), TransitionApplyError> {
    let active_player_id = parse_player_id(&post.turn.active_player)?;
    let mut state = world
        .get_resource_mut::<ReplayState>()
        .ok_or(TransitionApplyError::MissingResource("ReplayState"))?;
    state.day =
        u32::try_from(post.turn.day).map_err(|_| TransitionApplyError::OutOfRange("turn.day"))?;
    state.active_player_id = Some(active_player_id);
    Ok(())
}

fn sync_weather(world: &mut World, post: &Observation) {
    // Unlike ReplayState, weather is optional: headless fixtures reconcile board
    // and unit facts without installing the presentation resource.
    match world.get_resource_mut::<CurrentWeather>() {
        Some(mut weather) => weather.set(post.weather.kind),
        None => debug!("Skipping weather sync: CurrentWeather is not installed"),
    }
}

fn sync_tiles(
    world: &mut World,
    transitions: &[ObservedTransition],
) -> Result<HashMap<Pos, u8>, TransitionApplyError> {
    let first = &transitions[0].post;
    let tile_updates = first
        .board
        .positions()
        .map(|position| {
            let tile = transitions
                .iter()
                .map(|transition| transition.post.board.tile(position))
                .find(|tile| tile.visibility == awvm::semantic::TileVisibility::Visible)
                .unwrap_or_else(|| first.board.tile(position));
            let terrain_entity = world
                .resource::<BoardIndex>()
                .terrain_entity(position)
                .map_err(|_| TransitionApplyError::MissingTerrain(position))?;
            let graphical = world
                .get::<TerrainTile>(terrain_entity)
                .ok_or(TransitionApplyError::MissingTerrain(position))?
                .terrain;
            let owner = match &tile.owner {
                ObservedTileOwner::NotOwnable => None,
                ObservedTileOwner::Neutral => Some(MapFaction::Neutral),
                ObservedTileOwner::Owned(player) => {
                    let player = parse_player_id(player)?;
                    let faction = world
                        .resource::<ReplayPlayerRegistry>()
                        .faction_for_player(player)
                        .ok_or_else(|| TransitionApplyError::UnknownPlayer(player.as_u32()))?;
                    Some(MapFaction::Player(faction))
                }
            };
            let graphical = match (graphical, owner) {
                (awbrn_types::GraphicalTerrain::Property(property), Some(owner)) => {
                    awbrn_types::GraphicalTerrain::Property(property.with_owner(owner))
                }
                (terrain, _) => terrain,
            };
            let graphical = match (graphical, tile.silo) {
                (
                    awbrn_types::GraphicalTerrain::MissileSilo(_),
                    Some(awvm::semantic::Silo::Ready),
                ) => awbrn_types::GraphicalTerrain::MissileSilo(
                    awbrn_types::MissileSiloStatus::Loaded,
                ),
                (
                    awbrn_types::GraphicalTerrain::MissileSilo(_),
                    Some(awvm::semantic::Silo::Spent),
                ) => awbrn_types::GraphicalTerrain::MissileSilo(
                    awbrn_types::MissileSiloStatus::Unloaded,
                ),
                (terrain, _) => terrain,
            };
            let graphical = match (graphical, tile.terrain, tile.destructible_hp()) {
                (
                    awbrn_types::GraphicalTerrain::PipeSeam(direction),
                    awvm::ruleset::Terrain::Plain,
                    None,
                ) => awbrn_types::GraphicalTerrain::PipeRubble(match direction {
                    awbrn_types::PipeSeamType::Horizontal => {
                        awbrn_types::PipeRubbleType::Horizontal
                    }
                    awbrn_types::PipeSeamType::Vertical => awbrn_types::PipeRubbleType::Vertical,
                }),
                (terrain, _, _) => terrain,
            };
            let hp = tile
                .destructible_hp()
                .map(|hp| {
                    u8::try_from(hp)
                        .map(TerrainHp)
                        .map_err(|_| TransitionApplyError::OutOfRange("tile.destructible_hp"))
                })
                .transpose()?;
            Ok((position, terrain_entity, graphical, hp, tile.capture_points))
        })
        .collect::<Result<Vec<_>, TransitionApplyError>>()?;

    for (position, entity, graphical, hp, _) in &tile_updates {
        // Most tiles are untouched by any given action. Writing them anyway
        // would mark every terrain entity as changed and re-render the board.
        let unchanged = world
            .get::<TerrainTile>(*entity)
            .is_some_and(|tile| tile.terrain == *graphical)
            && world.get::<TerrainHp>(*entity).copied() == *hp;
        if unchanged {
            continue;
        }

        world
            .resource_mut::<GameMap>()
            .set_terrain(*position, *graphical);
        let mut entity = world.entity_mut(*entity);
        entity.insert(TerrainTile {
            terrain: *graphical,
        });
        if let Some(hp) = hp {
            entity.insert(*hp);
        } else {
            entity.remove::<TerrainHp>();
        }
    }
    Ok(tile_updates
        .into_iter()
        .filter_map(|(position, _, _, _, capture)| capture.map(|value| (position, value)))
        .collect())
}

fn sync_units(
    world: &mut World,
    units: &BTreeMap<RawAwbwUnitId, ObservedUnit>,
    capture_points: &HashMap<Pos, u8>,
) -> Result<(), TransitionApplyError> {
    let existing: Vec<(RawAwbwUnitId, Entity)> = {
        let mut query = world.query::<(Entity, &AwbwUnitId)>();
        query
            .iter(world)
            .map(|(entity, id)| (id.0, entity))
            .collect()
    };
    for (id, entity) in existing {
        if !units.contains_key(&id) {
            world.despawn(entity);
        }
    }

    let board_root = board_root(world);
    for (id, unit) in units {
        let entity = world
            .resource::<StrongIdMap<AwbwUnitId>>()
            .get(&AwbwUnitId(*id))
            .unwrap_or_else(|| {
                let mut spawned =
                    world.spawn((AwbwUnitId(*id), Unit(unit.kind), MapPosition::new(0, 0)));
                if let Some(root) = board_root {
                    spawned.insert(BoardOf(root));
                }
                spawned.id()
            });
        sync_unit_components(world, entity, unit)?;
    }

    // Rebuild relationships only after every transport exists. Sorting by the
    // typed cargo slot keeps Bevy's fixed relationship array deterministic.
    for id in units.keys() {
        let entity = world
            .resource::<StrongIdMap<AwbwUnitId>>()
            .get(&AwbwUnitId(*id))
            .ok_or(TransitionApplyError::MissingUnit(id.as_u32()))?;
        world.entity_mut(entity).remove::<CarriedBy>();
    }
    for (id, unit) in units {
        let entity = world
            .resource::<StrongIdMap<AwbwUnitId>>()
            .get(&AwbwUnitId(*id))
            .ok_or(TransitionApplyError::MissingUnit(id.as_u32()))?;
        match &unit.location {
            Location::Board { position } => {
                world
                    .entity_mut(entity)
                    .insert(MapPosition::from(*position));
            }
            Location::Cargo { .. } => {
                world.entity_mut(entity).remove::<MapPosition>();
            }
        }
    }
    let mut cargo = units
        .iter()
        .filter_map(|(id, unit)| match unit.location {
            Location::Cargo { transport, slot } => Some((*id, transport, slot)),
            Location::Board { .. } => None,
        })
        .collect::<Vec<_>>();
    cargo.sort_by_key(|(id, transport, slot)| (transport.get(), *slot, id.as_u32()));
    for (id, transport, _) in cargo {
        let entity = world
            .resource::<StrongIdMap<AwbwUnitId>>()
            .get(&AwbwUnitId(id))
            .ok_or(TransitionApplyError::MissingUnit(id.as_u32()))?;
        let transport = RawAwbwUnitId::new(transport.get());
        let transport_entity = world
            .resource::<StrongIdMap<AwbwUnitId>>()
            .get(&AwbwUnitId(transport))
            .ok_or(TransitionApplyError::MissingUnit(transport.as_u32()))?;
        world.entity_mut(entity).insert(CarriedBy(transport_entity));
    }

    sync_capture_progress(world, capture_points)
}

fn sync_unit_components(
    world: &mut World,
    entity: Entity,
    unit: &ObservedUnit,
) -> Result<(), TransitionApplyError> {
    let player = parse_player_id(&unit.owner)?;
    let faction = world
        .resource::<ReplayPlayerRegistry>()
        .faction_for_player(player)
        .ok_or_else(|| TransitionApplyError::UnknownPlayer(player.as_u32()))?;
    let fuel =
        u32::try_from(unit.fuel).map_err(|_| TransitionApplyError::OutOfRange("unit.fuel"))?;
    let ammo =
        u32::try_from(unit.ammo).map_err(|_| TransitionApplyError::OutOfRange("unit.ammo"))?;
    let vision = awvm::ruleset::profile(unit.kind).vision;
    let vision =
        u32::try_from(vision).map_err(|_| TransitionApplyError::OutOfRange("unit.vision"))?;

    let mut entity = world.entity_mut(entity);
    entity.insert((
        Unit(unit.kind),
        Faction(faction),
        Fuel(fuel),
        Ammo(ammo),
        VisionRange(vision),
    ));
    entity.insert(GraphicalHp::from(unit.hp));
    if unit.action == UnitAction::Ready {
        entity.insert(UnitActive);
    } else {
        entity.remove::<UnitActive>();
    }
    if unit.concealment == awvm::semantic::Concealment::Hidden {
        entity.insert(Hiding);
    } else {
        entity.remove::<Hiding>();
    }
    Ok(())
}

fn sync_capture_progress(
    world: &mut World,
    capture_points: &HashMap<Pos, u8>,
) -> Result<(), TransitionApplyError> {
    // AWVM stores points that the property still owes. The ECS component stores
    // points already paid, because capture actions add unit HP to it.
    let positions: Vec<(Entity, Pos)> = {
        let mut query = world.query_filtered::<(Entity, &MapPosition), With<Unit>>();
        query
            .iter(world)
            .map(|(entity, position)| (entity, position.position()))
            .collect()
    };
    for (entity, position) in positions {
        let mut entity = world.entity_mut(entity);
        if let Some(points) = capture_points.get(&position).copied()
            && let Some(progress) = CaptureProgress::from_remaining_points(points)
        {
            entity.insert(progress);
        } else {
            entity.remove::<CaptureProgress>();
        }
    }
    Ok(())
}

fn parse_player_id(id: &PlayerId) -> Result<AwbwGamePlayerId, TransitionApplyError> {
    id.as_str()
        .parse::<u32>()
        .map(AwbwGamePlayerId::new)
        .map_err(|_| TransitionApplyError::InvalidPlayerId(id.to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum TransitionApplyError {
    #[error("an observed transition set cannot be empty")]
    EmptyTransitionSet,
    #[error("recipient post-observations disagree on public state")]
    InconsistentPostObservations,
    #[error("no observed transition for player {0:?}")]
    MissingPlayerTransition(String),
    #[error("recipient post-observations disagree on unit {0}")]
    InconsistentUnit(u32),
    #[error("invalid AWBW player id {0:?}")]
    InvalidPlayerId(String),
    #[error("unknown AWBW player {0}")]
    UnknownPlayer(u32),
    #[error("missing AWBW unit {0}")]
    MissingUnit(u32),
    #[error("missing terrain at {0}")]
    MissingTerrain(Pos),
    #[error("missing ECS resource {0}")]
    MissingResource(&'static str),
    #[error("{0} exceeds the ECS presentation domain")]
    OutOfRange(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_capture_progress_counts_down_from_the_authoritative_value() {
        let mut world = World::new();
        let position = Pos::new(0, 0);
        let unit = world
            .spawn((
                MapPosition::from(position),
                Unit(awbrn_types::Unit::Infantry),
            ))
            .id();
        let capture_points = HashMap::from([(position, 15)]);

        sync_capture_progress(&mut world, &capture_points).unwrap();

        assert_eq!(
            world
                .get::<CaptureProgress>(unit)
                .map(|progress| progress.value()),
            Some(5)
        );
    }
}
