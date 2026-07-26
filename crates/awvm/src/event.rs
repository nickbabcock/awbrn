//! The authoritative facts a successful command produces.
//!
//! `execute` returns events; `observe_events` projects them per recipient, and
//! that projection is the only thing in the crate that reads one. Every variant
//! is one `oneOf` branch of `spec/schema/event.schema.json`, so a payload the
//! schema does not license cannot be constructed, and an event kind the
//! projection forgets stops compiling instead of silently vanishing from a
//! recipient's feed.
//!
//! These are the *authoritative* facts, not what any player sees. They may name
//! units and tiles a recipient cannot observe; withholding that is
//! `observe_events`' job, not this type's.

use serde::{Deserialize, Serialize};

use crate::combat::Weapon;
use crate::commander::{AreaStrikePolicy, PowerLevel};
use crate::semantic::{
    CommanderId, Concealment, Outcome, Phase, PlayerId, PlayerStatus, Position, ReasonId, Silo,
    TeamId, TerrainId, UnitAction, UnitId, UnitKindId, WeatherKind,
};

/// One authoritative fact from executing a command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    PhaseChanged {
        player: PlayerId,
        from: Phase,
        to: Phase,
    },
    /// The turn passed to the player in seat `position`.
    TurnSelected {
        player: PlayerId,
        position: usize,
    },
    DayAdvanced {
        from: u64,
        to: u64,
    },
    /// `path` is the route actually taken, which stops short of the requested
    /// destination when the mover was trapped.
    UnitMoved {
        unit: UnitId,
        from: Position,
        to: Position,
        path: Vec<Position>,
        fuel_spent: u64,
    },
    /// A hidden unit interrupted the mover at `position`.
    MovementTrapped {
        unit: UnitId,
        blocker: UnitId,
        position: Position,
    },
    UnitActionChanged {
        unit: UnitId,
        from: UnitAction,
        to: UnitAction,
        reason: ReasonId,
    },
    UnitCreated {
        unit: UnitId,
        kind: UnitKindId,
        owner: PlayerId,
        position: Position,
    },
    UnitRemoved {
        unit: UnitId,
        reason: ReasonId,
    },
    UnitDamaged {
        unit: UnitId,
        from_hp: u8,
        to_hp: u8,
        reason: ReasonId,
    },
    UnitRepaired {
        unit: UnitId,
        from_hp: u8,
        to_hp: u8,
        reason: ReasonId,
    },
    UnitResourced {
        unit: UnitId,
        fuel_before: u64,
        fuel_after: u64,
        ammo_before: u64,
        ammo_after: u64,
        reason: ReasonId,
    },
    UnitLoaded {
        unit: UnitId,
        transport: UnitId,
        slot: usize,
    },
    UnitUnloaded {
        unit: UnitId,
        transport: UnitId,
        position: Position,
    },
    /// `source` was absorbed into `target` and no longer exists.
    UnitsJoined {
        source: UnitId,
        target: UnitId,
    },
    ConcealmentChanged {
        unit: UnitId,
        from: Concealment,
        to: Concealment,
    },
    /// `None` on either side is an unowned tile.
    TileOwnerChanged {
        position: Position,
        from: Option<PlayerId>,
        to: Option<PlayerId>,
    },
    TileTerrainChanged {
        position: Position,
        from: TerrainId,
        to: TerrainId,
        reason: ReasonId,
    },
    /// Capture progress against the tile's threshold.
    CaptureChanged {
        position: Position,
        from: u8,
        to: u8,
    },
    SiloChanged {
        position: Position,
        from: Silo,
        to: Silo,
    },
    DestructibleDamaged {
        position: Position,
        from_hp: u8,
        to_hp: u8,
    },
    FundsChanged {
        player: PlayerId,
        from: u64,
        to: u64,
        reason: ReasonId,
    },
    /// Which weapon fired and what it was aimed at.
    ///
    /// Deliberately never projected to any recipient: it would disclose an
    /// attack a recipient may not be entitled to see.
    AttackResolved {
        attacker: UnitId,
        weapon: Weapon,
        target: AttackTarget,
    },
    /// One center of a multi-center commander area strike. `strike` is its
    /// index within that activation.
    AreaStrikeResolved {
        strike: usize,
        policy: AreaStrikePolicy,
        center: Position,
        radius: usize,
        damage: u8,
    },
    PowerActivated {
        player: PlayerId,
        commander: CommanderId,
        power: PowerLevel,
    },
    PowerEnded {
        player: PlayerId,
        commander: CommanderId,
        power: PowerLevel,
    },
    PowerChargeChanged {
        player: PlayerId,
        commander_slot: usize,
        from: u64,
        to: u64,
        reason: ReasonId,
    },
    CommanderSwapped {
        player: PlayerId,
        from_slot: usize,
        to_slot: usize,
    },
    WeatherChanged {
        from: WeatherKind,
        to: WeatherKind,
        remaining_turns: u64,
        reason: ReasonId,
    },
    /// The value drawn from the random tape, echoed so a replay can be checked
    /// against it.
    ///
    /// Deliberately never projected to any recipient: the tape is not a
    /// recipient's to read.
    RandomOutcome {
        kind: RandomKind,
        outcome: RandomValue,
    },
    /// Start-of-turn resupply from a property or a supply unit.
    AutomaticSupply {
        source: SupplySource,
        units: Vec<UnitId>,
    },
    AutomaticRepair {
        unit: UnitId,
        position: Position,
        hp_restored: u8,
        cost: u64,
    },
    /// Never emitted: no draw-offer command exists yet. The projection handles
    /// it so that adding the command does not also need a projection change.
    DrawOfferChanged {
        player: PlayerId,
        offered: bool,
    },
    PlayerStatusChanged {
        player: PlayerId,
        from: PlayerStatus,
        to: PlayerStatus,
    },
    TeamEliminated {
        team: TeamId,
        reason: ReasonId,
    },
    MatchCompleted {
        outcome: Outcome,
    },
}

impl Event {
    /// The `type` tag this event serializes under.
    ///
    /// The projection needs the tag as a value — both to label a public event
    /// and as the default reason for events that carry none — and serde only
    /// exposes it by serializing. Written out so that cost is not paid per
    /// event.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::PhaseChanged { .. } => "phase-changed",
            Self::TurnSelected { .. } => "turn-selected",
            Self::DayAdvanced { .. } => "day-advanced",
            Self::UnitMoved { .. } => "unit-moved",
            Self::MovementTrapped { .. } => "movement-trapped",
            Self::UnitActionChanged { .. } => "unit-action-changed",
            Self::UnitCreated { .. } => "unit-created",
            Self::UnitRemoved { .. } => "unit-removed",
            Self::UnitDamaged { .. } => "unit-damaged",
            Self::UnitRepaired { .. } => "unit-repaired",
            Self::UnitResourced { .. } => "unit-resourced",
            Self::UnitLoaded { .. } => "unit-loaded",
            Self::UnitUnloaded { .. } => "unit-unloaded",
            Self::UnitsJoined { .. } => "units-joined",
            Self::ConcealmentChanged { .. } => "concealment-changed",
            Self::TileOwnerChanged { .. } => "tile-owner-changed",
            Self::TileTerrainChanged { .. } => "tile-terrain-changed",
            Self::CaptureChanged { .. } => "capture-changed",
            Self::SiloChanged { .. } => "silo-changed",
            Self::DestructibleDamaged { .. } => "destructible-damaged",
            Self::FundsChanged { .. } => "funds-changed",
            Self::AttackResolved { .. } => "attack-resolved",
            Self::AreaStrikeResolved { .. } => "area-strike-resolved",
            Self::PowerActivated { .. } => "power-activated",
            Self::PowerEnded { .. } => "power-ended",
            Self::PowerChargeChanged { .. } => "power-charge-changed",
            Self::CommanderSwapped { .. } => "commander-swapped",
            Self::WeatherChanged { .. } => "weather-changed",
            Self::RandomOutcome { .. } => "random-outcome",
            Self::AutomaticSupply { .. } => "automatic-supply",
            Self::AutomaticRepair { .. } => "automatic-repair",
            Self::DrawOfferChanged { .. } => "draw-offer-changed",
            Self::PlayerStatusChanged { .. } => "player-status-changed",
            Self::TeamEliminated { .. } => "team-eliminated",
            Self::MatchCompleted { .. } => "match-completed",
        }
    }

    /// Why this happened, for events that say so.
    ///
    /// The projection labels an observed change with this, falling back to
    /// [`Event::kind`] when the event carries no reason of its own.
    pub fn reason(&self) -> &str {
        match self {
            Self::UnitActionChanged { reason, .. }
            | Self::UnitRemoved { reason, .. }
            | Self::UnitDamaged { reason, .. }
            | Self::UnitRepaired { reason, .. }
            | Self::UnitResourced { reason, .. }
            | Self::TileTerrainChanged { reason, .. }
            | Self::FundsChanged { reason, .. }
            | Self::PowerChargeChanged { reason, .. }
            | Self::WeatherChanged { reason, .. }
            | Self::TeamEliminated { reason, .. } => reason.as_str(),
            _ => self.kind(),
        }
    }
}

/// What an attack was aimed at.
///
/// Shared with [`Command`](crate::transition::Command): the target a command
/// names and the target the resulting event reports are the same value, and
/// keeping one type means they cannot describe it differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AttackTarget {
    Unit { unit: UnitId },
    Tile { position: Position },
}

/// What performed a start-of-turn resupply: a supply unit, or the property the
/// unit is standing on.
///
/// Untagged, because the schema admits a bare unit id or position under the
/// same key and their JSON forms — number and two-element array — are disjoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SupplySource {
    Unit(UnitId),
    Tile(Position),
}

/// Which decision a random token was drawn for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RandomKind {
    WeatherSelection,
}

/// The value a random draw produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RandomValue {
    Text(String),
    Integer(i64),
    Flag(bool),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn round_trip(event: Event) -> Value {
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            serde_json::from_value::<Event>(value.clone()).unwrap(),
            event
        );
        value
    }

    #[test]
    fn attack_targets_keep_their_tag() {
        assert_eq!(
            round_trip(Event::AttackResolved {
                attacker: UnitId::new(0),
                weapon: Weapon::Unlimited,
                target: AttackTarget::Unit {
                    unit: UnitId::new(1)
                },
            }),
            json!({
                "type":"attack-resolved","attacker":0,"weapon":"unlimited",
                "target":{"type":"unit","unit":1}
            })
        );
        assert_eq!(
            round_trip(Event::AttackResolved {
                attacker: UnitId::new(0),
                weapon: Weapon::Ammo,
                target: AttackTarget::Tile { position: [2, 3] },
            }),
            json!({
                "type":"attack-resolved","attacker":0,"weapon":"ammo",
                "target":{"type":"tile","position":[2,3]}
            })
        );
    }

    /// A supply source is told apart by JSON shape alone, so each form must
    /// decode back to the branch it was written from.
    #[test]
    fn supply_sources_round_trip_through_their_untagged_forms() {
        assert_eq!(
            round_trip(Event::AutomaticSupply {
                source: SupplySource::Unit(UnitId::new(4)),
                units: vec![UnitId::new(5)],
            }),
            json!({"type":"automatic-supply","source":4,"units":[5]})
        );
        assert_eq!(
            round_trip(Event::AutomaticSupply {
                source: SupplySource::Tile([1, 1]),
                units: vec![UnitId::new(5)],
            }),
            json!({"type":"automatic-supply","source":[1,1],"units":[5]})
        );
    }

    #[test]
    fn unowned_tiles_serialize_their_owner_as_null() {
        assert_eq!(
            round_trip(Event::TileOwnerChanged {
                position: [0, 0],
                from: None,
                to: Some(PlayerId::from("red")),
            }),
            json!({"type":"tile-owner-changed","position":[0,0],"from":null,"to":"red"})
        );
    }

    /// Every `kind` must be the tag serde actually writes, since the projection
    /// puts it on the wire as a public event's label.
    #[test]
    fn kind_matches_the_serialized_tag() {
        for event in [
            Event::DayAdvanced { from: 1, to: 2 },
            Event::UnitsJoined {
                source: UnitId::new(0),
                target: UnitId::new(1),
            },
            Event::MatchCompleted {
                outcome: Outcome::Cancelled {
                    reason: ReasonId::from("aborted"),
                },
            },
            Event::RandomOutcome {
                kind: RandomKind::WeatherSelection,
                outcome: RandomValue::Text("rain".into()),
            },
        ] {
            let kind = event.kind();
            assert_eq!(round_trip(event)["type"], json!(kind));
        }
    }

    /// Events without a `reason` field label their observed change with their
    /// own kind, which is what the projection relies on.
    #[test]
    fn reason_falls_back_to_the_event_kind() {
        assert_eq!(
            Event::UnitRemoved {
                unit: UnitId::new(0),
                reason: ReasonId::from("fuel-depleted"),
            }
            .reason(),
            "fuel-depleted"
        );
        assert_eq!(
            Event::UnitsJoined {
                source: UnitId::new(0),
                target: UnitId::new(1),
            }
            .reason(),
            "units-joined"
        );
    }
}
