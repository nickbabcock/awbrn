//! What the enemy can take off each tile, by the kind of unit that stands
//! there.
//!
//! This is the smallest useful influence map. It is a grid of rows, not a
//! grid of numbers: `threat[tile][kind]` is the funds an enemy could take off
//! a whole unit of `kind` standing on `tile`. One number for each tile cannot
//! be written honestly, because the damage table is indexed by the defender.
//! An anti-air and a b-copter read opposite answers from the same tile, and a
//! fighter does no damage at all to a tank while it destroys the copter. A
//! map of one number for each tile must choose one defender, and it is then
//! wrong for every other.
//!
//! # The two layers
//!
//! Which tiles one unit threatens depends on how it fires.
//! `transition/attack.rs` refuses a moving attack to an indirect unit on any
//! path longer than one tile: an artillery moves or it fires, never both in a
//! turn. So the map holds two layers, and a reader keeps them apart:
//!
//! - [`ThreatMap::immediate`], what can strike on the enemy's next turn. A
//!   direct unit threatens every tile beside a tile it can stop on, because
//!   it moves and fires in one turn. An indirect unit threatens only the
//!   range ring around the tile it stands on now.
//! - [`ThreatMap::deferred`], the turn after that: the range ring around
//!   every tile an indirect unit can stop on. Those tiles are firing
//!   positions only once it has spent a turn moving to one.
//!
//! An agent that merges the two is too timid. It reads ground it could safely
//! hold for a turn as ground under fire, and closing on an artillery through
//! that window is how an artillery is answered. An agent that keeps only the
//! immediate layer walks into a ring that shuts the turn after.
//!
//! # What the numbers carry
//!
//! - **Terrain defense belongs to the build.** An air unit gets no terrain
//!   stars anywhere; a ground or sea unit gets the stars of the tile
//!   (`transition/attack.rs`, `base_terrain_stars`). The tile and the
//!   defender kind are both known while the row is written, so the discount
//!   is exact here and could not be applied to a map of one number at all.
//! - **Ammo.** Enemy ammo is hidden under fog, so every enemy is assumed to
//!   hold a full magazine. That errs toward caution.
//! - **A damage row of `None` is zero, not a small number.** A fighter adds
//!   nothing to the row of any ground unit.
//! - Commander effects are not read. The probe that turns a commander into
//!   multipliers is a later tier, and until it lands both sides are scored
//!   with the plain table.
//!
//! The rows are close to free. The cost is one reachability search for each
//! enemy unit, and that search says the same thing whoever stands on the
//! tile, so filling twenty-five numbers instead of one is a few thousand adds
//! against the tens of thousands of commands this crate plays each second.

use awvm::combat::{self, Side};
use awvm::query::{MoveScratch, Sweep};
use awvm::ruleset::{self, CommanderKind, Domain, FireMode, UnitKind};
use awvm::semantic::{
    CellIdx, Dimensions, Location, PlayerIdx, Pos, PowerState, State, TeamId, TerrainId, Unit,
    UnitId, WeatherKind,
};
use awvm::session::Session;

/// Terrain defense runs from no stars to four, so a row holds five columns.
const STAR_LEVELS: usize = 5;

/// The neutral attack and defense a commander with no combat rule presents.
///
/// [`combat::damage`] reads both as percentages, so a hundred leaves the
/// formula with the plain table value.
const NEUTRAL: i64 = 100;

/// A defender at whole health, which is what a row is priced for.
const WHOLE: u8 = 100;

/// What the enemy can take off each tile.
///
/// Build it once for a position with [`ThreatMap::build`] and read it as
/// often as the caller likes. It says nothing about whose turn it is: every
/// enemy is scored from where it stands now, with the movement it will have
/// when the turn comes back to it, so a spent enemy threatens exactly as much
/// as a fresh one.
#[derive(Debug)]
pub struct ThreatMap {
    dimensions: Dimensions,
    /// `UnitKind::COUNT` funds for each tile, the tiles in cell order.
    immediate: Vec<f32>,
    deferred: Vec<f32>,
    /// The build each tile's row was last written in, one entry for each tile
    /// of each layer.
    ///
    /// A row a build never wrote to reads as zero and is emptied where it lies
    /// the first time that build writes to it. Emptying both layers up front
    /// instead means writing the whole board twice for each command of a
    /// match, and a threat map is mostly empty: only the tiles an enemy can
    /// reach carry anything.
    written: [Vec<u32>; 2],
    /// Which build the map holds. Rows stamped with anything else are stale.
    generation: u32,
    /// Which tiles the attacker being walked has already been added to. It
    /// holds the serial of that attacker rather than a flag, so the sweep
    /// clears it by counting up instead of by writing the board.
    stamp: Vec<u32>,
    serial: u32,
    /// The terrain stars of each tile, in cell order. Read once for every
    /// tile every attacker threatens, so it is worked out once for the board
    /// rather than looked up through the board each time.
    stars: Vec<u8>,
    /// The tiles the attacker being walked can stop on, taken out of its
    /// movement field so that the field is dropped before the rows are
    /// written.
    stops: Vec<Pos>,
    /// The grids and buckets every search after the first one reuses.
    scratch: MoveScratch,
    context: Option<ThreatContext>,
    occupancy: Vec<Option<UnitId>>,
    cached: Vec<CachedThreat>,
    collecting: [Vec<CellIdx>; 2],
    dependencies: Vec<CellIdx>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ThreatContext {
    seat: PlayerIdx,
    fog: bool,
    terrain: Vec<TerrainId>,
    weather: WeatherKind,
    players: Vec<PlayerThreatContext>,
    fog_units: Vec<Unit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlayerThreatContext {
    team: TeamId,
    commanders: Vec<(CommanderKind, bool)>,
    power: PowerState,
}

#[derive(Debug)]
struct CachedThreat {
    unit: Unit,
    table: DamageTable,
    dependencies: Vec<CellIdx>,
    layers: [Vec<CellIdx>; 2],
}

/// Empty `buffer` to `len` zeroes, reusing what it already holds.
///
/// `clear` then `resize` writes each element through the extend loop, where a
/// buffer that is already the right length is emptied where it lies.
fn zero<T: Copy + Default>(buffer: &mut Vec<T>, len: usize) {
    if buffer.len() == len {
        buffer.fill(T::default());
    } else {
        buffer.clear();
        buffer.resize(len, T::default());
    }
}

impl Default for ThreatMap {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreatMap {
    /// An empty map, sized by the first position it is built for.
    pub const fn new() -> Self {
        Self {
            dimensions: Dimensions::new(0, 0),
            immediate: Vec::new(),
            deferred: Vec::new(),
            written: [const { Vec::new() }; 2],
            generation: 0,
            stamp: Vec::new(),
            serial: 0,
            stars: Vec::new(),
            stops: Vec::new(),
            scratch: MoveScratch::new(),
            context: None,
            occupancy: Vec::new(),
            cached: Vec::new(),
            collecting: [const { Vec::new() }; 2],
            dependencies: Vec::new(),
        }
    }

    /// Work out what every player at war with `seat` threatens.
    ///
    /// The session holds the position the asking player can see, so a hidden
    /// enemy threatens nothing here. That is the same blindness the agent
    /// plays under everywhere else, and not a defect of the map.
    ///
    /// The searches go through the session's own board tables, one set per
    /// hostile seat, so a sweep of twenty enemy units builds the entry-cost
    /// and blocking grids once for each side rather than once for each unit.
    pub fn build(&mut self, session: &Session, seat: PlayerIdx) {
        let state = session.state();
        self.dimensions = state.board.dimensions();
        let cells = self.dimensions.len();

        let context = ThreatContext::of(state, seat);
        let occupancy = occupancy(state);
        let changed_occupancy = changed_cells(&self.occupancy, &occupancy, &self.dimensions);
        if self.context.as_ref() != Some(&context) {
            self.cached.clear();
        }

        let rows = cells * UnitKind::COUNT;
        // The rows themselves are not emptied. Only their stamps say what this
        // build holds, so a board that has not changed shape keeps both
        // allocations and every stale value in them.
        self.immediate.resize(rows, 0.0);
        self.deferred.resize(rows, 0.0);
        for written in &mut self.written {
            if written.len() == cells {
                continue;
            }
            written.clear();
            written.resize(cells, 0);
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            // The counter wrapped onto the stamp a fresh row carries, so every
            // row has to be made stale by hand this once.
            for written in &mut self.written {
                written.fill(0);
            }
            self.generation = 1;
        }
        zero(&mut self.stamp, cells);
        self.serial = 0;

        self.stars.clear();
        self.stars.extend(
            state
                .board
                .positions()
                .map(|position| stars_of(state, position)),
        );

        // One row per seat, filled for the hostile ones only, so the loop
        // below reads a sweep by seat index and skips a friendly unit by
        // finding none. The units are walked in their own order, because the
        // rows are sums of floats and grouping them by owner would round
        // differently.
        let mut sweeps: Vec<Option<Sweep<'_>>> = Vec::new();
        for (other, _) in state.players.seats() {
            if !hostile(state, seat, other) {
                continue;
            }
            let row = other.get();
            if sweeps.len() <= row {
                sweeps.resize_with(row + 1, || None);
            }
            sweeps[row] = session.sweep(other);
        }

        let mut previous = std::mem::take(&mut self.cached);
        for unit in state.units.iter() {
            let Location::Board { position } = unit.location else {
                continue;
            };
            let Some(Some(sweep)) = sweeps.get(unit.owner.get()) else {
                continue;
            };
            let reusable = previous
                .iter()
                .position(|cached| cached.unit.id == unit.id)
                .filter(|index| {
                    let cached = &previous[*index];
                    cached.unit == *unit
                        && !cached
                            .dependencies
                            .iter()
                            .any(|cell| changed_occupancy.contains(cell))
                });
            if let Some(index) = reusable {
                self.cached.push(previous.swap_remove(index));
                continue;
            }

            for layer in &mut self.collecting {
                layer.clear();
            }
            let table = DamageTable::of(unit);
            let dependencies = self.add(sweep, unit, position, &table);
            self.cached.push(CachedThreat {
                unit: *unit,
                table,
                dependencies,
                layers: [
                    std::mem::take(&mut self.collecting[0]),
                    std::mem::take(&mut self.collecting[1]),
                ],
            });
        }
        self.write_cached();
        self.context = Some(context);
        self.occupancy = occupancy;
    }

    /// The funds an enemy could take off a whole unit of `kind` standing on
    /// `cell` on the enemy's next turn.
    pub fn immediate(&self, cell: CellIdx, kind: UnitKind) -> f64 {
        self.read(Layer::Immediate, cell, kind)
    }

    /// The same for the turn after that, from the firing positions an
    /// indirect unit must first walk to.
    ///
    /// This does not include [`ThreatMap::immediate`]. A reader that wants
    /// both adds them, and discounts this one.
    pub fn deferred(&self, cell: CellIdx, kind: UnitKind) -> f64 {
        self.read(Layer::Deferred, cell, kind)
    }

    /// One tile's row of one layer, or zero where this build wrote nothing.
    fn read(&self, layer: Layer, cell: CellIdx, kind: UnitKind) -> f64 {
        let index = usize::from(cell.get());
        if self.written[layer.index()].get(index) != Some(&self.generation) {
            return 0.0;
        }
        let rows = match layer {
            Layer::Immediate => &self.immediate,
            Layer::Deferred => &self.deferred,
        };
        rows.get(index * UnitKind::COUNT + kind.index())
            .map_or(0.0, |value| f64::from(*value))
    }

    /// Add one enemy unit's threat to both layers.
    fn add(
        &mut self,
        sweep: &Sweep<'_>,
        unit: &Unit,
        position: Pos,
        table: &DamageTable,
    ) -> Vec<CellIdx> {
        let profile = ruleset::profile(unit.kind);
        // A unit with no weapon threatens nothing, and neither does one with
        // no health left to fire with.
        if profile.fire_mode == FireMode::None || unit.hp == 0 {
            return Vec::new();
        }
        if table.is_empty() {
            return Vec::new();
        }

        match profile.fire_mode {
            // A direct unit moves and fires in one turn, so every tile beside
            // a tile it can stop on is under fire now.
            FireMode::Direct => {
                self.collect_stops(sweep, unit);
                self.serial += 1;
                for index in 0..self.stops.len() {
                    let stop = self.stops[index];
                    for target in stop.orthogonal() {
                        self.mark(target, Layer::Immediate);
                    }
                }
            }
            // An indirect unit fires from where it stands, and from anywhere
            // it can walk to only on the turn after it walks there.
            FireMode::Indirect => {
                let range = profile.indirect_range.unwrap_or(ruleset::AttackRange {
                    minimum: 1,
                    maximum: 1,
                });
                self.serial += 1;
                self.ring(position, range, Layer::Immediate);

                self.collect_stops(sweep, unit);
                self.serial += 1;
                for index in 0..self.stops.len() {
                    let stop = self.stops[index];
                    // The tile it stands on is already the immediate ring
                    // above. Firing from where it is takes no turn, so the
                    // same ring must not also read as a turn away.
                    if stop == position {
                        continue;
                    }
                    self.ring(stop, range, Layer::Deferred);
                }
            }
            FireMode::None => {}
        }
        std::mem::take(&mut self.dependencies)
    }

    /// Every tile `unit` can come to rest on, its own tile included.
    ///
    /// The sweep is the one opened for this unit's own seat, which is what
    /// makes the search read tables that are already built.
    fn collect_stops(&mut self, sweep: &Sweep<'_>, unit: &Unit) {
        self.stops.clear();
        self.dependencies.clear();
        let Ok(field) = sweep.reachable_into(unit.id, &mut self.scratch) else {
            self.dependencies.extend(
                self.dimensions
                    .positions()
                    .filter_map(|position| self.dimensions.cell_index(position)),
            );
            return;
        };
        self.stops
            .extend(field.destinations().map(|(position, _)| position));
        self.dependencies.extend(
            field
                .reach()
                .filter_map(|(position, _)| self.dimensions.cell_index(position)),
        );
        field.recycle(&mut self.scratch);
    }

    /// Add the row to every tile in the range ring around `center`.
    fn ring(&mut self, center: Pos, range: ruleset::AttackRange, layer: Layer) {
        let reach = i16::try_from(range.maximum).unwrap_or(i16::MAX);
        for dy in -reach..=reach {
            let span = reach - dy.abs();
            for dx in -span..=span {
                let Some(target) = center.offset(dx, dy) else {
                    continue;
                };
                if center.distance(target) < range.minimum {
                    continue;
                }
                self.mark(target, layer);
            }
        }
    }

    /// Add the attacker's row to one tile, at most once for this attacker.
    fn mark(&mut self, target: Pos, layer: Layer) {
        let Some(cell) = self.dimensions.cell_index(target) else {
            return;
        };
        let index = usize::from(cell.get());
        if self.stamp[index] == self.serial {
            return;
        }
        self.stamp[index] = self.serial;

        self.collecting[layer.index()].push(cell);
    }

    /// Write cached unit contributions in roster order.
    ///
    /// This repeats the old floating-point addition order. Reusing a unit
    /// therefore cannot change a tie through a different rounding sequence.
    fn write_cached(&mut self) {
        for cached in &self.cached {
            let table = &cached.table;
            for layer in [Layer::Immediate, Layer::Deferred] {
                for cell in &cached.layers[layer.index()] {
                    let index = usize::from(cell.get());
                    let row = index * UnitKind::COUNT;
                    let stars = usize::from(self.stars[index]);
                    let generation = self.generation;
                    let fresh = self.written[layer.index()][index] != generation;
                    self.written[layer.index()][index] = generation;
                    let out = match layer {
                        Layer::Immediate => &mut self.immediate,
                        Layer::Deferred => &mut self.deferred,
                    };
                    let out = &mut out[row..row + UnitKind::COUNT];
                    if fresh {
                        for (kind, value) in out.iter_mut().enumerate() {
                            *value = table.funds[kind][stars];
                        }
                    } else {
                        for (kind, value) in out.iter_mut().enumerate() {
                            *value += table.funds[kind][stars];
                        }
                    }
                }
            }
        }
    }
}

impl ThreatContext {
    fn of(state: &State, seat: PlayerIdx) -> Self {
        Self {
            seat,
            fog: state.settings.fog,
            terrain: state.board.iter().map(|(_, tile)| tile.terrain).collect(),
            weather: state.weather.kind,
            players: state
                .players
                .seats()
                .map(|(_, player)| PlayerThreatContext {
                    team: player.team.clone(),
                    commanders: player
                        .commanders
                        .iter()
                        .map(|commander| (commander.id, commander.active))
                        .collect(),
                    power: player.power_state.clone(),
                })
                .collect(),
            // Fog visibility depends on the full unit roster. Use the simple
            // safe invalidation rule there. The profiled rollout has fog off.
            fog_units: if state.settings.fog {
                state.units.iter().copied().collect()
            } else {
                Vec::new()
            },
        }
    }
}

fn occupancy(state: &State) -> Vec<Option<UnitId>> {
    let dimensions = state.board.dimensions();
    let mut occupied = vec![None; dimensions.len()];
    for unit in state.units.iter() {
        let Location::Board { position } = unit.location else {
            continue;
        };
        if let Some(cell) = dimensions.cell_index(position) {
            occupied[usize::from(cell.get())] = Some(unit.id);
        }
    }
    occupied
}

fn changed_cells(
    previous: &[Option<UnitId>],
    current: &[Option<UnitId>],
    dimensions: &Dimensions,
) -> Vec<CellIdx> {
    previous
        .iter()
        .zip(current)
        .enumerate()
        .filter(|(_, (previous, current))| previous != current)
        .filter_map(|(index, _)| u16::try_from(index).ok().map(CellIdx::from_raw))
        .filter(|cell| dimensions.position_of(*cell).is_some())
        .collect()
}

/// The terrain defense of one tile, clamped to the table the rows carry.
fn stars_of(state: &State, position: Pos) -> u8 {
    state
        .board
        .get(position)
        .map(|tile| clamp_stars(ruleset::defense_stars(tile.terrain)))
        .unwrap_or(0)
}

/// Terrain past the widest row the ruleset holds is read as the widest one,
/// rather than dropped.
const fn clamp_stars(stars: u8) -> u8 {
    if stars as usize >= STAR_LEVELS {
        STAR_LEVELS as u8 - 1
    } else {
        stars
    }
}

/// Which layer a ring is written into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    Immediate,
    Deferred,
}

impl Layer {
    const fn index(self) -> usize {
        match self {
            Self::Immediate => 0,
            Self::Deferred => 1,
        }
    }
}

/// Whether `other` is a seat `seat` is at war with.
pub fn hostile(state: &State, seat: PlayerIdx, other: PlayerIdx) -> bool {
    let team = |seat: PlayerIdx| state.players.get(seat.get()).map(|player| &player.team);
    match (team(seat), team(other)) {
        (Some(mine), Some(theirs)) => mine != theirs,
        _ => false,
    }
}

/// What one attacker takes off a whole defender of each kind, in funds.
///
/// The columns are the terrain stars the defender stands in. A row is written
/// once for each enemy unit and read once for every tile it threatens, which
/// is what makes the rows cheap next to the search that finds those tiles.
#[derive(Debug)]
struct DamageTable {
    funds: [[f32; STAR_LEVELS]; UnitKind::COUNT],
    /// Whether any entry is above zero. An attacker that can hurt nothing on
    /// the board is dropped before its reachable set is searched for.
    armed: bool,
}

impl DamageTable {
    /// The table one enemy unit presents, at the health it has now.
    fn of(unit: &Unit) -> Self {
        let profile = ruleset::profile(unit.kind);
        let mut funds = [[0.0; STAR_LEVELS]; UnitKind::COUNT];
        let mut armed = false;

        // The striker's own terrain never enters the damage formula, which
        // reads defense and stars off the target alone, so one attacking side
        // serves the whole table.
        let attacker = Side {
            kind: unit.kind,
            hp: unit.hp,
            // Enemy ammo is hidden under fog. A full magazine is the cautious
            // guess.
            ammo: profile.max_ammo,
            attack: NEUTRAL,
            defense: NEUTRAL,
            terrain_stars: 0,
        };

        for defender in UnitKind::ALL {
            let cost = ruleset::profile(defender).cost as f32;
            // An air unit takes no terrain stars anywhere, so its row is one
            // column repeated. A ground or sea unit gets a column for each
            // rung of terrain the board can put it on.
            let flat = ruleset::profile(defender).domain == Domain::Air;
            let row = &mut funds[defender.index()];

            for (stars, entry) in row.iter_mut().enumerate() {
                let target = Side {
                    kind: defender,
                    hp: WHOLE,
                    ammo: 0,
                    attack: NEUTRAL,
                    defense: NEUTRAL,
                    terrain_stars: if flat { 0 } else { stars as u8 },
                };
                // No weapon reaches this defender at all, and the answer does
                // not change with the ground it stands on.
                let Some(hit) = combat::damage(attacker, target, 0) else {
                    break;
                };
                *entry = f32::from(hit.damage) / 100.0 * cost;
                armed |= hit.damage > 0;
            }
        }

        Self { funds, armed }
    }

    fn is_empty(&self) -> bool {
        !self.armed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::arena;
    use awvm::semantic::{Concealment, UnitAction};

    /// A seat off a real roster. [`PlayerIdx`] has no public constructor, on
    /// purpose: one built out of thin air is only correct against the roster
    /// it is read against.
    fn a_seat() -> PlayerIdx {
        let state = arena(false, 1);
        state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat")
    }

    /// One enemy unit, at whole health, standing where it is put.
    fn unit(kind: UnitKind, position: Pos) -> Unit {
        let profile = ruleset::profile(kind);
        Unit {
            id: UnitId::new(1),
            kind,
            owner: a_seat(),
            hp: WHOLE,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        }
    }

    fn funds(table: &DamageTable, defender: UnitKind, stars: usize) -> f32 {
        table.funds[defender.index()][stars]
    }

    /// The reason the map is a grid of rows. One attacker answers two
    /// defenders with numbers that do not follow from each other, and a
    /// fighter answers a ground unit with nothing at all.
    #[test]
    fn one_attacker_answers_each_defender_differently() {
        let table = DamageTable::of(&unit(UnitKind::Fighter, Pos::new(0, 0)));
        assert_eq!(
            funds(&table, UnitKind::Tank, 0),
            0.0,
            "a fighter has no weapon that reaches a tank"
        );
        assert!(
            funds(&table, UnitKind::BCopter, 0) > 0.0,
            "a fighter destroys a copter"
        );

        let anti_air = DamageTable::of(&unit(UnitKind::AntiAir, Pos::new(0, 0)));
        assert!(
            funds(&anti_air, UnitKind::BCopter, 0) > funds(&table, UnitKind::Tank, 0),
            "the same tile reads opposite answers for a copter and a tank"
        );
    }

    /// Terrain defense belongs to the build, and only a ground or sea unit
    /// gets any.
    #[test]
    fn terrain_stars_discount_the_row_of_a_ground_unit_alone() {
        let table = DamageTable::of(&unit(UnitKind::Tank, Pos::new(0, 0)));

        let open = funds(&table, UnitKind::Infantry, 0);
        let covered = funds(&table, UnitKind::Infantry, 4);
        assert!(open > 0.0);
        assert!(
            covered < open,
            "four stars of terrain must take something off {open}, not {covered}"
        );

        let air = &table.funds[UnitKind::BCopter.index()];
        assert!(
            air.iter().all(|value| *value == air[0]),
            "an air unit gets no terrain stars anywhere: {air:?}"
        );
    }

    /// A damaged attacker takes less off its target, in the same proportion
    /// the damage formula spends health.
    #[test]
    fn a_damaged_attacker_threatens_less() {
        let whole = DamageTable::of(&unit(UnitKind::Tank, Pos::new(0, 0)));
        let mut hurt = unit(UnitKind::Tank, Pos::new(0, 0));
        hurt.hp = 50;
        let hurt = DamageTable::of(&hurt);

        let whole = funds(&whole, UnitKind::Infantry, 0);
        let hurt = funds(&hurt, UnitKind::Infantry, 0);
        assert!(
            (hurt - whole / 2.0).abs() < whole * 0.05,
            "half a tank should threaten about half of {whole}, not {hurt}"
        );
    }

    /// A unit with no weapon threatens nothing, so it is never searched for.
    #[test]
    fn a_unit_that_cannot_fire_has_an_empty_table() {
        assert!(DamageTable::of(&unit(UnitKind::Apc, Pos::new(0, 0))).is_empty());
        assert!(!DamageTable::of(&unit(UnitKind::Infantry, Pos::new(0, 0))).is_empty());
    }

    /// The arena, emptied of units, with one enemy standing where the map's
    /// predeployed unit stood.
    ///
    /// That tile is known to hold a ground unit, which is the only thing the
    /// tests below need of it.
    fn one_enemy(kind: UnitKind) -> (State, PlayerIdx, Pos) {
        let mut state = arena(false, 1);
        let ours = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        let (theirs, position) = state
            .units
            .iter()
            .find_map(|unit| match unit.location {
                Location::Board { position } if unit.owner != ours => Some((unit.owner, position)),
                _ => None,
            })
            .expect("the arena starts one predeployed enemy unit");

        state.units.retain(|_| false);
        let mut enemy = unit(kind, position);
        enemy.id = UnitId::new(9_999);
        enemy.owner = theirs;
        state.units.push(enemy);

        (state, ours, position)
    }

    /// A map built over one that was built before answers what a map built
    /// from nothing answers.
    ///
    /// The rows are not emptied between builds — a build stamps the tiles it
    /// writes and every other tile reads as zero — so this is the standing
    /// proof that no row of an earlier position survives into a later one. The
    /// last position of each pair holds no enemy at all, which is the case a
    /// stale row shows up in most loudly.
    #[test]
    fn a_rebuilt_map_holds_nothing_of_the_build_before_it() {
        let mut reused = ThreatMap::new();
        let mut compared = 0;
        let mut carried = 0;
        for kind in [UnitKind::Tank, UnitKind::Artillery, UnitKind::Infantry] {
            for emptied in [false, true] {
                let (mut state, ours, _) = one_enemy(kind);
                if emptied {
                    state.units.retain(|_| false);
                }
                let session = Session::new(state);
                reused.build(&session, ours);

                let mut fresh = ThreatMap::new();
                fresh.build(&session, ours);

                let dimensions = session.state().board.dimensions();
                for position in session.state().board.positions() {
                    let cell = dimensions.cell_index(position).expect("a board position");
                    for defender in UnitKind::ALL {
                        let (was, now) = (
                            reused.immediate(cell, defender),
                            fresh.immediate(cell, defender),
                        );
                        assert_eq!(was, now, "immediate at {position} for {defender:?}");
                        let (was, now) = (
                            reused.deferred(cell, defender),
                            fresh.deferred(cell, defender),
                        );
                        assert_eq!(was, now, "deferred at {position} for {defender:?}");
                        compared += 1;
                        if now > 0.0 {
                            carried += 1;
                        }
                    }
                }
            }
        }
        assert!(compared > 10_000, "compared only {compared} rows");
        assert!(
            carried > 0,
            "every build read zero everywhere, so this proves nothing"
        );
    }

    /// An unchanged unit keeps the contribution buffers made for it.
    #[test]
    fn an_unchanged_unit_reuses_its_cached_contribution() {
        let (state, ours, _) = one_enemy(UnitKind::Tank);
        let session = Session::new(state);
        let mut map = ThreatMap::new();
        map.build(&session, ours);
        let before = map.cached[0].layers[Layer::Immediate.index()].as_ptr();

        map.build(&session, ours);

        let after = map.cached[0].layers[Layer::Immediate.index()].as_ptr();
        assert_eq!(
            before, after,
            "an unchanged unit must keep its contribution"
        );

        let mut fresh = ThreatMap::new();
        fresh.build(&session, ours);
        let dimensions = session.state().board.dimensions();
        for position in dimensions.positions() {
            let cell = dimensions.cell_index(position).expect("a board position");
            for defender in UnitKind::ALL {
                assert_eq!(
                    map.immediate(cell, defender),
                    fresh.immediate(cell, defender)
                );
                assert_eq!(map.deferred(cell, defender), fresh.deferred(cell, defender));
            }
        }
    }

    /// A direct unit moves and fires in one turn, so its threat is the tiles
    /// beside everywhere it can stop, and it defers nothing.
    #[test]
    fn a_direct_unit_threatens_beside_everywhere_it_can_reach() {
        let (state, ours, origin) = one_enemy(UnitKind::Tank);
        let session = Session::new(state);
        let state = session.state();
        let mut map = ThreatMap::new();
        map.build(&session, ours);

        let cell = |position: Pos| {
            state
                .board
                .dimensions()
                .cell_index(position)
                .expect("the position is on the board")
        };
        let beside = origin
            .orthogonal()
            .next()
            .expect("the origin has a neighbour");
        assert!(
            map.immediate(cell(beside), UnitKind::Infantry) > 0.0,
            "a tank threatens the tile beside it"
        );

        // A tank moves six, so a tile seven away is past everywhere it can
        // stop and past everything beside that.
        let far = state
            .board
            .positions()
            .find(|position| origin.distance(*position) > 7)
            .expect("the arena is wider than seven tiles");
        assert_eq!(map.immediate(cell(far), UnitKind::Infantry), 0.0);

        assert!(
            state
                .board
                .positions()
                .all(|position| map.deferred(cell(position), UnitKind::Infantry) == 0.0),
            "a direct unit defers nothing"
        );
    }

    /// An indirect unit fires from where it stands, and from anywhere it
    /// walks to only on the turn after it walks there. The two go into
    /// different layers.
    #[test]
    fn an_indirect_unit_defers_the_ring_it_must_walk_to() {
        let (state, ours, origin) = one_enemy(UnitKind::Artillery);
        let session = Session::new(state);
        let state = session.state();
        let range = ruleset::profile(UnitKind::Artillery)
            .indirect_range
            .expect("an artillery fires at range");
        let mut map = ThreatMap::new();
        map.build(&session, ours);

        let cell = |position: Pos| {
            state
                .board
                .dimensions()
                .cell_index(position)
                .expect("the position is on the board")
        };
        let at = |position: Pos| map.immediate(cell(position), UnitKind::Infantry);

        // Nothing inside the minimum range, including the tile beside it,
        // and something out at the maximum.
        let beside = origin
            .orthogonal()
            .next()
            .expect("the origin has a neighbour");
        assert_eq!(at(beside), 0.0, "an artillery cannot fire at its own feet");

        let in_range = state
            .board
            .positions()
            .find(|position| origin.distance(*position) == range.maximum)
            .expect("the arena holds a tile at the artillery's range");
        assert!(
            at(in_range) > 0.0,
            "the ring it stands in is under fire now"
        );

        // The tile it must first walk to is not under fire yet, but it is
        // deferred: an artillery moves or it fires, never both.
        let walked = state
            .board
            .positions()
            .find(|position| {
                let distance = origin.distance(*position);
                distance > range.maximum && map.deferred(cell(*position), UnitKind::Infantry) > 0.0
            })
            .expect("an artillery that can move defers a wider ring");
        assert_eq!(
            at(walked),
            0.0,
            "a tile only a walked-to firing position reaches is not immediate"
        );
    }
}
