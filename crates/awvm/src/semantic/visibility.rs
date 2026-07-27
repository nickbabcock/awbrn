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

use std::cell::OnceCell;

use crate::commander;
use crate::ruleset::{self, Domain, TerrainTrait};

use super::{Concealment, Location, PlayerId, Pos, State, TeamId, Unit, WeatherKind};

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
