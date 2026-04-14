use bevy::prelude::*;

use crate::player::PlayerId;

/// The phase of the current game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TurnPhase {
    /// The active player is issuing commands.
    PlayerTurn,
    /// The game has ended.
    GameOver { winner: Option<PlayerId> },
}

/// Authoritative game state resource stored in the server's World.
#[derive(Debug, Resource)]
pub struct ServerGameState {
    pub day: u32,
    pub active_player: PlayerId,
    pub phase: TurnPhase,
    /// Monotonic counter for assigning [`crate::ServerUnitId`] values.
    pub next_unit_id: u64,
}

impl ServerGameState {
    pub fn allocate_unit_id(&mut self) -> crate::ServerUnitId {
        let id = crate::ServerUnitId(self.next_unit_id);
        self.next_unit_id += 1;
        id
    }
}
