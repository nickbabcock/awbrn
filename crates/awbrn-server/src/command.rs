use awbrn_map::Pos;
pub use awbrn_protocol::PostMoveAction;
pub use awvm::commander::PowerLevel;
use tsify::Tsify;

use crate::unit_id::ServerUnitId;

/// A command submitted by a player during their turn.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Tsify)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GameCommand {
    /// Move a unit along a path, optionally performing an action at the destination.
    MoveUnit {
        #[tsify(type = "number")]
        unit_id: ServerUnitId,
        /// Full path from current position to destination (inclusive of both endpoints).
        /// Used for fuel consumption and client animation.
        #[tsify(type = "{ x: number; y: number }[]")]
        #[serde(with = "awbrn_map::xy::vec")]
        path: Vec<Pos>,
        /// Action to perform after arriving at the destination.
        action: Option<PostMoveAction>,
    },
    /// Build a new unit at a production facility.
    Build {
        #[tsify(type = "{ x: number; y: number }")]
        #[serde(with = "awbrn_map::xy")]
        position: Pos,
        unit_type: awvm::ruleset::UnitKind,
    },
    /// Unload one carried unit without moving or spending its transport.
    Unload {
        #[tsify(type = "number")]
        transport_id: ServerUnitId,
        #[tsify(type = "number")]
        cargo_id: ServerUnitId,
        #[tsify(type = "{ x: number; y: number }")]
        #[serde(with = "awbrn_map::xy")]
        position: Pos,
    },
    /// Remove one owned unit from the board without compensation.
    DeleteUnit {
        #[tsify(type = "number")]
        unit_id: ServerUnitId,
    },
    /// Activate the current commander's normal or super power.
    ActivatePower { level: PowerLevel },
    /// End the current player's turn.
    EndTurn,
}

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
                assert_eq!(path[1], Pos::new(2, 2));
                match action.unwrap() {
                    PostMoveAction::Attack { target } => assert_eq!(target, Pos::new(3, 2)),
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
    fn deserialize_standalone_unload() {
        let json = r#"{"type":"unload","transport_id":4,"cargo_id":7,"position":{"x":2,"y":3}}"#;
        let command: GameCommand = serde_json::from_str(json).unwrap();
        assert_eq!(
            command,
            GameCommand::Unload {
                transport_id: ServerUnitId(4),
                cargo_id: ServerUnitId(7),
                position: Pos::new(2, 3),
            }
        );
    }

    #[test]
    fn deserialize_activate_power() {
        let json = r#"{"type":"activatePower","level":"scop"}"#;
        let cmd: GameCommand = serde_json::from_str(json).unwrap();
        assert_eq!(
            cmd,
            GameCommand::ActivatePower {
                level: PowerLevel::Scop
            }
        );
    }

    #[test]
    fn deserialize_delete_unit() {
        let json = r#"{"type":"deleteUnit","unit_id":9}"#;
        let command: GameCommand = serde_json::from_str(json).unwrap();
        assert_eq!(
            command,
            GameCommand::DeleteUnit {
                unit_id: ServerUnitId(9)
            }
        );
    }

    #[test]
    fn deserialize_legacy_move_with_unload() {
        let json = r#"{"type":"moveUnit","unit_id":4,"path":[{"x":1,"y":2}],"action":{"type":"unload","cargo_id":7,"position":{"x":2,"y":2}}}"#;
        let command: GameCommand = serde_json::from_str(json).unwrap();
        assert_eq!(
            command,
            GameCommand::MoveUnit {
                unit_id: ServerUnitId(4),
                path: vec![Pos::new(1, 2)],
                action: Some(PostMoveAction::Unload {
                    cargo_id: 7,
                    position: Pos::new(2, 2),
                }),
            }
        );
    }

    #[test]
    fn deserialize_build() {
        let json = r#"{"type":"build","position":{"x":0,"y":0},"unit_type":"infantry"}"#;
        let cmd: GameCommand = serde_json::from_str(json).unwrap();
        match cmd {
            GameCommand::Build {
                position,
                unit_type,
            } => {
                assert_eq!(position, Pos::new(0, 0));
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
    fn deserialize_special_post_move_actions() {
        let repair: PostMoveAction =
            serde_json::from_str(r#"{"type":"repair","target_id":17}"#).unwrap();
        assert_eq!(repair, PostMoveAction::Repair { target_id: 17 });

        let launch: PostMoveAction =
            serde_json::from_str(r#"{"type":"launch","target":{"x":3,"y":4}}"#).unwrap();
        assert_eq!(
            launch,
            PostMoveAction::Launch {
                target: Pos::new(3, 4)
            }
        );

        let explode: PostMoveAction = serde_json::from_str(r#"{"type":"explode"}"#).unwrap();
        assert_eq!(explode, PostMoveAction::Explode);
    }

    #[test]
    fn wrong_tag_is_rejected() {
        let json = r#"{"type":"MoveUnit","unitId":1,"path":[],"action":null}"#;
        assert!(serde_json::from_str::<GameCommand>(json).is_err());
    }
}
