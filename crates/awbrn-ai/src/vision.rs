//! What a play does to what this player can see, and to what the enemy can.
//!
//! Fog is the one mode where a tile is worth standing on for a reason the
//! other maps cannot state. The capture field says where the properties are,
//! the threat map says what the enemy it *sees* can take off a tile, and
//! neither of them knows that half the board is dark. An agent with those two
//! maps and nothing else plays a fog game as if it were a standard one: it
//! walks at the properties, meets what it walks into, and never once buys a
//! recon or puts a soldier on a mountain.
//!
//! Two readings, and one grid behind both.
//!
//! - **Disclosure.** [`VisionMap::reveal`] counts the dark tiles a unit would
//!   light standing on a tile. It reads the same operators the ruleset reads
//!   (`awvm::semantic::visibility`), so a soldier gains the mountain's vision
//!   bonus, weather takes its tile off every radius, and concealing terrain
//!   stays dark beyond its own limit unless this unit sees into it. That is
//!   what makes a mountain worth more to a soldier than to a tank, and a
//!   recon worth more than either.
//! - **Concealment.** [`conceals`] says whether a tile hides what stands on
//!   it. Woods and reef are dark to an enemy that is not beside them, so a
//!   unit that ends its move in one is a unit the enemy cannot price.
//!
//! Both are worth nothing with fog off, and the agent builds neither there.
//! The grid is what the *asking* team can see now, so a tile that is already
//! lit is worth nothing to light again: a unit that stands still discloses
//! nothing, and a unit that walks into the dark discloses what it walks into.
//!
//! The reading is deliberately of this play alone. Two units that walk into
//! the same pocket of fog are each scored for the whole of it, because the
//! grid is built once for each play and not updated as the turn is spent. A
//! turn's second scout therefore repeats some of the first one's work. That
//! is the same approximation every other map in this crate makes, and the
//! honest fix is a grid rebuilt for each play, which is what this is.

use awvm::commander;
use awvm::ruleset::{self, TerrainTrait, WeatherKind};
use awvm::semantic::{AwbwVisibility, PlayerIdx, Pos, State, Unit, Viewpoint, Visibility};

/// What this player can see, one entry for each tile.
#[derive(Debug, Default)]
pub struct VisionMap {
    /// Row-major, and empty when the map is not built. A weighting that
    /// prices vision at nothing never pays for it, exactly as it never pays
    /// for the threat map.
    seen: Vec<bool>,
}

impl VisionMap {
    pub const fn new() -> Self {
        Self { seen: Vec::new() }
    }

    /// Whether a map has been built to read.
    pub fn is_built(&self) -> bool {
        !self.seen.is_empty()
    }

    /// Forget the map, so that a later reader knows there is none.
    pub fn forget(&mut self) {
        self.seen.clear();
    }

    /// What `seat`'s team can see of `state` right now.
    ///
    /// One viewpoint for the whole board rather than one for each question:
    /// the viewpoint resolves the team roster and every sighting unit's
    /// effective vision once, and answers each tile from that.
    pub fn build(&mut self, state: &State, seat: PlayerIdx) {
        self.seen.clear();
        let Some(player) = state.players.get(seat.get()) else {
            return;
        };
        let view = AwbwVisibility.view(state, &player.team);
        let dimensions = state.board.dimensions();
        self.seen.reserve(dimensions.len());
        self.seen
            .extend(dimensions.positions().map(|tile| view.position(tile)));
    }

    /// How many dark tiles `unit` would light standing at `position`.
    ///
    /// Zero without a map, which is what a weighting that prices vision at
    /// nothing reads.
    pub fn reveal(&self, state: &State, unit: &Unit, position: Pos) -> f64 {
        if self.seen.is_empty() {
            return 0.0;
        }
        let dimensions = state.board.dimensions();
        let profile = ruleset::profile(unit.kind);
        // The terrain bonus is the mountain, and only a unit that climbs one
        // to look gets it. It is read from the tile the play arrives on, which
        // is the whole of why a destination is worth scoring for vision.
        let bonus = if profile.elevated_vision {
            state.board.get(position).map_or(0, |tile| {
                ruleset::terrain(tile.terrain).vision_bonus.unwrap_or(0)
            })
        } else {
            0
        };
        let rain = -i64::from(matches!(state.weather.kind, WeatherKind::Rain));
        let sight = commander::effective_vision(state, unit, profile.vision, profile.domain);
        let sight = (sight + bonus + rain).max(1);
        let reveals_concealing = commander::reveals_concealing_terrain(state, unit);
        let radius = i16::try_from(sight).unwrap_or(i16::MAX);

        let mut lit = 0.0;
        for dy in -radius..=radius {
            // The radius is a Manhattan one, so a row of the diamond is as
            // wide as the range the row itself does not spend.
            let span = radius - dy.abs();
            for dx in -span..=span {
                let Some(target) = position.offset(dx, dy) else {
                    continue;
                };
                let Some(index) = dimensions.index(target) else {
                    continue;
                };
                if self.seen[index] {
                    continue;
                }
                let terrain = ruleset::terrain(state.board.tile(target).terrain);
                // A teleporter is dark to everyone, so walking up to one
                // discloses nothing.
                if terrain.has(TerrainTrait::Teleporter) {
                    continue;
                }
                let hidden = !reveals_concealing
                    && terrain
                        .vision_limit
                        .is_some_and(|limit| position.distance(target) > limit as u64);
                if hidden {
                    continue;
                }
                lit += 1.0;
            }
        }
        lit
    }
}

/// Whether a unit standing on this tile is hidden from an enemy that is not
/// beside it.
///
/// This is the terrain's own vision limit, which is what woods and reef carry.
/// A commander that sees into concealing terrain reads through it, and this
/// does not ask: the agent prices its own tile without knowing which commander
/// is looking, and the cautious reading is the one that is usually right.
pub fn conceals(state: &State, position: Pos) -> bool {
    state
        .board
        .get(position)
        .is_some_and(|tile| ruleset::terrain(tile.terrain).vision_limit.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::arena;
    use awvm::ruleset::{Terrain, UnitKind};
    use awvm::semantic::{Concealment, Location, State, UnitAction, UnitId};

    /// The arena board under fog, cleared of units, with `terrain` laid over
    /// the middle of it so that a vision reading is of one tile and not of the
    /// board the map happens to hold.
    fn empty_board(terrain: Terrain) -> (State, PlayerIdx, Pos) {
        let mut state = arena(true, 1);
        let seat = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        state.units.retain(|_| false);
        let position = Pos::new(5, 5);
        for tile in state.board.dimensions().positions() {
            if let Some(cell) = state.board.get_mut(tile) {
                cell.terrain = Terrain::Plain;
                cell.owner = awvm::semantic::TileOwner::Neutral;
                cell.capture_points = None;
            }
        }
        if let Some(cell) = state.board.get_mut(position) {
            cell.terrain = terrain;
        }
        (state, seat, position)
    }

    fn unit_of(kind: UnitKind, owner: PlayerIdx, position: Pos) -> Unit {
        let profile = ruleset::profile(kind);
        Unit {
            id: UnitId::new(1),
            kind,
            owner,
            hp: 100,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        }
    }

    /// What one unit of `kind` would light standing on `terrain`, from a board
    /// no other unit of ours can see any of.
    fn lit(kind: UnitKind, terrain: Terrain) -> f64 {
        let (state, seat, position) = empty_board(terrain);
        let mut map = VisionMap::new();
        map.build(&state, seat);
        map.reveal(&state, &unit_of(kind, seat, position), position)
    }

    #[test]
    fn a_soldier_sees_further_from_a_mountain() {
        let plain = lit(UnitKind::Infantry, Terrain::Plain);
        let mountain = lit(UnitKind::Infantry, Terrain::Mountain);
        assert!(
            mountain > plain,
            "a mountain lights {mountain} tiles and a plain {plain}"
        );
    }

    #[test]
    fn a_hull_that_climbs_nothing_reads_the_mountain_as_ground() {
        assert_eq!(
            lit(UnitKind::Tank, Terrain::Mountain),
            lit(UnitKind::Tank, Terrain::Plain),
            "only a soldier takes the mountain's vision bonus"
        );
    }

    #[test]
    fn a_recon_lights_more_than_the_soldier_it_costs_four_of() {
        let recon = lit(UnitKind::Recon, Terrain::Plain);
        let infantry = lit(UnitKind::Infantry, Terrain::Plain);
        assert!(
            recon > infantry,
            "a recon lights {recon} tiles and an infantry {infantry}"
        );
    }

    #[test]
    fn a_tile_this_player_already_sees_is_worth_nothing_to_light() {
        let (state, seat, position) = empty_board(Terrain::Plain);
        let mut state = state;
        state.units.push(unit_of(UnitKind::Recon, seat, position));
        let mut map = VisionMap::new();
        map.build(&state, seat);
        let unit = unit_of(UnitKind::Infantry, seat, position);
        assert_eq!(
            map.reveal(&state, &unit, position),
            0.0,
            "a second unit on the same tile discloses nothing the first did not"
        );
    }

    #[test]
    fn fog_off_leaves_nothing_dark_to_light() {
        let mut state = arena(false, 1);
        let seat = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        state.units.retain(|_| false);
        let mut map = VisionMap::new();
        map.build(&state, seat);
        let position = Pos::new(5, 5);
        let unit = unit_of(UnitKind::Recon, seat, position);
        assert_eq!(map.reveal(&state, &unit, position), 0.0);
    }

    #[test]
    fn the_woods_hide_what_stands_in_them_and_the_plain_does_not() {
        let (state, _, position) = empty_board(Terrain::Wood);
        assert!(conceals(&state, position));
        let (state, _, position) = empty_board(Terrain::Plain);
        assert!(!conceals(&state, position));
    }

    #[test]
    fn a_map_that_was_never_built_reads_nothing() {
        let (state, seat, position) = empty_board(Terrain::Plain);
        let map = VisionMap::new();
        assert!(!map.is_built());
        assert_eq!(
            map.reveal(&state, &unit_of(UnitKind::Recon, seat, position), position),
            0.0
        );
    }
}
