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
    CommanderId, Concealment, Outcome, Phase, PlayerId, PlayerStatus, Pos, Reason, Silo, TeamId,
    TerrainId, UnitAction, UnitId, UnitKindId, WeatherKind,
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
        from: Pos,
        to: Pos,
        path: Vec<Pos>,
        fuel_spent: u64,
    },
    /// A hidden unit interrupted the mover at `position`.
    MovementTrapped {
        unit: UnitId,
        blocker: UnitId,
        position: Pos,
    },
    UnitActionChanged {
        unit: UnitId,
        from: UnitAction,
        to: UnitAction,
        reason: Reason,
    },
    UnitCreated {
        unit: UnitId,
        kind: UnitKindId,
        owner: PlayerId,
        position: Pos,
    },
    UnitRemoved {
        unit: UnitId,
        reason: Reason,
    },
    UnitDamaged {
        unit: UnitId,
        from_hp: u8,
        to_hp: u8,
        reason: Reason,
    },
    UnitRepaired {
        unit: UnitId,
        from_hp: u8,
        to_hp: u8,
        reason: Reason,
    },
    UnitResourced {
        unit: UnitId,
        fuel_before: u64,
        fuel_after: u64,
        ammo_before: u64,
        ammo_after: u64,
        reason: Reason,
    },
    UnitLoaded {
        unit: UnitId,
        transport: UnitId,
        slot: usize,
    },
    UnitUnloaded {
        unit: UnitId,
        transport: UnitId,
        position: Pos,
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
        position: Pos,
        from: Option<PlayerId>,
        to: Option<PlayerId>,
    },
    TileTerrainChanged {
        position: Pos,
        from: TerrainId,
        to: TerrainId,
        reason: Reason,
    },
    /// Capture progress against the tile's threshold.
    CaptureChanged {
        position: Pos,
        from: u8,
        to: u8,
    },
    SiloChanged {
        position: Pos,
        from: Silo,
        to: Silo,
    },
    DestructibleDamaged {
        position: Pos,
        from_hp: u8,
        to_hp: u8,
    },
    FundsChanged {
        player: PlayerId,
        from: u64,
        to: u64,
        reason: Reason,
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
        center: Pos,
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
        reason: Reason,
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
        reason: Reason,
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
        position: Pos,
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
        reason: Reason,
    },
    MatchCompleted {
        outcome: Outcome,
    },
}

impl Event {
    /// Which fact this is.
    ///
    /// The projection needs the discriminant as a value — both to label a public
    /// event and as the default reason for events that carry none — and serde
    /// only exposes the `type` tag by serializing.
    pub const fn kind(&self) -> EventKind {
        match self {
            Self::PhaseChanged { .. } => EventKind::PhaseChanged,
            Self::TurnSelected { .. } => EventKind::TurnSelected,
            Self::DayAdvanced { .. } => EventKind::DayAdvanced,
            Self::UnitMoved { .. } => EventKind::UnitMoved,
            Self::MovementTrapped { .. } => EventKind::MovementTrapped,
            Self::UnitActionChanged { .. } => EventKind::UnitActionChanged,
            Self::UnitCreated { .. } => EventKind::UnitCreated,
            Self::UnitRemoved { .. } => EventKind::UnitRemoved,
            Self::UnitDamaged { .. } => EventKind::UnitDamaged,
            Self::UnitRepaired { .. } => EventKind::UnitRepaired,
            Self::UnitResourced { .. } => EventKind::UnitResourced,
            Self::UnitLoaded { .. } => EventKind::UnitLoaded,
            Self::UnitUnloaded { .. } => EventKind::UnitUnloaded,
            Self::UnitsJoined { .. } => EventKind::UnitsJoined,
            Self::ConcealmentChanged { .. } => EventKind::ConcealmentChanged,
            Self::TileOwnerChanged { .. } => EventKind::TileOwnerChanged,
            Self::TileTerrainChanged { .. } => EventKind::TileTerrainChanged,
            Self::CaptureChanged { .. } => EventKind::CaptureChanged,
            Self::SiloChanged { .. } => EventKind::SiloChanged,
            Self::DestructibleDamaged { .. } => EventKind::DestructibleDamaged,
            Self::FundsChanged { .. } => EventKind::FundsChanged,
            Self::AttackResolved { .. } => EventKind::AttackResolved,
            Self::AreaStrikeResolved { .. } => EventKind::AreaStrikeResolved,
            Self::PowerActivated { .. } => EventKind::PowerActivated,
            Self::PowerEnded { .. } => EventKind::PowerEnded,
            Self::PowerChargeChanged { .. } => EventKind::PowerChargeChanged,
            Self::CommanderSwapped { .. } => EventKind::CommanderSwapped,
            Self::WeatherChanged { .. } => EventKind::WeatherChanged,
            Self::RandomOutcome { .. } => EventKind::RandomOutcome,
            Self::AutomaticSupply { .. } => EventKind::AutomaticSupply,
            Self::AutomaticRepair { .. } => EventKind::AutomaticRepair,
            Self::DrawOfferChanged { .. } => EventKind::DrawOfferChanged,
            Self::PlayerStatusChanged { .. } => EventKind::PlayerStatusChanged,
            Self::TeamEliminated { .. } => EventKind::TeamEliminated,
            Self::MatchCompleted { .. } => EventKind::MatchCompleted,
        }
    }

    /// Why this happened, for events that say so.
    ///
    /// The projection labels an observed change with this, falling back to
    /// [`Event::kind`] when the event carries no reason of its own — which
    /// `spec/schema/observed-event.schema.json` permits, since an observed
    /// `reason` is an open `reason-id`.
    pub fn reason(&self) -> ObservedReason {
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
            | Self::TeamEliminated { reason, .. } => ObservedReason::Declared(reason.clone()),
            _ => ObservedReason::Kind(self.kind()),
        }
    }
}

/// Which fact an [`Event`] is, without its payload.
///
/// The `type` tag of `spec/schema/event.schema.json`, as a value. Serde's names
/// are authoritative and [`EventKind::as_str`] is pinned against them by
/// `kind_matches_the_serialized_tag`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    PhaseChanged,
    TurnSelected,
    DayAdvanced,
    UnitMoved,
    MovementTrapped,
    UnitActionChanged,
    UnitCreated,
    UnitRemoved,
    UnitDamaged,
    UnitRepaired,
    UnitResourced,
    UnitLoaded,
    UnitUnloaded,
    UnitsJoined,
    ConcealmentChanged,
    TileOwnerChanged,
    TileTerrainChanged,
    CaptureChanged,
    SiloChanged,
    DestructibleDamaged,
    FundsChanged,
    AttackResolved,
    AreaStrikeResolved,
    PowerActivated,
    PowerEnded,
    PowerChargeChanged,
    CommanderSwapped,
    WeatherChanged,
    RandomOutcome,
    AutomaticSupply,
    AutomaticRepair,
    DrawOfferChanged,
    PlayerStatusChanged,
    TeamEliminated,
    MatchCompleted,
}

impl EventKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 35] = [
        Self::PhaseChanged,
        Self::TurnSelected,
        Self::DayAdvanced,
        Self::UnitMoved,
        Self::MovementTrapped,
        Self::UnitActionChanged,
        Self::UnitCreated,
        Self::UnitRemoved,
        Self::UnitDamaged,
        Self::UnitRepaired,
        Self::UnitResourced,
        Self::UnitLoaded,
        Self::UnitUnloaded,
        Self::UnitsJoined,
        Self::ConcealmentChanged,
        Self::TileOwnerChanged,
        Self::TileTerrainChanged,
        Self::CaptureChanged,
        Self::SiloChanged,
        Self::DestructibleDamaged,
        Self::FundsChanged,
        Self::AttackResolved,
        Self::AreaStrikeResolved,
        Self::PowerActivated,
        Self::PowerEnded,
        Self::PowerChargeChanged,
        Self::CommanderSwapped,
        Self::WeatherChanged,
        Self::RandomOutcome,
        Self::AutomaticSupply,
        Self::AutomaticRepair,
        Self::DrawOfferChanged,
        Self::PlayerStatusChanged,
        Self::TeamEliminated,
        Self::MatchCompleted,
    ];

    /// The identifier this kind is written as on the wire.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PhaseChanged => "phase-changed",
            Self::TurnSelected => "turn-selected",
            Self::DayAdvanced => "day-advanced",
            Self::UnitMoved => "unit-moved",
            Self::MovementTrapped => "movement-trapped",
            Self::UnitActionChanged => "unit-action-changed",
            Self::UnitCreated => "unit-created",
            Self::UnitRemoved => "unit-removed",
            Self::UnitDamaged => "unit-damaged",
            Self::UnitRepaired => "unit-repaired",
            Self::UnitResourced => "unit-resourced",
            Self::UnitLoaded => "unit-loaded",
            Self::UnitUnloaded => "unit-unloaded",
            Self::UnitsJoined => "units-joined",
            Self::ConcealmentChanged => "concealment-changed",
            Self::TileOwnerChanged => "tile-owner-changed",
            Self::TileTerrainChanged => "tile-terrain-changed",
            Self::CaptureChanged => "capture-changed",
            Self::SiloChanged => "silo-changed",
            Self::DestructibleDamaged => "destructible-damaged",
            Self::FundsChanged => "funds-changed",
            Self::AttackResolved => "attack-resolved",
            Self::AreaStrikeResolved => "area-strike-resolved",
            Self::PowerActivated => "power-activated",
            Self::PowerEnded => "power-ended",
            Self::PowerChargeChanged => "power-charge-changed",
            Self::CommanderSwapped => "commander-swapped",
            Self::WeatherChanged => "weather-changed",
            Self::RandomOutcome => "random-outcome",
            Self::AutomaticSupply => "automatic-supply",
            Self::AutomaticRepair => "automatic-repair",
            Self::DrawOfferChanged => "draw-offer-changed",
            Self::PlayerStatusChanged => "player-status-changed",
            Self::TeamEliminated => "team-eliminated",
            Self::MatchCompleted => "match-completed",
        }
    }

    /// The payload-free public signal this kind projects to, if it projects to
    /// one.
    ///
    /// Exhaustive over every kind, so a new event has to say whether it is
    /// public here rather than defaulting into or out of the closed
    /// `public-event.kind` enum. `spec/model/observation.md:323` fixes the
    /// eleven that are.
    pub const fn public(self) -> Option<PublicEventKind> {
        match self {
            Self::PhaseChanged => Some(PublicEventKind::PhaseChanged),
            Self::TurnSelected => Some(PublicEventKind::TurnSelected),
            Self::DayAdvanced => Some(PublicEventKind::DayAdvanced),
            Self::WeatherChanged => Some(PublicEventKind::WeatherChanged),
            Self::PowerActivated => Some(PublicEventKind::PowerActivated),
            Self::PowerEnded => Some(PublicEventKind::PowerEnded),
            Self::CommanderSwapped => Some(PublicEventKind::CommanderSwapped),
            Self::DrawOfferChanged => Some(PublicEventKind::DrawOfferChanged),
            Self::PlayerStatusChanged => Some(PublicEventKind::PlayerStatusChanged),
            Self::TeamEliminated => Some(PublicEventKind::TeamEliminated),
            Self::MatchCompleted => Some(PublicEventKind::MatchCompleted),
            Self::UnitMoved
            | Self::MovementTrapped
            | Self::UnitActionChanged
            | Self::UnitCreated
            | Self::UnitRemoved
            | Self::UnitDamaged
            | Self::UnitRepaired
            | Self::UnitResourced
            | Self::UnitLoaded
            | Self::UnitUnloaded
            | Self::UnitsJoined
            | Self::ConcealmentChanged
            | Self::TileOwnerChanged
            | Self::TileTerrainChanged
            | Self::CaptureChanged
            | Self::SiloChanged
            | Self::DestructibleDamaged
            | Self::FundsChanged
            | Self::AttackResolved
            | Self::AreaStrikeResolved
            | Self::PowerChargeChanged
            | Self::RandomOutcome
            | Self::AutomaticSupply
            | Self::AutomaticRepair => None,
        }
    }
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The closed vocabulary of `public-event.kind`.
///
/// A `public-event` carries no payload by design: it signals that a public fact
/// changed, and the recipient reads every new value from the post-observation
/// (`spec/model/observation.md:329`). Distinct from [`EventKind`] because the
/// schema licenses only these eleven under that key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum PublicEventKind {
    PhaseChanged,
    TurnSelected,
    DayAdvanced,
    WeatherChanged,
    PowerActivated,
    PowerEnded,
    CommanderSwapped,
    DrawOfferChanged,
    PlayerStatusChanged,
    TeamEliminated,
    MatchCompleted,
}

impl PublicEventKind {
    /// The authoritative event kind this signal stands for.
    pub const fn kind(self) -> EventKind {
        match self {
            Self::PhaseChanged => EventKind::PhaseChanged,
            Self::TurnSelected => EventKind::TurnSelected,
            Self::DayAdvanced => EventKind::DayAdvanced,
            Self::WeatherChanged => EventKind::WeatherChanged,
            Self::PowerActivated => EventKind::PowerActivated,
            Self::PowerEnded => EventKind::PowerEnded,
            Self::CommanderSwapped => EventKind::CommanderSwapped,
            Self::DrawOfferChanged => EventKind::DrawOfferChanged,
            Self::PlayerStatusChanged => EventKind::PlayerStatusChanged,
            Self::TeamEliminated => EventKind::TeamEliminated,
            Self::MatchCompleted => EventKind::MatchCompleted,
        }
    }
}

/// The `reason` an observed change carries.
///
/// `spec/schema/observed-event.schema.json` types this as an open `reason-id`,
/// which is either a reason the authoritative event declared or — for the events
/// that declare none — that event's own kind. Keeping the two apart means the
/// fallback never has to be spelled as a string, so no allocation happens to
/// name a reason the ruleset already enumerates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservedReason {
    Declared(Reason),
    Kind(EventKind),
}

impl ObservedReason {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Declared(reason) => reason.as_str(),
            Self::Kind(kind) => kind.as_str(),
        }
    }
}

impl Serialize for ObservedReason {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ObservedReason {
    /// The wire carries one string, and the two variants exist to keep the
    /// fallback from being spelled as a fresh one — so a reader has to pick.
    ///
    /// An event kind wins, because that is the only way the projection produces
    /// the fallback, and the two vocabularies are disjoint:
    /// `no_event_kind_is_also_a_reason` fails if they ever overlap.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::IntoDeserializer;
        use serde::de::value::{Error as ValueError, StrDeserializer, StringDeserializer};
        let text = String::deserialize(deserializer)?;
        let kind: StrDeserializer<'_, ValueError> = text.as_str().into_deserializer();
        if let Ok(kind) = EventKind::deserialize(kind) {
            return Ok(Self::Kind(kind));
        }
        let declared: StringDeserializer<D::Error> = text.into_deserializer();
        Reason::deserialize(declared).map(Self::Declared)
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
    Tile { position: Pos },
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
    Tile(Pos),
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
    use crate::semantic::{KnownReason, ReasonId};
    use serde_json::{Value, json};

    fn round_trip(event: Event) -> Value {
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            serde_json::from_value::<Event>(value.clone()).unwrap(),
            event
        );
        value
    }

    /// [`ObservedReason`] is one string on the wire, and its reader resolves an
    /// event kind before a declared reason. That tie-break is only harmless
    /// while the two vocabularies are disjoint, which this pins.
    #[test]
    fn no_event_kind_is_also_a_reason() {
        let reasons: Vec<&str> = KnownReason::ALL.iter().map(|r| r.as_str()).collect();
        for kind in EventKind::ALL {
            assert!(
                !reasons.contains(&kind.as_str()),
                "{} is both an event kind and a reason",
                kind.as_str()
            );
        }
    }

    /// Both spellings of a reason survive the wire, and the fallback resolves
    /// back to the kind it came from rather than to a fresh string.
    #[test]
    fn observed_reasons_round_trip_in_both_spellings() {
        for reason in [
            ObservedReason::Declared(KnownReason::Combat.into()),
            ObservedReason::Declared(ReasonId::from("host-specific").into()),
            ObservedReason::Kind(EventKind::UnitMoved),
        ] {
            let wire = serde_json::to_value(&reason).unwrap();
            assert_eq!(wire, json!(reason.as_str()));
            assert_eq!(
                serde_json::from_value::<ObservedReason>(wire).unwrap(),
                reason
            );
        }
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
                target: AttackTarget::Tile {
                    position: Pos::new(2, 3)
                },
            }),
            json!({
                "type":"attack-resolved","attacker":0,"weapon":"ammo",
                "target":{"type":"tile","position":Pos::new(2, 3)}
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
                source: SupplySource::Tile(Pos::new(1, 1)),
                units: vec![UnitId::new(5)],
            }),
            json!({"type":"automatic-supply","source":Pos::new(1, 1),"units":[5]})
        );
    }

    #[test]
    fn unowned_tiles_serialize_their_owner_as_null() {
        assert_eq!(
            round_trip(Event::TileOwnerChanged {
                position: Pos::new(0, 0),
                from: None,
                to: Some(PlayerId::from("red")),
            }),
            json!({"type":"tile-owner-changed","position":Pos::new(0, 0),"from":null,"to":"red"})
        );
    }

    /// Three spellings of one kind have to agree: the tag serde writes for the
    /// event, the name serde writes for [`EventKind`], and
    /// [`EventKind::as_str`]. The projection puts the last of these on the wire
    /// as a reason and as a public event's label, so a disagreement is a wire
    /// bug that no type checks.
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
            Event::PowerChargeChanged {
                player: PlayerId::from("red"),
                commander_slot: 0,
                from: 0,
                to: 1,
                reason: KnownReason::Combat.into(),
            },
            Event::DrawOfferChanged {
                player: PlayerId::from("red"),
                offered: true,
            },
        ] {
            let kind = event.kind();
            let wire = round_trip(event);
            assert_eq!(wire["type"], json!(kind.as_str()));
            assert_eq!(serde_json::to_value(kind).unwrap(), wire["type"]);
        }
    }

    /// A public signal names the same kind as the event it stands for, so the
    /// two vocabularies cannot drift apart.
    #[test]
    fn public_signals_agree_with_the_kinds_they_stand_for() {
        for public in [
            PublicEventKind::PhaseChanged,
            PublicEventKind::TurnSelected,
            PublicEventKind::DayAdvanced,
            PublicEventKind::WeatherChanged,
            PublicEventKind::PowerActivated,
            PublicEventKind::PowerEnded,
            PublicEventKind::CommanderSwapped,
            PublicEventKind::DrawOfferChanged,
            PublicEventKind::PlayerStatusChanged,
            PublicEventKind::TeamEliminated,
            PublicEventKind::MatchCompleted,
        ] {
            assert_eq!(public.kind().public(), Some(public));
            assert_eq!(
                serde_json::to_value(public).unwrap(),
                json!(public.kind().as_str())
            );
        }
    }

    /// The events that reach a recipient individually are exactly the ones with
    /// no public envelope; `attack-resolved` and `random-outcome` have neither.
    #[test]
    fn only_the_documented_kinds_are_public() {
        assert_eq!(EventKind::UnitMoved.public(), None);
        assert_eq!(EventKind::UnitDamaged.public(), None);
        assert_eq!(EventKind::AttackResolved.public(), None);
        assert_eq!(EventKind::RandomOutcome.public(), None);
        assert_eq!(EventKind::PowerChargeChanged.public(), None);
        assert_eq!(EventKind::AreaStrikeResolved.public(), None);
    }

    /// Events without a `reason` field label their observed change with their
    /// own kind, which is what the projection relies on.
    #[test]
    fn reason_falls_back_to_the_event_kind() {
        assert_eq!(
            Event::UnitRemoved {
                unit: UnitId::new(0),
                reason: KnownReason::FuelDepleted.into(),
            }
            .reason(),
            ObservedReason::Declared(KnownReason::FuelDepleted.into())
        );
        assert_eq!(
            Event::UnitsJoined {
                source: UnitId::new(0),
                target: UnitId::new(1),
            }
            .reason(),
            ObservedReason::Kind(EventKind::UnitsJoined)
        );
    }

    /// Both halves of a reason travel as a bare string, which is what
    /// `reason-id` is.
    #[test]
    fn reasons_serialize_as_plain_identifiers() {
        assert_eq!(
            serde_json::to_value(ObservedReason::Declared(KnownReason::Combat.into())).unwrap(),
            json!("combat")
        );
        assert_eq!(
            serde_json::to_value(ObservedReason::Declared(
                ReasonId::from("adapter-defined").into()
            ))
            .unwrap(),
            json!("adapter-defined")
        );
        assert_eq!(
            serde_json::to_value(ObservedReason::Kind(EventKind::UnitsJoined)).unwrap(),
            json!("units-joined")
        );
    }
}
