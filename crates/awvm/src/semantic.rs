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

mod grid;
mod observe;
mod visibility;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::ruleset::{self, TerrainTrait};
use crate::setup::{MatchSetup, PlayerSetup, UnitDeployment};

pub use grid::{Cell, CellIdx, Dimensions, Grid};
pub use observe::{
    HiddenUnitHp, Observation, ObserveError, ObservedBoard, ObservedEvent, ObservedMatch,
    ObservedPlayer, ObservedTile, ObservedTileOwner, ObservedTransition, ObservedUnit,
    ObservedUnitHp, ObservedUnitRef, PublicCommander, Relation, TileVisibility, observe,
    observe_events, observe_into, observe_transition,
};
pub use visibility::{AwbwView, AwbwVisibility, Viewpoint, Visibility};

/// A board coordinate.
///
/// `[x, y]` on the wire, x first, which is the specification's canonical order
/// (`spec/model/violations.md`). Storing it as a named pair is the point: the
/// board is indexed row-major, so every hand-written `tiles[p.y][p.x]` had to
/// invert the pair by hand, and one that forgot read as valid Rust.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    /// The coordinate `dx` tiles right and `dy` tiles down, or `None` when
    /// that leaves the coordinate space.
    ///
    /// A board is a rectangle anchored at the origin, so a neighbour off the
    /// top or left edge has no coordinate at all. That is why this returns an
    /// option rather than wrapping: a caller that walks outward from a tile
    /// gets `None` for the tiles that do not exist, instead of a coordinate
    /// that silently names the far edge.
    pub fn offset(self, dx: i16, dy: i16) -> Option<Self> {
        let x = u8::try_from(i16::from(self.x) + dx).ok()?;
        let y = u8::try_from(i16::from(self.y) + dy).ok()?;
        Some(Self { x, y })
    }

    /// The four orthogonally adjacent coordinates that exist. A coordinate on
    /// an edge simply yields fewer.
    ///
    /// Written out rather than deferring to [`Pos::offset`]: enumeration walks
    /// this for every reachable tile of every unit, and it is measurably the
    /// hotter for keeping its own body.
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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

/// A player's seat in [`State::players`].
///
/// Resolving a player id to a seat once, at the edge of a command, and then
/// indexing is what keeps the reducer from re-scanning the roster for every
/// question it asks about the same player.
///
/// It is also how the state stores ownership. A held tile names a seat rather
/// than a [`PlayerId`], because a name is a `String`: held inline it made every
/// owned tile an allocation to clone and a pointer to free, on a board that is
/// copied once per command. A seat is one byte and makes the tile `Copy`. The
/// roster is the only place a player's name is spelled, and
/// [`State::player_id`] is how anything gets back to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerIdx(u8);

impl PlayerIdx {
    /// The seat at `index`. Building one out of thin air is only correct
    /// against the roster it will be read against, which is why this is not
    /// public: a seat comes from [`Roster::seat`] or [`State::player_index`],
    /// both of which read a name off the roster the seat will be used with.
    pub(crate) const fn from_seat(index: u8) -> Self {
        Self(index)
    }

    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

/// A roster with more seats than a [`PlayerIdx`] can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a roster holds at most 255 players, found {found}")]
pub struct RosterTooLarge {
    pub found: usize,
}

/// The seat `id` holds on `players`.
///
/// Decoding resolves every stored owner through this, so a roster longer than
/// a [`PlayerIdx`] can name has already been refused by then.
fn seat_of(players: &[Player], id: &PlayerId) -> Option<PlayerIdx> {
    let seat = players.iter().position(|candidate| candidate.id == id)?;
    u8::try_from(seat).ok().map(PlayerIdx)
}

/// The players of one match, in seat order.
///
/// A unit and a held tile both name their owner by seat, so a seat only means
/// something against the roster it indexes. That is the whole reason this is a
/// type and not a `Vec<Player>`: a seat is minted here, from a name this roster
/// holds, so a seat naming nobody cannot be built. Its length is checked once,
/// on the way in, which is what makes every seat it mints representable.
///
/// The roster is fixed for a match. It derefs to its players, so reading one is
/// what it always was, and a player's own mutable state — funds, status, the
/// power charge — is still reachable; adding and removing seats is not, and
/// neither is renaming one, because [`Player::id`] is read-only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Roster {
    players: Vec<Player>,
}

impl Roster {
    /// The roster these players sit on, or [`RosterTooLarge`] when there are
    /// more of them than a [`PlayerIdx`] can name.
    pub fn new(players: Vec<Player>) -> Result<Self, RosterTooLarge> {
        if u8::try_from(players.len()).is_err() {
            return Err(RosterTooLarge {
                found: players.len(),
            });
        }
        Ok(Self { players })
    }

    /// The seat `id` holds, which is the only way to name one.
    pub fn seat(&self, id: &PlayerId) -> Option<PlayerIdx> {
        seat_of(&self.players, id)
    }

    /// Every seat with the player sitting in it, in seat order.
    ///
    /// This is for whoever builds a state and must key their own vocabulary —
    /// a faction, a replay's player number — to seats. Reading a roster it
    /// just built by name would make a duplicate name ambiguous; this cannot.
    pub fn seats(&self) -> impl Iterator<Item = (PlayerIdx, &Player)> {
        // `new` refused a roster longer than a seat can name, so every index
        // here is one a `PlayerIdx` holds.
        self.players
            .iter()
            .enumerate()
            .filter_map(|(seat, player)| Some((PlayerIdx(u8::try_from(seat).ok()?), player)))
    }

    /// Every seat on `team`, with the player sitting in it, in seat order.
    pub fn on_team<'a>(
        &'a self,
        team: &'a TeamId,
    ) -> impl Iterator<Item = (PlayerIdx, &'a Player)> {
        self.seats().filter(move |(_, player)| player.team == *team)
    }

    /// Every seat on `team`, in seat order.
    ///
    /// A team is how the rules name a side — a projection's recipients, a
    /// power's targets — and a unit names its owner by seat, so the roster is
    /// turned into seats once and each unit is a lookup afterwards.
    pub fn seats_on_team<'a>(&'a self, team: &'a TeamId) -> impl Iterator<Item = PlayerIdx> + 'a {
        self.on_team(team).map(|(seat, _)| seat)
    }

    /// Every seat that is not on `team`, with the player sitting in it, in
    /// seat order.
    pub fn off_team<'a>(
        &'a self,
        team: &'a TeamId,
    ) -> impl Iterator<Item = (PlayerIdx, &'a Player)> {
        self.seats().filter(move |(_, player)| player.team != *team)
    }

    /// Every seat that is not on `team`, in seat order.
    pub fn seats_off_team<'a>(&'a self, team: &'a TeamId) -> impl Iterator<Item = PlayerIdx> + 'a {
        self.off_team(team).map(|(seat, _)| seat)
    }

    /// The player in `seat`, to be changed.
    ///
    /// One player at a time, never the slice: a roster hands out no way to add,
    /// remove or reorder a seat, and [`Player`] hands out no way to rename one,
    /// so a seat a tile or a unit already names keeps meaning what it meant.
    ///
    /// # Panics
    ///
    /// Panics when `seat` is not on this roster, which only a seat minted
    /// against a different roster can be.
    pub fn player_mut(&mut self, seat: PlayerIdx) -> &mut Player {
        &mut self.players[seat.get()]
    }

    /// The player `id` names, to be changed.
    pub fn find_mut(&mut self, id: &PlayerId) -> Option<&mut Player> {
        self.players.iter_mut().find(|candidate| candidate.id == id)
    }
}

impl std::ops::Deref for Roster {
    type Target = [Player];

    fn deref(&self) -> &Self::Target {
        &self.players
    }
}

impl<'a> IntoIterator for &'a Roster {
    type Item = &'a Player;
    type IntoIter = std::slice::Iter<'a, Player>;

    fn into_iter(self) -> Self::IntoIter {
        self.players.iter()
    }
}

impl<'de> Deserialize<'de> for Roster {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Vec::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A hasher for the dense integer ids that key [`UnitStore`].
///
/// The default hasher protects a map from keys an attacker chooses. Unit ids
/// come from a decoded state, and one turn of action enumeration looks them up
/// thousands of times, so the map pays for a protection it cannot use. One
/// multiply by an odd constant spreads the id across the bits the table reads.
#[derive(Debug, Default)]
pub struct IdHasher(u64);

/// Knuth's multiplicative constant, scaled to 64 bits.
const ID_SPREAD: u64 = 0x9E37_79B9_7F4A_7C15;

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    /// Never used by a `u32` key, and correct if some other key arrives.
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(ID_SPREAD);
        }
    }

    fn write_u32(&mut self, i: u32) {
        self.0 = u64::from(i).wrapping_mul(ID_SPREAD);
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
    by_id: HashMap<UnitId, usize, BuildHasherDefault<IdHasher>>,
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
        let mut by_id =
            HashMap::with_capacity_and_hasher(units.len(), BuildHasherDefault::default());
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

    /// The transports that are carrying at least one unit, in one pass.
    ///
    /// Cargo is spelled on the carried unit, so the only way to ask whether a
    /// transport is loaded is to look at everyone else. A renderer asks that of
    /// every unit it draws, which is why the answer is gathered once.
    pub fn loaded_transports(&self) -> HashSet<UnitId, BuildHasherDefault<IdHasher>> {
        self.units
            .iter()
            .filter_map(|unit| match unit.location {
                Location::Cargo { transport, .. } => Some(transport),
                Location::Board { .. } => None,
            })
            .collect()
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

impl UnitStore {
    /// The army as the wire spells it, with each owner named.
    ///
    /// A unit names its owner by seat, so only something holding the roster can
    /// spell one. That is why neither this nor [`Unit`] has serde of its own.
    fn to_fields<'a>(
        &'a self,
        players: &'a [Player],
    ) -> Result<Vec<UnitFields<'a>>, StateInvariant> {
        self.units
            .iter()
            .map(|unit| {
                let owner =
                    players
                        .get(unit.owner.get())
                        .ok_or(StateInvariant::UnitOwnerOffTheRoster {
                            unit: unit.id,
                            seat: unit.owner,
                        })?;
                Ok(UnitFields {
                    id: unit.id,
                    kind: &unit.kind,
                    owner: &owner.id,
                    hp: unit.hp,
                    fuel: unit.fuel,
                    ammo: unit.ammo,
                    action: unit.action,
                    concealment: unit.concealment,
                    location: &unit.location,
                })
            })
            .collect()
    }

    /// Rebuild an army from the wire, resolving each named owner to a seat.
    ///
    /// The roster is indexed once. Resolving each unit against the roster
    /// directly made decoding compare every name against every player, which an
    /// army of any size pays for on every state that arrives.
    fn from_wire(units: Vec<UnitWire>, players: &[Player]) -> Result<Self, UnitDecodeError> {
        let seats: HashMap<&PlayerId, PlayerIdx> = players
            .iter()
            .enumerate()
            .filter_map(|(seat, player)| {
                u8::try_from(seat)
                    .ok()
                    .map(|seat| (&player.id, PlayerIdx(seat)))
            })
            .collect();
        let units = units
            .into_iter()
            .map(|unit| {
                let owner = *seats.get(&unit.owner).ok_or(UnitDecodeError::UnknownOwner(
                    UnknownDecodedUnitOwner {
                        unit: unit.id,
                        owner: unit.owner.clone(),
                    },
                ))?;
                Ok(Unit {
                    id: unit.id,
                    kind: unit.kind,
                    owner,
                    hp: unit.hp,
                    fuel: unit.fuel,
                    ammo: unit.ammo,
                    action: unit.action,
                    concealment: unit.concealment,
                    location: unit.location,
                })
            })
            .collect::<Result<Vec<_>, UnitDecodeError>>()?;
        Self::new(units).map_err(UnitDecodeError::DuplicateId)
    }
}

/// A reason an army on the wire is not an army.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UnitDecodeError {
    #[error(transparent)]
    UnknownOwner(UnknownDecodedUnitOwner),
    #[error(transparent)]
    DuplicateId(DuplicateUnitId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub ruleset: RulesetRef,
    pub settings: Settings,
    pub board: Board,
    pub teams: Vec<Team>,
    pub players: Roster,
    pub turn: Turn,
    pub weather: Weather,
    pub units: UnitStore,
    pub next_unit_id: Option<u32>,
    pub match_state: Match,
}

#[derive(Serialize)]
struct MatchSetupFields<'a> {
    ruleset: &'a RulesetRef,
    settings: &'a Settings,
    board: BoardRows<'a>,
    players: &'a [PlayerSetup],
    deployments: &'a [UnitDeployment],
}

#[derive(Deserialize)]
struct MatchSetupWire {
    ruleset: RulesetRef,
    settings: Settings,
    board: BoardRowsWire,
    players: Vec<PlayerSetup>,
    deployments: Vec<UnitDeployment>,
}

impl Serialize for MatchSetup {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let players = Roster::new(
            self.players
                .iter()
                .map(|player| Player::new(player.id.clone(), player.team.clone()))
                .collect(),
        )
        .map_err(serde::ser::Error::custom)?;
        MatchSetupFields {
            ruleset: &self.ruleset,
            settings: &self.settings,
            board: self
                .board
                .to_rows(&players)
                .map_err(serde::ser::Error::custom)?,
            players: &self.players,
            deployments: &self.deployments,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for MatchSetup {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = MatchSetupWire::deserialize(deserializer)?;
        crate::setup::validate_players(&wire.players, wire.settings.tags)
            .map_err(serde::de::Error::custom)?;
        let roster = Roster::new(
            wire.players
                .iter()
                .map(|player| Player::new(player.id.clone(), player.team.clone()))
                .collect(),
        )
        .map_err(serde::de::Error::custom)?;
        let board = Board::from_rows(wire.board, &roster).map_err(serde::de::Error::custom)?;
        MatchSetup::new(
            wire.ruleset,
            wire.settings,
            board,
            wire.players,
            wire.deployments,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// The state as the wire spells it, borrowed for writing.
///
/// The board's tiles name their holder by seat, and only the roster turns a
/// seat back into a name, so the state is what serializes its own board. The
/// field order here is the wire's, and must stay so.
#[derive(Serialize)]
struct StateFields<'a> {
    ruleset: &'a RulesetRef,
    settings: &'a Settings,
    board: BoardRows<'a>,
    teams: &'a [Team],
    players: &'a [Player],
    turn: &'a Turn,
    weather: &'a Weather,
    units: Vec<UnitFields<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_unit_id: Option<u32>,
    #[serde(rename = "match")]
    match_state: &'a Match,
}

/// The same shape, owned, for reading.
#[derive(Deserialize)]
struct StateWire {
    ruleset: RulesetRef,
    settings: Settings,
    board: BoardRowsWire,
    teams: Vec<Team>,
    players: Roster,
    turn: Turn,
    weather: Weather,
    units: Vec<UnitWire>,
    #[serde(default)]
    next_unit_id: Option<u32>,
    #[serde(rename = "match")]
    match_state: Match,
}

impl Serialize for State {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StateFields {
            ruleset: &self.ruleset,
            settings: &self.settings,
            board: self
                .board
                .to_rows(&self.players)
                .map_err(serde::ser::Error::custom)?,
            teams: &self.teams,
            players: &self.players,
            turn: &self.turn,
            weather: &self.weather,
            units: self
                .units
                .to_fields(&self.players)
                .map_err(serde::ser::Error::custom)?,
            next_unit_id: self.next_unit_id,
            match_state: &self.match_state,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // `Roster` refuses a roster longer than a seat can name as it decodes,
        // so every seat resolved below is one a `PlayerIdx` can hold.
        let wire = StateWire::deserialize(deserializer)?;
        let board =
            Board::from_rows(wire.board, &wire.players).map_err(serde::de::Error::custom)?;
        let units =
            UnitStore::from_wire(wire.units, &wire.players).map_err(serde::de::Error::custom)?;
        Ok(Self {
            ruleset: wire.ruleset,
            settings: wire.settings,
            board,
            teams: wire.teams,
            players: wire.players,
            turn: wire.turn,
            weather: wire.weather,
            units,
            next_unit_id: wire.next_unit_id,
            match_state: wire.match_state,
        })
    }
}
impl State {
    /// The name of the player in `seat`, which is the only place names live.
    pub fn player_id(&self, seat: PlayerIdx) -> &PlayerId {
        &self.players[seat.get()].id
    }

    /// The name of the player in `seat`, or `None` when the roster is shorter.
    pub fn try_player_id(&self, seat: PlayerIdx) -> Option<&PlayerId> {
        self.players.get(seat.get()).map(|player| &player.id)
    }

    /// The seat a tile's owner names, if it is held.
    pub fn tile_owner_id(&self, owner: &TileOwner) -> Option<&PlayerId> {
        owner.player().map(|seat| self.player_id(seat))
    }

    /// The seat a player id names.
    ///
    /// Resolve once at the edge of a command and index afterwards. The roster
    /// is short, so this is about saying which player a later index means, not
    /// about speed.
    pub fn player_index(&self, id: &PlayerId) -> Option<PlayerIdx> {
        seat_of(&self.players, id)
    }

    pub fn player(&self, seat: PlayerIdx) -> &Player {
        &self.players[seat.get()]
    }

    pub fn player_mut(&mut self, seat: PlayerIdx) -> &mut Player {
        self.players.player_mut(seat)
    }

    pub fn find_player(&self, id: &PlayerId) -> Option<&Player> {
        self.players.iter().find(|candidate| candidate.id == id)
    }

    pub fn find_player_mut(&mut self, id: &PlayerId) -> Option<&mut Player> {
        self.players.find_mut(id)
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
        // Resolved once: the roster is scanned here so the loop can compare
        // seats instead of names.
        let active_seat = self.player_index(&self.turn.active_player);

        for unit in &self.units {
            highest = Some(highest.map_or(unit.id.get(), |seen| seen.max(unit.id.get())));
            if self.players.get(unit.owner.get()).is_none() {
                return Err(StateInvariant::UnitOwnerOffTheRoster {
                    unit: unit.id,
                    seat: unit.owner,
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
                if Some(unit.owner) != active_seat {
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
        validate_board_invariants(&self.board, self.players.len())
    }
}

pub(crate) fn validate_board_invariants(
    board: &Board,
    player_count: usize,
) -> Result<(), StateInvariant> {
    for (position, tile) in board.rows().flatten() {
        if let Some(seat) = tile.owner.player()
            && seat.get() >= player_count
        {
            return Err(StateInvariant::TileOwnerOffTheRoster { position, seat });
        }
        if tile.owner.is_ownable() != ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
            return Err(StateInvariant::TileOwnershipDisagreesWithTerrain {
                position,
                terrain: tile.terrain,
            });
        }
        if tile.capture_points.is_some() && !tile.owner.is_ownable() {
            return Err(StateInvariant::CapturePointsOnUnownableTile { position });
        }
        match (
            board.destructible_hp(position),
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
    #[error("unit {unit} is held by seat {}, which the roster does not have", seat.get())]
    UnitOwnerOffTheRoster { unit: UnitId, seat: PlayerIdx },
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
    #[error("tile {position} is held by seat {}, which the roster does not have", seat.get())]
    TileOwnerOffTheRoster { position: Pos, seat: PlayerIdx },
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
    /// The tiles, as the same row-major map every side table over the board
    /// uses. Holding a [`Grid`] rather than a width, a height and a `Vec`
    /// is what keeps one implementation of the index arithmetic and the
    /// bounds check, instead of the board restating what its own side tables
    /// already say.
    tiles: Grid<Tile>,
    /// State that belongs to a position rather than to every tile.
    ///
    /// Destructible HP, teleporter pairing and ruleset trait state are carried
    /// by a handful of tiles on any board and by none at all on most. Held
    /// inline they cost every tile a pointer and cost the clone one allocation
    /// per tile that has them; held here they cost the tiles that have them and
    /// nothing else. A tile with nothing to say has no entry, which is what
    /// lets equality stay derived.
    rare: BTreeMap<Pos, RareTileState>,
}

impl Board {
    /// Build a board from row-major tiles.
    ///
    /// Fails unless `tiles` holds exactly `width * height` entries, which is
    /// what makes every accessor below total.
    pub fn new(width: u8, height: u8, tiles: Vec<Tile>) -> Result<Self, BoardShapeError> {
        let found = tiles.len();
        let shape = || BoardShapeError {
            width,
            height,
            found,
        };
        if width == 0 || height == 0 {
            return Err(shape());
        }
        let tiles = Grid::from_cells(Dimensions::new(width, height), tiles).ok_or_else(shape)?;
        Ok(Self {
            tiles,
            rare: BTreeMap::new(),
        })
    }

    pub const fn width(&self) -> u8 {
        self.tiles.width()
    }

    pub const fn height(&self) -> u8 {
        self.tiles.height()
    }

    /// The shape every map over this board shares.
    pub const fn dimensions(&self) -> Dimensions {
        self.tiles.dimensions()
    }

    /// Whether a coordinate is on the board.
    pub const fn contains(&self, position: Pos) -> bool {
        self.dimensions().contains(position)
    }

    /// The tile at a coordinate, or `None` when it is off the board.
    pub fn get(&self, position: Pos) -> Option<&Tile> {
        self.tiles.get(position)
    }

    pub fn get_mut(&mut self, position: Pos) -> Option<&mut Tile> {
        self.tiles.get_mut(position)
    }

    /// The tile at a coordinate that has already been bounds-checked.
    ///
    /// Panics off the board. Use it only where a validator has established the
    /// coordinate is on it; [`Board::get`] is the accessor for everywhere else.
    pub fn tile(&self, position: Pos) -> &Tile {
        &self.tiles[position]
    }

    pub fn tile_mut(&mut self, position: Pos) -> &mut Tile {
        &mut self.tiles[position]
    }

    /// The tile at a cell this board's own [`Dimensions`] minted.
    pub fn at(&self, cell: Cell) -> &Tile {
        self.tiles.at(cell)
    }

    /// Every coordinate on the board, row by row.
    pub fn positions(&self) -> impl Iterator<Item = Pos> + use<> {
        self.dimensions().positions()
    }

    /// Every tile with its coordinate, row by row.
    pub fn iter(&self) -> impl Iterator<Item = (Pos, &Tile)> {
        self.tiles.iter()
    }

    /// The board as rows, for the projections whose wire shape is nested.
    pub fn rows(&self) -> impl Iterator<Item = impl Iterator<Item = (Pos, &Tile)>> {
        self.tiles.rows()
    }

    /// Remaining HP of a destructible terrain at `position`, such as a pipe
    /// seam.
    pub fn destructible_hp(&self, position: Pos) -> Option<u64> {
        self.rare.get(&position)?.destructible_hp
    }

    /// Which teleporter pair the tile at `position` belongs to.
    pub fn teleporter(&self, position: Pos) -> Option<&TeleporterId> {
        self.rare.get(&position)?.teleporter.as_ref()
    }

    /// State at `position` owned by a ruleset terrain trait.
    pub fn trait_state(&self, position: Pos) -> Option<&BTreeMap<TraitId, serde_json::Value>> {
        self.rare.get(&position)?.trait_state.as_ref()
    }

    pub fn set_destructible_hp(&mut self, position: Pos, hp: Option<u64>) {
        self.update_rare(position, |rare| rare.destructible_hp = hp);
    }

    pub fn set_teleporter(&mut self, position: Pos, teleporter: Option<TeleporterId>) {
        self.update_rare(position, |rare| rare.teleporter = teleporter);
    }

    pub fn set_trait_state(
        &mut self,
        position: Pos,
        trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
    ) {
        self.update_rare(position, |rare| rare.trait_state = trait_state);
    }

    /// Replace every rare tile state at once.
    ///
    /// A caller that already knows the whole board — decoding a wire form, and
    /// reifying a projection — builds the map itself. Setting the three fields
    /// tile by tile costs an occupancy test, a map search and a dropped
    /// default for every tile that has nothing rare to say, which is nearly
    /// all of them.
    pub(crate) fn set_rare_states(&mut self, rare: BTreeMap<Pos, RareTileState>) {
        debug_assert!(
            rare.iter()
                .all(|(position, state)| self.contains(*position) && !state.is_empty()),
            "a rare entry must name a tile of this board and say something"
        );
        self.rare = rare;
    }

    /// Everything rare about `position`, in one lookup.
    ///
    /// A reader that wants all three — the projection and the wire form both do
    /// — asks this rather than paying for three searches of the same map.
    pub(crate) fn rare_state(&self, position: Pos) -> Option<&RareTileState> {
        self.rare.get(&position)
    }

    /// Change the rare state at `position`, and hold no entry for a tile with
    /// nothing to say.
    ///
    /// Dropping an emptied entry is what keeps one spelling per state: to a
    /// derived `PartialEq`, a missing entry and a present-but-empty one are
    /// different boards, though they serialize to the same bytes. A tile that
    /// never had rare state and is being given none must not gain an entry
    /// either, which is why the vacant case never inserts.
    fn update_rare(&mut self, position: Pos, change: impl FnOnce(&mut RareTileState)) {
        if !self.contains(position) {
            return;
        }
        match self.rare.entry(position) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                change(entry.get_mut());
                if entry.get().is_empty() {
                    entry.remove();
                }
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                let mut state = RareTileState::default();
                change(&mut state);
                if !state.is_empty() {
                    entry.insert(state);
                }
            }
        }
    }

    pub fn tiles(&self) -> impl Iterator<Item = &Tile> {
        self.tiles.cells()
    }

    pub fn tiles_mut(&mut self) -> impl Iterator<Item = &mut Tile> {
        self.tiles.cells_mut()
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
///
/// A tile names its owner, and the state's tiles hold a seat rather than a
/// name, so only something holding the roster can spell one. That is why
/// neither [`Board`] nor [`Tile`] has serde of its own: [`State`] drives both,
/// and these are the shapes it drives them through.
#[derive(Serialize)]
struct BoardRows<'a> {
    width: u8,
    height: u8,
    tiles: Vec<Vec<TileFields<'a>>>,
}

/// The same shape, owned, for reading.
#[derive(Deserialize)]
struct BoardRowsWire {
    width: u8,
    height: u8,
    tiles: Vec<Vec<TileWire>>,
}

/// An owner a decoded board names that the roster does not hold.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("tile {position} is held by {owner}, who is not on the roster")]
pub struct UnknownTileOwner {
    pub position: Pos,
    pub owner: PlayerId,
}

impl Board {
    /// The board as the wire spells it, with each holder named.
    fn to_rows<'a>(&'a self, players: &'a [Player]) -> Result<BoardRows<'a>, StateInvariant> {
        let tiles = self
            .rows()
            .map(|row| {
                row.map(|(position, tile)| {
                    let rare = self.rare_state(position);
                    let owner = match tile.owner.player() {
                        Some(seat) => Some(Some(
                            &players
                                .get(seat.get())
                                .ok_or(StateInvariant::TileOwnerOffTheRoster { position, seat })?
                                .id,
                        )),
                        None => tile.owner.is_ownable().then_some(None),
                    };
                    Ok(TileFields {
                        terrain: tile.terrain,
                        owner,
                        capture_points: tile.capture_points,
                        silo: tile.silo,
                        destructible_hp: rare.and_then(|rare| rare.destructible_hp),
                        teleporter: rare.and_then(|rare| rare.teleporter.as_ref()),
                        trait_state: rare.and_then(|rare| rare.trait_state.as_ref()),
                    })
                })
                .collect::<Result<Vec<_>, StateInvariant>>()
            })
            .collect::<Result<Vec<_>, StateInvariant>>()?;
        Ok(BoardRows {
            width: self.width(),
            height: self.height(),
            tiles,
        })
    }

    /// Rebuild a board from the wire, resolving each named holder to a seat.
    fn from_rows(rows: BoardRowsWire, players: &[Player]) -> Result<Self, BoardDecodeError> {
        if rows.tiles.len() != usize::from(rows.height)
            || rows
                .tiles
                .iter()
                .any(|row| row.len() != usize::from(rows.width))
        {
            return Err(BoardDecodeError::Shape(BoardShapeError {
                width: rows.width,
                height: rows.height,
                found: rows.tiles.iter().map(Vec::len).sum(),
            }));
        }
        let mut tiles = Vec::with_capacity(usize::from(rows.width) * usize::from(rows.height));
        let mut rare = BTreeMap::new();
        for (y, row) in rows.tiles.into_iter().enumerate() {
            for (x, wire) in row.into_iter().enumerate() {
                let position = Pos {
                    x: x as u8,
                    y: y as u8,
                };
                let (tile, state) = wire.split(players, position)?;
                tiles.push(tile);
                if !state.is_empty() {
                    rare.insert(position, state);
                }
            }
        }
        let mut board =
            Self::new(rows.width, rows.height, tiles).map_err(BoardDecodeError::Shape)?;
        board.rare = rare;
        Ok(board)
    }
}

/// Why a board on the wire is not one this crate can hold.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
enum BoardDecodeError {
    #[error(transparent)]
    Shape(BoardShapeError),
    #[error(transparent)]
    Owner(#[from] UnknownTileOwner),
}

/// One square of the board.
///
/// The whole board is cloned once per `execute` and projected once per
/// `observe`, so what a tile costs is multiplied by the board's area. Only the
/// four fields every tile has are here; the three that a handful of terrains
/// ever carry live in [`Board`], keyed by position, which took a tile from 104
/// bytes to 40 and then to 32 and left it with no pointer to follow. The wire
/// form is unchanged — all seven keys stay flat on each tile, and each is
/// still absent when it has no value.
///
/// Cloning a tile therefore does not carry its rare state. Code that copies
/// tiles from one board to another must copy that state by position too, with
/// [`Board::destructible_hp`] and its neighbours.
///
/// `Copy` is the point of all of it: a tile holds no pointer and owns nothing,
/// so copying a board is one `memcpy` of six bytes a tile and dropping one is
/// a single deallocation, where it used to be per-tile clone and drop glue.
/// Capture points on a fully controlled property.
pub const CAPTURE_REQUIRED_POINTS: u8 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub terrain: TerrainId,
    pub owner: TileOwner,
    pub capture_points: Option<u8>,
    pub silo: Option<Silo>,
}

/// Tile state that most terrains never have.
///
/// Destructible HP belongs to pipe seams, `teleporter` to teleporter pairs, and
/// `trait_state` is the specification's extension point for ruleset traits that
/// keep per-tile state. Together they were 64 of a tile's 104 bytes, present on
/// every plain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RareTileState {
    pub(crate) destructible_hp: Option<u64>,
    pub(crate) teleporter: Option<TeleporterId>,
    pub(crate) trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
}

impl RareTileState {
    pub(crate) fn is_empty(&self) -> bool {
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
        }
    }
}

/// The flat seven-key object `spec/schema/state.schema.json` describes,
/// borrowed for writing.
#[derive(Serialize)]
struct TileFields<'a> {
    terrain: TerrainId,
    /// The holder's name. The outer layer is whether the tile is ownable at
    /// all, which is the difference between an absent `owner` key and a `null`
    /// one; the inner layer is whether anyone holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<Option<&'a PlayerId>>,
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

/// The same object, owned, for reading.
#[derive(Deserialize)]
struct TileWire {
    terrain: TerrainId,
    #[serde(default, deserialize_with = "deserialize_present_owner")]
    owner: Option<Option<PlayerId>>,
    capture_points: Option<u8>,
    silo: Option<Silo>,
    destructible_hp: Option<u64>,
    teleporter: Option<TeleporterId>,
    trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
}

/// An absent `owner` key and a `null` one mean different things, and only the
/// deserializer can tell them apart, so the present case is wrapped again.
fn deserialize_present_owner<'de, D>(deserializer: D) -> Result<Option<Option<PlayerId>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<PlayerId>::deserialize(deserializer).map(Some)
}

impl TileWire {
    /// Split one wire tile into the part every tile has and the part few do,
    /// resolving the holder's name to a seat on `players`.
    fn split(
        self,
        players: &[Player],
        position: Pos,
    ) -> Result<(Tile, RareTileState), UnknownTileOwner> {
        let owner = match self.owner {
            None => TileOwner::NotOwnable,
            Some(None) => TileOwner::Neutral,
            Some(Some(name)) => {
                let seat = seat_of(players, &name).ok_or(UnknownTileOwner {
                    position,
                    owner: name,
                })?;
                TileOwner::Owned(seat)
            }
        };
        Ok((
            Tile {
                terrain: self.terrain,
                owner,
                capture_points: self.capture_points,
                silo: self.silo,
            },
            RareTileState {
                destructible_hp: self.destructible_hp,
                teleporter: self.teleporter,
                trait_state: self.trait_state,
            },
        ))
    }
}

/// Who holds a tile, if anyone can.
///
/// Three states the wire spells three ways: an absent `owner` key means the
/// terrain cannot be owned at all, `null` means it can be but nobody does, and
/// a player id means it is held. That was an `Option<Option<PlayerId>>` whose
/// two layers could only be told apart by reading the deserializer, and which
/// every reader unwrapped twice by hand.
///
/// A state and a projection name a holder differently — see [`TileOwner`] and
/// [`ObservedTileOwner`] — so the holder is what this is generic over. The
/// three-state logic is written once; which vocabulary a tile speaks stays a
/// different type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileOwnerOf<Holder> {
    /// The terrain is not a property. Serializes by being absent.
    #[default]
    NotOwnable,
    /// A property nobody holds.
    Neutral,
    Owned(Holder),
}

/// Who holds a tile of a [`State`].
///
/// The holder is a seat, not a name — see [`PlayerIdx`] for why. That is what
/// makes this `Copy`, and a [`Tile`] with it. Ask [`State::tile_owner_id`] for
/// the name; the projection carries names of its own, in
/// [`ObservedTileOwner`].
pub type TileOwner = TileOwnerOf<PlayerIdx>;

impl<Holder> TileOwnerOf<Holder> {
    pub const fn is_not_ownable(&self) -> bool {
        matches!(self, Self::NotOwnable)
    }

    /// Whether this is a property, held or not.
    pub const fn is_ownable(&self) -> bool {
        !self.is_not_ownable()
    }

    /// The holder, if there is one.
    pub const fn holder(&self) -> Option<&Holder> {
        match self {
            Self::Owned(holder) => Some(holder),
            Self::NotOwnable | Self::Neutral => None,
        }
    }

    /// An ownable tile's holder, from the `null`-or-holder the wire resolved to.
    pub fn ownable(holder: Option<Holder>) -> Self {
        holder.map_or(Self::Neutral, Self::Owned)
    }

    /// The holder as the wire spells it for an ownable tile: `null` or a holder.
    pub fn to_optional(&self) -> Option<Holder>
    where
        Holder: Clone,
    {
        self.holder().cloned()
    }
}

impl TileOwner {
    /// The seat holding this tile, if it is held.
    pub const fn player(&self) -> Option<PlayerIdx> {
        match *self {
            Self::Owned(seat) => Some(seat),
            Self::NotOwnable | Self::Neutral => None,
        }
    }

    pub fn is_owned_by(&self, seat: PlayerIdx) -> bool {
        self.player() == Some(seat)
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
/// One player of a match.
///
/// The name is read-only: a tile and a unit both name their owner by the seat
/// the player sits in, so renaming a seat would silently hand every one of them
/// to somebody else. Everything a match changes — funds, status, the power
/// charge — stays writable, which is the split [`Roster`] exists to keep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    id: PlayerId,
    pub team: TeamId,
    pub funds: u64,
    pub status: PlayerStatus,
    pub commanders: Vec<Commander>,
    pub power_state: PowerState,
}

impl Player {
    /// A player of `team` who holds nothing yet: no funds, no commander, and
    /// no power running.
    pub fn new(id: PlayerId, team: TeamId) -> Self {
        Self {
            id,
            team,
            funds: 0,
            status: PlayerStatus::Active,
            commanders: Vec::new(),
            power_state: PowerState::None,
        }
    }

    /// The player's name, which only a [`Roster`] turns into a seat.
    pub fn id(&self) -> &PlayerId {
        &self.id
    }

    pub fn with_funds(mut self, funds: u64) -> Self {
        self.funds = funds;
        self
    }

    pub fn with_status(mut self, status: PlayerStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_commanders(mut self, commanders: Vec<Commander>) -> Self {
        self.commanders = commanders;
        self
    }

    pub fn with_power_state(mut self, power_state: PowerState) -> Self {
        self.power_state = power_state;
        self
    }

    /// A copy of this player under another name.
    ///
    /// A seat cannot be renamed where it sits — that is the whole point of a
    /// private name — so whoever needs a differently named player builds one
    /// and seats it in a [`Roster`] of its own. Fixtures do this to turn a
    /// one-player state into a two-player one.
    pub fn renamed(&self, id: PlayerId) -> Self {
        Self { id, ..self.clone() }
    }
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
/// A unit in play.
///
/// The owner is a seat, not a name, for the reason [`PlayerIdx`] gives: a name
/// is a `String`, and an army is cloned once per command, so a named owner made
/// every unit an allocation to copy and a pointer to free. Ask
/// [`State::player_id`] for the name.
///
/// Only the roster can spell a seat, so [`Unit`] has no serde of its own —
/// [`State`] drives it through [`UnitFields`] and [`UnitWire`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit {
    pub id: UnitId,
    pub kind: UnitKindId,
    pub owner: PlayerIdx,
    pub hp: u8,
    pub fuel: u64,
    pub ammo: u64,
    pub action: UnitAction,
    pub concealment: Concealment,
    pub location: Location,
}

/// A unit as the wire spells it, borrowed for writing.
#[derive(Serialize)]
struct UnitFields<'a> {
    id: UnitId,
    kind: &'a UnitKindId,
    owner: &'a PlayerId,
    hp: u8,
    fuel: u64,
    ammo: u64,
    action: UnitAction,
    concealment: Concealment,
    location: &'a Location,
}

/// The same shape, owned, for reading.
#[derive(Deserialize)]
struct UnitWire {
    id: UnitId,
    kind: UnitKindId,
    owner: PlayerId,
    hp: u8,
    fuel: u64,
    ammo: u64,
    action: UnitAction,
    concealment: Concealment,
    location: Location,
}

/// An owner a decoded unit names that the roster does not hold.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unit {unit} is held by {owner}, who is not on the roster")]
pub struct UnknownDecodedUnitOwner {
    pub unit: UnitId,
    pub owner: PlayerId,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
        serde_json::from_value::<Pos>(serde_json::json!([255, 255])).unwrap();
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
                TileOwner::Owned(PlayerIdx::from_seat(0)),
                serde_json::json!({"terrain":"plain","owner":"red"}),
            ),
        ] {
            let tile = Tile { owner, ..plain() };
            let wire = tile_wire(&one_tile(tile));
            assert_eq!(wire, expected, "{owner:?} serialized wrong");
            assert_eq!(decode_one_tile(wire).tile(Pos::new(0, 0)).owner, owner);
        }
    }

    #[test]
    fn only_a_held_property_names_a_player() {
        let red = PlayerIdx::from_seat(0);
        let blue = PlayerIdx::from_seat(1);
        assert_eq!(TileOwner::NotOwnable.player(), None);
        assert_eq!(TileOwner::Neutral.player(), None);
        assert!(!TileOwner::Neutral.is_owned_by(red));
        assert!(TileOwner::Owned(red).is_owned_by(red));
        assert!(!TileOwner::Owned(red).is_owned_by(blue));
        // A neutral property is still a property; a mountain is not.
        assert!(TileOwner::Neutral.is_ownable());
        assert!(!TileOwner::NotOwnable.is_ownable());
    }

    /// The index is what makes lookup constant time, so it must survive every
    /// mutation — a stale index silently returns the wrong unit.
    #[test]
    fn the_unit_index_survives_removal_and_growth() {
        let mut units = UnitStore::new(vec![
            unit(0, PlayerIdx::from_seat(0)),
            unit(1, PlayerIdx::from_seat(1)),
            unit(2, PlayerIdx::from_seat(0)),
        ])
        .expect("distinct ids");

        assert_eq!(units.index_of(UnitId::new(2)), Some(2));
        units.remove(0);
        assert_eq!(units.get(UnitId::new(0)), None);
        assert_eq!(units.index_of(UnitId::new(1)), Some(0));
        assert_eq!(units.index_of(UnitId::new(2)), Some(1));

        units.push(unit(7, PlayerIdx::from_seat(0)));
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
                unit(0, PlayerIdx::from_seat(0)),
                unit(0, PlayerIdx::from_seat(1)),
            ]),
            Err(DuplicateUnitId(UnitId::new(0)))
        );
    }

    /// The store is an array on the wire, and each owner is spelled by name.
    #[test]
    fn the_store_travels_as_a_plain_array() {
        let players = roster();
        let units = UnitStore::new(vec![
            unit(0, PlayerIdx::from_seat(0)),
            unit(1, PlayerIdx::from_seat(1)),
        ])
        .unwrap();
        let wire = serde_json::to_value(units.to_fields(&players).unwrap()).unwrap();
        assert!(wire.is_array());
        assert_eq!(wire.as_array().unwrap().len(), 2);
        assert_eq!(wire[0]["owner"], serde_json::json!("p1"));
        assert_eq!(wire[1]["owner"], serde_json::json!("p2"));
        let read: Vec<UnitWire> = serde_json::from_value(wire).unwrap();
        assert_eq!(UnitStore::from_wire(read, &players).unwrap(), units);
    }

    fn plain() -> Tile {
        Tile::new(TerrainId::Plain)
    }

    /// A one-tile board. Rare state belongs to the board, so the wire shape of
    /// a tile can only be exercised through one.
    fn one_tile(tile: Tile) -> Board {
        Board::new(1, 1, vec![tile]).expect("a 1x1 rectangle")
    }

    /// A state whose board is `board` and whose roster seats `red` first.
    ///
    /// A tile names its owner by seat, so a board only has a wire form inside
    /// a state — this is the smallest one that gives a seat a name.
    fn state_around(board: Board) -> State {
        State {
            ruleset: RulesetRef {
                id: RulesetId::from("awbw"),
                revision: RulesetRevision::from("2026-07-10"),
            },
            settings: serde_json::from_value(json!({
                "fog": false,
                "income_per_property": 1000,
                "starting_funds": 0,
                "powers": "enabled",
                "tags": false,
                "weather": "clear",
                "lab_units": [],
                "unit_bans": [],
                "commander_bans": {"lead": [], "backup": []},
                "capture_limit": null,
                "day_limit": null,
                "unit_limit": null,
            }))
            .expect("settings"),
            board,
            teams: vec![Team {
                id: TeamId::from("red-team"),
                status: TeamStatus::Active,
            }],
            players: Roster::new(vec![
                serde_json::from_value(json!({
                    "id": "red",
                    "team": "red-team",
                    "funds": 0,
                    "status": "active",
                    "commanders": [],
                    "power_state": {"type": "none"},
                }))
                .expect("player"),
            ])
            .expect("one player fits a roster"),
            turn: serde_json::from_value(json!({
                "day": 1,
                "active_player": "red",
                "phase": "unit-action",
                "order": ["red"],
                "position": 0,
            }))
            .expect("turn"),
            weather: serde_json::from_value(json!({"kind": "clear", "remaining_turns": 0}))
                .expect("weather"),
            units: UnitStore::default(),
            next_unit_id: None,
            match_state: Match::Active {
                draw_offers: Vec::new(),
            },
        }
    }

    /// The single tile object out of a serialized board.
    fn tile_wire(board: &Board) -> serde_json::Value {
        serde_json::to_value(state_around(board.clone())).unwrap()["board"]["tiles"][0][0].clone()
    }

    /// A board holding just the tile this object describes.
    fn decode_one_tile(wire: serde_json::Value) -> Board {
        let mut state = serde_json::to_value(state_around(one_tile(plain()))).unwrap();
        state["board"] = json!({"width": 1, "height": 1, "tiles": [[wire]]});
        serde_json::from_value::<State>(state).unwrap().board
    }

    /// Moving the rare three onto the board is a representation change, not a
    /// wire change: the object stays flat and seven-keyed, and each key is
    /// still absent when it has no value. The hand-written serde impls on
    /// [`Board`] are the only thing keeping that true, so both directions are
    /// pinned here.
    #[test]
    fn tiles_keep_their_flat_wire_shape_around_the_rare_block() {
        let bare = one_tile(plain());
        assert_eq!(tile_wire(&bare), json!({"terrain": "plain"}));
        assert_eq!(decode_one_tile(json!({"terrain":"plain"})), bare);

        let wire = json!({
            "terrain": "pipe-seam",
            "owner": null,
            "capture_points": 20,
            "silo": "ready",
            "destructible_hp": 99,
            "teleporter": "north",
            "trait_state": {"warp": 1},
        });
        let loaded = decode_one_tile(wire.clone());
        let origin = Pos::new(0, 0);
        assert_eq!(loaded.destructible_hp(origin), Some(99));
        assert_eq!(
            loaded.teleporter(origin),
            Some(&TeleporterId::from("north"))
        );
        assert_eq!(
            loaded
                .trait_state(origin)
                .and_then(|state| state.get(&TraitId::from("warp"))),
            Some(&json!(1))
        );
        assert_eq!(tile_wire(&loaded), wire);
    }

    /// The entry is dropped when the last rare value goes, so a destroyed pipe
    /// seam costs what the plain it becomes costs — and compares equal to a
    /// board whose tile never carried anything.
    #[test]
    fn a_tile_that_loses_its_rare_state_equals_one_that_never_had_any() {
        let origin = Pos::new(0, 0);
        let mut seam = one_tile(plain());
        seam.set_destructible_hp(origin, Some(99));
        assert_ne!(seam, one_tile(plain()));

        seam.set_destructible_hp(origin, None);
        assert_eq!(seam, one_tile(plain()));
        assert_eq!(tile_wire(&seam), json!({"terrain":"plain"}));
    }

    /// Rare state is keyed by position, so a coordinate off the board must not
    /// create an entry that no tile can ever answer for.
    #[test]
    fn rare_state_off_the_board_is_not_recorded() {
        let mut board = one_tile(plain());
        board.set_destructible_hp(Pos::new(1, 0), Some(99));
        assert_eq!(board, one_tile(plain()));
        assert_eq!(board.destructible_hp(Pos::new(1, 0)), None);
    }

    /// The rectangle is checked once, while decoding, so nothing downstream can
    /// hold a ragged board. This replaced `ObserveError::InvalidBoardShape`,
    /// which only `observe` checked — `execute` would have panicked.
    #[test]
    fn a_ragged_board_does_not_decode() {
        let mut ragged = serde_json::to_value(state_around(one_tile(plain()))).unwrap();
        ragged["board"] = json!({
            "width": 2, "height": 2,
            "tiles": [[{"terrain":"plain"}, {"terrain":"plain"}], [{"terrain":"plain"}]]
        });
        let error = serde_json::from_value::<State>(ragged).unwrap_err();
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
        let state = state_around(board.clone());
        let wire = serde_json::to_value(&state).unwrap();
        assert_eq!(wire["board"]["tiles"].as_array().unwrap().len(), 2);
        assert_eq!(wire["board"]["tiles"][0].as_array().unwrap().len(), 2);
        assert_eq!(serde_json::from_value::<State>(wire).unwrap().board, board);
    }

    /// A tile naming a holder the roster does not seat is a decoding failure,
    /// not a seat resolved to something arbitrary.
    #[test]
    fn a_tile_held_by_a_stranger_does_not_decode() {
        let mut wire = serde_json::to_value(state_around(one_tile(plain()))).unwrap();
        wire["board"] = json!({
            "width": 1, "height": 1,
            "tiles": [[{"terrain": "city", "owner": "green", "capture_points": 20}]]
        });
        let error = serde_json::from_value::<State>(wire).unwrap_err();
        assert!(
            error.to_string().contains("not on the roster"),
            "unexpected error: {error}"
        );
    }

    /// A unit naming an owner the roster does not seat fails the same way a
    /// tile does.
    #[test]
    fn a_unit_owned_by_a_stranger_does_not_decode() {
        let mut wire = serde_json::to_value(state_around(one_tile(plain()))).unwrap();
        wire["units"] = json!([{
            "id": 1,
            "kind": "infantry",
            "owner": "green",
            "hp": 100,
            "fuel": 99,
            "ammo": 0,
            "action": "ready",
            "concealment": "exposed",
            "location": {"type": "board", "position": [0, 0]},
        }]);
        let error = serde_json::from_value::<State>(wire).unwrap_err();
        assert!(
            error.to_string().contains("not on the roster"),
            "unexpected error: {error}"
        );
    }

    /// A roster longer than a seat can name is refused before anything reads
    /// it, because every stored owner resolves through a seat.
    #[test]
    fn a_roster_longer_than_a_seat_can_name_does_not_decode() {
        let mut wire = serde_json::to_value(state_around(one_tile(plain()))).unwrap();
        let players: Vec<_> = (0..256)
            .map(|seat| {
                json!({
                    "id": format!("p{seat}"),
                    "team": "red-team",
                    "funds": 0,
                    "status": "active",
                    "commanders": [],
                    "power_state": {"type": "none"},
                })
            })
            .collect();
        wire["players"] = json!(players);
        let error = serde_json::from_value::<State>(wire).unwrap_err();
        assert!(
            error.to_string().contains("a roster holds at most 255"),
            "unexpected error: {error}"
        );
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

    fn roster() -> Vec<Player> {
        ["p1", "p2"]
            .into_iter()
            .map(|id| Player {
                id: PlayerId::from(id),
                team: TeamId::from(id),
                funds: 0,
                status: PlayerStatus::Active,
                commanders: Vec::new(),
                power_state: PowerState::None,
            })
            .collect()
    }

    fn unit(id: u32, owner: PlayerIdx) -> Unit {
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
