//! Pure, presentation-independent AWVM state and recipient observation values.
//!
//! Identifier domains are distinct even where their wire representations are
//! strings. Adapters from replay/ECS identifiers belong at the boundary and
//! must not make this model depend on Bevy entities or AWBW replay IDs.
//!
//! This file is the authoritative state the reducer operates on
//! (`spec/model/state.md`). The two halves that read it live in submodules and
//! are re-exported here, so every path a caller already writes —
//! `semantic::Observation`, `semantic::observe`, `semantic::AwbwVisibility` —
//! keeps naming the same item:
//!
//! - `visibility` — the vision operators of `spec/semantics/fog.md`, asked by
//!   the projection *and* by the reducer, which is why they are neither's
//!   private detail.
//! - `observe` — the recipient projection of `spec/model/observation.md`, and
//!   with it two of the crate's three entry points.

mod observe;
mod visibility;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::ruleset::{self, TerrainTrait};

pub use observe::{
    HiddenUnitHp, Observation, ObserveError, ObservedBoard, ObservedEvent, ObservedMatch,
    ObservedPlayer, ObservedTile, ObservedTransition, ObservedUnit, ObservedUnitHp,
    ObservedUnitRef, PublicCommander, Relation, TileVisibility, observe, observe_events,
    observe_transition,
};
pub use visibility::{AwbwView, AwbwVisibility, Viewpoint, Visibility};

/// A board coordinate.
///
/// `[x, y]` on the wire, x first, which is the specification's canonical order
/// (`spec/model/violations.md`). Storing it as a named pair is the point: the
/// board is indexed row-major, so every hand-written `tiles[p.y][p.x]` had to
/// invert the pair by hand, and one that forgot read as valid Rust.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub x: u8,
    pub y: u8,
}

impl Pos {
    pub const fn new(x: u8, y: u8) -> Self {
        Self { x, y }
    }

    /// Manhattan distance, which is how the ruleset measures range and
    /// adjacency.
    pub fn distance(self, other: Self) -> u64 {
        u64::from(self.x.abs_diff(other.x)) + u64::from(self.y.abs_diff(other.y))
    }

    /// The four orthogonally adjacent coordinates that exist. A coordinate on
    /// an edge simply yields fewer.
    pub fn orthogonal(self) -> impl Iterator<Item = Self> {
        [(1i16, 0i16), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .filter_map(move |(dx, dy)| {
                let x = u8::try_from(i16::from(self.x) + dx).ok()?;
                let y = u8::try_from(i16::from(self.y) + dy).ok()?;
                Some(Self { x, y })
            })
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "({}, {})", self.x, self.y)
    }
}

impl Serialize for Pos {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        [self.x, self.y].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Pos {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Decoded through `u64` rather than `u8` so that a coordinate beyond any
        // representable board reports what it is, instead of serde's less
        // specific "invalid value" for the narrower type.
        let [x, y] = <[u64; 2]>::deserialize(deserializer)?;
        let narrow = |value: u64| {
            u8::try_from(value).map_err(|_| {
                serde::de::Error::custom(format!(
                    "coordinate {value} is beyond the largest representable board"
                ))
            })
        };
        Ok(Self {
            x: narrow(x)?,
            y: narrow(y)?,
        })
    }
}

/// The identifiers the specification leaves open, as newtypes over `String`.
///
/// An inline small-string representation was tried — one of these is stored on
/// every unit, every owned tile and every event, all of which `execute` clones
/// per command — and reverted. It measured ~5% on `execute` and ~0-2% on the
/// projection: a state clone is dominated by copying the board, so removing one
/// allocation per unit and per property does not move much, and it does not
/// improve at real army size either. See handoff.md phase 4.6.
macro_rules! string_id {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
            #[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
            #[serde(transparent)]
            pub struct $name(String);

            impl $name {
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }

            impl Deref for $name {
                type Target = str;

                fn deref(&self) -> &Self::Target {
                    self.as_str()
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.fmt(formatter)
                }
            }

            impl From<&str> for $name {
                fn from(value: &str) -> Self {
                    Self(value.into())
                }
            }

            impl From<String> for $name {
                fn from(value: String) -> Self {
                    Self(value)
                }
            }

            impl PartialEq<&$name> for $name {
                fn eq(&self, other: &&$name) -> bool {
                    self == *other
                }
            }

            impl PartialEq<str> for $name {
                fn eq(&self, other: &str) -> bool {
                    self.as_str() == other
                }
            }

            impl PartialEq<&str> for $name {
                fn eq(&self, other: &&str) -> bool {
                    self.as_str() == *other
                }
            }
        )+
    };
}

string_id!(
    RulesetId,
    RulesetRevision,
    PlayerId,
    TeamId,
    TeleporterId,
    TraitId,
    ReasonId,
);

// Identifiers the active ruleset enumerates are the ruleset's own vocabulary
// types, not open strings. They serialize under exactly the identifiers the
// specification documents use, so the wire form is unchanged; what changes is
// that a value outside the ruleset now fails to decode instead of travelling
// to a table lookup that cannot resolve it.
pub use crate::ruleset::{
    CommanderKind as CommanderId, DrawReason, KnownReason, Terrain as TerrainId,
    UnitKind as UnitKindId, VictoryReason, WeatherKind,
};

/// A reason carried by the protocol.
///
/// Reducer-authored reasons use the generated closed vocabulary. `Other`
/// preserves the specification's open `reason-id` boundary for external
/// cancellation reasons and externally supplied event projections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reason {
    Known(KnownReason),
    Other(ReasonId),
}

impl Reason {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Known(reason) => reason.as_str(),
            Self::Other(reason) => reason.as_str(),
        }
    }
}

impl From<KnownReason> for Reason {
    fn from(reason: KnownReason) -> Self {
        Self::Known(reason)
    }
}

impl From<ReasonId> for Reason {
    fn from(reason: ReasonId) -> Self {
        Self::Other(reason)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(transparent)]
pub struct UnitId(u32);

impl UnitId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for UnitId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<u32> for UnitId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// The ruleset a state, a request, or a fixture names.
///
/// Both halves are identifiers, not prose: a revision is a name the ruleset
/// directory carries, and typing it keeps `"2026-07-10"` from being compared
/// against a `String` in one place and a `&str` in another.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
pub struct RulesetRef {
    pub id: RulesetId,
    pub revision: RulesetRevision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
pub struct Settings {
    pub fog: bool,
    pub income_per_property: u64,
    pub starting_funds: u64,
    pub powers: Toggle,
    pub tags: bool,
    pub weather: WeatherSetting,
    #[serde(deserialize_with = "deserialize_unit_kind_set")]
    pub lab_units: Vec<crate::ruleset::UnitKind>,
    pub unit_bans: Vec<crate::ruleset::UnitKind>,
    pub commander_bans: CommanderBans,
    pub capture_limit: Option<u64>,
    pub day_limit: Option<u64>,
    pub unit_limit: Option<u64>,
}

fn deserialize_unit_kind_set<'de, D>(deserializer: D) -> Result<Vec<UnitKindId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Decoding already rejected kinds outside the ruleset; only duplicates are
    // still this validator's business.
    let kinds = Vec::<UnitKindId>::deserialize(deserializer)?;
    let mut seen = HashSet::with_capacity(kinds.len());
    for kind in &kinds {
        if !seen.insert(*kind) {
            return Err(serde::de::Error::custom(format!(
                "duplicate lab unit kind {kind}"
            )));
        }
    }
    Ok(kinds)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum Toggle {
    Enabled,
    Disabled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum WeatherSetting {
    Clear,
    Rain,
    Snow,
    Random,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
pub struct CommanderBans {
    pub lead: Vec<crate::ruleset::CommanderKind>,
    pub backup: Vec<crate::ruleset::CommanderKind>,
}

/// A player's index into [`State::players`].
///
/// Resolving a player id to a seat once, at the edge of a command, and then
/// indexing is what keeps the reducer from re-scanning the roster for every
/// question it asks about the same player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerIdx(usize);

impl PlayerIdx {
    pub const fn get(self) -> usize {
        self.0
    }
}

/// The units in play, indexed by id.
///
/// An array on the wire, as `spec/schema/state.schema.json` describes. The
/// side table exists because the reducer asks "where is unit N" constantly, and
/// answering it by scanning made every such question linear in the army size.
/// Ids are unique — checked once, while decoding — which is also what makes the
/// index a function rather than a guess.
#[derive(Clone, Debug, Default)]
pub struct UnitStore {
    units: Vec<Unit>,
    by_id: HashMap<UnitId, usize>,
}

impl PartialEq for UnitStore {
    fn eq(&self, other: &Self) -> bool {
        // The index is derived, so comparing it would only ever restate this.
        self.units == other.units
    }
}

impl Eq for UnitStore {}

impl UnitStore {
    /// Build a store, failing on a duplicate id.
    pub fn new(units: Vec<Unit>) -> Result<Self, DuplicateUnitId> {
        let mut by_id = HashMap::with_capacity(units.len());
        for (index, unit) in units.iter().enumerate() {
            if by_id.insert(unit.id, index).is_some() {
                return Err(DuplicateUnitId(unit.id));
            }
        }
        Ok(Self { units, by_id })
    }

    pub fn len(&self) -> usize {
        self.units.len()
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// The unit with this id, in constant time.
    pub fn get(&self, id: UnitId) -> Option<&Unit> {
        self.by_id.get(&id).map(|index| &self.units[*index])
    }

    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        let index = *self.by_id.get(&id)?;
        Some(&mut self.units[index])
    }

    pub fn contains(&self, id: UnitId) -> bool {
        self.by_id.contains_key(&id)
    }

    /// Positional access, for the few places that hold an index rather than an
    /// id. Prefer [`UnitStore::get`].
    pub fn at(&self, index: usize) -> Option<&Unit> {
        self.units.get(index)
    }

    pub fn at_mut(&mut self, index: usize) -> Option<&mut Unit> {
        self.units.get_mut(index)
    }

    pub fn index_of(&self, id: UnitId) -> Option<usize> {
        self.by_id.get(&id).copied()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Unit> {
        self.units.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Unit> {
        self.units.iter_mut()
    }

    pub fn as_slice(&self) -> &[Unit] {
        &self.units
    }

    /// Add a unit. Panics on a duplicate id, which the reducer must not
    /// produce: ids come from `next_unit_id`, which only ever moves forward.
    pub fn push(&mut self, unit: Unit) {
        let id = unit.id;
        assert!(
            self.by_id.insert(id, self.units.len()).is_none(),
            "unit {id} is already in play"
        );
        self.units.push(unit);
    }

    /// Remove the unit at a position, keeping the index in step.
    pub fn remove(&mut self, index: usize) -> Unit {
        let removed = self.units.remove(index);
        self.by_id.remove(&removed.id);
        for later in self.by_id.values_mut() {
            if *later > index {
                *later -= 1;
            }
        }
        removed
    }

    pub fn retain(&mut self, keep: impl FnMut(&Unit) -> bool) {
        self.units.retain(keep);
        self.reindex();
    }

    pub fn extend(&mut self, units: impl IntoIterator<Item = Unit>) {
        self.units.extend(units);
        self.reindex();
    }

    fn reindex(&mut self) {
        self.by_id.clear();
        self.by_id.extend(
            self.units
                .iter()
                .enumerate()
                .map(|(index, u)| (u.id, index)),
        );
    }
}

impl std::ops::Index<usize> for UnitStore {
    type Output = Unit;

    fn index(&self, index: usize) -> &Unit {
        &self.units[index]
    }
}

impl std::ops::IndexMut<usize> for UnitStore {
    fn index_mut(&mut self, index: usize) -> &mut Unit {
        &mut self.units[index]
    }
}

impl<'a> IntoIterator for &'a mut UnitStore {
    type Item = &'a mut Unit;
    type IntoIter = std::slice::IterMut<'a, Unit>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<'a> IntoIterator for &'a UnitStore {
    type Item = &'a Unit;
    type IntoIter = std::slice::Iter<'a, Unit>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Two units in the same state claiming one id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unit {0} appears more than once")]
pub struct DuplicateUnitId(pub UnitId);

impl Serialize for UnitStore {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.units.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for UnitStore {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Vec::<Unit>::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub ruleset: RulesetRef,
    pub settings: Settings,
    pub board: Board,
    pub teams: Vec<Team>,
    pub players: Vec<Player>,
    pub turn: Turn,
    pub weather: Weather,
    pub units: UnitStore,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_unit_id: Option<u32>,
    #[serde(rename = "match")]
    pub match_state: Match,
}
impl State {
    /// The seat a player id names.
    ///
    /// Resolve once at the edge of a command and index afterwards. The roster
    /// is short, so this is about saying which player a later index means, not
    /// about speed.
    pub fn player_index(&self, id: &PlayerId) -> Option<PlayerIdx> {
        self.players
            .iter()
            .position(|candidate| candidate.id == id)
            .map(PlayerIdx)
    }

    pub fn player(&self, seat: PlayerIdx) -> &Player {
        &self.players[seat.get()]
    }

    pub fn player_mut(&mut self, seat: PlayerIdx) -> &mut Player {
        &mut self.players[seat.get()]
    }

    pub fn find_player(&self, id: &PlayerId) -> Option<&Player> {
        self.players.iter().find(|candidate| candidate.id == id)
    }

    pub fn find_player_mut(&mut self, id: &PlayerId) -> Option<&mut Player> {
        self.players.iter_mut().find(|candidate| candidate.id == id)
    }

    /// Check the relational invariants of `spec/model/invariants.md`.
    ///
    /// Decoding already enforces everything a type can carry: the board is a
    /// rectangle, terrain and unit kinds are the ruleset's own vocabulary, and
    /// unit ids are unique. What is left is the relations *between* fields —
    /// an owner naming a player that exists, two units on one tile, cargo
    /// pointing at a transport that sank — and nothing checks those until a
    /// reducer trips over one mid-command and returns
    /// [`crate::transition::ExecuteError::InvalidState`].
    ///
    /// That is the right answer for the protocol, which is handed a state per
    /// request. It is the wrong one for a consumer that loads a map once and
    /// then plays a thousand commands against it: the defect is in the load,
    /// and it should be reported there. Run this at the boundary where a state
    /// enters the process — a map import, a database read, a replay adapter —
    /// and the reducer's `InvalidState` becomes the assertion it reads like.
    ///
    /// The scan is linear in tiles and units and allocates one set, so it is
    /// affordable per load and not per command.
    pub fn validate(&self) -> Result<(), StateInvariant> {
        self.validate_roster()?;
        self.validate_units()?;
        self.validate_board()?;
        Ok(())
    }

    /// Teams, players, and whose turn it is.
    fn validate_roster(&self) -> Result<(), StateInvariant> {
        let mut teams = HashSet::with_capacity(self.teams.len());
        for team in &self.teams {
            if !teams.insert(&team.id) {
                return Err(StateInvariant::DuplicateTeam(team.id.clone()));
            }
        }
        let mut players = HashSet::with_capacity(self.players.len());
        for player in &self.players {
            if !players.insert(&player.id) {
                return Err(StateInvariant::DuplicatePlayer(player.id.clone()));
            }
            if !teams.contains(&player.team) {
                return Err(StateInvariant::UnknownTeam {
                    player: player.id.clone(),
                    team: player.team.clone(),
                });
            }
        }
        if !players.contains(&self.turn.active_player) {
            return Err(StateInvariant::UnknownActivePlayer(
                self.turn.active_player.clone(),
            ));
        }
        let mut seen = HashSet::with_capacity(self.turn.order.len());
        for id in &self.turn.order {
            if !players.contains(id) {
                return Err(StateInvariant::UnknownPlayerInOrder(id.clone()));
            }
            if !seen.insert(id) {
                return Err(StateInvariant::RepeatedPlayerInOrder(id.clone()));
            }
        }
        match self.turn.order.get(self.turn.position) {
            None => Err(StateInvariant::TurnPositionOutOfRange {
                position: self.turn.position,
                length: self.turn.order.len(),
            }),
            Some(id) if *id != self.turn.active_player => {
                Err(StateInvariant::TurnPositionDisagrees {
                    position: self.turn.position,
                    seated: id.clone(),
                    active: self.turn.active_player.clone(),
                })
            }
            Some(_) => Ok(()),
        }
    }

    /// Units: ownership, placement, resources, cargo, and the `moved` rule.
    fn validate_units(&self) -> Result<(), StateInvariant> {
        let mut occupied: HashMap<Pos, UnitId> = HashMap::with_capacity(self.units.len());
        let mut slots: HashSet<(UnitId, usize)> = HashSet::new();
        let mut moved: Option<UnitId> = None;
        let mut highest: Option<u32> = None;

        for unit in &self.units {
            highest = Some(highest.map_or(unit.id.get(), |seen| seen.max(unit.id.get())));
            if self.find_player(&unit.owner).is_none() {
                return Err(StateInvariant::UnknownUnitOwner {
                    unit: unit.id,
                    owner: unit.owner.clone(),
                });
            }
            if unit.hp == 0 || unit.hp > 100 {
                return Err(StateInvariant::UnitHpOutOfRange {
                    unit: unit.id,
                    hp: unit.hp,
                });
            }
            let profile = ruleset::profile(unit.kind);
            if unit.fuel > profile.max_fuel {
                return Err(StateInvariant::UnitFuelExceedsMaximum {
                    unit: unit.id,
                    fuel: unit.fuel,
                    maximum: profile.max_fuel,
                });
            }
            if unit.ammo > profile.max_ammo {
                return Err(StateInvariant::UnitAmmoExceedsMaximum {
                    unit: unit.id,
                    ammo: unit.ammo,
                    maximum: profile.max_ammo,
                });
            }
            if unit.action == UnitAction::Moved {
                if self.turn.phase != Phase::UnitAction {
                    return Err(StateInvariant::MovedOutsideUnitAction { unit: unit.id });
                }
                if unit.owner != self.turn.active_player {
                    return Err(StateInvariant::MovedUnitIsNotActive { unit: unit.id });
                }
                if let Some(first) = moved.replace(unit.id) {
                    return Err(StateInvariant::SeveralMovedUnits {
                        first,
                        second: unit.id,
                    });
                }
            }
            match unit.location {
                Location::Board { position } => {
                    if !self.board.contains(position) {
                        return Err(StateInvariant::UnitOutOfBounds {
                            unit: unit.id,
                            position,
                        });
                    }
                    if let Some(other) = occupied.insert(position, unit.id) {
                        return Err(StateInvariant::TileOccupiedTwice {
                            position,
                            first: other,
                            second: unit.id,
                        });
                    }
                }
                Location::Cargo { transport, slot } => {
                    self.validate_cargo(unit, transport, slot, &mut slots)?;
                }
            }
        }

        // `next_unit_id` is `Option` because `spec/model/state.md:139` makes it
        // one: a state for a feature that never spawns units may omit it, and
        // production treats the absence as an inadmissible pre-state. What the
        // specification does require is that a present value be fresh.
        match (self.next_unit_id, highest) {
            (Some(next), Some(highest)) if next <= highest => {
                Err(StateInvariant::NextUnitIdIsNotFresh {
                    next_unit_id: next,
                    highest: UnitId::new(highest),
                })
            }
            _ => Ok(()),
        }
    }

    /// One cargo unit against the transport it names.
    fn validate_cargo(
        &self,
        unit: &Unit,
        transport: UnitId,
        slot: usize,
        slots: &mut HashSet<(UnitId, usize)>,
    ) -> Result<(), StateInvariant> {
        let cargo_error = |reason| StateInvariant::Cargo {
            unit: unit.id,
            transport,
            reason,
        };
        if transport == unit.id {
            return Err(cargo_error(CargoFault::CarriesItself));
        }
        let Some(carrier) = self.units.get(transport) else {
            return Err(cargo_error(CargoFault::TransportAbsent));
        };
        if carrier.owner != unit.owner {
            return Err(cargo_error(CargoFault::OwnerDiffers));
        }
        // AWBW has no nested transport capability, so a carrier is on the board
        // (`spec/model/state.md`, cargo invariants).
        if !matches!(carrier.location, Location::Board { .. }) {
            return Err(cargo_error(CargoFault::TransportNotOnBoard));
        }
        let Some(capability) = ruleset::profile(carrier.kind).transport else {
            return Err(cargo_error(CargoFault::TransportCarriesNothing));
        };
        if slot >= capability.capacity {
            return Err(cargo_error(CargoFault::SlotBeyondCapacity {
                slot,
                capacity: capability.capacity,
            }));
        }
        if !capability.cargo.contains(unit.kind) {
            return Err(cargo_error(CargoFault::KindNotCarryable(unit.kind)));
        }
        if !slots.insert((transport, slot)) {
            return Err(cargo_error(CargoFault::SlotTaken(slot)));
        }
        Ok(())
    }

    /// Tiles: an owner that exists, and mutable fields the terrain admits.
    fn validate_board(&self) -> Result<(), StateInvariant> {
        for (position, tile) in self.board.rows().flatten() {
            if let Some(owner) = tile.owner.player()
                && self.find_player(owner).is_none()
            {
                return Err(StateInvariant::UnknownTileOwner {
                    position,
                    owner: owner.clone(),
                });
            }
            if tile.owner.is_ownable()
                != ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable)
            {
                return Err(StateInvariant::TileOwnershipDisagreesWithTerrain {
                    position,
                    terrain: tile.terrain,
                });
            }
            if tile.capture_points.is_some() && !tile.owner.is_ownable() {
                return Err(StateInvariant::CapturePointsOnUnownableTile { position });
            }
            match (
                tile.destructible_hp(),
                ruleset::terrain(tile.terrain).destructible,
            ) {
                (Some(hp), Some(profile)) if hp > profile.maximum_hp => {
                    return Err(StateInvariant::DestructibleHpAboveMaximum {
                        position,
                        hp,
                        maximum: profile.maximum_hp,
                    });
                }
                (Some(_), None) => {
                    return Err(StateInvariant::DestructibleHpOnIndestructibleTile { position });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// A relation between two parts of a [`State`] that the specification forbids.
///
/// Each variant names the invariant it caught and the values that broke it, so
/// a loader can report which unit or tile is wrong rather than that something
/// is. The ones decoding already prevents — a ragged board, a repeated unit id,
/// an unknown terrain — are not here, because a value carrying them cannot be
/// built.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StateInvariant {
    #[error("team {0} appears more than once")]
    DuplicateTeam(TeamId),
    #[error("player {0} appears more than once")]
    DuplicatePlayer(PlayerId),
    #[error("player {player} belongs to unknown team {team}")]
    UnknownTeam { player: PlayerId, team: TeamId },
    #[error("active player {0} is not in the roster")]
    UnknownActivePlayer(PlayerId),
    #[error("turn order names unknown player {0}")]
    UnknownPlayerInOrder(PlayerId),
    #[error("turn order names {0} more than once")]
    RepeatedPlayerInOrder(PlayerId),
    #[error("turn position {position} is outside an order of {length}")]
    TurnPositionOutOfRange { position: usize, length: usize },
    #[error("turn position {position} seats {seated}, but {active} is active")]
    TurnPositionDisagrees {
        position: usize,
        seated: PlayerId,
        active: PlayerId,
    },
    #[error("unit {unit} is owned by unknown player {owner}")]
    UnknownUnitOwner { unit: UnitId, owner: PlayerId },
    #[error("unit {unit} has {hp} HP, which is outside 1..=100")]
    UnitHpOutOfRange { unit: UnitId, hp: u8 },
    #[error("unit {unit} holds {fuel} fuel above its maximum of {maximum}")]
    UnitFuelExceedsMaximum {
        unit: UnitId,
        fuel: u64,
        maximum: u64,
    },
    #[error("unit {unit} holds {ammo} ammo above its maximum of {maximum}")]
    UnitAmmoExceedsMaximum {
        unit: UnitId,
        ammo: u64,
        maximum: u64,
    },
    #[error("unit {unit} is moved outside the unit-action phase")]
    MovedOutsideUnitAction { unit: UnitId },
    #[error("unit {unit} is moved but is not the active player's")]
    MovedUnitIsNotActive { unit: UnitId },
    #[error("units {first} and {second} are both moved")]
    SeveralMovedUnits { first: UnitId, second: UnitId },
    #[error("unit {unit} stands at {position}, which is off the board")]
    UnitOutOfBounds { unit: UnitId, position: Pos },
    #[error("units {first} and {second} both stand at {position}")]
    TileOccupiedTwice {
        position: Pos,
        first: UnitId,
        second: UnitId,
    },
    #[error("cargo unit {unit} in transport {transport}: {reason}")]
    Cargo {
        unit: UnitId,
        transport: UnitId,
        reason: CargoFault,
    },
    #[error("next_unit_id {next_unit_id} does not exceed live unit {highest}")]
    NextUnitIdIsNotFresh { next_unit_id: u32, highest: UnitId },
    #[error("tile {position} is owned by unknown player {owner}")]
    UnknownTileOwner { position: Pos, owner: PlayerId },
    #[error("tile {position} carries ownership its terrain {terrain} does not admit")]
    TileOwnershipDisagreesWithTerrain { position: Pos, terrain: TerrainId },
    #[error("tile {position} records capture progress but cannot be owned")]
    CapturePointsOnUnownableTile { position: Pos },
    #[error("tile {position} has {hp} HP above its maximum of {maximum}")]
    DestructibleHpAboveMaximum {
        position: Pos,
        hp: u64,
        maximum: u64,
    },
    #[error("tile {position} has destructible HP but its terrain is not destructible")]
    DestructibleHpOnIndestructibleTile { position: Pos },
}

/// Which cargo invariant a cargo unit broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CargoFault {
    #[error("it names itself as its transport")]
    CarriesItself,
    #[error("the transport is not in play")]
    TransportAbsent,
    #[error("the transport belongs to another player")]
    OwnerDiffers,
    #[error("the transport is not on the board, which AWBW requires")]
    TransportNotOnBoard,
    #[error("the transport carries no cargo")]
    TransportCarriesNothing,
    #[error("slot {slot} is beyond a capacity of {capacity}")]
    SlotBeyondCapacity { slot: usize, capacity: usize },
    #[error("a {0} cannot be carried by it")]
    KindNotCarryable(UnitKindId),
    #[error("slot {0} already holds another unit")]
    SlotTaken(usize),
}

/// The board, stored flat and row-major.
///
/// The wire form is nested rows (`spec/schema/state.schema.json`), but a
/// `Vec<Vec<Tile>>` lets rows disagree with `width`, and every reader then has
/// to index two levels in the opposite order to the coordinate it holds. The
/// rectangle is checked once, while decoding, so nothing downstream can observe
/// a ragged board and no accessor can index past a short row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    width: u8,
    height: u8,
    tiles: Vec<Tile>,
}

impl Board {
    /// Build a board from row-major tiles.
    ///
    /// Fails unless `tiles` holds exactly `width * height` entries, which is
    /// what makes every accessor below total.
    pub fn new(width: u8, height: u8, tiles: Vec<Tile>) -> Result<Self, BoardShapeError> {
        let expected = usize::from(width) * usize::from(height);
        if width == 0 || height == 0 || tiles.len() != expected {
            return Err(BoardShapeError {
                width,
                height,
                found: tiles.len(),
            });
        }
        Ok(Self {
            width,
            height,
            tiles,
        })
    }

    pub const fn width(&self) -> u8 {
        self.width
    }

    pub const fn height(&self) -> u8 {
        self.height
    }

    /// Whether a coordinate is on the board.
    pub const fn contains(&self, position: Pos) -> bool {
        position.x < self.width && position.y < self.height
    }

    fn index(&self, position: Pos) -> Option<usize> {
        self.contains(position)
            .then(|| usize::from(position.y) * usize::from(self.width) + usize::from(position.x))
    }

    /// The tile at a coordinate, or `None` when it is off the board.
    pub fn get(&self, position: Pos) -> Option<&Tile> {
        self.index(position).map(|index| &self.tiles[index])
    }

    pub fn get_mut(&mut self, position: Pos) -> Option<&mut Tile> {
        self.index(position).map(|index| &mut self.tiles[index])
    }

    /// The tile at a coordinate that has already been bounds-checked.
    ///
    /// Panics off the board. Use it only where a validator has established the
    /// coordinate is on it; [`Board::get`] is the accessor for everywhere else.
    pub fn tile(&self, position: Pos) -> &Tile {
        self.get(position)
            .unwrap_or_else(|| panic!("{position} is off a {}x{} board", self.width, self.height))
    }

    pub fn tile_mut(&mut self, position: Pos) -> &mut Tile {
        let (width, height) = (self.width, self.height);
        self.get_mut(position)
            .unwrap_or_else(|| panic!("{position} is off a {width}x{height} board"))
    }

    /// Every coordinate on the board, row by row.
    pub fn positions(&self) -> impl Iterator<Item = Pos> + use<> {
        let (width, height) = (self.width, self.height);
        (0..height).flat_map(move |y| (0..width).map(move |x| Pos { x, y }))
    }

    /// Every tile with its coordinate, row by row.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, &Tile)> {
        self.positions().zip(self.tiles.iter())
    }

    /// The board as rows, for the projections whose wire shape is nested.
    pub fn rows(&self) -> impl Iterator<Item = impl Iterator<Item = (Pos, &Tile)>> {
        let width = self.width;
        (0..self.height).map(move |y| {
            let start = usize::from(y) * usize::from(width);
            self.tiles[start..start + usize::from(width)]
                .iter()
                .enumerate()
                .map(move |(x, tile)| (Pos { x: x as u8, y }, tile))
        })
    }

    pub fn tiles(&self) -> impl Iterator<Item = &Tile> {
        self.tiles.iter()
    }

    pub fn tiles_mut(&mut self) -> impl Iterator<Item = &mut Tile> {
        self.tiles.iter_mut()
    }
}

/// A `tiles` array that is not the rectangle `width` and `height` describe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "a {width}x{height} board needs {} tiles, found {found}",
    usize::from(*.width) * usize::from(*.height)
)]
pub struct BoardShapeError {
    pub width: u8,
    pub height: u8,
    pub found: usize,
}

/// The wire shape: nested rows, one per `y`.
#[derive(Serialize, Deserialize)]
struct BoardRows {
    width: u8,
    height: u8,
    tiles: Vec<Vec<Tile>>,
}

impl Serialize for Board {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BoardRows {
            width: self.width,
            height: self.height,
            tiles: self
                .tiles
                .chunks(usize::from(self.width))
                .map(<[Tile]>::to_vec)
                .collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Board {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let rows = BoardRows::deserialize(deserializer)?;
        if rows.tiles.len() != usize::from(rows.height)
            || rows
                .tiles
                .iter()
                .any(|row| row.len() != usize::from(rows.width))
        {
            return Err(serde::de::Error::custom(BoardShapeError {
                width: rows.width,
                height: rows.height,
                found: rows.tiles.iter().map(Vec::len).sum(),
            }));
        }
        Self::new(
            rows.width,
            rows.height,
            rows.tiles.into_iter().flatten().collect(),
        )
        .map_err(serde::de::Error::custom)
    }
}
/// One square of the board.
///
/// The whole board is cloned once per `execute` and projected once per
/// `observe`, so what a tile costs is multiplied by the board's area. The four
/// fields every tile has stay inline; the three only a handful of terrains ever
/// carry live behind one pointer, which takes a tile from 104 bytes to 40. The
/// wire form is unchanged — all seven keys stay flat, and each is still absent
/// when it has no value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub terrain: TerrainId,
    pub owner: TileOwner,
    pub capture_points: Option<u8>,
    pub silo: Option<Silo>,
    rare: Option<Box<RareTileState>>,
}

/// Tile state that most terrains never have.
///
/// Destructible HP belongs to pipe seams, `teleporter` to teleporter pairs, and
/// `trait_state` is the specification's extension point for ruleset traits that
/// keep per-tile state. Together they were 64 of a tile's 104 bytes, present on
/// every plain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RareTileState {
    destructible_hp: Option<u64>,
    teleporter: Option<TeleporterId>,
    trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
}

impl RareTileState {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl Tile {
    /// A tile of `terrain` with nothing on it.
    pub fn new(terrain: TerrainId) -> Self {
        Self {
            terrain,
            owner: TileOwner::NotOwnable,
            capture_points: None,
            silo: None,
            rare: None,
        }
    }

    /// Remaining HP of a destructible terrain, such as a pipe seam.
    pub fn destructible_hp(&self) -> Option<u64> {
        self.rare.as_ref().and_then(|rare| rare.destructible_hp)
    }

    /// Which teleporter pair this tile belongs to.
    pub fn teleporter(&self) -> Option<&TeleporterId> {
        self.rare.as_ref().and_then(|rare| rare.teleporter.as_ref())
    }

    /// Per-tile state owned by a ruleset terrain trait.
    pub fn trait_state(&self) -> Option<&BTreeMap<TraitId, serde_json::Value>> {
        self.rare
            .as_ref()
            .and_then(|rare| rare.trait_state.as_ref())
    }

    pub fn set_destructible_hp(&mut self, hp: Option<u64>) {
        self.rare_mut().destructible_hp = hp;
        self.shrink();
    }

    pub fn set_teleporter(&mut self, teleporter: Option<TeleporterId>) {
        self.rare_mut().teleporter = teleporter;
        self.shrink();
    }

    fn rare_mut(&mut self) -> &mut RareTileState {
        self.rare.get_or_insert_with(Box::default)
    }

    /// Give the pointer back once nothing is behind it.
    ///
    /// Two things depend on this. A tile that stopped being destructible costs
    /// what a plain costs, which is the point of boxing at all. And `rare` then
    /// has one spelling per state, which is what lets equality be derived: to a
    /// derive, `None` and an allocated-but-empty block are different tiles, even
    /// though they serialize to the same bytes. Every path that can set `rare` —
    /// [`Tile::new`], `Deserialize`, and the setters — leaves it `None` when
    /// there is nothing to hold, and
    /// `a_tile_that_loses_its_rare_state_equals_one_that_never_had_any` is what
    /// pins that.
    fn shrink(&mut self) {
        if self.rare.as_ref().is_some_and(|rare| rare.is_empty()) {
            self.rare = None;
        }
    }
}

/// The flat seven-key object `spec/schema/state.schema.json` describes,
/// borrowed for writing.
#[derive(Serialize)]
struct TileFields<'a> {
    terrain: TerrainId,
    #[serde(skip_serializing_if = "owner_is_absent")]
    owner: &'a TileOwner,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_points: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    silo: Option<Silo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    destructible_hp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    teleporter: Option<&'a TeleporterId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trait_state: Option<&'a BTreeMap<TraitId, serde_json::Value>>,
}

fn owner_is_absent(owner: &&TileOwner) -> bool {
    owner.is_not_ownable()
}

/// The same object, owned, for reading.
#[derive(Deserialize)]
struct TileWire {
    terrain: TerrainId,
    #[serde(default)]
    owner: TileOwner,
    capture_points: Option<u8>,
    silo: Option<Silo>,
    destructible_hp: Option<u64>,
    teleporter: Option<TeleporterId>,
    trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
}

impl Serialize for Tile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TileFields {
            terrain: self.terrain,
            owner: &self.owner,
            capture_points: self.capture_points,
            silo: self.silo,
            destructible_hp: self.destructible_hp(),
            teleporter: self.teleporter(),
            trait_state: self.trait_state(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Tile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = TileWire::deserialize(deserializer)?;
        let rare = RareTileState {
            destructible_hp: wire.destructible_hp,
            teleporter: wire.teleporter,
            trait_state: wire.trait_state,
        };
        Ok(Self {
            terrain: wire.terrain,
            owner: wire.owner,
            capture_points: wire.capture_points,
            silo: wire.silo,
            rare: (!rare.is_empty()).then(|| Box::new(rare)),
        })
    }
}

/// Who holds a tile, if anyone can.
///
/// Three states the wire spells three ways: an absent `owner` key means the
/// terrain cannot be owned at all, `null` means it can be but nobody does, and
/// a player id means it is held. That was an `Option<Option<PlayerId>>` whose
/// two layers could only be told apart by reading the deserializer, and which
/// every reader unwrapped twice by hand.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TileOwner {
    /// The terrain is not a property. Serializes by being absent.
    #[default]
    NotOwnable,
    /// A property nobody holds.
    Neutral,
    Owned(PlayerId),
}

impl TileOwner {
    pub const fn is_not_ownable(&self) -> bool {
        matches!(self, Self::NotOwnable)
    }

    /// Whether this is a property, held or not.
    pub const fn is_ownable(&self) -> bool {
        !self.is_not_ownable()
    }

    /// The holder, if there is one.
    pub const fn player(&self) -> Option<&PlayerId> {
        match self {
            Self::Owned(player) => Some(player),
            Self::NotOwnable | Self::Neutral => None,
        }
    }

    pub fn is_owned_by(&self, player: &PlayerId) -> bool {
        self.player().is_some_and(|held| held == player)
    }

    /// The holder as the wire spells it for an ownable tile: `null` or an id.
    ///
    /// Only meaningful for a property; a non-ownable tile also yields `None`.
    pub fn to_optional(&self) -> Option<PlayerId> {
        self.player().cloned()
    }

    /// An ownable tile's holder, from the `null`-or-id the wire carries.
    pub fn ownable(player: Option<PlayerId>) -> Self {
        player.map_or(Self::Neutral, Self::Owned)
    }
}

impl Serialize for TileOwner {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // `NotOwnable` is `skip_serializing_if`'d away by the field, so reaching
        // here at all means the key is present and `null` is the right value.
        self.player().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TileOwner {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Only called when the key is present, so the terrain is ownable and
        // `null` distinguishes neutral from held.
        Ok(Self::ownable(Option::<PlayerId>::deserialize(
            deserializer,
        )?))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum Silo {
    Ready,
    Spent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
pub struct Team {
    pub id: TeamId,
    pub status: TeamStatus,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum TeamStatus {
    Active,
    Eliminated,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub team: TeamId,
    pub funds: u64,
    pub status: PlayerStatus,
    pub commanders: Vec<Commander>,
    pub power_state: PowerState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum PlayerStatus {
    Active,
    Resigned,
    TimedOut,
    Eliminated,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
pub struct Commander {
    pub id: crate::ruleset::CommanderKind,
    pub active: bool,
    pub power_charge: u64,
    pub power_uses: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PowerState {
    None,
    Cop { commander_slot: u8 },
    Scop { commander_slot: u8 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
pub struct Turn {
    pub day: u64,
    pub active_player: PlayerId,
    pub phase: Phase,
    pub order: Vec<PlayerId>,
    pub position: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    TurnStart,
    UnitAction,
    TurnEnd,
    Finished,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
pub struct Weather {
    pub kind: WeatherKind,
    pub remaining_turns: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub kind: UnitKindId,
    pub owner: PlayerId,
    pub hp: u8,
    pub fuel: u64,
    pub ammo: u64,
    pub action: UnitAction,
    pub concealment: Concealment,
    pub location: Location,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum UnitAction {
    Ready,
    Moved,
    Spent,
    Immobilized,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(rename_all = "kebab-case")]
pub enum Concealment {
    Exposed,
    Hidden,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Location {
    Board {
        #[cfg_attr(feature = "typescript", tsify(type = "[number, number]"))]
        position: Pos,
    },
    Cargo {
        transport: UnitId,
        slot: usize,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Match {
    Active { draw_offers: Vec<PlayerId> },
    Finished { outcome: Outcome },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(tsify::Tsify))]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Outcome {
    Victory {
        winners: Vec<TeamId>,
        reason: VictoryReason,
    },
    Draw {
        teams: Vec<TeamId>,
        reason: DrawReason,
    },
    Cancelled {
        reason: ReasonId,
    },
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reasons_decode_known_values_without_closing_the_protocol_domain() {
        let known = serde_json::from_value::<Reason>(serde_json::json!("combat")).unwrap();
        assert_eq!(known, Reason::Known(KnownReason::Combat));
        assert_eq!(serde_json::to_value(known).unwrap(), "combat");

        let other = serde_json::from_value::<Reason>(serde_json::json!("adapter-defined")).unwrap();
        assert_eq!(other, Reason::Other(ReasonId::from("adapter-defined")));
        assert_eq!(serde_json::to_value(other).unwrap(), "adapter-defined");
    }

    /// The wire form is `[x, y]`, x first, and must survive a round trip.
    #[test]
    fn coordinates_travel_as_two_element_arrays() {
        let position = Pos::new(3, 7);
        let wire = serde_json::to_value(position).unwrap();
        assert_eq!(wire, serde_json::json!([3, 7]));
        assert_eq!(serde_json::from_value::<Pos>(wire).unwrap(), position);
    }

    /// `Pos` is a byte pair, so a coordinate beyond any representable board is
    /// now a decoding failure rather than a value that reaches validation and
    /// is rejected as out of bounds. No board approaches this, but the class of
    /// error did change; see handoff.md.
    #[test]
    fn a_coordinate_beyond_every_board_fails_to_decode() {
        let error = serde_json::from_value::<Pos>(serde_json::json!([256, 0])).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("beyond the largest representable board"),
            "unexpected error: {error}"
        );
        assert!(serde_json::from_value::<Pos>(serde_json::json!([255, 255])).is_ok());
    }

    /// Three states, three wire spellings: absent, `null`, and an id. Getting
    /// this wrong is invisible in Rust and load-bearing on the wire.
    #[test]
    fn tile_ownership_keeps_its_three_wire_spellings() {
        for (owner, expected) in [
            (
                TileOwner::NotOwnable,
                serde_json::json!({"terrain":"plain"}),
            ),
            (
                TileOwner::Neutral,
                serde_json::json!({"terrain":"plain","owner":null}),
            ),
            (
                TileOwner::Owned(PlayerId::from("red")),
                serde_json::json!({"terrain":"plain","owner":"red"}),
            ),
        ] {
            let tile = Tile {
                owner: owner.clone(),
                ..plain()
            };
            let wire = serde_json::to_value(&tile).unwrap();
            assert_eq!(wire, expected, "{owner:?} serialized wrong");
            assert_eq!(serde_json::from_value::<Tile>(wire).unwrap().owner, owner);
        }
    }

    #[test]
    fn only_a_held_property_names_a_player() {
        assert_eq!(TileOwner::NotOwnable.player(), None);
        assert_eq!(TileOwner::Neutral.player(), None);
        assert!(!TileOwner::Neutral.is_owned_by(&PlayerId::from("red")));
        assert!(TileOwner::Owned(PlayerId::from("red")).is_owned_by(&PlayerId::from("red")));
        assert!(!TileOwner::Owned(PlayerId::from("red")).is_owned_by(&PlayerId::from("blue")));
        // A neutral property is still a property; a mountain is not.
        assert!(TileOwner::Neutral.is_ownable());
        assert!(!TileOwner::NotOwnable.is_ownable());
    }

    /// The index is what makes lookup constant time, so it must survive every
    /// mutation — a stale index silently returns the wrong unit.
    #[test]
    fn the_unit_index_survives_removal_and_growth() {
        let mut units = UnitStore::new(vec![
            unit(0, PlayerId::from("p1")),
            unit(1, PlayerId::from("p2")),
            unit(2, PlayerId::from("p1")),
        ])
        .expect("distinct ids");

        assert_eq!(units.index_of(UnitId::new(2)), Some(2));
        units.remove(0);
        assert_eq!(units.get(UnitId::new(0)), None);
        assert_eq!(units.index_of(UnitId::new(1)), Some(0));
        assert_eq!(units.index_of(UnitId::new(2)), Some(1));

        units.push(unit(7, PlayerId::from("p1")));
        assert_eq!(units.index_of(UnitId::new(7)), Some(2));
        assert_eq!(units.get(UnitId::new(7)).unwrap().id, UnitId::new(7));

        units.retain(|held| held.id != UnitId::new(1));
        assert_eq!(units.get(UnitId::new(1)), None);
        assert_eq!(units.index_of(UnitId::new(7)), Some(1));
    }

    /// Unique ids are what let the index be a function at all, so a state that
    /// breaks that must not decode.
    #[test]
    fn duplicate_unit_ids_do_not_decode() {
        assert_eq!(
            UnitStore::new(vec![
                unit(0, PlayerId::from("p1")),
                unit(0, PlayerId::from("p2")),
            ]),
            Err(DuplicateUnitId(UnitId::new(0)))
        );
    }

    /// The store is an array on the wire, exactly as it was as a `Vec`.
    #[test]
    fn the_store_travels_as_a_plain_array() {
        let units = UnitStore::new(vec![
            unit(0, PlayerId::from("p1")),
            unit(1, PlayerId::from("p2")),
        ])
        .unwrap();
        let wire = serde_json::to_value(&units).unwrap();
        assert!(wire.is_array());
        assert_eq!(wire.as_array().unwrap().len(), 2);
        assert_eq!(serde_json::from_value::<UnitStore>(wire).unwrap(), units);
    }

    fn plain() -> Tile {
        Tile::new(TerrainId::Plain)
    }

    /// Boxing the rare three is a representation change, not a wire change: the
    /// object stays flat and seven-keyed, and each key is still absent when it
    /// has no value. The hand-written serde impls are the only thing keeping
    /// that true, so both directions are pinned here.
    #[test]
    fn tiles_keep_their_flat_wire_shape_around_the_rare_block() {
        let bare = plain();
        assert_eq!(
            serde_json::to_value(&bare).unwrap(),
            json!({"terrain": "plain"})
        );
        assert_eq!(
            serde_json::from_value::<Tile>(json!({"terrain":"plain"})).unwrap(),
            bare
        );

        let wire = json!({
            "terrain": "pipe-seam",
            "owner": null,
            "capture_points": 20,
            "silo": "ready",
            "destructible_hp": 99,
            "teleporter": "north",
            "trait_state": {"warp": 1},
        });
        let loaded: Tile = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(loaded.destructible_hp(), Some(99));
        assert_eq!(loaded.teleporter(), Some(&TeleporterId::from("north")));
        assert_eq!(
            loaded
                .trait_state()
                .and_then(|state| state.get(&TraitId::from("warp"))),
            Some(&json!(1))
        );
        assert_eq!(serde_json::to_value(&loaded).unwrap(), wire);
    }

    /// The pointer is handed back when the last rare value goes, so a destroyed
    /// pipe seam costs what the plain it becomes costs — and compares equal to
    /// one that never carried anything.
    #[test]
    fn a_tile_that_loses_its_rare_state_equals_one_that_never_had_any() {
        let mut seam = plain();
        seam.set_destructible_hp(Some(99));
        assert_ne!(seam, plain());

        seam.set_destructible_hp(None);
        assert_eq!(seam, plain());
        assert_eq!(
            serde_json::to_value(&seam).unwrap(),
            json!({"terrain":"plain"})
        );
    }

    /// The rectangle is checked once, while decoding, so nothing downstream can
    /// hold a ragged board. This replaced `ObserveError::InvalidBoardShape`,
    /// which only `observe` checked — `execute` would have panicked.
    #[test]
    fn a_ragged_board_does_not_decode() {
        let ragged = serde_json::json!({
            "width": 2, "height": 2,
            "tiles": [[{"terrain":"plain"}, {"terrain":"plain"}], [{"terrain":"plain"}]]
        });
        let error = serde_json::from_value::<Board>(ragged).unwrap_err();
        assert!(
            error.to_string().contains("needs 4 tiles, found 3"),
            "unexpected error: {error}"
        );
    }

    /// Row-major storage with an `[x, y]` coordinate is exactly where the old
    /// `tiles[p[1]][p[0]]` inversion lived, so pin that x and y are not swapped.
    #[test]
    fn tiles_are_addressed_by_x_then_y() {
        let mut corner = plain();
        corner.terrain = TerrainId::Mountain;
        let board = Board::new(
            3,
            2,
            vec![plain(), plain(), plain(), corner, plain(), plain()],
        )
        .expect("a 3x2 rectangle");

        assert_eq!(board.tile(Pos::new(0, 1)).terrain, TerrainId::Mountain);
        assert_eq!(board.tile(Pos::new(1, 0)).terrain, TerrainId::Plain);
        assert_eq!(board.get(Pos::new(3, 0)), None);
        assert_eq!(board.get(Pos::new(0, 2)), None);
        assert_eq!(
            board.positions().take(4).collect::<Vec<_>>(),
            vec![
                Pos::new(0, 0),
                Pos::new(1, 0),
                Pos::new(2, 0),
                Pos::new(0, 1)
            ]
        );
    }

    /// Serializing must rebuild the nested rows the schema describes.
    #[test]
    fn boards_round_trip_through_their_nested_wire_shape() {
        let board = Board::new(2, 2, vec![plain(), plain(), plain(), plain()]).unwrap();
        let wire = serde_json::to_value(&board).unwrap();
        assert_eq!(wire["tiles"].as_array().unwrap().len(), 2);
        assert_eq!(wire["tiles"][0].as_array().unwrap().len(), 2);
        assert_eq!(serde_json::from_value::<Board>(wire).unwrap(), board);
    }

    #[test]
    fn lab_unit_kinds_must_be_valid_and_unique() {
        let settings = serde_json::json!({
            "fog": false,
            "income_per_property": 1000,
            "starting_funds": 0,
            "powers": "disabled",
            "tags": false,
            "weather": "clear",
            "lab_units": ["infantry", "infantry"],
            "unit_bans": [],
            "commander_bans": { "lead": [], "backup": [] },
            "capture_limit": null,
            "day_limit": null,
            "unit_limit": null
        });
        assert!(
            serde_json::from_value::<Settings>(settings.clone())
                .unwrap_err()
                .to_string()
                .contains("duplicate lab unit kind infantry")
        );

        // A kind outside the ruleset is rejected by the vocabulary itself,
        // before the duplicate check ever sees it.
        let mut unknown = settings;
        unknown["lab_units"] = serde_json::json!(["not-a-unit"]);
        let message = serde_json::from_value::<Settings>(unknown)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("unknown variant `not-a-unit`"),
            "unexpected rejection: {message}"
        );
    }

    fn unit(id: u32, owner: PlayerId) -> Unit {
        Unit {
            id: id.into(),
            kind: UnitKindId::Infantry,
            owner,
            hp: 100,
            fuel: 99,
            ammo: 0,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board {
                position: Pos::new(0, 0),
            },
        }
    }
}
