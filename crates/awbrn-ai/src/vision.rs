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
//! - **Concealment.** [`VisionMap::conceals`] says whether a tile hides what
//!   stands on it *from the enemy that is on the board*. Woods and reef are
//!   dark to an enemy that is not beside them, so a unit that ends its move in
//!   one is a unit the enemy cannot price — unless an enemy is already beside
//!   it, or the enemy commander is one that reads through cover.
//!
//! Both are worth nothing with fog off, and the agent builds neither there.
//! The grid is what the *asking* team can see now, so a tile that is already
//! lit is worth nothing to light again: a unit that stands still discloses
//! nothing, and a unit that walks into the dark discloses what it walks into.
//!
//! **Every reading is a table lookup, and the tables are built once for each
//! play.** A radius scan for each candidate destination is the same diamond
//! walked several hundred times in one decision. The disclosure reading is a
//! prefix sum in the rotated coordinates `u = x + y` and `v = x - y`, where
//! Manhattan distance is the Chebyshev distance and a diamond is a square, so
//! a destination costs four loads however far a recon sees. The concealment
//! reading is one bool for each tile.
//!
//! The tables are of this play alone, which is the same approximation every
//! other map in this crate makes and is not the same thing as a stale map: the
//! harness observes again after every command, so a scout that has already
//! walked into a pocket of fog has lit it, and the next unit reads it lit.

use awvm::commander;
use awvm::ruleset::{self, TerrainTrait, WeatherKind};
use awvm::semantic::{
    AwbwVisibility, Dimensions, Location, PlayerIdx, Pos, State, Unit, Viewpoint, Visibility,
};

use crate::threat;

/// A count over every diamond of the board at once.
///
/// Rotate a tile to `u = x + y` and `v = x - y`, and the Manhattan distance
/// between two tiles becomes the larger of the two rotated differences. A
/// diamond of radius `r` is therefore a square of side `2r + 1` in the rotated
/// plane, and a two-dimensional prefix sum answers any of them in four loads.
///
/// The rotated plane is `2n - 1` on a side for a board `n` tiles across, and
/// half of it is unreachable by the parity of `x + y`. That waste is the whole
/// price: the table is built once for each play and read once for each
/// candidate destination, and there are far more destinations than tiles.
#[derive(Debug, Default)]
struct Diamonds {
    /// Side of the rotated plane.
    span: usize,
    /// `(span + 1)` squared, inclusive prefix sums with a zero row and column.
    sums: Vec<f64>,
}

impl Diamonds {
    const fn new() -> Self {
        Self {
            span: 0,
            sums: Vec::new(),
        }
    }

    /// Start a table over a board of these dimensions, holding nothing.
    fn clear(&mut self, dimensions: Dimensions) {
        self.span = usize::from(dimensions.width()) + usize::from(dimensions.height()) - 1;
        let stride = self.span + 1;
        self.sums.clear();
        self.sums.resize(stride * stride, 0.0);
    }

    /// Put `value` on the tile at `position`, before the sums are taken.
    fn add(&mut self, dimensions: Dimensions, position: Pos, value: f64) {
        let (u, v) = rotate(dimensions, position);
        let stride = self.span + 1;
        self.sums[(u + 1) * stride + (v + 1)] += value;
    }

    /// Take the sums, after everything is in place.
    fn finish(&mut self) {
        let stride = self.span + 1;
        for u in 1..stride {
            let mut row = 0.0;
            for v in 1..stride {
                row += self.sums[u * stride + v];
                self.sums[u * stride + v] = row + self.sums[(u - 1) * stride + v];
            }
        }
    }

    /// What this table holds within `radius` of the rotated point `(u, v)`.
    fn diamond(&self, u: usize, v: usize, radius: usize) -> f64 {
        if self.span == 0 {
            return 0.0;
        }
        let stride = self.span + 1;
        let last = self.span;
        let lo = |value: usize| value.saturating_sub(radius);
        let hi = |value: usize| (value + radius + 1).min(last);
        let (u0, u1) = (lo(u), hi(u));
        let (v0, v1) = (lo(v), hi(v));
        self.sums[u1 * stride + v1] - self.sums[u0 * stride + v1] - self.sums[u1 * stride + v0]
            + self.sums[u0 * stride + v0]
    }
}

/// A tile in the rotated plane, where a diamond is a square.
///
/// `v` is offset by the board's height so that both coordinates count from
/// zero, which is what lets them index a table.
fn rotate(dimensions: Dimensions, position: Pos) -> (usize, usize) {
    let x = usize::from(position.x);
    let y = usize::from(position.y);
    let height = usize::from(dimensions.height());
    (x + y, x + height - 1 - y)
}

/// Which halves of the vision term a weighting pays for.
///
/// Each half is a table of its own and neither is cheap, so a weighting that
/// prices one at nothing does not build it. That is the same guard the threat
/// map and the garrison field already have, and it is what keeps a published
/// number readable: `veil` prices cover and not disclosure, so `veil` builds
/// the cover grid and not the diamonds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Needs {
    /// The dark a play lights: [`VisionMap::reveal`].
    pub disclosure: bool,
    /// The cover a play takes: [`VisionMap::conceals`].
    pub cover: bool,
    /// The share of a dark tile's worth that comes from where it is.
    ///
    /// At nothing every dark tile counts one, which is the reading the
    /// disclosure term shipped with: the fog in front of our headquarters and
    /// the fog in a corner nobody will ever walk to are the same tile. At one
    /// a dark tile is worth only its nearness to a property, which is where
    /// both sides have to go. Between the two is a blend, and the blend is
    /// what a sweep reads.
    pub focus: f64,
    /// The decay for each tile between a dark tile and the nearest property.
    ///
    /// Tiles, and not turns: this measures a straight line from a tile to the
    /// ground worth fighting over, in the same unit
    /// [`crate::agents::Weights::hold_decay`] measures.
    pub focus_decay: f64,
}

impl Needs {
    /// Both halves, every dark tile counting one.
    pub const BOTH: Self = Self {
        disclosure: true,
        cover: true,
        focus: 0.0,
        focus_decay: 1.0,
    };

    /// Whether any of it is worth building.
    pub const fn any(self) -> bool {
        self.disclosure || self.cover
    }

    /// Whether a dark tile's worth depends on where it is.
    fn is_focused(self) -> bool {
        self.focus != 0.0
    }
}

/// What this player can see, one entry for each tile.
#[derive(Debug)]
pub struct VisionMap {
    /// Which halves the tables below hold.
    needs: Needs,
    /// Row-major, and empty when the map is not built. A weighting that
    /// prices vision at nothing never pays for it, exactly as it never pays
    /// for the threat map.
    seen: Vec<bool>,
    /// Whether a tile hides what stands on it from the enemy on the board.
    concealed: Vec<bool>,
    /// The board the tables were built over.
    dimensions: Dimensions,
    /// Dark tiles a unit lights however far away it stands.
    open: Diamonds,
    /// Dark tiles behind cover, one table for each vision limit the board
    /// holds. A unit reads one of these no further than the limit, because
    /// that is as far as the terrain lets anybody read it.
    limited: Vec<(usize, Diamonds)>,
    /// Tiles from each tile to the nearest property, when a dark tile's worth
    /// depends on where it is. Empty otherwise.
    to_property: Vec<u16>,
    /// The queue that walk runs on, kept so that a play does not allocate one.
    frontier: std::collections::VecDeque<Pos>,
}

impl Default for VisionMap {
    fn default() -> Self {
        Self::new()
    }
}

impl VisionMap {
    pub const fn new() -> Self {
        Self {
            needs: Needs {
                disclosure: false,
                cover: false,
                focus: 0.0,
                focus_decay: 1.0,
            },
            seen: Vec::new(),
            concealed: Vec::new(),
            // No board until one is built, which is what every reader sees
            // as "there is no map".
            dimensions: Dimensions::new(0, 0),
            open: Diamonds::new(),
            limited: Vec::new(),
            to_property: Vec::new(),
            frontier: std::collections::VecDeque::new(),
        }
    }

    /// Whether a map has been built to read.
    pub fn is_built(&self) -> bool {
        self.needs.any()
    }

    /// Forget the map, so that a later reader knows there is none.
    pub fn forget(&mut self) {
        self.needs = Needs::default();
        self.seen.clear();
        self.concealed.clear();
    }

    /// What `seat`'s team can see of `state` right now, and what a play would
    /// do to it.
    ///
    /// One viewpoint for the whole board rather than one for each question:
    /// the viewpoint resolves the team roster and every sighting unit's
    /// effective vision once, and answers each tile from that.
    pub fn build(&mut self, state: &State, seat: PlayerIdx, needs: Needs) {
        self.forget();
        if !needs.any() || state.players.get(seat.get()).is_none() {
            return;
        }
        self.needs = needs;
        self.dimensions = state.board.dimensions();
        if needs.disclosure {
            self.build_sight(state, seat);
            if needs.is_focused() {
                self.walk_to_property(state);
            }
            self.build_coverage(state);
        }
        if needs.cover {
            self.build_concealment(state, seat);
        }
    }

    /// What the asking team can see of the board right now.
    fn build_sight(&mut self, state: &State, seat: PlayerIdx) {
        let Some(player) = state.players.get(seat.get()) else {
            return;
        };
        let view = AwbwVisibility.view(state, &player.team);
        let dimensions = self.dimensions;
        self.seen.reserve(dimensions.len());
        self.seen
            .extend(dimensions.positions().map(|tile| view.position(tile)));
    }

    /// How far each tile is from the nearest property, in tiles.
    ///
    /// A walk over the four neighbours and not a route: this asks where the
    /// board's business is, and a mountain between a tile and a base does not
    /// move the base. Every property seeds the walk at once, so it costs one
    /// pass over the board however many properties the board holds.
    fn walk_to_property(&mut self, state: &State) {
        let dimensions = self.dimensions;
        self.to_property.clear();
        self.to_property.resize(dimensions.len(), u16::MAX);
        self.frontier.clear();
        for (position, tile) in state.board.iter() {
            if !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
                continue;
            }
            if let Some(index) = dimensions.index(position) {
                self.to_property[index] = 0;
                self.frontier.push_back(position);
            }
        }
        while let Some(position) = self.frontier.pop_front() {
            let Some(index) = dimensions.index(position) else {
                continue;
            };
            let next = self.to_property[index].saturating_add(1);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let Some(neighbour) = position.offset(dx, dy) else {
                    continue;
                };
                let Some(index) = dimensions.index(neighbour) else {
                    continue;
                };
                if self.to_property[index] <= next {
                    continue;
                }
                self.to_property[index] = next;
                self.frontier.push_back(neighbour);
            }
        }
    }

    /// What one dark tile is worth to light.
    ///
    /// One, until a weighting says a dark tile's worth depends on where it is.
    fn worth(&self, index: usize) -> f64 {
        if !self.needs.is_focused() {
            return 1.0;
        }
        let distance = self.to_property.get(index).copied().unwrap_or(u16::MAX);
        let nearness = self
            .needs
            .focus_decay
            .powi(i32::from(distance.min(u16::from(u8::MAX))));
        (1.0 - self.needs.focus) + self.needs.focus * nearness
    }

    /// The dark of the board, as a count over every diamond of it.
    fn build_coverage(&mut self, state: &State) {
        let dimensions = self.dimensions;
        self.open.clear(dimensions);
        for (_, table) in self.limited.iter_mut() {
            table.clear(dimensions);
        }
        for (position, tile) in state.board.iter() {
            let Some(index) = dimensions.index(position) else {
                continue;
            };
            if self.seen[index] {
                continue;
            }
            let terrain = ruleset::terrain(tile.terrain);
            // A teleporter is dark to everyone, so walking up to one
            // discloses nothing.
            if terrain.has(TerrainTrait::Teleporter) {
                continue;
            }
            let worth = self.worth(index);
            match terrain.vision_limit {
                None => self.open.add(dimensions, position, worth),
                Some(limit) => {
                    let table = match self.limited.iter_mut().find(|(known, _)| *known == limit) {
                        Some((_, table)) => table,
                        None => {
                            let mut table = Diamonds::new();
                            table.clear(dimensions);
                            self.limited.push((limit, table));
                            let last = self.limited.len() - 1;
                            &mut self.limited[last].1
                        }
                    };
                    table.add(dimensions, position, worth);
                }
            }
        }
        self.open.finish();
        for (_, table) in self.limited.iter_mut() {
            table.finish();
        }
    }

    /// Which tiles hide what stands on them, from the enemy on the board now.
    ///
    /// The terrain's own vision limit is where this starts, and it is not
    /// where it ends. Cover is only cover while nobody is inside it: an enemy
    /// within the limit reads the tile, and a commander that sees into
    /// concealing terrain reads every tile of it inside its own sight. Both
    /// are read off the enemy this player can see, which is the honest reading
    /// under fog and an exact one with the board lit.
    fn build_concealment(&mut self, state: &State, seat: PlayerIdx) {
        let dimensions = self.dimensions;
        self.concealed.clear();
        self.concealed.resize(dimensions.len(), false);
        // The furthest any of this board's cover can be read from, which is
        // how near an enemy has to be to take it away.
        let mut reach = 0;
        for (position, tile) in state.board.iter() {
            let Some(limit) = ruleset::terrain(tile.terrain).vision_limit else {
                continue;
            };
            let Some(index) = dimensions.index(position) else {
                continue;
            };
            self.concealed[index] = true;
            reach = reach.max(limit);
        }

        for unit in state.units.iter() {
            let Location::Board { position } = unit.location else {
                continue;
            };
            if !threat::hostile(state, seat, unit.owner) {
                continue;
            }
            // What this enemy reads through cover: every tile of it within its
            // own sight when its commander sees through cover, and the tiles
            // beside it otherwise. A limit of one is the woods and the reef,
            // and the enemy standing next to either of them is what takes the
            // cover away.
            let radius = if commander::reveals_concealing_terrain(state, unit) {
                sight_of(state, unit, position)
            } else {
                reach
            };
            self.expose(position, radius);
        }
    }

    /// Take the cover off every tile within `radius` of `position`.
    fn expose(&mut self, position: Pos, radius: usize) {
        let radius = i16::try_from(radius).unwrap_or(i16::MAX);
        for dy in -radius..=radius {
            let span = radius - dy.abs();
            for dx in -span..=span {
                let Some(target) = position.offset(dx, dy) else {
                    continue;
                };
                if let Some(index) = self.dimensions.index(target) {
                    self.concealed[index] = false;
                }
            }
        }
    }

    /// How many dark tiles `unit` would light standing at `position`.
    ///
    /// Zero without a map, which is what a weighting that prices vision at
    /// nothing reads.
    pub fn reveal(&self, state: &State, unit: &Unit, position: Pos) -> f64 {
        if !self.needs.disclosure {
            return 0.0;
        }
        let radius = sight_of(state, unit, position);
        let (u, v) = rotate(self.dimensions, position);
        let mut lit = self.open.diamond(u, v, radius);
        let reveals_concealing = commander::reveals_concealing_terrain(state, unit);
        for (limit, table) in &self.limited {
            // Cover is read no further than the terrain lets it be read,
            // unless this commander reads through it.
            let reach = if reveals_concealing {
                radius
            } else {
                radius.min(*limit)
            };
            lit += table.diamond(u, v, reach);
        }
        lit
    }

    /// Whether a unit standing on this tile is hidden from the enemy.
    ///
    /// The terrain's own vision limit, less the enemy that is already reading
    /// through it. Nothing is concealed without a map, which is what a
    /// weighting that prices cover at nothing reads, and what a lit board
    /// reads.
    pub fn conceals(&self, position: Pos) -> bool {
        if !self.needs.cover {
            return false;
        }
        self.dimensions
            .index(position)
            .is_some_and(|index| self.concealed.get(index).copied().unwrap_or(false))
    }
}

/// How far `unit` sees, standing at `position`.
///
/// The commander's own rule, the mountain bonus for the units that climb one
/// to look, and the tile the rain takes off everybody. At least one tile: a
/// unit always sees the ground it stands on.
fn sight_of(state: &State, unit: &Unit, position: Pos) -> usize {
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
    usize::try_from((sight + bonus + rain).max(1)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::arena;
    use awvm::ruleset::{Terrain, UnitKind};
    use awvm::semantic::{Concealment, Location, State, TileOwner, UnitAction, UnitId};

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
                cell.owner = TileOwner::Neutral;
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

    /// The enemy seat: the one this board holds that is not `seat`.
    fn enemy_of(state: &State, seat: PlayerIdx) -> PlayerIdx {
        state
            .players
            .seats()
            .map(|(other, _)| other)
            .find(|other| *other != seat)
            .expect("the arena seats two players")
    }

    /// What one unit of `kind` would light standing on `terrain`, from a board
    /// no other unit of ours can see any of.
    fn lit(kind: UnitKind, terrain: Terrain) -> f64 {
        let (state, seat, position) = empty_board(terrain);
        let mut map = VisionMap::new();
        map.build(&state, seat, Needs::BOTH);
        map.reveal(&state, &unit_of(kind, seat, position), position)
    }

    /// The dark within `radius` of `position`, counted one tile at a time.
    ///
    /// This is the radius scan the prefix tables replaced, kept as the
    /// reference the tables are checked against.
    fn scanned(state: &State, seen: &[bool], position: Pos, radius: i16, reveals: bool) -> f64 {
        let dimensions = state.board.dimensions();
        let mut lit = 0.0;
        for dy in -radius..=radius {
            let span = radius - dy.abs();
            for dx in -span..=span {
                let Some(target) = position.offset(dx, dy) else {
                    continue;
                };
                let Some(index) = dimensions.index(target) else {
                    continue;
                };
                if seen[index] {
                    continue;
                }
                let terrain = ruleset::terrain(state.board.tile(target).terrain);
                if terrain.has(TerrainTrait::Teleporter) {
                    continue;
                }
                let hidden = !reveals
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
        map.build(&state, seat, Needs::BOTH);
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
        map.build(&state, seat, Needs::BOTH);
        let position = Pos::new(5, 5);
        let unit = unit_of(UnitKind::Recon, seat, position);
        assert_eq!(map.reveal(&state, &unit, position), 0.0);
    }

    /// The table answers what the scan answered, on the board the arena plays
    /// and not on a cleared one.
    ///
    /// Every tile, every kind that sees differently, and the woods and
    /// mountains the arena map really holds. A prefix sum that dropped the
    /// parity of the rotated plane, or clamped a diamond at the wrong edge,
    /// disagrees here.
    #[test]
    fn the_diamond_tables_count_what_a_radius_scan_counts() {
        let mut state = arena(true, 1);
        let seat = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        state.units.retain(|_| false);
        // One unit of ours, so that part of the board is lit and the rest is
        // not: a board that is dark everywhere cannot tell a table that drops
        // a tile from one that does not.
        state
            .units
            .push(unit_of(UnitKind::Infantry, seat, Pos::new(2, 2)));

        let mut map = VisionMap::new();
        map.build(&state, seat, Needs::BOTH);
        let seen: Vec<bool> = map.seen.clone();

        for kind in [UnitKind::Infantry, UnitKind::Recon, UnitKind::Tank] {
            for position in state.board.dimensions().positions() {
                let unit = unit_of(kind, seat, position);
                let radius = i16::try_from(sight_of(&state, &unit, position)).expect("a radius");
                let reveals = commander::reveals_concealing_terrain(&state, &unit);
                assert_eq!(
                    map.reveal(&state, &unit, position),
                    scanned(&state, &seen, position, radius, reveals),
                    "{kind:?} at {position:?} reads a different count from the table"
                );
            }
        }
    }

    #[test]
    fn the_woods_hide_what_stands_in_them_and_the_plain_does_not() {
        let (state, seat, position) = empty_board(Terrain::Wood);
        let mut map = VisionMap::new();
        map.build(&state, seat, Needs::BOTH);
        assert!(map.conceals(position));

        let (state, seat, position) = empty_board(Terrain::Plain);
        let mut map = VisionMap::new();
        map.build(&state, seat, Needs::BOTH);
        assert!(!map.conceals(position));
    }

    /// Cover is cover until somebody is standing beside it.
    #[test]
    fn an_enemy_beside_the_woods_takes_the_cover_off_them() {
        let (mut state, seat, position) = empty_board(Terrain::Wood);
        let enemy = enemy_of(&state, seat);
        let beside = Pos::new(position.x + 1, position.y);
        state.units.push(unit_of(UnitKind::Infantry, enemy, beside));

        let mut map = VisionMap::new();
        map.build(&state, seat, Needs::BOTH);
        assert!(
            !map.conceals(position),
            "an enemy one tile away reads the woods"
        );

        let mut away = state.clone();
        away.units.retain(|_| false);
        away.units.push(unit_of(
            UnitKind::Infantry,
            enemy,
            Pos::new(position.x + 4, position.y),
        ));
        let mut map = VisionMap::new();
        map.build(&away, seat, Needs::BOTH);
        assert!(
            map.conceals(position),
            "an enemy four tiles away reads nothing of them"
        );
    }

    /// The dark in front of a property outbids the dark behind us.
    #[test]
    fn a_focused_reading_prefers_the_dark_near_a_property() {
        let (mut state, seat, _) = empty_board(Terrain::Plain);
        // One property, on one side of an otherwise empty board.
        let property = Pos::new(2, 5);
        state
            .board
            .get_mut(property)
            .expect("the tile is on the board")
            .terrain = Terrain::Base;

        let near = Pos::new(4, 5);
        let far = Pos::new(8, 5);
        let read = |needs: Needs, position: Pos| {
            let mut map = VisionMap::new();
            map.build(&state, seat, needs);
            map.reveal(&state, &unit_of(UnitKind::Recon, seat, position), position)
        };

        let flat = Needs::BOTH;
        let focused = Needs {
            focus: 1.0,
            focus_decay: 0.5,
            ..Needs::BOTH
        };
        // A ratio and not a count: the two tiles do not hold the same amount
        // of board, and the term is about which dark is worth lighting.
        let flat_ratio = read(flat, near) / read(flat, far);
        let focused_ratio = read(focused, near) / read(focused, far);
        assert!(
            focused_ratio > flat_ratio * 1.5,
            "focused {focused_ratio} against flat {flat_ratio}"
        );
    }

    /// A half priced at nothing is a table that is never built.
    #[test]
    fn a_half_nobody_prices_is_not_built() {
        let (state, seat, position) = empty_board(Terrain::Wood);
        let unit = unit_of(UnitKind::Recon, seat, position);

        let mut cover_only = VisionMap::new();
        cover_only.build(
            &state,
            seat,
            Needs {
                disclosure: false,
                cover: true,
                ..Needs::BOTH
            },
        );
        assert!(cover_only.conceals(position));
        assert_eq!(
            cover_only.reveal(&state, &unit, position),
            0.0,
            "a weighting that does not price disclosure builds no diamonds"
        );

        let mut sight_only = VisionMap::new();
        sight_only.build(
            &state,
            seat,
            Needs {
                disclosure: true,
                cover: false,
                ..Needs::BOTH
            },
        );
        assert!(sight_only.reveal(&state, &unit, position) > 0.0);
        assert!(
            !sight_only.conceals(position),
            "a weighting that does not price cover builds no cover grid"
        );

        let mut neither = VisionMap::new();
        neither.build(&state, seat, Needs::default());
        assert!(!neither.is_built());
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
        assert!(!map.conceals(position), "no map hides nothing");
    }
}
