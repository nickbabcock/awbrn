//! Pure, presentation-independent AWVM state and recipient observation values.
//!
//! Identifier domains are distinct even where their wire representations are
//! strings. Adapters from replay/ECS identifiers belong at the boundary and
//! must not make this model depend on Bevy entities or AWBW replay IDs.

use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::commander::{self, AreaStrikePolicy};
use crate::event::{Event, ObservedReason, PublicEventKind};
use crate::ruleset::{self, Domain, TerrainTrait};

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

macro_rules! string_id {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

string_id!(RulesetId, PlayerId, TeamId, TeleporterId, TraitId, ReasonId,);

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesetRef {
    pub id: RulesetId,
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub fog: bool,
    pub income_per_property: u64,
    pub starting_funds: u64,
    pub powers: Toggle,
    pub tags: bool,
    pub weather: WeatherSetting,
    #[serde(deserialize_with = "deserialize_unit_kind_set")]
    pub lab_units: Vec<UnitKindId>,
    pub unit_bans: Vec<UnitKindId>,
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
#[serde(rename_all = "kebab-case")]
pub enum Toggle {
    Enabled,
    Disabled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeatherSetting {
    Clear,
    Rain,
    Snow,
    Random,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommanderBans {
    pub lead: Vec<CommanderId>,
    pub backup: Vec<CommanderId>,
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
#[serde(rename_all = "kebab-case")]
pub enum Silo {
    Ready,
    Spent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    pub status: TeamStatus,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(rename_all = "kebab-case")]
pub enum PlayerStatus {
    Active,
    Resigned,
    TimedOut,
    Eliminated,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commander {
    pub id: CommanderId,
    pub active: bool,
    pub power_charge: u64,
    pub power_uses: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PowerState {
    None,
    Cop { commander_slot: u8 },
    Scop { commander_slot: u8 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub day: u64,
    pub active_player: PlayerId,
    pub phase: Phase,
    pub order: Vec<PlayerId>,
    pub position: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    TurnStart,
    UnitAction,
    TurnEnd,
    Finished,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(rename_all = "kebab-case")]
pub enum UnitAction {
    Ready,
    Moved,
    Spent,
    Immobilized,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Concealment {
    Exposed,
    Hidden,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Location {
    Board { position: Pos },
    Cargo { transport: UnitId, slot: usize },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Match {
    Active { draw_offers: Vec<PlayerId> },
    Finished { outcome: Outcome },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// Ruleset-owned visibility, as a factory for per-recipient viewpoints.
///
/// A viewpoint is asked about many tiles and many units for the same state and
/// the same team — the board projection asks about every tile — so the ruleset
/// gets one place to resolve the team roster and its sighting units, instead of
/// redoing that inside every query. Implementations may build this from
/// `world::fog`; the state projection stays independent of Bevy and of cached
/// viewpoints.
pub trait Visibility {
    type View<'a>: Viewpoint
    where
        Self: 'a;

    /// What `team` can see of `state`.
    fn view<'a>(&'a self, state: &'a State, team: &TeamId) -> Self::View<'a>;
}

/// What one team can see of one state.
///
/// Every method answers for the state and team the viewpoint was built from, so
/// a caller cannot accidentally ask one ruleset's question with another's
/// roster.
pub trait Viewpoint {
    /// Whether the tile at `position` is visible. A coordinate off the board is
    /// never visible.
    fn position(&self, position: Pos) -> bool;

    /// Whether `unit` is visible where it currently is.
    fn unit(&self, unit: &Unit) -> bool;

    /// Whether `unit` would be visible standing at `position`.
    ///
    /// The projection needs this to report which steps of an enemy's route the
    /// recipient could watch, without building a unit per step to ask about.
    fn unit_at(&self, unit: &Unit, position: Pos) -> bool;
}

/// Visibility operators for the `awbw/2026-07-10` profile.
///
/// Carries no state: every value it needs is in [`crate::ruleset`].
#[derive(Clone, Copy, Debug, Default)]
pub struct AwbwVisibility;

impl Visibility for AwbwVisibility {
    type View<'a> = AwbwView<'a>;

    fn view<'a>(&'a self, state: &'a State, team: &TeamId) -> AwbwView<'a> {
        AwbwView::new(state, team)
    }
}

/// One team's view of one state under the `awbw/2026-07-10` profile.
///
/// Resolving the team roster and each sighting unit's effective vision are
/// per-state facts, not per-query ones. Computing them here rather than inside
/// every query is what keeps the board projection off an O(tiles x units) path
/// through the commander tables.
#[derive(Clone, Debug)]
pub struct AwbwView<'a> {
    state: &'a State,
    fog: bool,
    /// The viewing team's players. Short enough that a scan beats hashing.
    teammates: Vec<&'a PlayerId>,
    /// Resolved on first use rather than up front. A reducer builds a view to
    /// ask whether one tile is occupied by something it can see, and in a match
    /// without fog or hidden units that answer never consults a sighting.
    sightings: OnceCell<Vec<Sighting>>,
}

/// A friendly unit on the board, with its vision already resolved.
#[derive(Clone, Copy, Debug)]
struct Sighting {
    position: Pos,
    /// Effective vision after the commander, terrain bonus and weather, floored
    /// at one tile.
    sight: u64,
    /// Whether this unit sees into concealing terrain, which lifts the target
    /// terrain's own vision limit.
    reveals_concealing: bool,
}

impl<'a> AwbwView<'a> {
    fn new(state: &'a State, team: &TeamId) -> Self {
        let teammates: Vec<&'a PlayerId> = state
            .players
            .iter()
            .filter(|player| player.team == team)
            .map(|player| &player.id)
            .collect();
        Self {
            state,
            fog: state.settings.fog,
            teammates,
            sightings: OnceCell::new(),
        }
    }

    /// Every friendly unit that can see, with its effective vision already
    /// worked out.
    ///
    /// Each unit's sight depends on its commander, the terrain under it and the
    /// weather — none of which vary by the tile being asked about. Resolving
    /// them here rather than inside the per-tile loop is what takes the board
    /// projection off an O(tiles x units) path through the commander tables.
    fn sightings(&self) -> &[Sighting] {
        self.sightings.get_or_init(|| {
            let state = self.state;
            let rain = -i64::from(matches!(state.weather.kind, WeatherKind::Rain));
            state
                .units
                .iter()
                .filter(|unit| self.teammates.contains(&&unit.owner))
                .filter_map(|unit| {
                    let Location::Board { position } = unit.location else {
                        return None;
                    };
                    let profile = ruleset::profile(unit.kind);
                    let bonus = if profile.elevated_vision {
                        ruleset::terrain(state.board.tile(position).terrain)
                            .vision_bonus
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    let vision =
                        commander::effective_vision(state, unit, profile.vision, profile.domain);
                    Some(Sighting {
                        position,
                        sight: (vision + bonus + rain).max(1) as u64,
                        reveals_concealing: commander::reveals_concealing_terrain(state, unit),
                    })
                })
                .collect()
        })
    }

    fn holds(&self, owner: Option<&PlayerId>) -> bool {
        owner.is_some_and(|owner| self.teammates.contains(&owner))
    }

    fn vision_level(&self, position: Pos) -> VisionLevel {
        if !self.state.board.contains(position) {
            return VisionLevel::None;
        }
        if !self.fog {
            return VisionLevel::Full;
        }
        let tile = self.state.board.tile(position);
        let target_terrain = ruleset::terrain(tile.terrain);
        if target_terrain.has(TerrainTrait::Teleporter) {
            return VisionLevel::None;
        }
        if self.holds(tile.owner.player()) {
            return VisionLevel::Full;
        }
        if target_terrain.has(TerrainTrait::AlwaysVisible) {
            return VisionLevel::Full;
        }
        let mut level = VisionLevel::None;
        for sighting in self.sightings() {
            let distance = sighting.position.distance(position);
            if distance > sighting.sight {
                continue;
            }
            let contribution = if sighting.reveals_concealing
                || target_terrain
                    .vision_limit
                    .is_none_or(|limit| distance <= limit as u64)
            {
                VisionLevel::Full
            } else {
                VisionLevel::AirOnly
            };
            level = level.max(contribution);
        }
        level
    }
}

#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
enum VisionLevel {
    None,
    AirOnly,
    Full,
}

impl Viewpoint for AwbwView<'_> {
    fn position(&self, position: Pos) -> bool {
        self.vision_level(position) == VisionLevel::Full
    }

    fn unit(&self, unit: &Unit) -> bool {
        match unit.location {
            Location::Board { position } => self.unit_at(unit, position),
            // Cargo is only ever visible to its own team, which `unit_at`
            // establishes before it looks at a position.
            Location::Cargo { .. } => self.holds(Some(&unit.owner)),
        }
    }

    fn unit_at(&self, unit: &Unit, position: Pos) -> bool {
        if self.holds(Some(&unit.owner)) {
            return true;
        }
        if self.holds(
            self.state
                .board
                .get(position)
                .and_then(|tile| tile.owner.player()),
        ) {
            return true;
        }
        // A hidden unit is given away only by standing next to something of the
        // viewing team, whether or not the match is fogged.
        if unit.concealment == Concealment::Hidden {
            return self
                .sightings()
                .iter()
                .any(|sighting| sighting.position.distance(position) == 1);
        }
        if !self.fog {
            return true;
        }
        match self.vision_level(position) {
            VisionLevel::Full => true,
            VisionLevel::AirOnly => ruleset::profile(unit.kind).domain == Domain::Air,
            VisionLevel::None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Observation {
    pub ruleset: RulesetRef,
    pub recipient: PlayerId,
    pub settings: Settings,
    pub board: ObservedBoard,
    pub teams: Vec<Team>,
    pub players: Vec<ObservedPlayer>,
    pub turn: Turn,
    pub weather: Weather,
    pub units: Vec<ObservedUnit>,
    #[serde(rename = "match")]
    pub match_state: ObservedMatch,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ObservedUnitRef {
    Friendly { unit: UnitId },
    Enemy { position: Pos },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservedUnit {
    #[serde(rename = "ref")]
    pub reference: ObservedUnitRef,
    pub kind: UnitKindId,
    pub owner: PlayerId,
    pub hp: u8,
    pub fuel: u64,
    pub ammo: u64,
    pub action: UnitAction,
    pub concealment: Concealment,
    pub location: Location,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservedBoard {
    pub width: u8,
    pub height: u8,
    pub tiles: Vec<Vec<ObservedTile>>,
}
impl ObservedBoard {
    /// The projected tile at a coordinate.
    ///
    /// The projection keeps the nested wire shape, so this is the one place
    /// that still turns an `[x, y]` coordinate into a row and a column.
    pub fn tile(&self, position: Pos) -> &ObservedTile {
        &self.tiles[usize::from(position.y)][usize::from(position.x)]
    }
}

/// A tile as one recipient sees it.
///
/// Boxes its rare state for the same reason [`Tile`] does: an observation holds
/// one of these per square, and one is built per recipient per command. Its
/// `rare` is only ever built by the projection, which leaves it `None` when
/// there is nothing to hold — see [`Tile::shrink`] for why that matters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedTile {
    pub terrain: TerrainId,
    pub visibility: TileVisibility,
    pub owner: TileOwner,
    pub capture_points: Option<u8>,
    pub silo: Option<Silo>,
    rare: Option<Box<RareTileState>>,
}

impl ObservedTile {
    pub fn destructible_hp(&self) -> Option<u64> {
        self.rare.as_ref().and_then(|rare| rare.destructible_hp)
    }

    pub fn teleporter(&self) -> Option<&TeleporterId> {
        self.rare.as_ref().and_then(|rare| rare.teleporter.as_ref())
    }

    pub fn trait_state(&self) -> Option<&BTreeMap<TraitId, serde_json::Value>> {
        self.rare
            .as_ref()
            .and_then(|rare| rare.trait_state.as_ref())
    }
}

#[derive(Serialize)]
struct ObservedTileFields<'a> {
    terrain: TerrainId,
    visibility: TileVisibility,
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

impl Serialize for ObservedTile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ObservedTileFields {
            terrain: self.terrain,
            visibility: self.visibility,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TileVisibility {
    Visible,
    Fogged,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ObservedPlayer {
    Private {
        id: PlayerId,
        team: TeamId,
        relation: Relation,
        funds: u64,
        status: PlayerStatus,
        commanders: Vec<Commander>,
        power_state: PowerState,
    },
    Public {
        id: PlayerId,
        team: TeamId,
        relation: Relation,
        status: PlayerStatus,
        commanders: Vec<PublicCommander>,
        power_state: PowerState,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    #[serde(rename = "self")]
    Self_,
    Ally,
    Opponent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PublicCommander {
    pub id: CommanderId,
    pub active: bool,
    pub power_charge: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ObservedMatch {
    Active { own_team_offers: Vec<PlayerId> },
    Finished { outcome: Outcome },
}

/// One authoritative fact as a single recipient is entitled to see it.
///
/// Every variant is one `oneOf` branch of
/// `spec/schema/observed-event.schema.json`, so a projection the schema does not
/// license cannot be constructed — the same property [`Event`] gives the
/// authoritative side. A consumer translating these into a presentation model
/// matches exhaustively rather than reading a `type` string, and a new branch
/// stops its match compiling.
///
/// This is deliberately *not* [`Event`] with fields removed. A recipient sees
/// enemies by position rather than by id, learns of appearances and
/// disappearances that no authoritative event names, and receives the payload-free
/// [`PublicEventKind`] envelope in place of ten different public facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ObservedEvent {
    /// `path` holds only the positions along the route the recipient could see
    /// the mover occupy, so a move through fog is reported with gaps.
    UnitMoved {
        unit: ObservedUnitRef,
        from: Pos,
        to: Pos,
        path: Vec<Pos>,
    },
    /// A unit the recipient could not see before is now visible.
    UnitAppeared { unit: ObservedUnit, position: Pos },
    /// A unit the recipient could see is no longer visible, without the
    /// recipient learning why.
    UnitDisappeared {
        unit: ObservedUnitRef,
        position: Pos,
    },
    /// The mover was interrupted. Reported without naming the blocker, which the
    /// recipient may not see.
    MovementStopped { unit: ObservedUnitRef },
    UnitChanged {
        unit: ObservedUnitRef,
        state: ObservedUnit,
        reason: ObservedReason,
    },
    UnitRemoved {
        unit: ObservedUnitRef,
        reason: ObservedReason,
    },
    TileChanged {
        position: Pos,
        tile: ObservedTile,
        reason: ObservedReason,
    },
    PlayerChanged {
        player: PlayerId,
        state: ObservedPlayer,
        reason: ObservedReason,
    },
    /// Public in full even when fog hides the units it hits
    /// (`spec/model/observation.md:318`).
    AreaStrikeResolved {
        strike: usize,
        policy: AreaStrikePolicy,
        center: Pos,
        radius: usize,
        damage: u8,
    },
    /// A public fact changed. Carries no payload by design; the recipient reads
    /// every new value from the post-observation
    /// (`spec/model/observation.md:329`), which is why [`observe_transition`]
    /// hands that back alongside these.
    PublicEvent { kind: PublicEventKind },
}

/// What one command looked like to one recipient.
///
/// The post-observation is not a convenience: `public-event` carries no payload,
/// so `spec/model/observation.md:329` makes `post` the authority for every
/// public value the events only signal. Projecting the events already requires
/// computing it, so returning it costs nothing and saves the caller a second
/// full projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservedTransition {
    pub post: Observation,
    pub events: Vec<ObservedEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ObserveError {
    #[error("UnknownRecipient({0:?})")]
    UnknownRecipient(PlayerId),
    #[error("UnknownUnitOwner({0:?})")]
    UnknownUnitOwner(PlayerId),
    /// An event names a unit that is in neither the prior nor the next state.
    /// The three inputs are supplied independently over the protocol, so they
    /// can disagree; a typed event cannot fail to decode, but it can still
    /// reference a unit the caller did not send.
    #[error("UnknownUnit({0:?})")]
    UnknownUnit(UnitId),
}

/// Project a state as one recipient is entitled to see it.
pub fn observe(
    rules: &impl Visibility,
    state: &State,
    recipient: &PlayerId,
) -> Result<Observation, ObserveError> {
    let team = recipient_team(state, recipient)?;
    project_state(&rules.view(state, team), state, recipient, team)
}

/// Project one command for one recipient: the post-state as that recipient sees
/// it, and the events it is entitled to.
///
/// Both halves are needed together. A `public-event` carries no payload, so
/// `spec/model/observation.md:329` makes the post-observation the authority for
/// every value those events merely signal — and projecting the events has to
/// compute it regardless, to fill in the tile and player snapshots that
/// `tile-changed` and `player-changed` carry.
pub fn observe_transition(
    rules: &impl Visibility,
    state: &State,
    next_state: &State,
    events: &[Event],
    recipient: &PlayerId,
) -> Result<ObservedTransition, ObserveError> {
    let team = recipient_team(state, recipient)?;
    // The prior state is validated, not projected. This used to call `observe`
    // and drop the result, which cost a whole board projection to reach these
    // two checks.
    for unit in &state.units {
        if state.find_player(&unit.owner).is_none() {
            return Err(ObserveError::UnknownUnitOwner(unit.owner.clone()));
        }
    }
    // The recipient's team is taken from `state` throughout, which is what the
    // event projection has always done. A recipient who changed teams between
    // the two states is not a transition the specification admits; this still
    // rejects one who is absent from `next_state` altogether.
    recipient_team(next_state, recipient)?;

    let pre_view = rules.view(state, team);
    let post_view = rules.view(next_state, team);
    let post = project_state(&post_view, next_state, recipient, team)?;

    let teammates: Vec<&PlayerId> = state
        .players
        .iter()
        .filter(|player| player.team == team)
        .map(|player| &player.id)
        .collect();
    let visible = |view: &dyn Fn(&Unit) -> bool, units: &UnitStore| -> HashSet<UnitId> {
        units
            .iter()
            .filter(|unit| view(unit))
            .map(|u| u.id)
            .collect()
    };
    let mut projection = Projection {
        pre_view: &pre_view,
        post_view: &post_view,
        state,
        next_state,
        post,
        teammates,
        visible_pre: visible(&|unit| pre_view.unit(unit), &state.units),
        visible_post: visible(&|unit| post_view.unit(unit), &next_state.units),
        appeared: HashSet::new(),
        disappeared: HashSet::new(),
        output: Vec::new(),
    };
    projection.project(events)?;
    Ok(ObservedTransition {
        post: projection.post,
        events: projection.output,
    })
}

/// Project authoritative transition events for one recipient.
///
/// A caller that also needs the post-observation — which anything acting on a
/// `public-event` does — should use [`observe_transition`] rather than calling
/// both, since this discards an observation it had to compute.
pub fn observe_events(
    rules: &impl Visibility,
    state: &State,
    next_state: &State,
    events: &[Event],
    recipient: &PlayerId,
) -> Result<Vec<ObservedEvent>, ObserveError> {
    observe_transition(rules, state, next_state, events, recipient)
        .map(|transition| transition.events)
}

fn recipient_team<'a>(state: &'a State, recipient: &PlayerId) -> Result<&'a TeamId, ObserveError> {
    state
        .find_player(recipient)
        .map(|player| &player.team)
        .ok_or_else(|| ObserveError::UnknownRecipient(recipient.clone()))
}

fn project_state(
    view: &impl Viewpoint,
    state: &State,
    recipient: &PlayerId,
    team: &TeamId,
) -> Result<Observation, ObserveError> {
    let owners: HashMap<&PlayerId, &TeamId> =
        state.players.iter().map(|p| (&p.id, &p.team)).collect();
    let tiles = state
        .board
        .rows()
        .map(|row| {
            row.map(|(position, t)| {
                let visible = view.position(position);
                // A teleporter pairing is disclosed even through fog; the rest
                // of a tile's rare state is not.
                let rare = RareTileState {
                    destructible_hp: visible.then_some(t.destructible_hp()).flatten(),
                    teleporter: t.teleporter().cloned(),
                    trait_state: visible.then(|| t.trait_state().cloned()).flatten(),
                };
                ObservedTile {
                    terrain: t.terrain,
                    visibility: if visible {
                        TileVisibility::Visible
                    } else {
                        TileVisibility::Fogged
                    },
                    owner: if visible {
                        t.owner.clone()
                    } else {
                        TileOwner::NotOwnable
                    },
                    capture_points: visible.then_some(t.capture_points).flatten(),
                    silo: visible.then_some(t.silo).flatten(),
                    rare: (!rare.is_empty()).then(|| Box::new(rare)),
                }
            })
            .collect()
        })
        .collect();
    let players = state
        .players
        .iter()
        .map(|p| {
            if p.team == team {
                ObservedPlayer::Private {
                    id: p.id.clone(),
                    team: p.team.clone(),
                    relation: if p.id == recipient {
                        Relation::Self_
                    } else {
                        Relation::Ally
                    },
                    funds: p.funds,
                    status: p.status,
                    commanders: p.commanders.clone(),
                    power_state: p.power_state.clone(),
                }
            } else {
                ObservedPlayer::Public {
                    id: p.id.clone(),
                    team: p.team.clone(),
                    relation: Relation::Opponent,
                    status: p.status,
                    commanders: p
                        .commanders
                        .iter()
                        .map(|c| PublicCommander {
                            id: c.id,
                            active: c.active,
                            power_charge: c.power_charge,
                        })
                        .collect(),
                    power_state: p.power_state.clone(),
                }
            }
        })
        .collect();
    let mut units = Vec::new();
    for u in &state.units {
        let owner_team = *owners
            .get(&u.owner)
            .ok_or_else(|| ObserveError::UnknownUnitOwner(u.owner.clone()))?;
        // A viewpoint already reports a teammate's unit as visible wherever it
        // is, including inside a transport, and an opponent's cargo as hidden.
        if view.unit(u) {
            units.push(observed_unit_snapshot(u, owner_team == team));
        }
    }
    units.sort_by_key(|unit| unit.reference);
    let match_state = match &state.match_state {
        Match::Active { draw_offers } => {
            let mut offers: Vec<_> = draw_offers
                .iter()
                .filter(|id| owners.get(id).is_some_and(|t| *t == team))
                .cloned()
                .collect();
            offers.sort();
            ObservedMatch::Active {
                own_team_offers: offers,
            }
        }
        Match::Finished { outcome } => ObservedMatch::Finished {
            outcome: outcome.clone(),
        },
    };
    Ok(Observation {
        ruleset: state.ruleset.clone(),
        recipient: recipient.clone(),
        settings: state.settings.clone(),
        board: ObservedBoard {
            width: state.board.width(),
            height: state.board.height(),
            tiles,
        },
        teams: state.teams.clone(),
        players,
        turn: state.turn.clone(),
        weather: state.weather.clone(),
        units,
        match_state,
    })
}

/// The context every event's projection shares.
///
/// These were eleven parameters threaded through free functions, which is why
/// two of them carried `#[allow(clippy::too_many_arguments)]`. Nothing here is
/// derivable from an event alone: whether a unit was visible before and after is
/// what decides between reporting a change, an appearance, and a disappearance.
struct Projection<'a, V: Viewpoint> {
    pre_view: &'a V,
    post_view: &'a V,
    state: &'a State,
    next_state: &'a State,
    /// The post-state as the recipient sees it, which is where `tile-changed`
    /// and `player-changed` take their snapshots from, and which
    /// [`observe_transition`] hands back.
    post: Observation,
    /// The recipient's own team. Short enough that a scan beats hashing.
    teammates: Vec<&'a PlayerId>,
    visible_pre: HashSet<UnitId>,
    visible_post: HashSet<UnitId>,
    /// Appearances and disappearances already announced, so that several events
    /// about one unit produce at most one of each.
    appeared: HashSet<UnitId>,
    disappeared: HashSet<UnitId>,
    output: Vec<ObservedEvent>,
}

impl<V: Viewpoint> Projection<'_, V> {
    fn owns(&self, player: &PlayerId) -> bool {
        self.teammates.contains(&player)
    }

    fn project(&mut self, events: &[Event]) -> Result<(), ObserveError> {
        for event in events {
            let reason = event.reason();
            match event {
                Event::UnitActionChanged { unit, .. }
                | Event::UnitDamaged { unit, .. }
                | Event::UnitRepaired { unit, .. }
                | Event::UnitResourced { unit, .. }
                | Event::ConcealmentChanged { unit, .. }
                | Event::AutomaticRepair { unit, .. } => self.unit_fact(*unit, reason),
                Event::AutomaticSupply { units, .. } => {
                    for unit in units {
                        self.unit_fact(*unit, reason.clone());
                    }
                }
                Event::UnitMoved {
                    unit: id,
                    from,
                    to,
                    path,
                    ..
                } => self.movement(*id, *from, *to, path)?,
                Event::MovementTrapped { unit: id, .. } => {
                    let id = *id;
                    if self
                        .state
                        .units
                        .get(id)
                        .is_some_and(|unit| self.owns(&unit.owner))
                    {
                        self.output.push(ObservedEvent::MovementStopped {
                            unit: ObservedUnitRef::Friendly { unit: id },
                        });
                    }
                }
                Event::UnitCreated {
                    unit: id, position, ..
                } => {
                    let id = *id;
                    if self.visible_post.contains(&id)
                        && let Some(unit) = self.next_state.units.get(id)
                    {
                        let friendly = self.owns(&unit.owner);
                        self.push_appeared(unit, *position, friendly);
                    }
                }
                Event::UnitRemoved { unit, .. } => self.removal(*unit, reason)?,
                Event::UnitsJoined { source, target } => {
                    self.removal(*source, reason.clone())?;
                    self.unit_fact(*target, reason);
                }
                Event::UnitLoaded { unit: id, .. } => {
                    let id = *id;
                    let unit = self.state.units.get(id);
                    if unit.is_some_and(|unit| self.owns(&unit.owner)) {
                        self.unit_fact(id, reason);
                    } else if self.visible_pre.contains(&id)
                        && let Some(position) = unit.and_then(board_position)
                    {
                        self.push_disappeared(id, position);
                    }
                }
                Event::UnitUnloaded {
                    unit: id, position, ..
                } => {
                    let id = *id;
                    let unit = self.next_state.units.get(id);
                    if unit.is_some_and(|unit| self.owns(&unit.owner)) {
                        self.unit_fact(id, reason);
                    } else if self.visible_post.contains(&id)
                        && let Some(unit) = unit
                    {
                        self.push_appeared(unit, *position, false);
                    }
                }
                Event::TileOwnerChanged { position, .. }
                | Event::TileTerrainChanged { position, .. }
                | Event::CaptureChanged { position, .. }
                | Event::SiloChanged { position, .. }
                | Event::DestructibleDamaged { position, .. } => {
                    let position = *position;
                    if self.pre_view.position(position) || self.post_view.position(position) {
                        self.output.push(ObservedEvent::TileChanged {
                            position,
                            tile: self.post.board.tile(position).clone(),
                            reason,
                        });
                    }
                }
                // A player only learns their own funds changed; a power charge is
                // public, so it is projected to everyone.
                Event::FundsChanged { player, .. } if !self.owns(player) => {}
                Event::FundsChanged { player, .. } | Event::PowerChargeChanged { player, .. } => {
                    if let Some(snapshot) =
                        self.post.players.iter().find(|candidate| match candidate {
                            ObservedPlayer::Private { id, .. }
                            | ObservedPlayer::Public { id, .. } => id == player,
                        })
                    {
                        self.output.push(ObservedEvent::PlayerChanged {
                            player: player.clone(),
                            state: snapshot.clone(),
                            reason,
                        });
                    }
                }
                Event::DrawOfferChanged { player, .. } => {
                    if self.owns(player) {
                        self.public(event);
                    }
                }
                Event::PhaseChanged { .. }
                | Event::TurnSelected { .. }
                | Event::DayAdvanced { .. }
                | Event::WeatherChanged { .. }
                | Event::PowerActivated { .. }
                | Event::PowerEnded { .. }
                | Event::CommanderSwapped { .. }
                | Event::PlayerStatusChanged { .. }
                | Event::TeamEliminated { .. }
                | Event::MatchCompleted { .. } => self.public(event),
                Event::AreaStrikeResolved {
                    strike,
                    policy,
                    center,
                    radius,
                    damage,
                } => self.output.push(ObservedEvent::AreaStrikeResolved {
                    strike: *strike,
                    policy: *policy,
                    center: *center,
                    radius: *radius,
                    damage: *damage,
                }),
                // Deliberately unprojected. `spec/model/observation.md:337` withholds
                // both: `attack-resolved` would disclose the weapon and target of an
                // attack a recipient may not see, and `random-outcome` would leak the
                // tape a recipient is not entitled to read. The damage and state
                // changes they cause reach recipients through their own events.
                Event::AttackResolved { .. } | Event::RandomOutcome { .. } => {}
            }
        }
        Ok(())
    }

    /// Emit the payload-free envelope for a public fact.
    ///
    /// [`EventKind::public`] is exhaustive over every kind, so an event that
    /// reaches here without a public spelling is one this match should not have
    /// routed here — and `only_the_documented_kinds_are_public` pins which are
    /// which.
    fn public(&mut self, event: &Event) {
        self.output.extend(
            event
                .kind()
                .public()
                .map(|kind| ObservedEvent::PublicEvent { kind }),
        );
    }

    /// Project an ordinary fact about one unit: a change if the recipient could
    /// see it throughout, otherwise the appearance or disappearance that
    /// crossing the visibility boundary amounts to.
    fn unit_fact(&mut self, id: UnitId, reason: ObservedReason) {
        let Some(unit) = self.next_state.units.get(id) else {
            return;
        };
        match (
            self.visible_pre.contains(&id),
            self.visible_post.contains(&id),
        ) {
            (true, true) => {
                let snapshot = observed_unit_snapshot(unit, self.owns(&unit.owner));
                self.output.push(ObservedEvent::UnitChanged {
                    unit: snapshot.reference,
                    state: snapshot,
                    reason,
                });
            }
            (false, true) => {
                if let Some(position) = board_position(unit) {
                    let friendly = self.owns(&unit.owner);
                    self.push_appeared(unit, position, friendly);
                }
            }
            (true, false) => {
                if let Some(position) = self.state.units.get(id).and_then(board_position) {
                    self.push_disappeared(id, position);
                }
            }
            (false, false) => {}
        }
    }

    /// Project a move. The recipient's own move is reported in full; an
    /// opponent's is reported only along the stretch the recipient could watch.
    fn movement(
        &mut self,
        id: UnitId,
        from: Pos,
        to: Pos,
        path: &[Pos],
    ) -> Result<(), ObserveError> {
        let unit = self
            .state
            .units
            .get(id)
            .ok_or(ObserveError::UnknownUnit(id))?;
        if self.owns(&unit.owner) {
            self.output.push(ObservedEvent::UnitMoved {
                unit: ObservedUnitRef::Friendly { unit: id },
                from,
                to,
                path: path.to_vec(),
            });
            return Ok(());
        }
        match (
            self.visible_pre.contains(&id),
            self.visible_post.contains(&id),
        ) {
            (true, false) => self.push_disappeared(id, from),
            (false, true) => {
                if let Some(snapshot) = self.next_state.units.get(id) {
                    self.push_appeared(snapshot, to, false);
                }
            }
            (true, true) => {
                let post_unit = self
                    .next_state
                    .units
                    .get(id)
                    .ok_or(ObserveError::UnknownUnit(id))?;
                let observed_path = path
                    .iter()
                    .copied()
                    .filter(|position| {
                        self.pre_view.unit_at(post_unit, *position)
                            || self.post_view.unit_at(post_unit, *position)
                    })
                    .collect();
                self.output.push(ObservedEvent::UnitMoved {
                    unit: enemy_unit_ref(to),
                    from,
                    to,
                    path: observed_path,
                });
            }
            (false, false) => {}
        }
        Ok(())
    }

    /// Project a removal. An opponent's removal is only disclosed where the
    /// recipient can see the tile it happened on; otherwise the unit merely
    /// disappears.
    fn removal(&mut self, id: UnitId, reason: ObservedReason) -> Result<(), ObserveError> {
        let unit = self
            .state
            .units
            .get(id)
            .ok_or(ObserveError::UnknownUnit(id))?;
        if self.owns(&unit.owner) {
            self.output.push(ObservedEvent::UnitRemoved {
                unit: ObservedUnitRef::Friendly { unit: id },
                reason,
            });
        } else if self.visible_pre.contains(&id) {
            // A removed enemy that was visible must have been on the board:
            // visibility is only ever computed for board positions.
            let position = board_position(unit).ok_or(ObserveError::UnknownUnit(id))?;
            if self.post_view.position(position) {
                self.output.push(ObservedEvent::UnitRemoved {
                    unit: enemy_unit_ref(position),
                    reason,
                });
            } else {
                self.push_disappeared(id, position);
            }
        }
        Ok(())
    }

    fn push_appeared(&mut self, unit: &Unit, position: Pos, friendly: bool) {
        if self.appeared.insert(unit.id) {
            self.output.push(ObservedEvent::UnitAppeared {
                unit: observed_unit_snapshot(unit, friendly),
                position,
            });
        }
    }

    fn push_disappeared(&mut self, id: UnitId, position: Pos) {
        if self.disappeared.insert(id) {
            self.output.push(ObservedEvent::UnitDisappeared {
                unit: enemy_unit_ref(position),
                position,
            });
        }
    }
}

fn observed_unit_snapshot(unit: &Unit, friendly: bool) -> ObservedUnit {
    ObservedUnit {
        reference: if friendly {
            ObservedUnitRef::Friendly { unit: unit.id }
        } else {
            enemy_unit_ref(
                board_position(unit).expect("an observed enemy unit must be on the board"),
            )
        },
        kind: unit.kind,
        owner: unit.owner.clone(),
        hp: unit.hp,
        fuel: unit.fuel,
        ammo: unit.ammo,
        action: unit.action,
        concealment: unit.concealment,
        location: unit.location.clone(),
    }
}

fn enemy_unit_ref(position: Pos) -> ObservedUnitRef {
    ObservedUnitRef::Enemy { position }
}

fn board_position(unit: &Unit) -> Option<Pos> {
    match unit.location {
        Location::Board { position } => Some(position),
        Location::Cargo { .. } => None,
    }
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

    /// Every branch of `spec/schema/observed-event.schema.json`, in the shape
    /// the schema names it. The goldens cover whichever branches the corpus
    /// happens to produce; this covers all ten.
    #[test]
    fn observed_events_serialize_as_their_schema_branches() {
        let enemy = ObservedUnitRef::Enemy {
            position: Pos::new(1, 2),
        };
        let friendly = ObservedUnitRef::Friendly {
            unit: UnitId::new(7),
        };
        let unit = ObservedUnit {
            reference: friendly,
            kind: UnitKindId::Infantry,
            owner: PlayerId::from("p1"),
            hp: 100,
            fuel: 99,
            ammo: 0,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board {
                position: Pos::new(1, 2),
            },
        };
        let cases = [
            (
                ObservedEvent::UnitMoved {
                    unit: enemy,
                    from: Pos::new(0, 0),
                    to: Pos::new(1, 2),
                    path: vec![Pos::new(0, 0), Pos::new(1, 2)],
                },
                json!({"type":"unit-moved","unit":{"type":"enemy","position":[1,2]},
                       "from":[0,0],"to":[1,2],"path":[[0,0],[1,2]]}),
            ),
            (
                ObservedEvent::UnitAppeared {
                    unit: unit.clone(),
                    position: Pos::new(1, 2),
                },
                json!({"type":"unit-appeared","unit":serde_json::to_value(&unit).unwrap(),
                       "position":[1,2]}),
            ),
            (
                ObservedEvent::UnitDisappeared {
                    unit: enemy,
                    position: Pos::new(1, 2),
                },
                json!({"type":"unit-disappeared","unit":{"type":"enemy","position":[1,2]},
                       "position":[1,2]}),
            ),
            (
                ObservedEvent::MovementStopped { unit: friendly },
                json!({"type":"movement-stopped","unit":{"type":"friendly","unit":7}}),
            ),
            (
                ObservedEvent::UnitChanged {
                    unit: friendly,
                    state: unit.clone(),
                    reason: ObservedReason::Declared(KnownReason::Combat.into()),
                },
                json!({"type":"unit-changed","unit":{"type":"friendly","unit":7},
                       "state":serde_json::to_value(&unit).unwrap(),"reason":"combat"}),
            ),
            (
                ObservedEvent::UnitRemoved {
                    unit: friendly,
                    reason: ObservedReason::Kind(crate::event::EventKind::UnitsJoined),
                },
                json!({"type":"unit-removed","unit":{"type":"friendly","unit":7},
                       "reason":"units-joined"}),
            ),
            (
                ObservedEvent::AreaStrikeResolved {
                    strike: 0,
                    policy: AreaStrikePolicy::UnitValue,
                    center: Pos::new(3, 4),
                    radius: 2,
                    damage: 30,
                },
                json!({"type":"area-strike-resolved","strike":0,"policy":"unit-value",
                       "center":[3,4],"radius":2,"damage":30}),
            ),
            (
                ObservedEvent::PublicEvent {
                    kind: PublicEventKind::DayAdvanced,
                },
                json!({"type":"public-event","kind":"day-advanced"}),
            ),
        ];
        for (event, expected) in cases {
            assert_eq!(serde_json::to_value(&event).unwrap(), expected);
        }

        let tile = ObservedTile {
            terrain: TerrainId::Plain,
            visibility: TileVisibility::Visible,
            owner: TileOwner::NotOwnable,
            capture_points: None,
            silo: None,
            rare: None,
        };
        assert_eq!(
            serde_json::to_value(ObservedEvent::TileChanged {
                position: Pos::new(1, 2),
                tile,
                reason: ObservedReason::Kind(crate::event::EventKind::CaptureChanged),
            })
            .unwrap(),
            json!({"type":"tile-changed","position":[1,2],
                   "tile":{"terrain":"plain","visibility":"visible"},
                   "reason":"capture-changed"})
        );

        let player = ObservedPlayer::Public {
            id: PlayerId::from("p2"),
            team: TeamId::from("t2"),
            relation: Relation::Opponent,
            status: PlayerStatus::Active,
            commanders: vec![],
            power_state: PowerState::None,
        };
        assert_eq!(
            serde_json::to_value(ObservedEvent::PlayerChanged {
                player: PlayerId::from("p2"),
                state: player.clone(),
                reason: ObservedReason::Declared(KnownReason::Combat.into()),
            })
            .unwrap(),
            json!({"type":"player-changed","player":"p2",
                   "state":serde_json::to_value(&player).unwrap(),"reason":"combat"})
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
    /// A viewpoint that answers the same for everything, for tests that pin what
    /// the projection does with a given visibility rather than what the ruleset's
    /// visibility computes.
    struct Constant(bool);

    impl Viewpoint for Constant {
        fn position(&self, _: Pos) -> bool {
            self.0
        }
        fn unit(&self, _: &Unit) -> bool {
            self.0
        }
        fn unit_at(&self, _: &Unit, _: Pos) -> bool {
            self.0
        }
    }

    struct NoneVisible;
    impl Visibility for NoneVisible {
        type View<'a> = Constant;
        fn view<'a>(&'a self, _: &'a State, _: &TeamId) -> Constant {
            Constant(false)
        }
    }
    #[test]
    fn relation_self_serializes_as_schema_value() {
        assert_eq!(serde_json::to_value(Relation::Self_).unwrap(), "self");
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

    #[test]
    fn hidden_enemy_substitution_does_not_change_observation() {
        let mut s = fixture();
        let recipient = PlayerId::from("p1");
        let a = observe(&NoneVisible, &s, &recipient).unwrap();
        s.units[1].id = UnitId::new(2);
        s.units.push(Unit {
            id: UnitId::new(3),
            ..s.units[1].clone()
        });
        assert_eq!(a, observe(&NoneVisible, &s, &recipient).unwrap());
    }

    #[test]
    fn visible_enemy_authoritative_id_is_not_observed() {
        struct AllVisible;
        impl Visibility for AllVisible {
            type View<'a> = Constant;
            fn view<'a>(&'a self, _: &'a State, _: &TeamId) -> Constant {
                Constant(true)
            }
        }

        let mut state = fixture();
        let recipient = PlayerId::from("p1");
        let before = observe(&AllVisible, &state, &recipient).unwrap();
        state.units[1].id = UnitId::new(99);
        assert_eq!(before, observe(&AllVisible, &state, &recipient).unwrap());
        assert_eq!(
            before
                .units
                .iter()
                .find(|unit| unit.owner == "p2")
                .unwrap()
                .reference,
            ObservedUnitRef::Enemy {
                position: Pos::new(0, 0)
            }
        );
    }

    #[test]
    fn enemy_path_keeps_visible_positions_on_both_sides_of_woods() {
        let mut state = fixture();
        state.board = Board::new(
            6,
            1,
            (0..6)
                .map(|x| Tile {
                    terrain: if x == 4 {
                        TerrainId::Wood
                    } else {
                        TerrainId::Plain
                    },
                    owner: TileOwner::NotOwnable,
                    capture_points: None,
                    silo: None,
                    rare: None,
                })
                .collect(),
        )
        .expect("a single row is a rectangle");
        state.units[0].kind = UnitKindId::Recon;
        state.units[0].location = Location::Board {
            position: Pos::new(0, 0),
        };
        state.units[1].kind = UnitKindId::Tank;
        state.units[1].location = Location::Board {
            position: Pos::new(5, 0),
        };

        let mut next_state = state.clone();
        next_state.units[1].location = Location::Board {
            position: Pos::new(3, 0),
        };
        let events = vec![Event::UnitMoved {
            unit: UnitId::new(1),
            from: Pos::new(5, 0),
            to: Pos::new(3, 0),
            path: vec![Pos::new(5, 0), Pos::new(4, 0), Pos::new(3, 0)],
            fuel_spent: 2,
        }];

        assert_eq!(
            observe_events(
                &AwbwVisibility,
                &state,
                &next_state,
                &events,
                &PlayerId::from("p1"),
            )
            .unwrap(),
            vec![ObservedEvent::UnitMoved {
                unit: ObservedUnitRef::Enemy {
                    position: Pos::new(3, 0)
                },
                from: Pos::new(5, 0),
                to: Pos::new(3, 0),
                path: vec![Pos::new(5, 0), Pos::new(3, 0)],
            }]
        );
    }

    #[test]
    fn teleporter_tiles_cannot_receive_vision_in_fog() {
        let mut state = fixture();
        let mut teleporter = plain();
        teleporter.terrain = TerrainId::Teleporter;
        state.board =
            Board::new(2, 1, vec![plain(), teleporter]).expect("a two-tile row is a rectangle");
        state.units[0].kind = UnitKindId::Recon;
        state.units[0].location = Location::Board {
            position: Pos::new(0, 0),
        };
        let team = TeamId::from("t1");

        assert!(!AwbwVisibility.view(&state, &team).position(Pos::new(1, 0)));

        state.settings.fog = false;
        assert!(AwbwVisibility.view(&state, &team).position(Pos::new(1, 0)));
    }

    fn fixture() -> State {
        State {
            ruleset: RulesetRef {
                id: "awbw".into(),
                revision: "2026-07-10".into(),
            },
            settings: Settings {
                fog: true,
                income_per_property: 1000,
                starting_funds: 0,
                powers: Toggle::Enabled,
                tags: false,
                weather: WeatherSetting::Clear,
                lab_units: vec![],
                unit_bans: vec![],
                commander_bans: CommanderBans {
                    lead: vec![],
                    backup: vec![],
                },
                capture_limit: None,
                day_limit: None,
                unit_limit: None,
            },
            board: Board::new(
                1,
                1,
                vec![Tile {
                    terrain: TerrainId::Plain,
                    owner: TileOwner::NotOwnable,
                    capture_points: None,
                    silo: None,
                    rare: None,
                }],
            )
            .expect("a single tile is a rectangle"),
            teams: vec![
                Team {
                    id: "t1".into(),
                    status: TeamStatus::Active,
                },
                Team {
                    id: "t2".into(),
                    status: TeamStatus::Active,
                },
            ],
            players: vec![
                player(PlayerId::from("p1"), TeamId::from("t1")),
                player(PlayerId::from("p2"), TeamId::from("t2")),
            ],
            turn: Turn {
                day: 1,
                active_player: "p1".into(),
                phase: Phase::UnitAction,
                order: vec!["p1".into(), "p2".into()],
                position: 0,
            },
            weather: Weather {
                kind: WeatherKind::Clear,
                remaining_turns: 0,
            },
            units: UnitStore::new(vec![
                unit(0, PlayerId::from("p1")),
                unit(1, PlayerId::from("p2")),
            ])
            .expect("distinct ids"),
            next_unit_id: None,
            match_state: Match::Active {
                draw_offers: vec![],
            },
        }
    }
    fn player(id: PlayerId, team: TeamId) -> Player {
        Player {
            id,
            team,
            funds: 0,
            status: PlayerStatus::Active,
            commanders: vec![Commander {
                id: CommanderId::Andy,
                active: true,
                power_charge: 0,
                power_uses: 0,
            }],
            power_state: PowerState::None,
        }
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
