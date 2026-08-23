//! An agent that scores every legal play and takes the best one.
//!
//! One decision at a time, and the best one it can name from the position in
//! front of it. There is no plan for the turn and no search: the harness asks
//! again after every command, and the agent scores the new action space from
//! nothing. A unit that acts is spent, so a turn is the sequence of plays this
//! policy ranked highest, most valuable first.
//!
//! The ranking is a weighted sum, and the weights are ordered so that the
//! objectives above dominate the objectives below:
//!
//! 1. Capture a property that produces land units, and the nearer the better.
//! 2. Capture a property that produces air units.
//! 3. Finish a capture of a property that pays income.
//! 4. Unit value, in funds, won and lost in an exchange.
//! 5. Unit count.
//! 6. Capture a property that produces naval units.
//! 7. A capture that can finish within two turns, weighted heavily.
//!
//! Proximity is a decay over Manhattan distance rather than over movement
//! cost. A shortest-path field for each target on each play is what tier 2
//! pays for; a straight-line decay puts a unit on the tile of its reachable
//! set that is nearest its best target, which is the same answer on open
//! ground and a slightly worse one across a mountain.
//!
//! Against that sum the agent prices what a tile is exposed to. The threat
//! map ([`crate::threat`]) says what the enemy can take off each tile, by the
//! kind of unit that stands there, and every arrival pays it. That is what
//! stops the agent parking an artillery where a tank can reach it, and it is
//! what prices the reply to a trade: a tank that kills a weak soldier and
//! stops where three tanks can answer made a bad exchange, and the forecast
//! alone scores it as a good one.

use awvm::combat::Forecast;
use awvm::ruleset::{self, Terrain, TerrainTrait, UnitKind};
use awvm::semantic::{
    CellIdx, Location, Observation, PlayerIdx, Pos, State, Tile, TileOwner, Unit,
};
use awvm::session::{Legal, Order, OrderKind, Session, UnitIdx};

use crate::agent::{Agent, Play};
use crate::rng::Rng;
use crate::threat::{self, ThreatMap};

/// What each objective is worth, in one place.
///
/// Every field is a score rather than a quantity, and only the ratios between
/// them mean anything. They are listed in the order of the priorities the
/// agent plays to, and a field below is smaller than the field above it by
/// enough that no sum of the lower ones outranks a single higher one.
#[derive(Clone, Copy, Debug)]
pub struct Weights {
    /// A headquarters, once a unit is standing on it. Completing this capture
    /// wins the match outright, so it is not on the same scale as the
    /// properties below it and is not meant to be.
    pub hq: f64,
    /// A headquarters, as a place to walk to.
    ///
    /// The walk is scored separately from the capture, and far lower. An
    /// infantry that arrives alone at a defended headquarters captures
    /// nothing, so the match-winning weight belongs to the capture it
    /// completes and not to every tile pointed at it. At this weight the
    /// enemy headquarters outranks a base of the same distance and loses to
    /// one that is two tiles nearer.
    pub hq_approach: f64,
    /// A property that produces land units: the base.
    pub land: f64,
    /// A property that produces air units: the airport.
    pub air: f64,
    /// A property that pays income and produces nothing: the city.
    pub income: f64,
    /// A property that produces naval units: the port.
    pub naval: f64,
    /// A capturable property that neither produces nor pays: the com tower
    /// and the lab. The priorities do not name these, so they sit between the
    /// income properties and the naval ones.
    pub other_property: f64,

    /// How much of a property's weight standing on it and capturing is worth.
    pub capture: f64,
    /// The share of a property's weight added when the capture completes on
    /// this play. This is the "finish" in "finish capturing income": the
    /// property pays from the turn it changes hands, and a capture left half
    /// done pays nothing at all.
    pub capture_completion: f64,
    /// The share added when the capture cannot complete now but will complete
    /// on the next turn. Heavily weighted on purpose: it is the difference
    /// between a unit that is two turns from paying and one that is three.
    pub capture_two_turn: f64,
    /// The decay for each tile of Manhattan distance between a tile and a
    /// property, which is what "in close proximity" means here.
    pub proximity_decay: f64,

    /// One point of funds, as a score. This prices an exchange: damage dealt
    /// is the defender's cost scaled by the health removed, and damage taken
    /// is the same for the attacker.
    pub funds: f64,
    /// A unit gained or lost, on top of what it is worth in funds. This is
    /// what makes two half-health kills beat one whole-health one.
    pub unit_count: f64,
    /// What a strike is worth when the position cannot forecast it, as a
    /// share of the target's cost. A fogged defender's health is unknown, and
    /// a forecast against a guessed health would be a lie.
    pub blind_attack: f64,

    /// A capture-capable unit that has no property to walk to is worth less
    /// than one that has. This is added for each property that is neither
    /// held nor already claimed by a capturer of ours, over the number of
    /// capturers we hold, so it falls to nothing once the board is covered.
    pub capturer_shortfall: f64,
    /// The pull a unit that cannot capture feels toward the nearest enemy,
    /// scaled by that enemy's cost. Without it an army with no target in
    /// range stands still, and a greedy agent that never closes never trades.
    pub advance: f64,
    /// Stopping an enemy capturer, as a share of the weight of the property
    /// it stands on.
    ///
    /// A property is worth what it is worth whichever way it changes hands,
    /// so this is priced against the same weights a capture of ours is. It is
    /// what makes an enemy soldier standing on our base worth killing, when
    /// the funds it costs to replace say it is worth almost nothing: an
    /// infantry is 1000 funds and a base is not.
    ///
    /// The headquarters needs no special arm. `property_weight` already
    /// answers `hq` for it, so a soldier one turn from taking our
    /// headquarters outranks every other play on the board, which is what it
    /// is.
    pub deny: f64,
    /// The share of that when the property is neutral rather than ours.
    ///
    /// Denying a neutral property is worth less than holding one we already
    /// have: they gain it, but we have not lost anything we held.
    pub deny_neutral: f64,
    /// The decay for each further turn an enemy needs to finish its capture.
    ///
    /// A soldier that completes on its next turn is the emergency. One that
    /// needs four more turns is a problem for a later turn, and the board may
    /// answer it without a strike.
    pub deny_decay: f64,

    /// One point of funds a tile is exposed to, as a score. It is priced
    /// level with a point of funds won in an exchange: ground the enemy can
    /// take a whole tank off is worth as much to avoid as a whole tank is
    /// worth to kill.
    pub threat: f64,
    /// The share of the deferred threat layer that is counted.
    ///
    /// The deferred layer is the ring an indirect unit opens only after it
    /// has spent a turn walking to a firing position, so it is ground that is
    /// safe to hold for one turn. Counting it whole gives an agent that never
    /// closes on an artillery, which is the one thing that answers one.
    pub deferred_threat: f64,

    /// A commander power, whenever the position offers one. The power is
    /// worth more on some turns than on others, and reading which is a term
    /// this tier does not carry.
    pub power: f64,
    /// Resupplying the units around an APC.
    pub supply: f64,
}

impl Weights {
    /// The ordering the agent was asked for.
    pub const DEFAULT: Self = Self {
        hq: 100_000.0,
        hq_approach: 2_000.0,
        land: 1_000.0,
        air: 800.0,
        income: 600.0,
        naval: 100.0,
        other_property: 300.0,

        capture: 1.0,
        capture_completion: 0.8,
        capture_two_turn: 0.5,
        proximity_decay: 0.75,

        funds: 0.02,
        unit_count: 20.0,
        blind_attack: 0.3,

        capturer_shortfall: 150.0,
        advance: 0.01,
        deny: 1.0,
        deny_neutral: 0.5,
        deny_decay: 0.5,
        threat: 0.02,
        deferred_threat: 0.35,
        power: 200.0,
        supply: 10.0,
    };

    /// The threat map, but with an enemy capture priced only by what the
    /// capturer costs to replace.
    ///
    /// The baseline the denial term is measured against.
    pub const WITHOUT_DENIAL: Self = Self {
        deny: 0.0,
        ..Self::DEFAULT
    };

    /// The same ranking again with what a tile is exposed to priced at
    /// nothing.
    ///
    /// This is what tier 1 landed with: neither the threat map nor the
    /// denial term. It is the baseline the arena measures the threat map
    /// against, and it holds one term less than [`Weights::WITHOUT_DENIAL`]
    /// so that the pair measures the threat map and nothing else. Keeping it
    /// named rather than deleted is what lets a later change be compared with
    /// the score that is already published rather than with a rebuilt
    /// approximation of it.
    pub const THREATLESS: Self = Self {
        threat: 0.0,
        deferred_threat: 0.0,
        ..Self::WITHOUT_DENIAL
    };
}

impl Default for Weights {
    fn default() -> Self {
        Self::DEFAULT
    }
}

pub struct GreedyAgent {
    /// Ties are common — a mirror board answers the same score from two
    /// tiles — and breaking them the same way every time makes an agent that
    /// walks one flank. The draw is seeded, so a game still repeats.
    rng: Rng,
    weights: Weights,
    /// Held across calls so that a turn's enumeration reuses one allocation.
    orders: Vec<Order>,
    /// The pull each tile feels toward the properties worth capturing, and the
    /// pull it feels toward the enemy. One entry for each tile of the board,
    /// rebuilt once for each play rather than once for each candidate.
    capture_field: Vec<f64>,
    advance_field: Vec<f64>,
    /// `proximity_decay` raised to each distance the board can hold, so that
    /// building the fields is a multiply rather than a power.
    decay: Vec<f64>,
    /// The unit standing on each tile, by its index in the roster.
    occupant: Vec<Option<u16>>,
    /// What the enemy can take off each tile. Built once for each position,
    /// which is once for each play: the harness applies one command between
    /// calls, and a command moves a unit, so a map held across calls would be
    /// a map of a board that is gone.
    threat: ThreatMap,
}

impl GreedyAgent {
    pub const fn from_seed(seed: u64) -> Self {
        Self::with_weights(seed, Weights::DEFAULT)
    }

    pub const fn with_weights(seed: u64, weights: Weights) -> Self {
        Self {
            rng: Rng::from_seed(seed),
            weights,
            orders: Vec::new(),
            capture_field: Vec::new(),
            advance_field: Vec::new(),
            decay: Vec::new(),
            occupant: Vec::new(),
            threat: ThreatMap::new(),
        }
    }

    pub const fn weights(&self) -> &Weights {
        &self.weights
    }
}

impl Agent for GreedyAgent {
    fn act(&mut self, view: &Observation) -> Option<Play> {
        // A projection this player may not act on reports nothing legal, so
        // the session answers the question rather than an error path.
        let session = Session::from_observation(view).ok()?;
        if !session.is_commandable() {
            return None;
        }
        let state = session.state();
        let seat = state.players.seat(&state.turn.active_player)?;

        let Self {
            rng,
            weights,
            orders,
            capture_field,
            advance_field,
            decay,
            occupant,
            threat,
        } = self;

        let board = Board {
            state,
            seat,
            weights,
        };
        board.decay_table(decay);
        board.occupants(occupant);
        board.capture_field(capture_field, decay, occupant);
        board.advance_field(advance_field, decay);
        // A map priced at nothing is never read, so it is never built. That
        // is what keeps the threatless weighting at the throughput it was
        // measured at, and it is what makes the two a comparison of one term.
        if weights.threat != 0.0 {
            threat.build(state, seat);
        }

        orders.clear();
        session.legal().orders(orders);

        let legal = session.legal();
        let scorer = Scorer {
            board: &board,
            legal: &legal,
            capture_field,
            advance_field,
            occupant,
            threat,
            shortfall: board.capturer_shortfall(occupant),
        };

        // The best play, with ties drawn uniformly: a running count of how
        // many orders have shared the best score so far is a reservoir over
        // exactly those orders.
        let mut best = 0.0;
        let mut tied = 0u64;
        let mut chosen = None;
        for order in orders.iter().copied() {
            let score = scorer.score(order);
            if score > best {
                best = score;
                tied = 1;
                chosen = Some(order);
            } else if score == best && chosen.is_some() {
                tied += 1;
                if rng.below(tied) == 0 {
                    chosen = Some(order);
                }
            }
        }

        // Nothing scores above zero when every unit has acted and nothing is
        // left worth doing, which is what ends the turn.
        Play::from_order(&session, chosen?)
    }
}

/// The board as the scoring reads it, with the seat that is asking.
struct Board<'a> {
    state: &'a State,
    seat: PlayerIdx,
    weights: &'a Weights,
}

impl Board<'_> {
    fn cells(&self) -> usize {
        self.state.board.dimensions().len()
    }

    fn cell(&self, position: Pos) -> Option<usize> {
        self.state
            .board
            .dimensions()
            .cell_index(position)
            .map(|cell| usize::from(cell.get()))
    }

    fn position(&self, cell: CellIdx) -> Option<Pos> {
        self.state.board.dimensions().position_of(cell)
    }

    /// Whether `seat` is one this player is at war with.
    fn hostile(&self, other: PlayerIdx) -> bool {
        threat::hostile(self.state, self.seat, other)
    }

    /// What one property is worth as a place to walk to.
    ///
    /// The same as [`Board::property_weight`] except for the headquarters,
    /// which is worth the match when it is captured and one more property
    /// while it is still being walked at.
    fn approach_weight(&self, terrain: Terrain) -> f64 {
        if ruleset::terrain_has(terrain, TerrainTrait::CaptureDefeatsOwner) {
            self.weights.hq_approach
        } else {
            self.property_weight(terrain)
        }
    }

    /// What one property is worth to this agent, before proximity.
    ///
    /// The arms are tested in the order of the priorities, so a property that
    /// carries several traits is scored by the highest of them: an airport
    /// pays income as well, and is worth the air weight rather than the
    /// income one.
    fn property_weight(&self, terrain: Terrain) -> f64 {
        let has = |value| ruleset::terrain_has(terrain, value);
        if has(TerrainTrait::CaptureDefeatsOwner) {
            self.weights.hq
        } else if has(TerrainTrait::ProducesGround) {
            self.weights.land
        } else if has(TerrainTrait::ProducesAir) {
            self.weights.air
        } else if has(TerrainTrait::ProducesSea) {
            self.weights.naval
        } else if has(TerrainTrait::Income) {
            self.weights.income
        } else {
            self.weights.other_property
        }
    }

    /// Whether the property on this tile is one this player may capture.
    fn capturable(&self, tile: &Tile) -> bool {
        if !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
            return false;
        }
        match tile.owner {
            TileOwner::NotOwnable => false,
            TileOwner::Neutral => true,
            TileOwner::Owned(holder) => self.hostile(holder),
        }
    }

    /// `proximity_decay` for each distance this board can hold.
    fn decay_table(&self, out: &mut Vec<f64>) {
        let span = usize::from(self.state.board.width()) + usize::from(self.state.board.height());
        out.clear();
        let mut value = 1.0;
        for _ in 0..=span {
            out.push(value);
            value *= self.weights.proximity_decay;
        }
    }

    /// The unit standing on each tile, by roster index.
    fn occupants(&self, out: &mut Vec<Option<u16>>) {
        out.clear();
        out.resize(self.cells(), None);
        for (index, unit) in self.state.units.iter().enumerate() {
            let Location::Board { position } = unit.location else {
                continue;
            };
            if let (Some(cell), Ok(index)) = (self.cell(position), u16::try_from(index)) {
                out[cell] = Some(index);
            }
        }
    }

    /// How much each tile wants a capture-capable unit.
    ///
    /// A property already carrying a capturer of ours pulls nothing. Without
    /// that, every capturer on the board walks at the same property, and the
    /// ones that arrive second stand next to it doing nothing at all.
    fn capture_field(&self, out: &mut Vec<f64>, decay: &[f64], occupant: &[Option<u16>]) {
        out.clear();
        out.resize(self.cells(), 0.0);
        for (target, tile) in self.state.board.iter() {
            if !self.capturable(tile) {
                continue;
            }
            let Some(cell) = self.cell(target) else {
                continue;
            };
            if occupant[cell].is_some_and(|index| self.is_our_capturer(index)) {
                continue;
            }
            let weight = self.approach_weight(tile.terrain);
            for (from, value) in self.state.board.positions().zip(out.iter_mut()) {
                let pull = weight * decay[target.distance(from) as usize];
                if pull > *value {
                    *value = pull;
                }
            }
        }
    }

    /// How much each tile wants a unit that fights.
    ///
    /// The nearest enemy, priced by what it costs to build. This is the term
    /// that keeps an army walking when nothing is in range of it yet.
    fn advance_field(&self, out: &mut Vec<f64>, decay: &[f64]) {
        out.clear();
        out.resize(self.cells(), 0.0);
        for unit in self.state.units.iter() {
            let Location::Board { position: target } = unit.location else {
                continue;
            };
            if !self.hostile(unit.owner) {
                continue;
            }
            let weight = self.weights.advance * cost(unit.kind) * health(unit);
            for (from, value) in self.state.board.positions().zip(out.iter_mut()) {
                let pull = weight * decay[target.distance(from) as usize];
                if pull > *value {
                    *value = pull;
                }
            }
        }
    }

    fn is_our_capturer(&self, index: u16) -> bool {
        self.state
            .units
            .at(usize::from(index))
            .is_some_and(|unit| unit.owner == self.seat && ruleset::profile(unit.kind).can_capture)
    }

    /// The properties this player could still take, less the capturers it
    /// already holds.
    ///
    /// This is what makes an infantry worth building. It falls as the board
    /// fills, so production moves to the units that fight once every property
    /// has somebody walking at it.
    fn capturer_shortfall(&self, occupant: &[Option<u16>]) -> f64 {
        let open = self
            .state
            .board
            .iter()
            .filter(|(position, tile)| {
                self.capturable(tile)
                    && self.cell(*position).is_some_and(|cell| {
                        !occupant[cell].is_some_and(|index| self.is_our_capturer(index))
                    })
            })
            .count();
        let capturers = self
            .state
            .units
            .iter()
            .filter(|unit| unit.owner == self.seat && ruleset::profile(unit.kind).can_capture)
            .count();
        f64::from(u32::try_from(open.saturating_sub(capturers)).unwrap_or(u32::MAX))
    }
}

/// What one unit costs to replace, in funds.
fn cost(kind: UnitKind) -> f64 {
    ruleset::profile(kind).cost as f64
}

/// A unit's health as a share of a whole one.
fn health(unit: &Unit) -> f64 {
    f64::from(unit.hp) / 100.0
}

/// Capture progress one turn of this unit puts into a property.
///
/// The ruleset spends the unit's visible health, so a unit at half health
/// takes twice as long. Commander capture multipliers are not read here; the
/// two commanders that carry one are a term this tier does not model.
fn capture_progress(unit: &Unit) -> u8 {
    unit.hp.div_ceil(10)
}

/// The `capture_points` of a property nobody is capturing.
const WHOLE_PROPERTY: u8 = 20;

/// What one turn of capturing a property of `weight` is worth.
///
/// `points` is what the property has left and `progress` is what this unit
/// takes off it this turn. The two bonuses are the whole of the "finish"
/// clause of the priorities: a capture that completes now pays from this turn,
/// one that completes on the next turn pays soon, and one that leaves the
/// property still standing pays nothing yet.
fn capture_value(weights: &Weights, weight: f64, points: u8, progress: u8) -> f64 {
    let remaining = points.saturating_sub(progress);
    let bonus = if remaining == 0 {
        weights.capture_completion
    } else if remaining <= progress {
        weights.capture_two_turn
    } else {
        0.0
    };
    weight * (weights.capture + bonus)
}

/// Everything one play is scored against.
struct Scorer<'a> {
    board: &'a Board<'a>,
    legal: &'a Legal<'a>,
    capture_field: &'a [f64],
    advance_field: &'a [f64],
    occupant: &'a [Option<u16>],
    threat: &'a ThreatMap,
    shortfall: f64,
}

impl Scorer<'_> {
    fn weights(&self) -> &Weights {
        self.board.weights
    }

    fn unit(&self, index: UnitIdx) -> Option<&Unit> {
        self.board.state.units.at(usize::from(index.get()))
    }

    fn unit_at(&self, cell: CellIdx) -> Option<&Unit> {
        let index = self.occupant.get(usize::from(cell.get()))?.as_ref()?;
        self.board.state.units.at(usize::from(*index))
    }

    /// What one order is worth. Zero is the floor: an order worth nothing is
    /// one the agent would rather not give, and a turn ends when every order
    /// left is worth nothing.
    fn score(&self, order: Order) -> f64 {
        match order.kind() {
            OrderKind::Capture => self.capture(order),
            OrderKind::Attack(target) => self.attack(order, target),
            OrderKind::Wait | OrderKind::Unload(_) => self.arrival(order),
            // Both cost a unit from the roster and give its health to another,
            // so both are scored as the arrival less what the count is worth.
            OrderKind::Join | OrderKind::Load => self.arrival(order) - self.weights().unit_count,
            OrderKind::Supply => self.arrival(order) + self.weights().supply,
            OrderKind::Produce(kind) => self.produce(kind),
            OrderKind::Power(_) => self.weights().power,
            // Nothing below is scored. Resignation and deletion decide a game
            // for a reason no policy holds; ending the turn is what `None`
            // means, so an order for it would end a turn twice; and the rest
            // are plays this tier cannot price — a launch and an explosion
            // both need the damage over an area, and hiding needs to know
            // what is hunting.
            OrderKind::Delete
            | OrderKind::Resign
            | OrderKind::EndTurn
            | OrderKind::Tag
            | OrderKind::Explode
            | OrderKind::Hide
            | OrderKind::Reveal
            | OrderKind::Repair(_)
            | OrderKind::Launch(_) => 0.0,
        }
    }

    /// Standing on a property and turning the crank.
    fn capture(&self, order: Order) -> f64 {
        let Some(unit) = order.unit().and_then(|index| self.unit(index)) else {
            return 0.0;
        };
        let Some(position) = self.board.position(order.destination()) else {
            return 0.0;
        };
        let Some(tile) = self.board.state.board.get(position) else {
            return 0.0;
        };

        capture_value(
            self.weights(),
            self.board.property_weight(tile.terrain),
            tile.capture_points.unwrap_or(WHOLE_PROPERTY),
            capture_progress(unit),
        )
    }

    /// An exchange, priced in funds and in units.
    fn attack(&self, order: Order, target: CellIdx) -> f64 {
        let Some(index) = order.unit() else {
            return 0.0;
        };
        let (Some(attacker), Some(defender)) = (self.unit(index), self.unit_at(target)) else {
            return 0.0;
        };
        let weights = self.weights();

        let Some(forecast) = self.legal.forecast(index, order.destination(), target) else {
            // A fogged defender has no exact health, so there is nothing to
            // forecast against. A strike is still usually worth making, and
            // this says so without pretending to know how much.
            return weights.blind_attack * weights.funds * cost(defender.kind) + self.pull(order);
        };

        let mean = |low: u16, high: u16| f64::from(low + high) / 2.0;
        let dealt = mean(forecast.attack.low, forecast.attack.high)
            .min(f64::from(forecast.target_hp))
            / 100.0;
        let taken = forecast
            .counter
            .map(|counter| {
                mean(counter.low, counter.high).min(f64::from(forecast.attacker_hp)) / 100.0
            })
            .unwrap_or(0.0);

        let mut score = weights.funds * (dealt * cost(defender.kind) - taken * cost(attacker.kind));
        // A kill is worth the count as well as the funds, and only the worst
        // roll proves one. The best roll destroying it is a hope.
        if f64::from(forecast.attack.low) >= f64::from(forecast.target_hp) {
            score += weights.unit_count;
        }
        if taken * 100.0 >= f64::from(forecast.attacker_hp) {
            score -= weights.unit_count;
        }
        score += self.denial(target, defender, &forecast);
        // The pull of the tile, and not the arrival: a strike is not charged
        // for what the tile it fires from is exposed to. The forecast above
        // already prices the reply, and charging the exposure on top prices
        // the same exchange twice. The arena is plain about it — the more of
        // the exposure an attack pays, the worse the agent plays, and it is
        // worst at the whole of it.
        score + self.pull(order)
    }

    /// What stopping this defender's capture is worth.
    ///
    /// Only a capture-capable enemy standing on a property counts, and only a
    /// property we would rather it did not have. The value is the property's
    /// own weight, scaled by how much later this strike makes the capture:
    /// destroying the unit resets the tile to whole (`reset_capture_on_removal`
    /// in `transition/attack.rs`), and merely damaging it slows the unit,
    /// because the ruleset spends visible health for capture points.
    ///
    /// The worst roll is what proves a kill, the same rule the unit count
    /// above plays by. A strike that only might destroy the capturer is
    /// scored as the damage it certainly does.
    fn denial(&self, target: CellIdx, defender: &Unit, forecast: &Forecast) -> f64 {
        let weights = self.weights();
        if !ruleset::profile(defender.kind).can_capture {
            return 0.0;
        }
        let Some(position) = self.board.position(target) else {
            return 0.0;
        };
        let Some(tile) = self.board.state.board.get(position) else {
            return 0.0;
        };
        if !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
            return 0.0;
        }
        // A property of ours is a loss. A neutral one is a gain we are denied.
        // One held by somebody else at war with us is neither.
        let stake = match tile.owner {
            TileOwner::Owned(holder) if holder == self.board.seat => 1.0,
            TileOwner::Neutral => weights.deny_neutral,
            _ => return 0.0,
        };

        let points = tile.capture_points.unwrap_or(WHOLE_PROPERTY);
        let before = self.urgency(points, capture_progress(defender));
        // A kill resets the tile, so the whole of the urgency goes away. The
        // worst roll is what proves one.
        let after = if f64::from(forecast.attack.low) >= f64::from(forecast.target_hp) {
            0.0
        } else {
            let dealt = u8::try_from(forecast.attack.low).unwrap_or(u8::MAX);
            self.urgency(
                points,
                forecast.target_hp.saturating_sub(dealt).div_ceil(10),
            )
        };

        weights.deny * stake * self.board.property_weight(tile.terrain) * (before - after)
    }

    /// How near a capture of `points` is, for a unit taking `progress` a turn.
    ///
    /// One at the next turn, and `deny_decay` for each turn after that. A unit
    /// that takes nothing a turn never finishes and is worth nothing to stop.
    fn urgency(&self, points: u8, progress: u8) -> f64 {
        if progress == 0 {
            return 0.0;
        }
        let turns = points.div_ceil(progress);
        self.weights()
            .deny_decay
            .powi(i32::from(turns.saturating_sub(1)))
    }

    /// What arriving on a tile is worth, whatever the unit does there.
    ///
    /// The pull of the destination, less what the enemy can take off it. This
    /// is the whole of the agent's movement.
    fn arrival(&self, order: Order) -> f64 {
        let Some(unit) = order.unit().and_then(|index| self.unit(index)) else {
            return 0.0;
        };
        self.pull(order) - self.exposure(order.destination(), unit)
    }

    /// How much the destination wants this unit, before any exposure.
    ///
    /// A unit that captures walks at properties; a unit that fights walks at
    /// the enemy. The destination of the reachable set with the strongest
    /// pull is where the unit goes.
    fn pull(&self, order: Order) -> f64 {
        let Some(unit) = order.unit().and_then(|index| self.unit(index)) else {
            return 0.0;
        };
        let cell = usize::from(order.destination().get());
        let field = if ruleset::profile(unit.kind).can_capture {
            self.capture_field
        } else {
            self.advance_field
        };
        field.get(cell).copied().unwrap_or(0.0)
    }

    /// What standing on `cell` costs this unit, in score.
    ///
    /// The two layers of the threat map are read apart and the deferred one
    /// is discounted, because ground an artillery can only shoot after it has
    /// walked to a firing position is ground this unit may safely hold for a
    /// turn. Merging them gives an agent that never closes.
    ///
    fn exposure(&self, cell: CellIdx, unit: &Unit) -> f64 {
        let weights = self.weights();
        let immediate = self.threat.immediate(cell, unit.kind);
        let deferred = self.threat.deferred(cell, unit.kind);
        weights.threat * (immediate + weights.deferred_threat * deferred)
    }

    /// What building this kind is worth.
    ///
    /// The unit is worth what it costs, less nothing: the funds it spends buy
    /// it, and a greedy agent that counted the price twice would never build
    /// anything. A capture-capable kind is worth more while properties are
    /// open that no capturer of ours is walking at.
    fn produce(&self, kind: UnitKind) -> f64 {
        let weights = self.weights();
        let profile = ruleset::profile(kind);
        let mut score = weights.funds * cost(kind) + weights.unit_count;
        if profile.can_capture {
            score += weights.capturer_shortfall * self.shortfall;
        }
        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::arena;
    use awvm::semantic::{AwbwVisibility, Concealment, UnitAction, UnitId, observe};

    /// One unit of `kind`, at whole health, standing where it is put.
    fn test_unit(kind: UnitKind, position: Pos, owner: PlayerIdx) -> Unit {
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

    /// The priorities, read back off the weights.
    #[test]
    fn the_weights_rank_the_objectives_in_the_order_they_were_given() {
        let weights = Weights::DEFAULT;
        assert!(weights.land > weights.air, "land units before air units");
        assert!(
            weights.air > weights.income,
            "air units before plain income"
        );
        assert!(
            weights.income > weights.naval,
            "income before naval production"
        );
        // A capture that wins the match is not one objective among several.
        assert!(weights.hq > weights.land * 10.0);
        // The walk toward a headquarters is, and it beats a base two tiles
        // further away and loses to one two tiles nearer.
        assert!(weights.hq_approach > weights.land);
        assert!(weights.hq_approach < weights.land / weights.proximity_decay.powi(3));
    }

    /// The two capture bonuses, in the order the priorities put them.
    #[test]
    fn a_capture_is_worth_more_the_sooner_it_finishes() {
        let weights = Weights::DEFAULT;
        let value = |points, progress| capture_value(&weights, weights.income, points, progress);

        // A whole soldier takes ten points off a whole property, so it
        // finishes on the second turn.
        let finishes_now = value(10, 10);
        let finishes_next_turn = value(WHOLE_PROPERTY, 10);
        // A soldier at three health takes three points off, which is five
        // turns away from a property nobody has started.
        let slow = value(WHOLE_PROPERTY, 3);

        assert!(finishes_now > finishes_next_turn);
        assert!(finishes_next_turn > slow);
        // The heavy weighting the priorities asked for: two turns is worth
        // half a property more than a capture that takes longer.
        assert!(finishes_next_turn - slow >= weights.income * 0.4);
    }

    /// The first play of the match, on a board that starts with no units.
    ///
    /// Nothing can move, so production is the whole action space, and every
    /// property on the board is open. That is the position the capturer
    /// shortfall was written for: the answer is a soldier, not the most
    /// expensive vehicle a base will sell.
    #[test]
    fn the_first_play_of_the_arena_buys_a_capturer() {
        let state = arena(false, 1);
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes their own position");
        let play = GreedyAgent::from_seed(1)
            .act(&view)
            .expect("the opening position offers a play");

        let OrderKind::Produce(kind) = play.kind() else {
            panic!("the opening play was {:?}, not a build", play.kind());
        };
        assert!(
            ruleset::profile(kind).can_capture,
            "the opening build was a {kind:?}, which captures nothing"
        );
    }

    /// The emergency the priorities did not name: an enemy soldier standing
    /// on our headquarters, one turn from winning the match.
    ///
    /// Tier 1 scored that strike at what an infantry costs to replace, which
    /// is 1000 funds and loses to walking one tile nearer a property. So the
    /// one unit that could answer spent its turn capturing something else and
    /// the headquarters fell. Production is not what was in the way — a build
    /// costs no unit its turn, and the agent still built. What was in the way
    /// is that a unit acts once, and capture outbid defence.
    ///
    /// This drives the whole turn rather than reading the first play, because
    /// the first play is not the question. The question is what the one unit
    /// beside the headquarters did with its turn.
    #[test]
    fn the_unit_beside_our_headquarters_answers_instead_of_walking_away() {
        let (mut session, guard, raider) = headquarters_under_capture();
        let mut agent = GreedyAgent::from_seed(1);
        let mut entropy = Rng::from_seed(7);
        let mut answered = false;

        for _ in 0..12 {
            let state = session.state();
            let Ok(view) = observe(&AwbwVisibility, state, &state.turn.active_player) else {
                break;
            };
            let Some(play) = agent.act(&view) else { break };
            if play.unit() == Some(guard) {
                answered = matches!(play.kind(), OrderKind::Attack(_));
            }
            let Some(command) = play.command(&session) else {
                break;
            };
            if session
                .apply_command::<()>(command, &mut entropy, &mut ())
                .is_err()
            {
                break;
            }
        }

        assert!(
            answered,
            "the unit beside our headquarters spent its turn on something \
             other than the soldier taking it"
        );
        // The strike does not have to kill. A damaged capturer takes fewer
        // capture points a turn, which is the whole of what denial buys here.
        assert!(
            session
                .state()
                .units
                .get(raider)
                .is_some_and(|unit| unit.hp < 100),
            "the raider was not touched"
        );
    }

    /// Our headquarters, one enemy turn from falling, and one unit of ours
    /// beside it. Every other unit is off the board so that the position asks
    /// exactly one question.
    fn headquarters_under_capture() -> (Session, UnitId, UnitId) {
        let mut state = arena(false, 1);
        let ours = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        let theirs = state
            .players
            .seats()
            .map(|(seat, _)| seat)
            .find(|seat| *seat != ours)
            .expect("the arena seats two players");

        let hq = state
            .board
            .iter()
            .find(|(_, tile)| {
                ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner)
                    && tile.owner == TileOwner::Owned(ours)
            })
            .map(|(position, _)| position)
            .expect("the arena gives each seat a headquarters");
        let beside = hq
            .orthogonal()
            .find(|position| {
                state.board.get(*position).is_some_and(|tile| {
                    ruleset::movement_cost(
                        tile.terrain,
                        state.weather.kind,
                        ruleset::profile(UnitKind::Infantry).movement_class,
                    )
                    .is_some()
                })
            })
            .expect("the headquarters has a walkable neighbour");

        let (guard, raider) = (UnitId::new(9_002), UnitId::new(9_001));
        state.units.retain(|_| false);
        let mut soldier = test_unit(UnitKind::Infantry, hq, theirs);
        soldier.id = raider;
        state.units.push(soldier);
        let mut ours_soldier = test_unit(UnitKind::Infantry, beside, ours);
        ours_soldier.id = guard;
        state.units.push(ours_soldier);
        state.board.tile_mut(hq).capture_points = Some(10);

        (Session::new(state), guard, raider)
    }

    /// The urgency ramp, and what it is multiplied by.
    #[test]
    fn a_capture_is_worth_more_to_stop_the_sooner_it_lands() {
        let weights = Weights::DEFAULT;
        // A property is worth what it is worth whichever way it changes hands,
        // so stopping a capture of our base is priced against making one.
        assert_eq!(weights.deny, weights.capture);
        // Denying a neutral property is worth less than keeping one we hold.
        assert!(weights.deny_neutral < 1.0);
        // Each further turn the enemy needs is worth less.
        assert!(weights.deny_decay < 1.0);
        // The headquarters needs no arm of its own: it carries `hq`, which is
        // not on the same scale as the properties below it.
        assert!(weights.hq > weights.land * 10.0);
    }

    /// The guard that skips an unpriced map must be invisible.
    ///
    /// The threatless weighting is the baseline every measurement of the
    /// threat map is read against, so it has to play the game tier 1 played.
    /// A map priced at nothing and a map never built must give the same game.
    #[test]
    fn a_map_priced_at_nothing_changes_no_decision() {
        use crate::harness::{Limits, play};
        use awvm::session::Session;

        fn game(weights: Weights) -> String {
            let mut first = GreedyAgent::with_weights(1, weights);
            let mut second = GreedyAgent::with_weights(2, weights);
            let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];
            let state = arena(false, 1);
            let mut session = Session::new(state.clone());
            let record = play(
                state,
                &mut session,
                &mut agents,
                &mut Rng::from_seed(3),
                Limits {
                    days: 20,
                    ..Limits::default()
                },
            );
            let end = session.state();
            format!(
                "{:?} {} {:?}",
                record.days,
                end.units.iter().count(),
                end.players
                    .seats()
                    .map(|(_, player)| player.funds)
                    .collect::<Vec<_>>()
            )
        }

        // A threat priced too low to outrank anything still builds the map
        // and still reads it, which is the half of the guard under test.
        let read = game(Weights {
            threat: f64::MIN_POSITIVE,
            deferred_threat: 0.0,
            ..Weights::THREATLESS
        });
        assert_eq!(game(Weights::THREATLESS), read);
    }

    /// A board with nothing left to take is a board that builds soldiers no
    /// faster than it needs them.
    #[test]
    fn production_stops_favouring_capturers_once_the_board_is_covered() {
        let weights = Weights::DEFAULT;
        let with_shortfall = weights.capturer_shortfall * 4.0;
        // An infantry against a medium tank, with four properties open and
        // with none.
        let soldier = weights.funds * cost(UnitKind::Infantry) + weights.unit_count;
        let tank = weights.funds * cost(UnitKind::MdTank) + weights.unit_count;
        assert!(
            soldier + with_shortfall > tank,
            "open properties buy soldiers"
        );
        assert!(tank > soldier, "a covered board buys what fights");
    }
}
