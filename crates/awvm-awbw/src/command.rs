//! Compatibility lowering for archived AWBW actions.
//!
//! This module only describes the AWVM command represented by a recorded
//! action. It deliberately does not execute that command: archived replays do
//! not carry the random tape or exact HP needed to advance deterministic AWVM
//! state. Recorded outcomes remain authoritative for legacy playback.

use awbw_replay::turn_models::{
    Action, AttackSeamAction, CombatInfo, CombatUnit, FireAction, MoveAction, TargetedPlayer,
    UnitProperty,
};
use awvm::commander::PowerLevel;
use awvm::event::AttackTarget;
use awvm::semantic::{PlayerId, Pos, UnitId};
use awvm::transition::Command;

use crate::targeting::{targeted_hidden, targeted_value, visible_targeted_unit, visible_unit};

/// Lower one archived action to the typed command it records.
///
/// `player` is the active AWVM player at this point in the recorded stream.
/// It is supplied by the caller because AWBW's flattened [`Action`] value does
/// not consistently retain the acting player (notably `End`, `Tag`, and
/// `Delete`).
pub fn diagnostic_command(
    player: PlayerId,
    action: &Action,
) -> Result<Command, CommandAdapterError> {
    let command = match action {
        Action::AttackSeam {
            move_action,
            attack_seam_action,
        } => {
            let (unit, path) = combat_move(
                "AttackSeam",
                move_action.as_ref(),
                seam_attacker(attack_seam_action),
            )?;
            Command::MoveAttack {
                player,
                unit,
                path,
                target: AttackTarget::Tile {
                    position: pos(
                        attack_seam_action.seam_x,
                        attack_seam_action.seam_y,
                        "attack-seam target",
                    )?,
                },
            }
        }
        Action::Build { new_unit, .. } => {
            let unit = visible_unit(new_unit).ok_or(CommandAdapterError::MissingUnit {
                action: "Build",
                role: "produced",
            })?;
            Command::ProduceUnit {
                player,
                position: unit_pos(unit, "build position")?,
                kind: unit.units_name,
            }
        }
        Action::Capt { move_action, .. } => {
            let (unit, path) = required_move("Capt", move_action.as_ref())?;
            Command::MoveCapture { player, unit, path }
        }
        Action::End { .. } => Command::EndTurn { player },
        Action::Fire {
            move_action,
            fire_action,
        } => {
            let (attacker, defender) = fire_combat(fire_action)?;
            let (unit, path) = combat_move("Fire", move_action.as_ref(), attacker)?;
            Command::MoveAttack {
                player,
                unit,
                path,
                target: AttackTarget::Unit {
                    unit: unit_id(defender.units_id),
                },
            }
        }
        Action::Join {
            move_action,
            join_action,
        } => {
            let (unit, path) = required_move("Join", move_action.as_ref())?;
            let joining =
                targeted_hidden(&join_action.join_id).ok_or(CommandAdapterError::MissingUnit {
                    action: "Join",
                    role: "joining",
                })?;
            ensure_same_unit("Join", unit, joining)?;
            let target =
                visible_unit(&join_action.unit).ok_or(CommandAdapterError::MissingUnit {
                    action: "Join",
                    role: "target",
                })?;
            Command::MoveJoin {
                player,
                unit,
                path,
                target: unit_id(target.units_id),
            }
        }
        Action::Launch {
            move_action,
            launch_action,
        } => {
            let (unit, path) = required_move("Launch", move_action.as_ref())?;
            Command::MoveLaunch {
                player,
                unit,
                path,
                target: pos(
                    launch_action.target_x,
                    launch_action.target_y,
                    "launch target",
                )?,
            }
        }
        Action::Load {
            move_action,
            load_action,
        } => {
            let (unit, path) = required_move("Load", move_action.as_ref())?;
            let loaded =
                targeted_hidden(&load_action.loaded).ok_or(CommandAdapterError::MissingUnit {
                    action: "Load",
                    role: "cargo",
                })?;
            ensure_same_unit("Load", unit, loaded.as_u32())?;
            let transport = targeted_hidden(&load_action.transport).ok_or(
                CommandAdapterError::MissingUnit {
                    action: "Load",
                    role: "transport",
                },
            )?;
            Command::MoveLoad {
                player,
                unit,
                path,
                transport: unit_id(transport),
            }
        }
        Action::Move(move_action) => {
            let (unit, path) = lower_move("Move", move_action)?;
            Command::MoveWait { player, unit, path }
        }
        Action::Explode {
            move_action,
            explode_action,
        } => {
            let (move_unit, path) = required_move("Explode", move_action.as_ref())?;
            let recorded_unit = unit_id(explode_action.unit_id);
            if move_unit != recorded_unit {
                return Err(CommandAdapterError::ConflictingUnit {
                    action: "Explode",
                    movement: move_unit.get(),
                    payload: recorded_unit.get(),
                });
            }
            Command::MoveExplode {
                player,
                unit: recorded_unit,
                path,
            }
        }
        Action::Power(power_action) => Command::ActivatePower {
            player,
            level: match power_action.co_power.as_str() {
                "Y" | "Power" => PowerLevel::Cop,
                "S" | "Super" => PowerLevel::Scop,
                value => {
                    return Err(CommandAdapterError::UnknownPowerLevel(value.to_owned()));
                }
            },
        },
        Action::Repair {
            move_action,
            repair_action,
        } => {
            let (unit, path) = required_move("Repair", move_action.as_ref())?;
            let repairing =
                targeted_hidden(&repair_action.unit).ok_or(CommandAdapterError::MissingUnit {
                    action: "Repair",
                    role: "repairing",
                })?;
            ensure_same_unit("Repair", unit, repairing)?;
            let target = targeted_value(&repair_action.repaired)
                .ok_or(CommandAdapterError::MissingUnit {
                    action: "Repair",
                    role: "target",
                })?
                .units_id;
            Command::MoveRepair {
                player,
                unit,
                path,
                target: unit_id(target),
            }
        }
        Action::Resign { .. } => Command::Resign { player },
        Action::Supply {
            move_action,
            supply_action,
        } => {
            let (unit, path) = required_move("Supply", move_action.as_ref())?;
            let recorded_unit =
                targeted_hidden(&supply_action.unit).ok_or(CommandAdapterError::MissingUnit {
                    action: "Supply",
                    role: "supplying",
                })?;
            ensure_same_unit("Supply", unit, recorded_unit)?;
            Command::MoveSupply { player, unit, path }
        }
        Action::Unload {
            unit, transport_id, ..
        } => {
            let cargo = visible_unit(unit).ok_or(CommandAdapterError::MissingUnit {
                action: "Unload",
                role: "cargo",
            })?;
            Command::Unload {
                player,
                transport: unit_id(*transport_id),
                cargo: unit_id(cargo.units_id),
                destination: unit_pos(cargo, "unload destination")?,
            }
        }
        Action::Delete { delete_action } => {
            let unit = delete_action
                .unit_id
                .as_ref()
                .and_then(targeted_hidden)
                .ok_or(CommandAdapterError::MissingUnit {
                    action: "Delete",
                    role: "deleted",
                })?;
            Command::DeleteUnit {
                player,
                unit: unit_id(unit),
            }
        }
        Action::Hide { move_action } => {
            let (unit, path) = required_move("Hide", move_action.as_ref())?;
            Command::MoveHide { player, unit, path }
        }
        Action::Unhide { move_action } => {
            let (unit, path) = required_move("Unhide", move_action.as_ref())?;
            Command::MoveReveal { player, unit, path }
        }
        Action::Tag { .. } => Command::Tag { player },
    };
    Ok(command)
}

/// A named reason why an archived payload cannot describe a typed command.
///
/// The names are intentionally stable so corpus coverage can count and publish
/// every compatibility tail instead of silently dropping malformed or
/// recipient-masked actions.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommandAdapterError {
    #[error("{0} has no movement payload")]
    MissingMove(&'static str),
    #[error("{action} has no visible {role} unit")]
    MissingUnit {
        action: &'static str,
        role: &'static str,
    },
    #[error("{0} has no visible movement path")]
    MissingPath(&'static str),
    #[error("{context} has no recorded {axis} coordinate")]
    MissingCoordinate { context: &'static str, axis: char },
    #[error("{context} coordinate {axis}={value} exceeds AWVM's coordinate domain")]
    Coordinate {
        context: &'static str,
        axis: char,
        value: u32,
    },
    #[error("{action} names movement unit {movement} but payload unit {payload}")]
    ConflictingUnit {
        action: &'static str,
        movement: u32,
        payload: u32,
    },
    #[error("Fire has no combat view with an attacker or defender")]
    MissingCombat,
    #[error("unknown AWBW power level {0:?}")]
    UnknownPowerLevel(String),
}

impl CommandAdapterError {
    /// Stable histogram bucket for compatibility diagnostics.
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::MissingMove(_) => "missing-move",
            Self::MissingUnit { .. } => "missing-unit",
            Self::MissingPath(_) => "missing-path",
            Self::MissingCoordinate { .. } => "missing-coordinate",
            Self::Coordinate { .. } => "coordinate-out-of-range",
            Self::ConflictingUnit { .. } => "conflicting-unit",
            Self::MissingCombat => "missing-combat",
            Self::UnknownPowerLevel(_) => "unknown-power-level",
        }
    }
}

fn required_move(
    action: &'static str,
    move_action: Option<&MoveAction>,
) -> Result<(UnitId, Vec<Pos>), CommandAdapterError> {
    lower_move(
        action,
        move_action.ok_or(CommandAdapterError::MissingMove(action))?,
    )
}

fn lower_move(
    action: &'static str,
    move_action: &MoveAction,
) -> Result<(UnitId, Vec<Pos>), CommandAdapterError> {
    let (target, unit) =
        visible_targeted_unit(&move_action.unit).ok_or(CommandAdapterError::MissingUnit {
            action,
            role: "moving",
        })?;
    let path = move_action
        .paths
        .get(&TargetedPlayer::Global)
        .or_else(|| move_action.paths.get(&target))
        .ok_or(CommandAdapterError::MissingPath(action))?
        .iter()
        .map(|tile| pos(tile.x, tile.y, "movement path"))
        .collect::<Result<Vec<_>, _>>()?;
    if path.is_empty() {
        return Err(CommandAdapterError::MissingPath(action));
    }
    Ok((unit_id(unit.units_id), path))
}

fn combat_move(
    action: &'static str,
    move_action: Option<&MoveAction>,
    attacker: Option<&CombatUnit>,
) -> Result<(UnitId, Vec<Pos>), CommandAdapterError> {
    if let Some(move_action) = move_action {
        let movement = lower_move(action, move_action)?;
        if let Some(attacker) = attacker {
            ensure_same_unit(action, movement.0, attacker.units_id.as_u32())?;
        }
        return Ok(movement);
    }
    let attacker = attacker.ok_or(CommandAdapterError::MissingUnit {
        action,
        role: "attacker",
    })?;
    Ok((
        unit_id(attacker.units_id),
        vec![pos(
            attacker.units_x,
            attacker.units_y,
            "stationary attacker",
        )?],
    ))
}

/// The best archived combat view for a `Fire` action.
///
/// A view naming both units is preferred; a defender-only view still describes
/// the recorded target, so it is the fallback. Every accepted view therefore
/// discloses a defender, which is what the caller needs to name the target.
fn fire_combat(
    action: &FireAction,
) -> Result<(Option<&CombatUnit>, &CombatUnit), CommandAdapterError> {
    let complete = combat_view(action, |combat| {
        combat.attacker.get_value().is_some() && combat.defender.get_value().is_some()
    });
    let combat = complete
        .or_else(|| combat_view(action, |combat| combat.defender.get_value().is_some()))
        .ok_or(CommandAdapterError::MissingCombat)?;
    let defender = combat
        .defender
        .get_value()
        .expect("an accepted combat view discloses its defender");
    Ok((combat.attacker.get_value(), defender))
}

/// Global-first, then first recipient view satisfying `predicate`.
fn combat_view(
    action: &FireAction,
    predicate: impl Fn(&CombatInfo) -> bool,
) -> Option<&CombatInfo> {
    action
        .combat_info_vision
        .get(&TargetedPlayer::Global)
        .map(|view| &view.combat_info)
        .filter(|combat| predicate(combat))
        .or_else(|| {
            action
                .combat_info_vision
                .values()
                .map(|view| &view.combat_info)
                .find(|combat| predicate(combat))
        })
}

fn seam_attacker(action: &AttackSeamAction) -> Option<&CombatUnit> {
    action
        .unit
        .get(&TargetedPlayer::Global)
        .and_then(|view| view.combat_info.get_value())
        .or_else(|| {
            action
                .unit
                .values()
                .find_map(|view| view.combat_info.get_value())
        })
}

fn unit_pos(unit: &UnitProperty, context: &'static str) -> Result<Pos, CommandAdapterError> {
    let x = unit
        .units_x
        .ok_or(CommandAdapterError::MissingCoordinate { context, axis: 'x' })?;
    let y = unit
        .units_y
        .ok_or(CommandAdapterError::MissingCoordinate { context, axis: 'y' })?;
    pos(x, y, context)
}

fn pos(x: u32, y: u32, context: &'static str) -> Result<Pos, CommandAdapterError> {
    Ok(Pos::new(
        u8::try_from(x).map_err(|_| CommandAdapterError::Coordinate {
            context,
            axis: 'x',
            value: x,
        })?,
        u8::try_from(y).map_err(|_| CommandAdapterError::Coordinate {
            context,
            axis: 'y',
            value: y,
        })?,
    ))
}

fn unit_id(id: awbrn_types::AwbwUnitId) -> UnitId {
    UnitId::new(id.as_u32())
}

fn ensure_same_unit(
    action: &'static str,
    movement: UnitId,
    payload: u32,
) -> Result<(), CommandAdapterError> {
    if movement.get() == payload {
        Ok(())
    } else {
        Err(CommandAdapterError::ConflictingUnit {
            action,
            movement: movement.get(),
            payload,
        })
    }
}
