use awbrn_map::Position;

use crate::unit_id::ServerUnitId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_move_unit_with_attack() {
        // Variant names are camelCase; field names are snake_case (no field-level rename_all).
        let json = r#"{"type":"moveUnit","unit_id":3,"path":[{"x":1,"y":2},{"x":2,"y":2}],"action":{"type":"attack","target":{"x":3,"y":2}}}"#;
        let cmd: GameCommand = serde_json::from_str(json).unwrap();
        match cmd {
            GameCommand::MoveUnit {
                unit_id,
                path,
                action,
            } => {
                assert_eq!(unit_id, ServerUnitId(3));
                assert_eq!(path.len(), 2);
                assert_eq!(path[1], Position::new(2, 2));
                match action.unwrap() {
                    PostMoveAction::Attack { target } => assert_eq!(target, Position::new(3, 2)),
                    other => panic!("expected Attack, got {other:?}"),
                }
            }
            other => panic!("expected MoveUnit, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_move_unit_with_wait() {
        let json =
            r#"{"type":"moveUnit","unit_id":1,"path":[{"x":0,"y":0}],"action":{"type":"wait"}}"#;
        let cmd: GameCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(
            cmd,
            GameCommand::MoveUnit {
                action: Some(PostMoveAction::Wait),
                ..
            }
        ));
    }

    #[test]
    fn deserialize_end_turn() {
        let json = r#"{"type":"endTurn"}"#;
        let cmd: GameCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, GameCommand::EndTurn));
    }

    #[test]
    fn deserialize_build() {
        let json = r#"{"type":"build","position":{"x":0,"y":0},"unit_type":"Infantry"}"#;
        let cmd: GameCommand = serde_json::from_str(json).unwrap();
        match cmd {
            GameCommand::Build {
                position,
                unit_type,
            } => {
                assert_eq!(position, Position::new(0, 0));
                assert_eq!(unit_type, awbrn_types::Unit::Infantry);
            }
            other => panic!("expected Build, got {other:?}"),
        }
    }

    #[test]
    fn deserialize_post_move_action_capture() {
        let json = r#"{"type":"capture"}"#;
        let action: PostMoveAction = serde_json::from_str(json).unwrap();
        assert!(matches!(action, PostMoveAction::Capture));
    }

    #[test]
    fn wrong_tag_is_rejected() {
        let json = r#"{"type":"MoveUnit","unitId":1,"path":[],"action":null}"#;
        assert!(serde_json::from_str::<GameCommand>(json).is_err());
    }
}

/// A command submitted by a player during their turn.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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
