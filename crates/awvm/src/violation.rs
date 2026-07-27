//! The closed set of reasons `validate` rejects a command.
//!
//! A violation is a leaf value: `transition` constructs one, `protocol` puts it
//! on the wire, and nothing in between reads it. It carries a stable `code` and
//! only the payload `spec/schema/violation.schema.json` licenses for that code —
//! no prose, since human-readable messages are adapter-owned and key off the
//! code (`spec/model/violations.md`).
//!
//! Every variant here is one `oneOf` branch of that schema, so a payload the
//! schema does not license cannot be constructed. Field order matches the
//! schema's, which keeps the serialized bytes identical to the hand-written
//! `json!` literals this replaced.

use serde::{Deserialize, Serialize};

use crate::semantic::{Phase, PlayerId, Pos, UnitId, UnitKindId};

/// One primary rejection from `validate`.
///
/// Validation returns exactly one of these; checks within a command family run
/// in a documented order and stop at the first failure, so this is never a set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Violation {
    /// The submitter lacks the authority the command requires. Checked before
    /// any state-dependent rejection, and never reveals hidden state.
    AuthorityRequired {
        authority: Authority,
    },
    /// Gameplay is terminal.
    MatchFinished,
    WrongPhase {
        expected: Phase,
        actual: Phase,
    },
    NotActivePlayer {
        player: PlayerId,
    },
    UnitNotFound {
        unit: UnitId,
    },
    /// The acting unit is cargo.
    UnitNotOnBoard {
        unit: UnitId,
    },
    /// The acting unit is not `ready`.
    UnitAlreadyActed {
        unit: UnitId,
    },
    UnitNotOwned {
        unit: UnitId,
        player: PlayerId,
    },
    /// The path's first position is not where the unit stands.
    PathOriginMismatch {
        expected: Pos,
        actual: Pos,
    },
    /// The step ending at `index` is not orthogonally adjacent.
    PathNonAdjacent {
        index: usize,
        from: Pos,
        to: Pos,
    },
    PathRepeatedPosition {
        index: usize,
        position: Pos,
        first_index: usize,
    },
    PathOutOfBounds {
        index: usize,
        position: Pos,
    },
    /// The mover cannot enter this terrain.
    ///
    /// `index` is present when the tile is a step of a path being validated and
    /// absent when the tile was reached some other way, which is the difference
    /// between the schema's `index-position` and `terrain-position` branches.
    TerrainImpassable {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        position: Pos,
    },
    /// A disclosed intermediate obstruction blocks the path.
    PathOccupied {
        index: usize,
        position: Pos,
    },
    InsufficientMovement {
        required: u64,
        available: u64,
    },
    InsufficientFuel {
        required: u64,
        available: u64,
    },
    InsufficientFunds {
        required: u64,
        available: u64,
    },
    InsufficientPower {
        required: u64,
        available: u64,
    },
    /// Disclosed occupancy of the destination forbids the action.
    DestinationOccupied {
        position: Pos,
    },
    /// The target is absent or inapplicable.
    ///
    /// `target` is optional because a command may name no target at all, or one
    /// that must not be disclosed.
    InvalidTarget {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<Target>,
    },
    TargetOutOfRange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<Target>,
    },
    /// The unit lacks the requested capability.
    ActionNotSupported {
        action: Action,
    },
    /// The actor already owns at least the configured maximum number of units.
    UnitLimitReached {
        current: u64,
        limit: u64,
    },
}

/// The principal a command requires in order to be submitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Authority {
    Player,
    MatchAuthority,
}

/// What a rejected command was aimed at.
///
/// Untagged, because the schema admits a bare unit id, unit kind, or position
/// under the same key: their JSON forms — number, string, and two-element array
/// — are disjoint, so the shape alone identifies the branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Unit(UnitId),
    Kind(UnitKindId),
    Pos(Pos),
}

impl From<UnitId> for Target {
    fn from(unit: UnitId) -> Self {
        Self::Unit(unit)
    }
}

impl From<UnitKindId> for Target {
    fn from(kind: UnitKindId) -> Self {
        Self::Kind(kind)
    }
}

impl From<Pos> for Target {
    fn from(position: Pos) -> Self {
        Self::Pos(position)
    }
}

/// The action a unit was asked for and cannot perform.
///
/// The specification types this as an open identifier; enumerating the ones
/// this implementation can actually refuse keeps the value from drifting into a
/// typo, and adding a command means adding a variant here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    ActivatePower,
    Attack,
    Capture,
    MoveAndFire,
    MoveExplode,
    MoveHide,
    MoveLaunch,
    MoveRepair,
    MoveReveal,
    MoveSupply,
    Tag,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn wire(violation: Violation) -> Value {
        serde_json::to_value(&violation).unwrap()
    }

    /// A violation must survive the wire it is defined by: the schema is the
    /// contract, so anything we emit we must also be able to read back.
    fn round_trip(violation: Violation) -> Value {
        let value = wire(violation.clone());
        assert_eq!(
            serde_json::from_value::<Violation>(value.clone()).unwrap(),
            violation
        );
        value
    }

    #[test]
    fn payload_free_codes_carry_only_the_code() {
        assert_eq!(
            round_trip(Violation::MatchFinished),
            json!({"code":"MATCH_FINISHED"})
        );
    }

    /// `protocol` embeds the violation in a `serde_json` object, whose map is a
    /// `BTreeMap`, so the bytes are key-sorted no matter what order the variant
    /// declares. Pinned here because it is the reason this migration is
    /// invisible on the wire.
    #[test]
    fn payload_keys_reach_the_wire_sorted() {
        assert_eq!(
            round_trip(Violation::PathRepeatedPosition {
                index: 2,
                position: Pos::new(3, 4),
                first_index: 0,
            })
            .to_string(),
            r#"{"code":"PATH_REPEATED_POSITION","first_index":0,"index":2,"position":[3,4]}"#
        );
    }

    #[test]
    fn impassable_terrain_omits_the_index_when_it_has_none() {
        assert_eq!(
            round_trip(Violation::TerrainImpassable {
                index: None,
                position: Pos::new(1, 2),
            }),
            json!({"code":"TERRAIN_IMPASSABLE","position":Pos::new(1, 2)})
        );
        assert_eq!(
            round_trip(Violation::TerrainImpassable {
                index: Some(1),
                position: Pos::new(1, 2),
            }),
            json!({"code":"TERRAIN_IMPASSABLE","index":1,"position":Pos::new(1, 2)})
        );
    }

    /// The three target forms are told apart by JSON shape alone, so each must
    /// decode back to the branch it was written from.
    #[test]
    fn targets_round_trip_through_their_untagged_forms() {
        for (target, expected) in [
            (Target::Unit(UnitId::new(7)), json!(7)),
            (Target::Kind(UnitKindId::Infantry), json!("infantry")),
            (Target::Pos(Pos::new(2, 5)), json!(Pos::new(2, 5))),
        ] {
            let violation = Violation::InvalidTarget {
                target: Some(target),
            };
            assert_eq!(
                round_trip(violation),
                json!({"code":"INVALID_TARGET","target":expected})
            );
        }
        assert_eq!(
            round_trip(Violation::InvalidTarget { target: None }),
            json!({"code":"INVALID_TARGET"})
        );
    }

    #[test]
    fn actions_serialize_as_the_command_names_they_refuse() {
        assert_eq!(
            round_trip(Violation::ActionNotSupported {
                action: Action::MoveAndFire
            }),
            json!({"code":"ACTION_NOT_SUPPORTED","action":"move-and-fire"})
        );
    }
}
