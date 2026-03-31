use awbrn_map::Position;

use crate::unit_id::ServerUnitId;

/// A command submitted by a player during their turn.
#[derive(Debug, Clone)]
pub enum GameCommand {
    /// Move a unit along a path, optionally performing an action at the destination.
    MoveUnit {
        unit_id: ServerUnitId,
        /// Full path from current position to destination (inclusive of both endpoints).
        /// Used for fuel consumption and client animation.
        path: Vec<Position>,
        /// Action to perform after arriving at the destination.
        action: Option<PostMoveAction>,
    },
    /// Build a new unit at a production facility.
    Build {
        position: Position,
        unit_type: awbrn_types::Unit,
    },
    /// End the current player's turn.
    EndTurn,
}

/// An action to perform after a unit moves.
#[derive(Debug, Clone)]
pub enum PostMoveAction {
    /// Attack a target at the given position.
    Attack { target: Position },
    /// Begin or continue capturing the building at the unit's destination.
    Capture,
    /// Load into a transport at the unit's destination.
    Load { transport_id: ServerUnitId },
    /// Unload a carried unit to the given position.
    Unload {
        cargo_id: ServerUnitId,
        position: Position,
    },
    /// Supply adjacent friendly units (APC ability).
    Supply,
    /// Dive / activate stealth.
    Hide,
    /// Surface / deactivate stealth.
    Unhide,
    /// Join with a friendly unit of the same type at the destination.
    Join { target_id: ServerUnitId },
    /// Wait at the destination (do nothing).
    Wait,
}
