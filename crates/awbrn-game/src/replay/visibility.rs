//! Selecting a recipient projection and restating it as presentation state.
//!
//! The ECS holds a merged presentation roster: `apply_observed_transitions`
//! takes every player's projection so that a unit hidden from one recipient is
//! still available through its owner's. Rendering, however, shows one
//! viewpoint. This module keeps the projections that produced the current ECS
//! state, selects the one the viewpoint names, and restates it as
//! [`ViewerVisibility`] and [`ReplayTerrainKnowledge`].
//!
//! Switching viewpoints re-selects; it never recomputes vision.

use std::collections::{HashMap, HashSet};

use awbrn_map::{Dimensions, Pos};
use awbrn_types::{AwbwGamePlayerId, AwbwUnitId as RawAwbwUnitId, PlayerFaction};
use awvm::semantic::{
    Observation, ObservedPlayer, ObservedUnitHp, ObservedUnitRef, TileVisibility,
};
use bevy::prelude::*;

use crate::replay::{AwbwUnitId, ReplayState};
use crate::world::{BoardIndex, FriendlyFactions, GameMap, GraphicalHp, ViewerVisibility};

/// Selects whose perspective the presentation is drawn from.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub enum ReplayViewpoint {
    #[default]
    Spectator,
    /// Follow the active player each turn.
    ActivePlayer,
    /// Locked to a specific player.
    Player(AwbwGamePlayerId),
}

/// The recipient projections the current ECS state was reconciled from.
///
/// Archived playback supplies one per player; a live client supplies only its
/// own. Holding them is what lets a viewpoint change re-select instead of
/// asking the rules engine again.
#[derive(Resource, Default, Debug)]
pub struct RecipientObservations(Vec<Observation>);

impl RecipientObservations {
    pub fn set(&mut self, observations: Vec<Observation>) {
        self.0 = observations;
    }

    pub fn for_player(&self, player: AwbwGamePlayerId) -> Option<&Observation> {
        self.0
            .iter()
            .find(|observation| parse_player_id(observation.recipient.as_str()) == Some(player))
    }

    /// The single projection a live client holds, if that is all there is.
    ///
    /// A live match sends only the recipient's own view, so there is nothing
    /// to select between and no viewpoint to name it with.
    fn sole(&self) -> Option<&Observation> {
        match self.0.as_slice() {
            [observation] => Some(observation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplayKnowledgeKey {
    Player(AwbwGamePlayerId),
    Team(u8),
}

/// The last terrain each viewpoint saw at each position.
///
/// A projection reports a fogged tile's terrain but not its owner
/// (`spec/semantics/fog.md`), so the property sprite a viewer remembers is
/// presentation memory the observation cannot supply.
#[derive(Resource, Default, Clone)]
pub struct ReplayTerrainKnowledge {
    pub by_view: HashMap<ReplayKnowledgeKey, HashMap<Pos, awbrn_types::GraphicalTerrain>>,
}

impl ReplayTerrainKnowledge {
    pub fn from_map_and_registry(game_map: &GameMap, registry: &ReplayPlayerRegistry) -> Self {
        let terrain_by_position = (0..game_map.height())
            .flat_map(|y| {
                (0..game_map.width()).filter_map(move |x| {
                    let position = Pos::new(x, y);
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

/// Maps factions to players and teams. Built once at bootstrap from player data.
#[derive(Resource, Default)]
pub struct ReplayPlayerRegistry {
    players: Vec<ReplayPlayerInfo>,
}

impl ReplayPlayerRegistry {
    /// Add a player. Call in turn-order (sorted by `order` field before calling).
    /// `team` should be 0 for FFA games, or the team byte (`b'A'`, `b'B'`, etc.)
    /// for team games.
    pub fn add_player(
        &mut self,
        game_player_id: AwbwGamePlayerId,
        faction: PlayerFaction,
        team: u8,
    ) {
        self.players.push(ReplayPlayerInfo {
            game_player_id,
            faction,
            team,
        });
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
            return HashSet::from([player.faction]);
        }

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

/// Restate the selected recipient projection as presentation state.
///
/// Run after the ECS has been reconciled — enemy units are identified by the
/// position their projection reports them at, so `BoardIndex` must already
/// agree with the observation.
pub fn refresh_viewer_visibility(world: &mut World) {
    world.init_resource::<RecipientObservations>();
    world.init_resource::<ViewerVisibility>();
    world.init_resource::<FriendlyFactions>();

    let viewer = selected_player(world);
    let observation = viewer
        .and_then(|player| {
            world
                .resource::<RecipientObservations>()
                .for_player(player)
                .cloned()
        })
        .or_else(|| world.resource::<RecipientObservations>().sole().cloned());

    let friendly = viewer
        .map(|player| {
            world
                .resource::<ReplayPlayerRegistry>()
                .friendly_factions_for_player(player)
        })
        .unwrap_or_default();
    let mut friendly_factions = world.resource_mut::<FriendlyFactions>();
    if friendly_factions.0 != friendly {
        friendly_factions.0 = friendly;
    }

    let Some(observation) = observation else {
        world.resource_mut::<ViewerVisibility>().clear();
        restore_spectator_hp(world);
        return;
    };

    apply_observation_visibility(world, &observation);
    refresh_terrain_knowledge(world, viewer);
}

fn selected_player(world: &World) -> Option<AwbwGamePlayerId> {
    match world.get_resource::<ReplayViewpoint>()? {
        ReplayViewpoint::Spectator => None,
        ReplayViewpoint::ActivePlayer => world.get_resource::<ReplayState>()?.active_player_id,
        ReplayViewpoint::Player(player) => Some(*player),
    }
}

fn apply_observation_visibility(world: &mut World, observation: &Observation) {
    let dimensions = Dimensions::new(observation.board.width(), observation.board.height());

    // A friendly unit keeps its semantic id in the ECS. An enemy is referenced
    // only by position, so resolve it against the board before borrowing the
    // resource mutably.
    let visible_units = observation
        .units
        .iter()
        .filter_map(|unit| match unit.reference {
            ObservedUnitRef::Friendly { unit: id } => Some((RawAwbwUnitId::new(id.get()), unit.hp)),
            ObservedUnitRef::Enemy { position } => unit_at(world, position).map(|id| (id, unit.hp)),
        })
        .collect::<Vec<_>>();

    for (unit, hp) in &visible_units {
        let Some(entity) = world
            .resource::<crate::world::StrongIdMap<AwbwUnitId>>()
            .get(&AwbwUnitId(*unit))
        else {
            continue;
        };
        world.entity_mut(entity).insert(GraphicalHp::from(*hp));
    }

    let mut visibility = world.resource_mut::<ViewerVisibility>();
    visibility.reset(observation.settings.fog, dimensions);
    for position in observation.board.positions() {
        if observation.board.tile(position).visibility == TileVisibility::Visible {
            visibility.set_tile_visible(position);
        }
    }
    for (unit, _) in visible_units {
        visibility.set_unit_visible(unit);
    }
    for player in &observation.players {
        // Only a teammate's projection carries funds and commander state, and
        // that is exactly the disclosure the roster reports.
        if let ObservedPlayer::Private { id, .. } = player
            && let Some(player) = parse_player_id(id.as_str())
        {
            visibility.set_player_disclosed(player);
        }
    }
}

fn restore_spectator_hp(world: &mut World) {
    let exact = world
        .resource::<RecipientObservations>()
        .0
        .iter()
        .flat_map(|observation| {
            observation
                .units
                .iter()
                .filter_map(|unit| match (unit.reference, unit.hp) {
                    (ObservedUnitRef::Friendly { unit: id }, ObservedUnitHp::Exact(hp))
                        if unit.owner == observation.recipient =>
                    {
                        Some((RawAwbwUnitId::new(id.get()), hp))
                    }
                    _ => None,
                })
        })
        .collect::<Vec<_>>();
    for (unit, hp) in exact {
        let Some(entity) = world
            .resource::<crate::world::StrongIdMap<AwbwUnitId>>()
            .get(&AwbwUnitId(unit))
        else {
            continue;
        };
        world
            .entity_mut(entity)
            .insert(GraphicalHp::from(ObservedUnitHp::Exact(hp)));
    }
}

/// Record the terrain the selected viewpoint can currently see.
///
/// A fogged property keeps the owner the viewer last saw on it, which is what
/// `terrain_for_viewer` draws.
fn refresh_terrain_knowledge(world: &mut World, viewer: Option<AwbwGamePlayerId>) {
    if !world.resource::<ViewerVisibility>().fog_active() {
        return;
    }
    let Some(key) = viewer.and_then(|player| {
        world
            .get_resource::<ReplayPlayerRegistry>()?
            .knowledge_key_for_player(player)
    }) else {
        return;
    };
    if !world.contains_resource::<ReplayTerrainKnowledge>() {
        return;
    }

    let seen = {
        let visibility = world.resource::<ViewerVisibility>();
        let game_map = world.resource::<GameMap>();
        (0..game_map.height())
            .flat_map(|y| (0..game_map.width()).map(move |x| Pos::new(x, y)))
            .filter(|position| !visibility.is_fogged(*position))
            .filter_map(|position| {
                game_map
                    .terrain_at(position)
                    .map(|terrain| (position, terrain))
            })
            .collect::<Vec<_>>()
    };

    let mut knowledge = world.resource_mut::<ReplayTerrainKnowledge>();
    let Some(known) = knowledge.by_view.get_mut(&key) else {
        return;
    };
    for (position, terrain) in seen {
        known.insert(position, terrain);
    }
}

fn unit_at(world: &World, position: Pos) -> Option<RawAwbwUnitId> {
    let entity = world
        .get_resource::<BoardIndex>()?
        .unit_entity(position)
        .ok()
        .flatten()?;
    world.get::<AwbwUnitId>(entity).map(|id| id.0)
}

fn parse_player_id(id: &str) -> Option<AwbwGamePlayerId> {
    id.parse::<u32>().ok().map(AwbwGamePlayerId::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ffa_returns_single_faction() {
        let mut registry = ReplayPlayerRegistry::default();
        registry.add_player(AwbwGamePlayerId::new(1), PlayerFaction::OrangeStar, 0);
        registry.add_player(AwbwGamePlayerId::new(2), PlayerFaction::BlueMoon, 0);

        let friendly = registry.friendly_factions_for_player(AwbwGamePlayerId::new(1));
        assert_eq!(friendly, HashSet::from([PlayerFaction::OrangeStar]));
        assert!(!friendly.contains(&PlayerFaction::BlueMoon));
    }

    #[test]
    fn registry_team_returns_all_team_factions() {
        let mut registry = ReplayPlayerRegistry::default();
        registry.add_player(AwbwGamePlayerId::new(1), PlayerFaction::OrangeStar, b'A');
        registry.add_player(AwbwGamePlayerId::new(2), PlayerFaction::GreenEarth, b'A');
        registry.add_player(AwbwGamePlayerId::new(3), PlayerFaction::BlueMoon, b'B');

        let friendly = registry.friendly_factions_for_player(AwbwGamePlayerId::new(1));
        assert!(friendly.contains(&PlayerFaction::OrangeStar));
        assert!(friendly.contains(&PlayerFaction::GreenEarth));
        assert!(!friendly.contains(&PlayerFaction::BlueMoon));
    }

    #[test]
    fn registry_player_indices_follow_turn_order() {
        // Caller is responsible for adding in sorted order
        let mut registry = ReplayPlayerRegistry::default();
        registry.add_player(AwbwGamePlayerId::new(20), PlayerFaction::BlueMoon, 0);
        registry.add_player(AwbwGamePlayerId::new(30), PlayerFaction::GreenEarth, 0);
        registry.add_player(AwbwGamePlayerId::new(10), PlayerFaction::OrangeStar, 0);

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
    fn non_team_games_treat_all_players_as_ffa() {
        // has_teams=false → team=0 for everyone
        let mut registry = ReplayPlayerRegistry::default();
        registry.add_player(AwbwGamePlayerId::new(3252378), PlayerFaction::OrangeStar, 0);
        registry.add_player(AwbwGamePlayerId::new(3252473), PlayerFaction::BlueMoon, 0);

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
}
