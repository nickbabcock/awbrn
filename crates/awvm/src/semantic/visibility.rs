//! The vision operators of `spec/semantics/fog.md`.
//!
//! Both readers of the state ask these: the recipient projection in
//! [`super::observe`], and the reducer, which has to know whether a blocker or
//! a target is disclosed to the acting team before it can pick a violation.
//! That shared audience is why they sit beside the state rather than inside
//! either caller.
//!
//! There are no unit tests here on purpose. Every operator below is a claim
//! about a board a fixture can state, and `spec/fixtures/fog/` states them —
//! vision sources and terrain, the rain radius and its floor, concealing
//! terrain, detection, the teleporter exclusion and its fog-off complement.
//! A stub viewpoint in Rust would assert against a second implementation of
//! the same table.

use std::cell::{Cell, OnceCell};

use crate::commander;
use crate::ruleset::{self, Domain, TerrainTrait};

use super::{
    Concealment, Grid, Location, PlayerIdx, Pos, State, TeamId, Unit, UnitId, WeatherKind,
};

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
    /// The viewing team's seats. Short enough that a scan beats hashing.
    teammates: Vec<PlayerIdx>,
    /// Resolved on first use rather than up front. A reducer builds a view to
    /// ask whether one tile is occupied by something it can see, and in a match
    /// without fog or hidden units that answer never consults a sighting.
    sightings: OnceCell<Vec<UnitSight>>,
    /// What this team perceives standing on each tile. Resolved on first use
    /// for the same reason as `sightings`.
    occupancy: OnceCell<Grid<Occupant>>,
}

/// Which unit is standing on each tile.
///
/// "Is my destination blocked", "may my route pass here", "who would I join",
/// and "does a hidden unit stop me short" are four readings of one table, and
/// each used to scan every unit again for every candidate destination.
#[derive(Clone, Debug)]
struct Occupant {
    /// Index into `State::units`, absent where the tile is empty.
    unit: Option<u32>,
    /// What has been worked out about this occupant so far.
    ///
    /// Whether the viewing team sees a unit is the costly half of every
    /// question here, and under fog it walks the sightings. Resolving it for
    /// the whole board up front loses to the scan this table replaced on the
    /// single-command path; resolving it per question loses to it during
    /// enumeration, which asks about the same tile for every candidate
    /// destination. Remembering each answer the first time is what wins both.
    resolved: Cell<u8>,
}

/// Bits of [`Occupant::resolved`].
const RESOLVED: u8 = 1;
const DISCLOSED: u8 = 2;
const HOSTILE: u8 = 4;

/// A unit on the board, with its vision already resolved.
///
/// The board projection resolves one of these for every friendly unit. An
/// interface that draws a single unit's vision asks for the same value, which
/// is why [`unit_sight`] is public and the calculation has one home.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitSight {
    /// Where the unit stands. Sight is measured from here.
    pub position: Pos,
    /// Effective vision after the commander, terrain bonus and weather, floored
    /// at one tile.
    pub sight: u64,
    /// Whether this unit sees into concealing terrain, which lifts the target
    /// terrain's own vision limit.
    pub reveals_concealing: bool,
}

/// How well a viewer sees one tile.
#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum VisionLevel {
    /// Nothing on the tile is seen.
    None,
    /// The tile is in range, but it conceals: only an air unit is spotted
    /// there.
    AirOnly,
    /// Anything standing on the tile is seen.
    Full,
}

/// The vision one board unit has, or `None` where it has none to give.
///
/// A unit inside a transport and a unit off the board both see nothing. The
/// weather applies here rather than at the tile, because rain shortens the
/// sight of the viewer and does not hide the tile from everyone equally.
pub fn unit_sight(state: &State, unit: &Unit) -> Option<UnitSight> {
    let Location::Board { position } = unit.location else {
        return None;
    };
    unit_sight_at(state, unit, position)
}

/// The vision `unit` would have standing at `position`.
///
/// Sight is not a property of the unit alone: rain shortens it, and a mountain
/// lengthens it for a unit that climbs to see. An interface that shows what a
/// move uncovers has to ask about the destination rather than the tile the
/// unit still stands on, so the tile is a parameter here and [`unit_sight`]
/// passes the one the unit is on.
///
/// A unit that is not on the board sees nothing, wherever it is asked about.
pub fn unit_sight_at(state: &State, unit: &Unit, position: Pos) -> Option<UnitSight> {
    if !matches!(unit.location, Location::Board { .. }) || !state.board.contains(position) {
        return None;
    }
    let profile = ruleset::profile(unit.kind);
    let rain = -i64::from(matches!(state.weather.kind, WeatherKind::Rain));
    let bonus = if profile.elevated_vision {
        ruleset::terrain(state.board.tile(position).terrain)
            .vision_bonus
            .unwrap_or(0)
    } else {
        0
    };
    let vision = commander::effective_vision(state, unit, profile.vision, profile.domain);
    Some(UnitSight {
        position,
        sight: (vision + bonus + rain).max(1) as u64,
        reveals_concealing: commander::reveals_concealing_terrain(state, unit),
    })
}

/// What one viewer alone reveals of the tile at `position`.
///
/// This answers for that viewer and nothing else. Whether the team sees the
/// tile for some other reason — it holds the property, the terrain is always
/// visible, or a second unit stands closer — is [`AwbwView`]'s question, and
/// asking this one instead gives a smaller answer rather than a wrong one.
pub fn sight_of(state: &State, sight: &UnitSight, position: Pos) -> VisionLevel {
    if !state.board.contains(position) {
        return VisionLevel::None;
    }
    let terrain = ruleset::terrain(state.board.tile(position).terrain);
    if terrain.has(TerrainTrait::Teleporter) {
        return VisionLevel::None;
    }
    sighting_level(sight, terrain, position)
}

/// [`sight_of`] against a terrain the caller has already resolved.
///
/// The per-tile loop in [`AwbwView::vision_level`] resolves the terrain once
/// and then asks about every sighting, so it takes this entry rather than
/// paying for the same lookup per unit.
fn sighting_level(
    sight: &UnitSight,
    terrain: &ruleset::TerrainProfile,
    position: Pos,
) -> VisionLevel {
    let distance = sight.position.distance(position);
    if distance > sight.sight {
        return VisionLevel::None;
    }
    if sight.reveals_concealing
        || terrain
            .vision_limit
            .is_none_or(|limit| distance <= limit as u64)
    {
        VisionLevel::Full
    } else {
        VisionLevel::AirOnly
    }
}

impl<'a> AwbwView<'a> {
    pub(crate) fn new(state: &'a State, team: &TeamId) -> Self {
        let teammates: Vec<PlayerIdx> = state.players.seats_on_team(team).collect();
        Self {
            state,
            fog: state.settings.fog,
            teammates,
            sightings: OnceCell::new(),
            occupancy: OnceCell::new(),
        }
    }

    /// Index where every unit stands, so later occupancy questions are lookups.
    ///
    /// Building the table costs a pass over the board; answering one question
    /// from it costs nothing. A caller that will ask about many tiles — a
    /// movement search, or a turn's whole action space — calls this once and
    /// wins. A caller asking about a single destination must not: the table
    /// costs more to build than the scan it would replace, which is why the
    /// questions below fall back to that scan until this is called.
    pub(crate) fn index_occupancy(&self) {
        self.occupancy();
    }

    fn occupancy(&self) -> &Grid<Occupant> {
        self.occupancy.get_or_init(|| {
            let state = self.state;
            let mut tiles = Grid::filled(
                state.board.dimensions(),
                Occupant {
                    unit: None,
                    resolved: Cell::new(0),
                },
            );
            for (index, unit) in state.units.iter().enumerate() {
                let Location::Board { position } = unit.location else {
                    continue;
                };
                let Some(tile) = tiles.get_mut(position) else {
                    continue;
                };
                tile.unit = Some(u32::try_from(index).expect("a unit index fits u32"));
            }
            tiles
        })
    }

    /// The occupant of `position` and what this team makes of it.
    fn resolved_occupant(&self, position: Pos) -> Option<(&'a Unit, u8)> {
        let Some(occupancy) = self.occupancy.get() else {
            if !self.state.board.contains(position) {
                return None;
            }
            let unit = self.scan_occupant(position)?;
            return Some((unit, self.resolve(unit, position)));
        };
        let tile = occupancy.get(position)?;
        let unit = &self.state.units[tile.unit? as usize];
        let cached = tile.resolved.get();
        if cached & RESOLVED != 0 {
            return Some((unit, cached));
        }
        let flags = self.resolve(unit, position);
        tile.resolved.set(flags);
        Some((unit, flags))
    }

    /// Whoever is standing at `position`, found by walking the units.
    ///
    /// This is what [`Self::index_occupancy`] replaces. One question is
    /// cheaper answered this way than by building the table first. Should two
    /// units claim one tile, the last of them answers, because that is the one
    /// the table keeps.
    fn scan_occupant(&self, position: Pos) -> Option<&'a Unit> {
        self.state
            .units
            .iter()
            .rfind(|unit| unit.location == Location::Board { position })
    }

    fn resolve(&self, unit: &Unit, position: Pos) -> u8 {
        let mut flags = RESOLVED;
        if self.unit_at(unit, position) {
            flags |= DISCLOSED;
        }
        if !self.teammates.contains(&unit.owner) {
            flags |= HOSTILE;
        }
        flags
    }

    /// Whoever is standing at `position`, whether or not this team sees them.
    ///
    /// Occupancy is a fact about the board, and a caller that only needs to
    /// know which unit a join or load would name asks this. A caller deciding
    /// whether the tile *blocks* must ask [`Self::blocking_occupant`] instead,
    /// or it leaks a hidden unit.
    pub(crate) fn occupant(&self, position: Pos) -> Option<UnitId> {
        match self.state.board.dimensions().cell(position) {
            Some(cell) => self.occupant_at(cell),
            None => None,
        }
    }

    /// [`Self::occupant`] for a caller that already holds the cell.
    ///
    /// A walk over a weapon's range box asks the board and the occupancy index
    /// about the same tile, so it resolves the coordinate once and reads both
    /// with the answer.
    pub(crate) fn occupant_at(&self, cell: super::Cell) -> Option<UnitId> {
        let Some(occupancy) = self.occupancy.get() else {
            return self.scan_occupant(cell.position()).map(|unit| unit.id);
        };
        let unit = occupancy.at(cell).unit?;
        Some(self.state.units[unit as usize].id)
    }

    /// Whether this team sees a unit standing at `position`, whoever it is.
    ///
    /// A movement search asks this of every tile, and the answer changes only
    /// where the asking unit itself stands, so a caller building one table for
    /// a whole turn asks this rather than [`Self::blocking_occupant`] and
    /// handles the mover's own tile itself.
    pub(crate) fn occupant_disclosed(&self, position: Pos) -> bool {
        self.resolved_occupant(position)
            .is_some_and(|(_, flags)| flags & DISCLOSED != 0)
    }

    /// Whether a disclosed enemy stands at `position`, whoever it is.
    pub(crate) fn occupant_obstructs(&self, position: Pos) -> bool {
        self.resolved_occupant(position)
            .is_some_and(|(_, flags)| flags & (DISCLOSED | HOSTILE) == DISCLOSED | HOSTILE)
    }

    /// The unit at `position` that stops `mover` from ending its move there.
    pub(crate) fn blocking_occupant(&self, position: Pos, mover: UnitId) -> Option<UnitId> {
        self.resolved_occupant(position)
            .filter(|(unit, flags)| unit.id != mover && flags & DISCLOSED != 0)
            .map(|(unit, _)| unit.id)
    }

    /// The unit at `position` that would trap `mover` there, unseen.
    pub(crate) fn hidden_occupant(&self, position: Pos, mover: UnitId) -> Option<UnitId> {
        self.resolved_occupant(position)
            .filter(|(unit, flags)| unit.id != mover && flags & DISCLOSED == 0)
            .map(|(unit, _)| unit.id)
    }

    /// Whether this view hides any unit that can block movement.
    pub(crate) fn has_hidden_board_unit(&self) -> bool {
        self.state
            .units
            .iter()
            .any(|unit| matches!(unit.location, Location::Board { .. }) && !self.unit(unit))
    }

    /// Every friendly unit that can see, with its effective vision already
    /// worked out.
    ///
    /// Each unit's sight depends on its commander, the terrain under it and the
    /// weather — none of which vary by the tile being asked about. Resolving
    /// them here rather than inside the per-tile loop is what takes the board
    /// projection off an O(tiles x units) path through the commander tables.
    fn sightings(&self) -> &[UnitSight] {
        self.sightings.get_or_init(|| {
            let state = self.state;
            state
                .units
                .iter()
                .filter(|unit| self.teammates.contains(&unit.owner))
                .filter_map(|unit| unit_sight(state, unit))
                .collect()
        })
    }

    /// Whether the viewing team holds a seat.
    fn holds_seat(&self, seat: Option<PlayerIdx>) -> bool {
        seat.is_some_and(|seat| self.teammates.contains(&seat))
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
        if self.holds_seat(tile.owner.player()) {
            return VisionLevel::Full;
        }
        if target_terrain.has(TerrainTrait::AlwaysVisible) {
            return VisionLevel::Full;
        }
        let mut level = VisionLevel::None;
        for sighting in self.sightings() {
            level = level.max(sighting_level(sighting, target_terrain, position));
        }
        level
    }
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
            Location::Cargo { .. } => self.holds_seat(Some(unit.owner)),
        }
    }

    fn unit_at(&self, unit: &Unit, position: Pos) -> bool {
        if self.holds_seat(Some(unit.owner)) {
            return true;
        }
        // Standard play discloses every unit except one that explicitly hid.
        // Test this before terrain ownership, which is not relevant to an
        // ordinary unit when fog is off.
        if !self.fog && unit.concealment != Concealment::Hidden {
            return true;
        }
        if self.holds_seat(
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
        debug_assert!(self.fog);
        match self.vision_level(position) {
            VisionLevel::Full => true,
            VisionLevel::AirOnly => ruleset::profile(unit.kind).domain == Domain::Air,
            VisionLevel::None => false,
        }
    }
}
