//! Pure, presentation-independent AWVM state and recipient observation values.
//!
//! Identifier domains are distinct even where their wire representations are
//! strings. Adapters from replay/ECS identifiers belong at the boundary and
//! must not make this model depend on Bevy entities or AWBW replay IDs.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::commander;
use crate::event::Event;
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: TerrainId,
    #[serde(default, skip_serializing_if = "TileOwner::is_not_ownable")]
    pub owner: TileOwner,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_points: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silo: Option<Silo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructible_hp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleporter: Option<TeleporterId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
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

/// Ruleset-owned visibility. Implementations may build this from `world::fog`;
/// the state projection remains independent of Bevy and cached viewpoints.
pub trait Visibility {
    fn visible_position(&self, state: &State, team: &TeamId, position: Pos) -> bool;
    fn visible_unit(&self, state: &State, team: &TeamId, unit: &Unit) -> bool;
}

/// Visibility operators for the `awbw/2026-07-10` profile.
///
/// Carries no state: every value it needs is in [`crate::ruleset`].
#[derive(Clone, Copy, Debug, Default)]
pub struct AwbwVisibility;

impl AwbwVisibility {
    fn team_players<'a>(&self, state: &'a State, team: &TeamId) -> HashSet<&'a PlayerId> {
        state
            .players
            .iter()
            .filter(|player| player.team == team)
            .map(|player| &player.id)
            .collect()
    }

    fn vision_level(&self, state: &State, team: &TeamId, position: Pos) -> VisionLevel {
        if position.x >= state.board.width() || position.y >= state.board.height() {
            return VisionLevel::None;
        }
        if !state.settings.fog {
            return VisionLevel::Full;
        }
        let team_players = self.team_players(state, team);
        let tile = &state.board.tile(position);
        let target_terrain = ruleset::terrain(tile.terrain);
        if target_terrain.has(TerrainTrait::Teleporter) {
            return VisionLevel::None;
        }
        if tile
            .owner
            .player()
            .is_some_and(|owner| team_players.contains(owner))
        {
            return VisionLevel::Full;
        }

        if target_terrain.has(TerrainTrait::AlwaysVisible) {
            return VisionLevel::Full;
        }
        let mut level = VisionLevel::None;
        for unit in &state.units {
            if !team_players.contains(&unit.owner) {
                continue;
            }
            let Location::Board { position: source } = unit.location else {
                continue;
            };
            let profile = ruleset::profile(unit.kind);
            let source_terrain = &state.board.tile(source).terrain;
            let bonus = if profile.elevated_vision {
                ruleset::terrain(*source_terrain).vision_bonus.unwrap_or(0)
            } else {
                0
            };
            let rain = -i64::from(matches!(state.weather.kind, WeatherKind::Rain));
            let vision = commander::effective_vision(state, unit, profile.vision, profile.domain);
            let sight = (vision + bonus + rain).max(1) as u64;
            let distance = source.distance(position);
            if distance > sight {
                continue;
            }
            let contribution = if commander::reveals_concealing_terrain(state, unit)
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

impl Visibility for AwbwVisibility {
    fn visible_position(&self, state: &State, team: &TeamId, position: Pos) -> bool {
        self.vision_level(state, team, position) == VisionLevel::Full
    }

    fn visible_unit(&self, state: &State, team: &TeamId, unit: &Unit) -> bool {
        let team_players = self.team_players(state, team);
        if team_players.contains(&unit.owner) {
            return true;
        }
        let Location::Board { position } = unit.location else {
            return false;
        };
        if state
            .board
            .tile(position)
            .owner
            .player()
            .is_some_and(|owner| team_players.contains(owner))
        {
            return true;
        }
        if unit.concealment == Concealment::Hidden {
            return state.units.iter().any(|source| {
                team_players.contains(&source.owner)
                    && matches!(
                            source.location,
                            Location::Board { position: source_position }
                                if source_position.x.abs_diff(position.x)
                                    + source_position.y.abs_diff(position.y)
                                    == 1
                    )
            });
        }
        if !state.settings.fog {
            return true;
        }
        match self.vision_level(state, team, position) {
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
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ObservedTile {
    pub terrain: TerrainId,
    pub visibility: TileVisibility,
    #[serde(skip_serializing_if = "TileOwner::is_not_ownable")]
    pub owner: TileOwner,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_points: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silo: Option<Silo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructible_hp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teleporter: Option<TeleporterId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trait_state: Option<BTreeMap<TraitId, serde_json::Value>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

pub fn observe(
    rules: &impl Visibility,
    state: &State,
    recipient: &PlayerId,
) -> Result<Observation, ObserveError> {
    let recipient_player = state
        .players
        .iter()
        .find(|p| p.id == recipient)
        .ok_or_else(|| ObserveError::UnknownRecipient(recipient.clone()))?;
    let owners: HashMap<&PlayerId, &TeamId> =
        state.players.iter().map(|p| (&p.id, &p.team)).collect();
    let team = &recipient_player.team;
    let tiles = state
        .board
        .rows()
        .map(|row| {
            row.map(|(position, t)| {
                let visible = !state.settings.fog || rules.visible_position(state, team, position);
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
                    destructible_hp: visible.then_some(t.destructible_hp).flatten(),
                    teleporter: t.teleporter.clone(),
                    trait_state: visible.then_some(t.trait_state.clone()).flatten(),
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
        if owner_team == team
            || (matches!(u.location, Location::Board { .. }) && rules.visible_unit(state, team, u))
        {
            units.push(observed_unit_snapshot(u, owner_team == team));
        }
    }
    units.sort_by(|a, b| a.reference.cmp(&b.reference));
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

/// Project authoritative transition events for one recipient.
pub fn observe_events(
    rules: &impl Visibility,
    state: &State,
    next_state: &State,
    events: &[Event],
    recipient: &PlayerId,
) -> Result<Vec<serde_json::Value>, ObserveError> {
    let recipient_player = state
        .players
        .iter()
        .find(|player| player.id == recipient)
        .ok_or_else(|| ObserveError::UnknownRecipient(recipient.clone()))?;
    let team = &recipient_player.team;
    let team_players: HashSet<&PlayerId> = state
        .players
        .iter()
        .filter(|player| player.team == team)
        .map(|player| &player.id)
        .collect();
    observe(rules, state, recipient)?;
    let post = observe(rules, next_state, recipient)?;
    let visible_pre: HashSet<UnitId> = state
        .units
        .iter()
        .filter(|unit| team_players.contains(&unit.owner) || rules.visible_unit(state, team, unit))
        .map(|unit| unit.id)
        .collect();
    let visible_post: HashSet<UnitId> = next_state
        .units
        .iter()
        .filter(|unit| {
            team_players.contains(&unit.owner) || rules.visible_unit(next_state, team, unit)
        })
        .map(|unit| unit.id)
        .collect();
    let mut appeared = HashSet::new();
    let mut disappeared = HashSet::new();
    let mut output = Vec::new();

    for event in events {
        let reason = event.reason();
        match event {
            Event::UnitActionChanged { unit, .. }
            | Event::UnitDamaged { unit, .. }
            | Event::UnitRepaired { unit, .. }
            | Event::UnitResourced { unit, .. }
            | Event::ConcealmentChanged { unit, .. }
            | Event::AutomaticRepair { unit, .. } => {
                project_unit_fact(
                    *unit,
                    reason,
                    state,
                    next_state,
                    &visible_pre,
                    &visible_post,
                    &team_players,
                    &mut appeared,
                    &mut disappeared,
                    &mut output,
                );
            }
            Event::AutomaticSupply { units, .. } => {
                for unit in units {
                    project_unit_fact(
                        *unit,
                        reason,
                        state,
                        next_state,
                        &visible_pre,
                        &visible_post,
                        &team_players,
                        &mut appeared,
                        &mut disappeared,
                        &mut output,
                    );
                }
            }
            Event::UnitMoved {
                unit: id,
                from,
                to,
                path,
                ..
            } => {
                let id = *id;
                let unit = state
                    .units
                    .iter()
                    .find(|unit| unit.id == id)
                    .ok_or(ObserveError::UnknownUnit(id))?;
                if team_players.contains(&unit.owner) {
                    output.push(serde_json::json!({
                        "type": "unit-moved",
                        "unit": ObservedUnitRef::Friendly { unit: id },
                        "from": from,
                        "to": to,
                        "path": path
                    }));
                } else if visible_pre.contains(&id) && !visible_post.contains(&id) {
                    push_disappeared(id, *from, &mut disappeared, &mut output);
                } else if !visible_pre.contains(&id) && visible_post.contains(&id) {
                    if let Some(snapshot) = next_state.units.iter().find(|unit| unit.id == id) {
                        push_appeared(snapshot, *to, false, &mut appeared, &mut output);
                    }
                } else if visible_pre.contains(&id) && visible_post.contains(&id) {
                    let post_unit = next_state
                        .units
                        .iter()
                        .find(|unit| unit.id == id)
                        .ok_or(ObserveError::UnknownUnit(id))?;
                    let mut observed_path = Vec::with_capacity(path.len());
                    for position in path.iter().copied() {
                        let mut probe = post_unit.clone();
                        probe.location = Location::Board { position };
                        if rules.visible_unit(state, team, &probe)
                            || rules.visible_unit(next_state, team, &probe)
                        {
                            observed_path.push(position);
                        }
                    }
                    output.push(serde_json::json!({
                        "type": "unit-moved", "unit": enemy_unit_ref(*to), "from": from, "to": to,
                        "path": observed_path
                    }));
                }
            }
            Event::MovementTrapped { unit: id, .. } => {
                let id = *id;
                let actor = state.units.iter().find(|unit| unit.id == id);
                if actor.is_some_and(|unit| team_players.contains(&unit.owner)) {
                    output.push(serde_json::json!({
                        "type":"movement-stopped",
                        "unit":ObservedUnitRef::Friendly { unit:id }
                    }));
                }
            }
            Event::UnitCreated {
                unit: id, position, ..
            } => {
                let id = *id;
                if visible_post.contains(&id)
                    && let Some(unit) = next_state.units.iter().find(|unit| unit.id == id)
                {
                    push_appeared(
                        unit,
                        *position,
                        team_players.contains(&unit.owner),
                        &mut appeared,
                        &mut output,
                    );
                }
            }
            Event::UnitRemoved { unit, .. } => {
                project_removal(
                    *unit,
                    reason,
                    rules,
                    state,
                    next_state,
                    team,
                    &team_players,
                    &visible_pre,
                    &mut disappeared,
                    &mut output,
                )?;
            }
            Event::UnitsJoined { source, target } => {
                project_removal(
                    *source,
                    reason,
                    rules,
                    state,
                    next_state,
                    team,
                    &team_players,
                    &visible_pre,
                    &mut disappeared,
                    &mut output,
                )?;
                project_unit_fact(
                    *target,
                    reason,
                    state,
                    next_state,
                    &visible_pre,
                    &visible_post,
                    &team_players,
                    &mut appeared,
                    &mut disappeared,
                    &mut output,
                );
            }
            Event::UnitLoaded { unit: id, .. } => {
                let id = *id;
                let own = state
                    .units
                    .iter()
                    .find(|unit| unit.id == id)
                    .is_some_and(|unit| team_players.contains(&unit.owner));
                if own {
                    project_unit_fact(
                        id,
                        reason,
                        state,
                        next_state,
                        &visible_pre,
                        &visible_post,
                        &team_players,
                        &mut appeared,
                        &mut disappeared,
                        &mut output,
                    );
                } else if visible_pre.contains(&id)
                    && let Some(position) = state
                        .units
                        .iter()
                        .find_map(|unit| (unit.id == id).then(|| board_position(unit)).flatten())
                {
                    push_disappeared(id, position, &mut disappeared, &mut output);
                }
            }
            Event::UnitUnloaded {
                unit: id, position, ..
            } => {
                let id = *id;
                let own = next_state
                    .units
                    .iter()
                    .find(|unit| unit.id == id)
                    .is_some_and(|unit| team_players.contains(&unit.owner));
                if own {
                    project_unit_fact(
                        id,
                        reason,
                        state,
                        next_state,
                        &visible_pre,
                        &visible_post,
                        &team_players,
                        &mut appeared,
                        &mut disappeared,
                        &mut output,
                    );
                } else if visible_post.contains(&id)
                    && let Some(unit) = next_state.units.iter().find(|unit| unit.id == id)
                {
                    push_appeared(unit, *position, false, &mut appeared, &mut output);
                }
            }
            Event::TileOwnerChanged { position, .. }
            | Event::TileTerrainChanged { position, .. }
            | Event::CaptureChanged { position, .. }
            | Event::SiloChanged { position, .. }
            | Event::DestructibleDamaged { position, .. } => {
                let position = *position;
                if rules.visible_position(state, team, position)
                    || rules.visible_position(next_state, team, position)
                {
                    let tile = post.board.tile(position);
                    output.push(serde_json::json!({
                        "type":"tile-changed", "position":position, "tile":tile, "reason":reason
                    }));
                }
            }
            // A player only learns their own funds changed; a power charge is
            // public, so it is projected to everyone.
            Event::FundsChanged { player, .. } if !team_players.contains(player) => {}
            Event::FundsChanged { player, .. } | Event::PowerChargeChanged { player, .. } => {
                if let Some(snapshot) = post.players.iter().find(|candidate| match candidate {
                    ObservedPlayer::Private { id, .. } | ObservedPlayer::Public { id, .. } => {
                        id == player
                    }
                }) {
                    output.push(serde_json::json!({
                        "type":"player-changed", "player":player, "state":snapshot,
                        "reason":reason
                    }));
                }
            }
            Event::DrawOfferChanged { player, .. } => {
                if team_players.contains(player) {
                    output.push(serde_json::json!({"type":"public-event","kind":event.kind()}));
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
            | Event::MatchCompleted { .. } => {
                output.push(serde_json::json!({"type":"public-event","kind":event.kind()}));
            }
            Event::AreaStrikeResolved { .. } => {
                output.push(serde_json::to_value(event).expect("an event serializes"));
            }
            // Deliberately unprojected. `spec/model/observation.md:337` withholds
            // both: `attack-resolved` would disclose the weapon and target of an
            // attack a recipient may not see, and `random-outcome` would leak the
            // tape a recipient is not entitled to read. The damage and state
            // changes they cause reach recipients through their own events.
            Event::AttackResolved { .. } | Event::RandomOutcome { .. } => {}
        }
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn project_unit_fact(
    id: UnitId,
    reason: &str,
    state: &State,
    next_state: &State,
    visible_pre: &HashSet<UnitId>,
    visible_post: &HashSet<UnitId>,
    team_players: &HashSet<&PlayerId>,
    appeared: &mut HashSet<UnitId>,
    disappeared: &mut HashSet<UnitId>,
    output: &mut Vec<serde_json::Value>,
) {
    let Some(unit) = next_state.units.iter().find(|unit| unit.id == id) else {
        return;
    };
    match (visible_pre.contains(&id), visible_post.contains(&id)) {
        (true, true) => {
            let friendly = team_players.contains(&unit.owner);
            let snapshot = observed_unit_snapshot(unit, friendly);
            output.push(serde_json::json!({
                "type":"unit-changed", "unit":snapshot.reference, "state":snapshot, "reason":reason
            }));
        }
        (false, true) => {
            if let Some(position) = board_position(unit) {
                push_appeared(
                    unit,
                    position,
                    team_players.contains(&unit.owner),
                    appeared,
                    output,
                );
            }
        }
        (true, false) => {
            if let Some(position) = state
                .units
                .iter()
                .find(|unit| unit.id == id)
                .and_then(board_position)
            {
                push_disappeared(id, position, disappeared, output);
            }
        }
        (false, false) => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn project_removal(
    id: UnitId,
    reason: &str,
    rules: &impl Visibility,
    state: &State,
    next_state: &State,
    team: &TeamId,
    team_players: &HashSet<&PlayerId>,
    visible_pre: &HashSet<UnitId>,
    disappeared: &mut HashSet<UnitId>,
    output: &mut Vec<serde_json::Value>,
) -> Result<(), ObserveError> {
    let unit = state
        .units
        .iter()
        .find(|unit| unit.id == id)
        .ok_or(ObserveError::UnknownUnit(id))?;
    if team_players.contains(&unit.owner) {
        output.push(serde_json::json!({
            "type":"unit-removed",
            "unit":ObservedUnitRef::Friendly { unit:id },
            "reason":reason
        }));
    } else if visible_pre.contains(&id) {
        // A removed enemy that was visible must have been on the board:
        // visibility is only ever computed for board positions.
        let position = board_position(unit).ok_or(ObserveError::UnknownUnit(id))?;
        if rules.visible_position(next_state, team, position) {
            output.push(serde_json::json!({
                "type":"unit-removed","unit":enemy_unit_ref(position),"reason":reason
            }));
        } else {
            push_disappeared(id, position, disappeared, output);
        }
    }
    Ok(())
}

fn push_appeared(
    unit: &Unit,
    position: Pos,
    friendly: bool,
    appeared: &mut HashSet<UnitId>,
    output: &mut Vec<serde_json::Value>,
) {
    if appeared.insert(unit.id) {
        output.push(serde_json::json!({
            "type":"unit-appeared",
            "unit":observed_unit_snapshot(unit, friendly),
            "position":position
        }));
    }
}

fn push_disappeared(
    id: UnitId,
    position: Pos,
    disappeared: &mut HashSet<UnitId>,
    output: &mut Vec<serde_json::Value>,
) {
    if disappeared.insert(id) {
        output.push(serde_json::json!({
            "type":"unit-disappeared","unit":enemy_unit_ref(position),"position":position
        }));
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
        Tile {
            terrain: TerrainId::Plain,
            owner: TileOwner::NotOwnable,
            capture_points: None,
            silo: None,
            destructible_hp: None,
            teleporter: None,
            trait_state: None,
        }
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
    struct NoneVisible;
    impl Visibility for NoneVisible {
        fn visible_position(&self, _: &State, _: &TeamId, _: Pos) -> bool {
            false
        }
        fn visible_unit(&self, _: &State, _: &TeamId, _: &Unit) -> bool {
            false
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
            fn visible_position(&self, _: &State, _: &TeamId, _: Pos) -> bool {
                true
            }
            fn visible_unit(&self, _: &State, _: &TeamId, _: &Unit) -> bool {
                true
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
                    destructible_hp: None,
                    teleporter: None,
                    trait_state: None,
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
            vec![serde_json::json!({
                "type": "unit-moved",
                "unit": {"type": "enemy", "position": Pos::new(3, 0)},
                "from": Pos::new(5, 0),
                "to": Pos::new(3, 0),
                "path": [Pos::new(5, 0), Pos::new(3, 0)]
            })]
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
        let visibility = AwbwVisibility;

        assert!(!visibility.visible_position(&state, &TeamId::from("t1"), Pos::new(1, 0)));

        state.settings.fog = false;
        assert!(visibility.visible_position(&state, &TeamId::from("t1"), Pos::new(1, 0)));
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
                    destructible_hp: None,
                    teleporter: None,
                    trait_state: None,
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
