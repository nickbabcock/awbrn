use awbrn_map::Pos;
pub use awbrn_protocol::PostMoveAction;
pub use awvm::commander::PowerLevel;
use awvm::event::AttackTarget;
use awvm::semantic::{Location, State, UnitId};
use awvm::transition::Command;

use crate::unit_id::ServerUnitId;

/// A command submitted by a player during their turn.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GameCommand {
    /// Move a unit along a path, optionally performing an action at the destination.
    MoveUnit {
        #[cfg_attr(feature = "typescript", tsify(type = "number"))]
        unit_id: ServerUnitId,
        /// Full path from current position to destination (inclusive of both endpoints).
        /// Used for fuel consumption and client animation.
        #[cfg_attr(feature = "typescript", tsify(type = "{ x: number; y: number }[]"))]
        #[serde(with = "awbrn_map::xy::vec")]
        path: Vec<Pos>,
        /// Action to perform after arriving at the destination.
        action: Option<PostMoveAction>,
    },
    /// Build a new unit at a production facility.
    Build {
        #[cfg_attr(feature = "typescript", tsify(type = "{ x: number; y: number }"))]
        #[serde(with = "awbrn_map::xy")]
        position: Pos,
        unit_type: awvm::ruleset::UnitKind,
    },
    /// Unload one carried unit without moving or spending its transport.
    Unload {
        #[cfg_attr(feature = "typescript", tsify(type = "number"))]
        transport_id: ServerUnitId,
        #[cfg_attr(feature = "typescript", tsify(type = "number"))]
        cargo_id: ServerUnitId,
        #[cfg_attr(feature = "typescript", tsify(type = "{ x: number; y: number }"))]
        #[serde(with = "awbrn_map::xy")]
        position: Pos,
    },
    /// Remove one owned unit from the board without compensation.
    DeleteUnit {
        #[cfg_attr(feature = "typescript", tsify(type = "number"))]
        unit_id: ServerUnitId,
    },
    /// Activate the current commander's normal or super power.
    ActivatePower { level: PowerLevel },
    /// End the current player's turn.
    EndTurn,
    /// Give the match up and leave it.
    ///
    /// The one order a seat may send while another seat holds the turn. What
    /// it does to the board is the same either way — the seat is eliminated
    /// and its holdings are disposed of — but only a resignation on the
    /// player's own turn hands the turn on, because only that one crosses a
    /// turn boundary.
    Resign,
    /// Remove the current player because their clock ran out.
    ///
    /// The host issues this when the match clock expires. A player cannot send
    /// it: the match durable object rejects it on the player websocket.
    Timeout,
}

/// A command AWVM accepts that has no wire spelling.
///
/// The wire vocabulary is what a client may send, and it is smaller than the
/// reducer's own. A command outside it never came from a client, so this is a
/// fault in whatever built it rather than a rules violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedCommand {
    /// The command's variant, for a message a person reads.
    pub command: &'static str,
}

impl std::fmt::Display for UnmappedCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} has no wire command", self.command)
    }
}

impl std::error::Error for UnmappedCommand {}

/// The wire command one AWVM command is, read against the position it acts on.
///
/// The inverse of the expansion the authority does when it accepts a wire
/// command. It exists for a seat that decides in AWVM's own vocabulary — an AI
/// — and has to leave the same record in the log a person's seat leaves, so a
/// match with an AI in it replays through the one path every match replays
/// through.
///
/// `state` is the position the command was chosen against, which is where an
/// attack on a unit reads back as the tile that unit stands on. A command that
/// names a target no longer there reads as a tile, which is what the authority
/// makes of that command anyway.
pub fn game_command(command: &Command, state: &State) -> Result<GameCommand, UnmappedCommand> {
    let unmapped = |command: &'static str| Err(UnmappedCommand { command });
    let moved = |unit: &UnitId, path: &[Pos], action: PostMoveAction| GameCommand::MoveUnit {
        unit_id: ServerUnitId(u64::from(unit.get())),
        path: path.to_vec(),
        action: Some(action),
    };

    Ok(match command {
        Command::MoveWait { unit, path, .. } => moved(unit, path, PostMoveAction::Wait),
        Command::MoveCapture { unit, path, .. } => moved(unit, path, PostMoveAction::Capture),
        Command::MoveSupply { unit, path, .. } => moved(unit, path, PostMoveAction::Supply),
        Command::MoveHide { unit, path, .. } => moved(unit, path, PostMoveAction::Hide),
        Command::MoveReveal { unit, path, .. } => moved(unit, path, PostMoveAction::Unhide),
        Command::MoveExplode { unit, path, .. } => moved(unit, path, PostMoveAction::Explode),
        Command::MoveLaunch {
            unit, path, target, ..
        } => moved(unit, path, PostMoveAction::Launch { target: *target }),
        Command::MoveJoin {
            unit, path, target, ..
        } => moved(
            unit,
            path,
            PostMoveAction::Join {
                target_id: u64::from(target.get()),
            },
        ),
        Command::MoveRepair {
            unit, path, target, ..
        } => moved(
            unit,
            path,
            PostMoveAction::Repair {
                target_id: u64::from(target.get()),
            },
        ),
        Command::MoveLoad {
            unit,
            path,
            transport,
            ..
        } => moved(
            unit,
            path,
            PostMoveAction::Load {
                transport_id: u64::from(transport.get()),
            },
        ),
        Command::MoveAttack {
            unit, path, target, ..
        } => {
            // The wire names the tile fired on. The authority reads the unit
            // standing there back out of it, so a target that has since left
            // the board spells as the tile it occupied, which is the same
            // command the authority would have built from that tile.
            let position = match target {
                AttackTarget::Tile { position } => *position,
                AttackTarget::Unit { unit } => match state.units.get(*unit).map(|u| &u.location) {
                    Some(&Location::Board { position }) => position,
                    _ => return unmapped("an attack on a unit that is not on the board"),
                },
            };
            moved(unit, path, PostMoveAction::Attack { target: position })
        }
        Command::Unload {
            transport,
            cargo,
            destination,
            ..
        } => GameCommand::Unload {
            transport_id: ServerUnitId(u64::from(transport.get())),
            cargo_id: ServerUnitId(u64::from(cargo.get())),
            position: *destination,
        },
        Command::DeleteUnit { unit, .. } => GameCommand::DeleteUnit {
            unit_id: ServerUnitId(u64::from(unit.get())),
        },
        Command::ProduceUnit { position, kind, .. } => GameCommand::Build {
            position: *position,
            unit_type: *kind,
        },
        Command::ActivatePower { level, .. } => GameCommand::ActivatePower { level: *level },
        Command::EndTurn { .. } => GameCommand::EndTurn,
        Command::Timeout { .. } => GameCommand::Timeout,
        Command::Tag { .. } => return unmapped("a commander tag"),
        Command::Resign { .. } => GameCommand::Resign,
        Command::Unsupported => return unmapped("an unsupported command"),
    })
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
        serde_json::from_str::<GameCommand>(json).unwrap_err();
    }
}
