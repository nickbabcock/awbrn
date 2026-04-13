use std::collections::HashSet;
use std::num::NonZeroU8;

use awbrn_types::{Co, CoStats, PlayerFaction};

/// Opaque player identifier assigned by the server at game creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerId(pub u8);

/// A player slot in the game.
#[derive(Debug, Clone)]
pub struct PlayerSlot {
    pub id: PlayerId,
    pub faction: PlayerFaction,
    /// Team identifier. `None` means FFA (no team).
    pub team: Option<NonZeroU8>,
    pub funds: u32,
    pub eliminated: bool,
    pub co: Co,
}

/// Manages the set of players and turn order. Stored as a Bevy resource.
#[derive(Debug, bevy::prelude::Resource)]
pub struct PlayerRegistry {
    players: Vec<PlayerSlot>,
}

impl PlayerRegistry {
    pub fn new(players: Vec<PlayerSlot>) -> Self {
        Self { players }
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    pub fn get(&self, player: PlayerId) -> Option<&PlayerSlot> {
        self.players.iter().find(|p| p.id == player)
    }

    pub fn get_mut(&mut self, player: PlayerId) -> Option<&mut PlayerSlot> {
        self.players.iter_mut().find(|p| p.id == player)
    }

    pub fn faction_for_player(&self, player: PlayerId) -> Option<PlayerFaction> {
        self.get(player).map(|p| p.faction)
    }

    pub fn co_stats_for_player(&self, player: PlayerId) -> Option<CoStats> {
        self.get(player).map(|slot| slot.co.stats())
    }

    /// Get the set of factions friendly to the given player (same team, or just
    /// the player's own faction if FFA).
    pub fn friendly_factions_for_player(&self, player: PlayerId) -> HashSet<PlayerFaction> {
        let Some(slot) = self.get(player) else {
            return HashSet::new();
        };

        let Some(team) = slot.team else {
            return HashSet::from([slot.faction]);
        };

        self.players
            .iter()
            .filter(|p| p.team == Some(team))
            .map(|p| p.faction)
            .collect()
    }

    /// Get the position index of a player in the turn order.
    pub fn player_index(&self, player: PlayerId) -> Option<usize> {
        self.players.iter().position(|p| p.id == player)
    }

    /// Get the player who owns the given faction.
    pub fn player_for_faction(&self, faction: PlayerFaction) -> Option<PlayerId> {
        self.players
            .iter()
            .find(|p| p.faction == faction)
            .map(|p| p.id)
    }

    /// Get the next player in turn order after the given player.
    /// Skips eliminated players. Returns `None` if no active players remain.
    pub fn next_active_player_after(&self, current: PlayerId) -> Option<PlayerId> {
        let current_idx = self.players.iter().position(|p| p.id == current)?;
        let count = self.players.len();
        for offset in 1..=count {
            let idx = (current_idx + offset) % count;
            if !self.players[idx].eliminated {
                return Some(self.players[idx].id);
            }
        }
        None
    }

    pub fn players(&self) -> &[PlayerSlot] {
        &self.players
    }
}
