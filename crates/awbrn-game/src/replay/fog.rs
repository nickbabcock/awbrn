use std::collections::{HashMap, HashSet};

use awbrn_map::Position;
use awbrn_types::{AwbwGamePlayerId, PlayerFaction};
use bevy::prelude::*;

use crate::replay::ReplayState;
use crate::world::{FogActive, FriendlyFactions, GameMap};

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

/// Trigger to recompute the fog map from current game state.
#[derive(Event, Debug, Default, Clone, Copy)]
pub struct ReplayFogDirty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplayKnowledgeKey {
    Player(AwbwGamePlayerId),
    Team(u8),
}

#[derive(Resource, Default)]
pub struct ReplayTerrainKnowledge {
    pub by_view: HashMap<ReplayKnowledgeKey, HashMap<Position, awbrn_types::GraphicalTerrain>>,
}

impl ReplayTerrainKnowledge {
    pub fn from_map_and_registry(game_map: &GameMap, registry: &ReplayPlayerRegistry) -> Self {
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

/// Update `FogActive` and `FriendlyFactions` from the current `ReplayViewpoint`,
/// then trigger `ReplayFogDirty` so the map is recomputed.
pub fn sync_viewpoint(world: &mut World) {
    let viewpoint = world.resource::<ReplayViewpoint>().clone();
    let fog_enabled = world.resource::<ReplayFogEnabled>().0;
    let active_player_id = world.resource::<ReplayState>().active_player_id;
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

/// Trigger a full fog recompute. Call when weather changes during a replay.
pub fn trigger_fog_recompute_on_weather_change(world: &mut World) {
    world.trigger(ReplayFogDirty);
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
