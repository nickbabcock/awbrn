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
//!
//! Under fog it also prices what a tile does to what either side can see. The
//! vision map ([`crate::vision`]) says which tiles are dark, so a play is
//! scored for the dark it lights — the mountain a soldier climbs, the ground a
//! recon covers — and for the cover it takes, which is the woods. Both are
//! zero with the board lit, and the map is not built there.

use awvm::combat::{self, Forecast, Side};
use awvm::commander;
use awvm::query::{self};
use awvm::ruleset::{self, MovementClass, Terrain, TerrainTrait, UnitKind};
use awvm::semantic::{
    CellIdx, Location, Observation, PlayerIdx, Pos, State, Tile, TileOwner, Unit,
};
use awvm::session::{Legal, Order, OrderKind, Session, UnitIdx};

use crate::agent::{Agent, NodeBudget, Play};
use crate::map::ContestMap;
use crate::rng::Rng;
use crate::threat::{self, ThreatMap};
use crate::vision::{Needs, VisionMap};

/// What each objective is worth, in one place.
///
/// Every field is a score rather than a quantity, and only the ratios between
/// them mean anything. They are listed in the order of the priorities the
/// agent plays to, and a field below is smaller than the field above it by
/// enough that no sum of the lower ones outranks a single higher one.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
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
    /// completes and not to every tile pointed at it. At zero, which is the
    /// Amber Valley baseline, the headquarters has no
    /// general approach pull: it matters when a capturer can finish the
    /// capture, not merely because the tile is the match-winning objective.
    /// Positive values can restore an HQ rush when a different board calls
    /// for one.
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
    /// The decay for each step between a tile and what pulls it, which is
    /// what "in close proximity" means here.
    ///
    /// The two fields do not measure a step the same way. [`CaptureFields`]
    /// measures turns, through the movement points a route really costs;
    /// [`Board::advance_field`] measures tiles of Manhattan distance, because
    /// its targets are enemy units, which move and can be of any class. A turn
    /// is worth several tiles, so one number serving both is a compromise and
    /// is the first thing to split if the sweep asks for it.
    pub proximity_decay: f64,
    /// The decay for each turn the enemy reaches a property before we do.
    ///
    /// This is the whole of what the contest map ([`crate::map::ContestMap`])
    /// prices. The capture field measures the distance from us and nothing
    /// else, so a property four turns away behind the enemy headquarters and
    /// one four turns away behind ours pull the same. They are not the same
    /// property: our soldier arrives at the first of them after theirs has
    /// taken it, with a base of theirs behind it building what kills him.
    ///
    /// One at no discount, which is the reading the agent shipped with and
    /// which builds no map at all. Below one a property the enemy stands
    /// nearer to pulls less, one decay for each turn of the deficit, and the
    /// deficit is capped at [`crate::map::MAX_DEFICIT`] turns.
    pub contest_decay: f64,

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

    /// Standing on one of our own capturable properties, as a share of the
    /// weight of that property.
    ///
    /// This is the other half of [`Weights::deny`]. Denial pays to strike an
    /// enemy capturer that has already arrived; this pays to be there first.
    /// A property with a unit on it cannot be captured at all, so occupancy
    /// refuses a capture outright and costs nothing but a turn of that unit's
    /// time.
    ///
    /// The headquarters needs no special arm, exactly as denial needs none:
    /// `property_weight` already answers `hq` for it.
    pub hold: f64,
    /// The decay for each tile between the property and the nearest enemy
    /// unit that can capture it.
    ///
    /// Guarding everything is guarding nothing. A property no enemy capturer
    /// is near is worth almost nothing to stand on, and this is what says so
    /// without a cutoff that a unit one tile beyond steps over. Distance is
    /// measured in tiles, because step 5 measured turns against tiles for the
    /// capture field and the two tied.
    pub hold_decay: f64,

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

    /// One dark tile a play lights, as a score. **Fog only.**
    ///
    /// The count comes from the ruleset's own vision operators, so one weight
    /// prices every reading of vision the board holds: a soldier on a mountain
    /// counts the mountain's bonus, a recon counts its five tiles against a
    /// tank's three, and rain takes a tile off both. A tile that is already
    /// lit counts for nothing, so this pays to walk into the dark and not to
    /// stand in the light.
    ///
    /// It is priced per tile, and a recon in the open lights twenty or more of
    /// them, so the weight that pays for one scouting move is far below what
    /// a property is worth.
    pub scout: f64,
    /// Ending a move on terrain that hides the unit standing on it: the woods
    /// and the reef. **Fog only.**
    ///
    /// The threat map cannot state this. It prices what the enemy can take off
    /// a tile if it goes there, and a unit the enemy cannot see is one it does
    /// not go at. This is the other half of the vision term: the first pays to
    /// see, and this pays not to be seen.
    pub conceal: f64,
    /// How much of a dark tile's worth is where it is. **Fog only.**
    ///
    /// Measured at nothing on this board. A sweep over `scout` under fog read
    /// 0.5292, 0.5583, 0.5458 and 0.5000 at a quarter, a half, three quarters
    /// and the whole of it over 60 pairs, and the plateau at a half read
    /// 0.5417, 0.5433 and 0.4717 over three seeds of 150 — three readings that
    /// do not agree, which is the file's own test of a term that is not there.
    /// It is kept at nothing rather than deleted because the mechanism is one
    /// file and a rerun, and because the map analysis prices the same ground
    /// from the other side.
    ///
    /// At nothing every dark tile counts one: the fog in front of our
    /// headquarters and the fog in the corner behind us are the same tile, and
    /// a recon that walks away from the game lights as many of them as one
    /// that walks into it. At one a dark tile is worth only its nearness to a
    /// property, which is the ground both sides have to stand on. Between the
    /// two the term is a blend of the two readings.
    pub scout_focus: f64,
    /// The decay for each tile between a dark tile and the nearest property.
    ///
    /// Read only where [`Weights::scout_focus`] prices it. Tiles rather than
    /// turns, on the same reading as [`Weights::hold_decay`]: the question is
    /// how far the dark is from the ground worth holding, and a tile of it is
    /// a tile of it whatever walks there.
    pub scout_decay: f64,
    /// What one tile of a kind's vision is worth to build. **Fog only.**
    ///
    /// A recon sees five tiles and costs four thousand funds, and no other
    /// term in the scorer can tell it from a tank that sees three. Priced
    /// against the funds the build spends, on the same reading as
    /// [`Weights::funds_efficiency`], so that the vision is bought where it is
    /// cheap rather than on the dearest hull that carries it.
    pub scout_build: f64,

    /// What one point of the price of a unit is worth to build.
    ///
    /// This is the whole of what a build used to be worth, and it ranks the
    /// kinds by cost: the dearest kind a factory offers wins every time. It
    /// is kept as a weight rather than deleted because the two modes want
    /// different answers from it — a standard game is a race that cheap
    /// capturers win, and a fog game is a war that army value wins.
    pub build_cost: f64,
    /// What one point of funds an exchange is expected to win is worth when
    /// a unit is bought rather than when it fires.
    ///
    /// This is what a unit is *for*, and without it a unit is worth what it
    /// costs. Pricing a build by its price buys the dearest kind the factory
    /// offers: a mech where an infantry captures the same property at the
    /// same rate, and a missile — which has no weapon that reaches a ground
    /// unit at all — over an anti-air that answers the infantry the enemy
    /// actually fields.
    pub counter: f64,
    /// How much of a build is priced against the funds it spends.
    ///
    /// At nothing a kind is worth what it does. At one it is worth what it
    /// does for each thousand funds, which is what makes three infantry beat
    /// one mech. Between the two because both readings are true: funds are
    /// the constraint early, and the factories are the constraint late. It is
    /// nothing in every weighting below `army`, so that the weighting a
    /// published number was taken at reads the same as it did.
    pub funds_efficiency: f64,

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
        hq_approach: 500.0,
        land: 1_000.0,
        air: 800.0,
        income: 600.0,
        naval: 100.0,
        other_property: 300.0,

        capture: 1.0,
        capture_completion: 1.0,
        capture_two_turn: 0.2,
        proximity_decay: 0.6,
        contest_decay: 1.0,

        funds: 0.02,
        unit_count: 20.0,
        blind_attack: 0.3,

        capturer_shortfall: 0.0,
        advance: 0.01,
        deny: 1.0,
        deny_neutral: 0.5,
        deny_decay: 0.5,
        hold: 0.0,
        hold_decay: 0.5,
        build_cost: 0.02,
        counter: 0.0,
        funds_efficiency: 0.0,
        threat: 0.02,
        deferred_threat: 0.35,
        scout: 0.0,
        scout_focus: 0.0,
        scout_decay: 0.75,
        conceal: 0.0,
        scout_build: 0.0,
        power: 200.0,
        supply: 10.0,
    };

    /// Tier 1 as it landed: neither the threat map, nor the denial term,
    /// nor the garrison term.
    ///
    /// A term priced at nothing is never built, so this is also the fastest
    /// weighting the agent holds.
    pub const TIER1: Self = Self {
        threat: 0.0,
        deferred_threat: 0.0,
        deny: 0.0,
        hold: 0.0,
        counter: 0.0,
        ..Self::DEFAULT
    };

    /// Tier 1 and the threat map, and nothing else.
    ///
    /// The baseline the denial term is measured against.
    pub const THREAT: Self = Self {
        threat: Self::DEFAULT.threat,
        deferred_threat: Self::DEFAULT.deferred_threat,
        ..Self::TIER1
    };

    /// The threat map and the denial term: the weighting the agent ships.
    pub const DENY: Self = Self::DEFAULT;

    /// The denial term and the garrison term.
    ///
    /// The baseline the garrison term is measured against is
    /// [`Weights::DENY`], which is the same weighting with `hold` at nothing.
    pub const DEFEND: Self = Self {
        hold: 0.75,
        ..Self::DENY
    };

    /// The garrison term, and a build priced against the funds it spends.
    ///
    /// An infantry and a mech take the same capture points a turn, and one
    /// costs three times the other. This is the weighting that knows it.
    pub const ARMY: Self = Self {
        funds_efficiency: 1.0,
        ..Self::DEFEND
    };

    /// The same again, and a build priced against the army in front of us.
    ///
    /// **Standard only.** The table is worth about 91 Elo in a standard game
    /// and loses about 62 under fog, where most of what it reads is the
    /// guess, not the enemy. See the handoff note.
    pub const COUNTER: Self = Self {
        counter: 7.5,
        funds_efficiency: 0.5,
        ..Self::ARMY
    };

    /// The counter table and the cover half of the vision term. **Fog only.**
    ///
    /// A unit that ends its move in the woods or on a reef is one the enemy
    /// cannot see, cannot price and does not walk at. This is the half of the
    /// vision term that carries the gain, and it is built on
    /// [`Weights::COUNTER`] and not on [`Weights::ARMY`] for a reason worth
    /// stating: the counter table loses under fog on its own, because what it
    /// reads there is the guess about the unseen enemy rather than the enemy.
    /// Given eyes it is the strongest weighting the arena holds.
    pub const VEIL: Self = Self {
        conceal: 2.0,
        ..Self::COUNTER
    };

    /// The cover half, and the disclosure half. **Fog only.**
    ///
    /// Walking into the dark, priced per tile lit. It is worth about 60 Elo
    /// laid over [`Weights::ARMY`] and **loses about 25 Elo** over
    /// [`Weights::VEIL`]: 0.4567 and 0.4733 over two seeds of 150 pairs. The
    /// shape table says which of the two stories is true, and it is the
    /// unkinder one — the scouts die. This weighting builds slightly more than
    /// `veil` and takes slightly more property, and loses 42.1 units a game
    /// against 37.4, ending on a fifth less unit value. Walking into the dark
    /// is a trade, and at this weight the agent takes it at a loss.
    pub const SCOUT: Self = Self {
        scout: 0.05,
        // Measured at nothing. A recon is five tiles of vision for four
        // thousand funds, and every reading of this weight from 1 to 360 lost
        // to the same weighting without it, so what the term buys is not the
        // eyes but the hull under them, and the hull is a bad one. It is kept
        // at nothing rather than deleted because the sweep that says so is
        // one file and a rerun.
        scout_build: 0.0,
        ..Self::VEIL
    };

    /// The current standard Amber Valley winner. It is kept as a named
    /// baseline so future tuning rounds can keep it in the opponent pool.
    /// R8 kept the counter and funds-efficiency terms, but removed the
    /// garrison hold that `COUNTER` inherits from `ARMY`.
    pub const BASELINE: Self = Self {
        hold: 0.0,
        ..Self::COUNTER
    };

    /// The cover half, and the board's own reading of whose the properties
    /// are. **Fog only.**
    ///
    /// The contest map discounts a property the enemy's production stands
    /// nearer to than ours, one decay for each turn of the deficit. Worth
    /// about 77 Elo over [`Weights::VEIL`] under fog — 0.6117, 0.5917 and
    /// 0.6250 over three seeds of 150 pairs — and a loss of about 91 in a
    /// standard game, where the same discount read 0.3725 over 200 pairs. The
    /// two modes disagree about it as plainly as they disagree about the
    /// counter table.
    pub const CONTEST: Self = Self {
        contest_decay: 0.5,
        ..Self::VEIL
    };

    /// The weightings this crate names, weakest first.
    ///
    /// Each one adds one term to the one before it, so any adjacent pair is
    /// the measurement of that term and nothing else. They are weightings and
    /// not agents: one greedy agent reads any of them, which is what stops a
    /// new term from needing a new agent to seat it.
    ///
    /// The chain forks once, at [`Weights::VEIL`]: `scout` and `contest` each
    /// add one term to it and neither adds anything to the other, so each of
    /// them is measured against `veil` and not against the name above it in
    /// this list.
    pub const PRESETS: [(&'static str, Self); 11] = [
        ("default", Self::DEFAULT),
        ("tier1", Self::TIER1),
        ("threat", Self::THREAT),
        ("deny", Self::DENY),
        ("defend", Self::DEFEND),
        ("army", Self::ARMY),
        ("counter", Self::COUNTER),
        ("baseline", Self::BASELINE),
        ("veil", Self::VEIL),
        ("scout", Self::SCOUT),
        ("contest", Self::CONTEST),
    ];

    /// The weighting of this name, or `None` for a name this crate does not
    /// hold.
    pub fn preset(name: &str) -> Option<Self> {
        Self::PRESETS
            .into_iter()
            .find(|(known, _)| *known == name)
            .map(|(_, weights)| weights)
    }

    /// The names [`Weights::preset`] answers, for a usage message.
    pub fn preset_names() -> String {
        Self::PRESETS.map(|(name, _)| name).join(", ")
    }
}

impl Default for Weights {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug)]
pub struct GreedyAgent {
    /// Ties are common — a mirror board answers the same score from two
    /// tiles — and breaking them the same way every time makes an agent that
    /// walks one flank. The draw is seeded, so a game still repeats.
    rng: Rng,
    weights: Weights,
    /// Optional unit restriction used while a stratified planner owns only
    /// one part of the turn.
    restricted: bool,
    allowed_units: Vec<awvm::semantic::UnitId>,
    allow_unitless: bool,
    /// Held across calls so that a turn's enumeration reuses one allocation.
    orders: Vec<Order>,
    /// The pull each tile feels toward the properties worth capturing, one
    /// field for each movement class that can capture, and the pull it feels
    /// toward the enemy. One entry for each tile of the board, rebuilt once
    /// for each play rather than once for each candidate.
    capture_fields: CaptureFields,
    advance_field: Vec<f64>,
    /// What standing on each tile is worth to hold it, which is nothing
    /// anywhere except on a property of ours an enemy capturer is near.
    hold_field: Vec<f64>,
    /// `proximity_decay` raised to each turn a field can hold, so that
    /// building the fields is a multiply rather than a power.
    decay: Vec<f64>,
    /// The unit standing on each tile, by its index in the roster.
    occupant: Vec<Option<u16>>,
    /// What each kind is worth to build against the army in front of us.
    counter: CounterTable,
    /// How many turns the enemy is ahead of us at each tile, which is what
    /// says whether a property is ours to take. Built once for each position,
    /// and reused across every position that did not move it.
    contest: ContestMap,
    /// What this player can see, and what it would see from each tile it can
    /// reach. Built once for each position, for the same reason the threat map
    /// is: a play moves a unit, so a map held across calls is a map of a board
    /// that is gone. Never built with fog off, where it answers the same for
    /// every tile.
    vision: VisionMap,
    /// What the enemy can take off each tile. Built once for each position,
    /// which is once for each play: the harness applies one command between
    /// calls, and a command moves a unit, so a map held across calls would be
    /// a map of a board that is gone.
    threat: ThreatMap,
}

impl GreedyAgent {
    pub const fn from_seed(seed: u64) -> Self {
        Self::with_weights(seed, Weights::BASELINE)
    }

    pub const fn with_weights(seed: u64, weights: Weights) -> Self {
        Self {
            rng: Rng::from_seed(seed),
            weights,
            restricted: false,
            allowed_units: Vec::new(),
            allow_unitless: true,
            orders: Vec::new(),
            capture_fields: CaptureFields::new(),
            advance_field: Vec::new(),
            hold_field: Vec::new(),
            decay: Vec::new(),
            occupant: Vec::new(),
            counter: CounterTable::new(),
            contest: ContestMap::new(),
            vision: VisionMap::new(),
            threat: ThreatMap::new(),
        }
    }

    pub const fn weights(&self) -> &Weights {
        &self.weights
    }

    /// Select only orders owned by the supplied units.
    ///
    /// Unitless actions are independently controlled because production and
    /// powers belong to the rear stratum rather than to a unit role.
    pub fn act_for_units(
        &mut self,
        view: &Observation,
        budget: NodeBudget,
        units: &[awvm::semantic::UnitId],
        allow_unitless: bool,
    ) -> Option<Play> {
        self.restricted = true;
        self.allowed_units.clear();
        self.allowed_units.extend_from_slice(units);
        self.allow_unitless = allow_unitless;
        let play = self.act(view, budget);
        self.restricted = false;
        self.allowed_units.clear();
        self.allow_unitless = true;
        play
    }
}

/// The pull toward capturable property, measured in turns rather than tiles.
///
/// Manhattan distance says a mountain and a road are the same distance apart,
/// and that a river is no further than the bridge over it. On a board twenty
/// tiles wide that is most of what the pull field is deciding, so the field
/// asks [`query::Travel`] what a route really costs and divides by what a unit
/// can spend in a turn.
///
/// One field for each movement class that can capture, because the two classes
/// that can — foot at three points a turn and boot at two — are not the same
/// distance from anything. Both fields are read by tile, so the extra work is
/// all in the building.
///
/// The searches are grouped by what a property is worth and not run once for
/// each property. A group's pull is `weight * decay^turns` and the field takes
/// the best of them, so the best over the targets of one weight is that weight
/// at the nearest of them — which is one search seeded at all of them at once.
/// The board holds at most a handful of distinct weights, so this is a
/// handful of searches rather than one for each of the thirty-eight
/// properties.
#[derive(Debug)]
struct CaptureFields {
    /// One field for each movement class, filled only for those that capture
    /// and only while our side holds a unit of the class.
    fields: [Vec<f64>; MovementClass::COUNT],
    /// What each field in hand was built from, so that a play which changed
    /// none of it reads the field again instead of searching for it again.
    /// `None` for a class holding no field.
    built: [Option<Built>; MovementClass::COUNT],
    /// The targets of each distinct approach weight, rebuilt each play.
    groups: Vec<(f64, Vec<Pos>)>,
    /// Movement points to the nearest target of the group being walked.
    points: Vec<Option<u16>>,
}

/// Everything one class's field was derived from.
///
/// The agent is asked for a play after every command, and rebuilds this field
/// each time, but a command moves one unit and most commands move nothing the
/// field reads. Over the fixture match the inputs take 47 distinct values
/// across 150 rebuilds, so about seven in ten searches recompute an answer
/// already in hand.
///
/// Keeping one is only sound while it names every input, so it names them by
/// holding them rather than by summarising them:
///
/// - `costs` is [`query::Travel::costs`], the table the search reads. Terrain,
///   weather, the seat's commander and its power are folded into it already,
///   which is why this is the table and not a list of those four. A rule added
///   to any of them moves this table and invalidates the field, without this
///   code knowing the rule exists.
/// - `targets` and `allowance` are the search's other two arguments.
/// - `decay` turns the search's answer into the pull that is stored. It comes
///   from the weights, which do not change while an agent plays, but a field
///   that outlived a weight change would be wrong in a way nothing else here
///   would catch.
///
/// Those are the whole input of the loop below. Comparing them costs a walk
/// over the board and a handful of positions, against the several full
/// searches it decides not to run.
#[derive(Clone, Debug, PartialEq)]
struct Built {
    allowance: u16,
    costs: Vec<Option<u16>>,
    targets: Vec<(f64, Vec<Pos>)>,
    decay: Vec<f64>,
}

impl Built {
    /// Whether a field built from this still answers for these inputs.
    fn still_answers(
        &self,
        allowance: u16,
        costs: &[Option<u16>],
        targets: &[(f64, Vec<Pos>)],
        decay: &[f64],
    ) -> bool {
        self.allowance == allowance
            && self.costs == costs
            && self.targets == targets
            && self.decay == decay
    }
}

/// The movement classes that can capture, and what one of them spends in a
/// turn.
///
/// Only two unit profiles carry `can_capture`, so the whole table is two
/// classes wide however many kinds the ruleset grows. The field uses the
/// effective movement allowance, including the active commander's power.
/// Fuel is unit-specific and can change after resupply, so it is not a route
/// property and does not limit this multi-turn estimate.
const CAPTURE_CLASSES: [MovementClass; 2] = [MovementClass::Foot, MovementClass::Boot];

impl CaptureFields {
    const fn new() -> Self {
        Self {
            fields: [const { Vec::new() }; MovementClass::COUNT],
            built: [const { None }; MovementClass::COUNT],
            groups: Vec::new(),
            points: Vec::new(),
        }
    }

    /// Drop every field, so that the next play searches for all of them.
    fn forget(&mut self) {
        for (field, built) in self.fields.iter_mut().zip(self.built.iter_mut()) {
            field.clear();
            *built = None;
        }
    }

    /// The field a unit of this class reads. Empty for a class that cannot
    /// capture, which answers no pull rather than a wrong one.
    fn of(&self, class: MovementClass) -> &[f64] {
        &self.fields[class.index()]
    }

    /// Rebuild both fields for the position in front of the agent.
    ///
    /// A property already carrying a capturer of ours pulls nothing. Without
    /// that, every capturer on the board walks at the same property, and the
    /// ones that arrive second stand next to it doing nothing at all.
    fn build(
        &mut self,
        session: &Session,
        board: &Board<'_>,
        decay: &[f64],
        occupant: &[Option<u16>],
        contest: &ContestMap,
    ) {
        let cells = board.cells();
        self.groups.clear();
        for (position, tile) in board.state.board.iter() {
            if !board.capturable(tile) {
                continue;
            }
            let Some(cell) = board.cell(position) else {
                continue;
            };
            if occupant[cell].is_some_and(|index| board.is_our_capturer(index)) {
                continue;
            }
            // What the property is worth, less what the board says about
            // whose it is. A property the enemy reaches first is one we would
            // arrive at second, and the contest map is the only thing here
            // that knows the difference.
            let weight = board.approach_weight(tile.terrain)
                * board
                    .weights
                    .contest_decay
                    .powi(i32::from(contest.deficit(cell)));
            match self.groups.iter_mut().find(|(known, _)| *known == weight) {
                Some((_, targets)) => targets.push(position),
                None => self.groups.push((weight, vec![position])),
            }
        }
        if self.groups.is_empty() {
            self.forget();
            return;
        }

        // The session's own entry-cost grids, which the legal-order walk and
        // the threat sweep read as well. Opening a travel of its own would
        // build the same grid for every class a second time.
        let Some(mut travel) = session.travel(board.seat) else {
            self.forget();
            return;
        };
        for class in CAPTURE_CLASSES {
            let index = class.index();
            // A class our side does not field costs a search for each weight
            // on the board and is read by nothing.
            let Some(allowance) = board.capture_allowance(class) else {
                self.fields[index].clear();
                self.built[index] = None;
                continue;
            };
            // Ask for the table before the searches, because deciding not to
            // run them is what this is for.
            let costs = travel.costs(class);
            if self.built[index]
                .as_ref()
                .is_some_and(|built| built.still_answers(allowance, costs, &self.groups, decay))
            {
                continue;
            }
            self.built[index] = Some(Built {
                allowance,
                costs: costs.to_vec(),
                targets: self.groups.clone(),
                decay: decay.to_vec(),
            });

            let field = &mut self.fields[index];
            field.clear();
            field.resize(cells, 0.0);
            for (weight, targets) in &self.groups {
                travel.points_to(class, allowance, targets.iter().copied(), &mut self.points);
                for (cell, value) in field.iter_mut().enumerate() {
                    let Some(Some(points)) = self.points.get(cell).copied() else {
                        continue;
                    };
                    let turns = usize::from(query::Travel::turns(points, allowance));
                    // Past the end of the table the pull is smaller than
                    // anything that decides a play, so the last entry stands
                    // for every distance beyond it.
                    let step = decay[turns.min(decay.len() - 1)];
                    let pull = weight * step;
                    if pull > *value {
                        *value = pull;
                    }
                }
            }
        }
    }
}

/// What each kind of unit is worth to build against the army in front of us.
///
/// A build is the one play this agent makes that nothing on the board argues
/// for. A capture is worth the property, an attack is worth the forecast, and
/// a build was worth its own price — which ranks the kinds by cost and buys
/// the dearest one the factory offers. This ranks them by what they do to the
/// units the enemy actually fields.
///
/// One entry for each kind, in funds: what a whole one of that kind takes off
/// the average enemy unit in one strike, less what that enemy takes back. A
/// missile against an army of soldiers reads a loss, because no weapon it
/// carries reaches one; an anti-air against the same army reads most of a
/// thousand funds.
///
/// It is rebuilt only when the army it reads moves. Most commands change
/// nobody's roster, and the table is `UnitKind::COUNT` rows over the kinds
/// the enemy fields.
#[derive(Debug)]
struct CounterTable {
    /// The enemy roster this was built from: one entry for each kind they
    /// field, holding the health of all of them as a share of a whole unit.
    army: Vec<(UnitKind, f64)>,
    seen: Vec<(UnitKind, f64)>,
    values: Vec<f64>,
}

impl CounterTable {
    const fn new() -> Self {
        Self {
            army: Vec::new(),
            seen: Vec::new(),
            values: Vec::new(),
        }
    }

    /// What one strike of `kind` is worth against that army, in funds.
    /// Zero while no enemy is in sight, which is an answer and not a guess.
    fn of(&self, kind: UnitKind) -> f64 {
        self.values.get(kind.index()).copied().unwrap_or(0.0)
    }

    fn build(&mut self, board: &Board<'_>) {
        board.enemy_army(&mut self.seen);
        if self.seen == self.army && !self.values.is_empty() {
            return;
        }
        std::mem::swap(&mut self.army, &mut self.seen);

        self.values.clear();
        self.values.resize(UnitKind::COUNT, 0.0);
        let whole: f64 = self.army.iter().map(|(_, health)| health).sum();
        if whole <= 0.0 {
            return;
        }

        for kind in UnitKind::ALL {
            let ours = cost(kind);
            let value: f64 = self
                .army
                .iter()
                .map(|(theirs, health)| {
                    let dealt = strike(kind, *theirs) * cost(*theirs);
                    // What the average enemy of that kind does back. A unit
                    // is not worth building because it kills what cannot
                    // answer it only; it is worth building for the exchange.
                    let taken = strike(*theirs, kind) * ours;
                    health * (dealt - taken)
                })
                .sum();
            self.values[kind.index()] = value / whole;
        }
    }
}

/// The share of a whole unit of `defender` that one strike of `attacker`
/// removes, on flat ground against a commander with no combat rule.
///
/// Both sides are read at whole health and a full magazine, which is the
/// unit this table is about: the one that has not been built yet.
fn strike(attacker: UnitKind, defender: UnitKind) -> f64 {
    /// The attack and defense a commander with no combat rule presents.
    const NEUTRAL: i64 = 100;

    let side = |kind: UnitKind, ammo| Side {
        kind,
        hp: 100,
        ammo,
        attack: NEUTRAL,
        defense: NEUTRAL,
        terrain_stars: 0,
    };
    let ammo = ruleset::profile(attacker).max_ammo;
    combat::damage(side(attacker, ammo), side(defender, 0), 0)
        .map_or(0.0, |hit| f64::from(hit.damage) / 100.0)
}

impl Agent for GreedyAgent {
    fn act(&mut self, view: &Observation, _budget: NodeBudget) -> Option<Play> {
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
            restricted,
            allowed_units,
            allow_unitless,
            orders,
            capture_fields,
            advance_field,
            hold_field,
            decay,
            occupant,
            counter,
            contest,
            vision,
            threat,
        } = self;

        let board = Board {
            state,
            seat,
            weights,
        };
        board.decay_table(decay);
        board.occupants(occupant);
        // A map priced at nothing is never built, and a contest at no discount
        // is priced at nothing: every property then reads the weight its
        // terrain gives it, which is the reading the field shipped with.
        if weights.contest_decay != 1.0 {
            contest.build(state, seat);
        } else {
            contest.forget();
        }
        capture_fields.build(&session, &board, decay, occupant, contest);
        board.advance_field(advance_field, decay);
        // A term priced at nothing decides nothing, so the field it reads is
        // not built. That is what keeps every weighting below `defend` at the
        // throughput its numbers were taken at.
        if weights.hold != 0.0 {
            board.hold_field(hold_field);
        } else {
            hold_field.clear();
        }
        // A map priced at nothing is never read, so it is never built. That
        // is what keeps the threatless weighting at the throughput it was
        // measured at, and it is what makes the two a comparison of one term.
        if weights.threat != 0.0 {
            threat.build(&session, seat);
        }
        // A table priced at nothing decides nothing, so a weighting without
        // this term never pays to build one.
        if weights.counter != 0.0 {
            counter.build(&board);
        }
        // With fog off every tile is lit and every unit is seen, so the whole
        // of the vision term is zero however it is weighted. Not building the
        // map there is what keeps a standard game at the throughput its
        // numbers were taken at.
        let needs = Needs {
            focus: weights.scout_focus,
            focus_decay: weights.scout_decay,
            // The build term reads a kind's own vision and not the board, but
            // it is a fog term all the same, and it is priced beside the tiles
            // a play lights.
            disclosure: weights.scout != 0.0 || weights.scout_build != 0.0,
            cover: weights.conceal != 0.0,
        };
        if state.settings.fog {
            vision.build(state, seat, needs);
        } else {
            vision.forget();
        }

        orders.clear();
        session.legal().orders(orders);

        let legal = session.legal();
        let scorer = Scorer {
            board: &board,
            legal: &legal,
            capture_fields,
            advance_field,
            hold_field,
            occupant,
            counter,
            vision,
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
            if *restricted {
                match session.unit_of(order) {
                    Some(unit) if allowed_units.contains(&unit) => {}
                    None if *allow_unitless => {}
                    _ => continue,
                }
            }
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

    /// `proximity_decay` for each turn, and for each tile of the board's
    /// span, which is longer than any field needs and short enough to build.
    ///
    /// [`CaptureFields`] reads it by turns and [`Board::advance_field`] by
    /// tiles, and the two are not the same unit. Both clamp at the end of the
    /// table, where the pull is already smaller than anything that decides a
    /// play.
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

    /// The effective allowance of our capturer in this movement class.
    ///
    /// A pull field is read only by capturers, so a class without one is a
    /// search for each weight on the board that answers nobody.
    fn capture_allowance(&self, class: MovementClass) -> Option<u16> {
        self.state.units.iter().find_map(|unit| {
            let profile = ruleset::profile(unit.kind);
            (matches!(unit.location, Location::Board { .. })
                && unit.owner == self.seat
                && profile.can_capture
                && profile.movement_class == class)
                .then(|| {
                    commander::effective_move(self.state, unit, profile.movement, profile.domain)
                        .min(u64::from(u16::MAX)) as u16
                })
        })
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
            let weight = self.weights.advance * cost(unit.kind) * health_of(unit);
            for (from, value) in self.state.board.positions().zip(out.iter_mut()) {
                let pull = weight * decay[target.distance(from) as usize];
                if pull > *value {
                    *value = pull;
                }
            }
        }
    }

    /// What standing on each tile is worth as a garrison.
    ///
    /// Only a property of ours can be garrisoned, and only one an enemy
    /// capturer is near is worth a unit's turn. The value is the property's
    /// own weight, decayed by the tiles between it and the nearest enemy that
    /// could take it, so a headquarters with a soldier beside it outbids a
    /// city on the far side of the board without either being named.
    ///
    /// Distance is the straight line and not the route, which is the fidelity
    /// [`Board::advance_field`] already has. The nearest enemy capturer is
    /// what decides the deadline, so this is a walk over our properties
    /// against the enemy capturers on the board, and not a search.
    fn hold_field(&self, out: &mut Vec<f64>) {
        out.clear();
        out.resize(self.cells(), 0.0);
        for (position, tile) in self.state.board.iter() {
            if !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
                continue;
            }
            if tile.owner != TileOwner::Owned(self.seat) {
                continue;
            }
            let Some(cell) = self.cell(position) else {
                continue;
            };
            let Some(distance) = self.nearest_hostile_capturer(position) else {
                continue;
            };
            let decay = self
                .weights
                .hold_decay
                .powi(i32::try_from(distance).unwrap_or(i32::MAX));
            out[cell] = self.weights.hold * self.property_weight(tile.terrain) * decay;
        }
    }

    /// The tiles between `position` and the nearest enemy unit that captures,
    /// or `None` when the enemy fields none we can see.
    fn nearest_hostile_capturer(&self, position: Pos) -> Option<u64> {
        self.state
            .units
            .iter()
            .filter_map(|unit| {
                let Location::Board { position: from } = unit.location else {
                    return None;
                };
                (self.hostile(unit.owner) && ruleset::profile(unit.kind).can_capture)
                    .then(|| from.distance(position))
            })
            .min()
    }

    /// The army to price a build against, one entry for each kind in it.
    ///
    /// The health of every unit of a kind, added up as shares of a whole
    /// unit, so a damaged army counts for less than a whole one and a kind
    /// nobody fields is not in the list at all.
    ///
    /// **What is not seen is assumed to look like us.** Under fog the enemy
    /// units in sight are a handful of the ones they hold, and an army priced
    /// against a handful is priced against nothing: every kind reads the same
    /// value and the factory draws one at random. Where our own army is
    /// larger than what we can see of theirs, the difference is filled in
    /// with our own composition. It is the one guess on the board that costs
    /// nothing to make and is wrong in the same direction for both players.
    fn enemy_army(&self, out: &mut Vec<(UnitKind, f64)>) {
        out.clear();
        let mut seen = 0.0;
        let mut ours = 0.0;
        for unit in self.state.units.iter() {
            if !matches!(unit.location, Location::Board { .. }) {
                continue;
            }
            if unit.owner == self.seat {
                ours += health_of(unit);
                continue;
            }
            if !self.hostile(unit.owner) {
                continue;
            }
            seen += health_of(unit);
            add_to(out, unit.kind, health_of(unit));
        }

        let missing = ours - seen;
        if missing <= 0.0 || ours <= 0.0 {
            out.sort_unstable_by_key(|(kind, _)| kind.index());
            return;
        }
        let share = missing / ours;
        for unit in self.state.units.iter() {
            if unit.owner != self.seat || !matches!(unit.location, Location::Board { .. }) {
                continue;
            }
            add_to(out, unit.kind, health_of(unit) * share);
        }
        // The roster order is the board's own, so a list that is the same
        // army must read the same both times it is compared.
        out.sort_unstable_by_key(|(kind, _)| kind.index());
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

/// Add one kind's health to a roster summary, in place.
fn add_to(army: &mut Vec<(UnitKind, f64)>, kind: UnitKind, health: f64) {
    match army.iter_mut().find(|(known, _)| *known == kind) {
        Some((_, held)) => *held += health,
        None => army.push((kind, health)),
    }
}

/// What one unit costs to replace, in funds.
fn cost(kind: UnitKind) -> f64 {
    ruleset::profile(kind).cost as f64
}

/// A unit's health as a share of a whole one.
fn health_of(unit: &Unit) -> f64 {
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
    capture_fields: &'a CaptureFields,
    advance_field: &'a [f64],
    hold_field: &'a [f64],
    occupant: &'a [Option<u16>],
    counter: &'a CounterTable,
    vision: &'a VisionMap,
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
            OrderKind::Capture => self.capture(order) + self.sight(order),
            OrderKind::Attack(target) => self.attack(order, target),
            OrderKind::Wait | OrderKind::Unload(_) => self.arrival(order) + self.sight(order),
            // Both cost a unit from the roster and give its health to another,
            // so both are scored as the arrival less what the count is worth.
            // Neither is scored for vision: a unit that joins another adds no
            // eye the tile does not already hold, and one that loads into a
            // transport is cargo, which sees nothing at all.
            OrderKind::Join | OrderKind::Load => self.arrival(order) - self.weights().unit_count,
            OrderKind::Supply => self.arrival(order) + self.weights().supply + self.sight(order),
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
            return weights.blind_attack * weights.funds * cost(defender.kind)
                + self.pull(order)
                + self.sight(order);
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
        score + self.pull(order) + self.sight(order)
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
        let profile = ruleset::profile(unit.kind);
        let field = if profile.can_capture {
            self.capture_fields.of(profile.movement_class)
        } else {
            self.advance_field
        };
        // Holding a property of ours is added rather than taken the best of.
        // The two answer different questions — where this unit is going, and
        // what it is worth for it to stop where it is — and a tile that is
        // both a garrison and a step toward the next property is worth both.
        let hold = self.hold_field.get(cell).copied().unwrap_or(0.0);
        field.get(cell).copied().unwrap_or(0.0) + hold
    }

    /// What this play is worth for what it does to vision.
    ///
    /// The dark tiles the destination lights, and the cover the destination
    /// itself gives. Both are zero with fog off, where the map is not built
    /// at all, and both are zero for a weighting that prices them at nothing.
    fn sight(&self, order: Order) -> f64 {
        if !self.vision.is_built() {
            return 0.0;
        }
        let Some(unit) = order.unit().and_then(|index| self.unit(index)) else {
            return 0.0;
        };
        let Some(position) = self.board.position(order.destination()) else {
            return 0.0;
        };
        let weights = self.weights();
        let state = self.board.state;
        let mut score = if weights.scout != 0.0 {
            weights.scout * self.vision.reveal(state, unit, position)
        } else {
            0.0
        };
        if weights.conceal != 0.0 && self.vision.conceals(position) {
            score += weights.conceal;
        }
        score
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
        let mut score = weights.unit_count;
        // Both roles are priced against the funds the build spends, because
        // the question a factory asks is not which kind is strongest but
        // which kind is worth the money. An infantry and a mech take the same
        // capture points a turn and one of them costs three times the other.
        let efficiency = (1000.0 / cost(kind).max(1.0)).powf(weights.funds_efficiency);
        if profile.can_capture {
            score += weights.capturer_shortfall * self.shortfall * efficiency;
        }
        score += weights.build_cost * cost(kind);
        // The eyes a hull carries, priced where the map is dark. A recon is a
        // cheap five tiles of it and every other land kind is two or three,
        // which no other term here can tell apart.
        if self.vision.is_built() {
            let sight = f64::from(i32::try_from(profile.vision).unwrap_or(0)).max(0.0);
            score += weights.scout_build * sight * efficiency;
        }
        score + weights.counter * self.counter.of(kind) * efficiency
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{amber_valley, arena};
    use awvm::ruleset::WeatherKind;
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

    /// A kept capture field says exactly what a field with no memory says.
    ///
    /// The field is rebuilt after every command in the fixture match, and
    /// most commands change nothing it reads, so the play a cached field
    /// chooses must be the play a rebuilt one chooses — every time, not
    /// mostly. This plays a whole match and compares the two at every
    /// decision, and refuses to pass unless the cache was actually reused,
    /// because a cache that never hits agrees with anything.
    #[test]
    fn a_kept_capture_field_answers_what_a_rebuilt_one_answers() {
        struct Checker {
            inner: GreedyAgent,
            fresh: CaptureFields,
            contest: ContestMap,
            last: [Option<Built>; MovementClass::COUNT],
            decay: Vec<f64>,
            occupant: Vec<Option<u16>>,
            compared: u32,
            reused: u32,
        }

        impl Agent for Checker {
            fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
                let play = self.inner.act(view, budget);

                let Ok(session) = Session::from_observation(view) else {
                    return play;
                };
                if !session.is_commandable() {
                    return play;
                }
                let state = session.state();
                let Some(seat) = state.players.seat(&state.turn.active_player) else {
                    return play;
                };
                let board = Board {
                    state,
                    seat,
                    weights: self.inner.weights(),
                };
                board.decay_table(&mut self.decay);
                board.occupants(&mut self.occupant);

                // No memory at all: every search run again from nothing.
                self.fresh = CaptureFields::new();
                self.fresh
                    .build(&session, &board, &self.decay, &self.occupant, &self.contest);

                for class in CAPTURE_CLASSES {
                    assert_eq!(
                        self.inner.capture_fields.of(class),
                        self.fresh.of(class),
                        "a kept {class:?} field disagrees with a rebuilt one"
                    );
                    let index = class.index();
                    if self.inner.capture_fields.built[index].is_some()
                        && self.last[index] == self.inner.capture_fields.built[index]
                    {
                        self.reused += 1;
                    }
                    self.last[index] = self.inner.capture_fields.built[index].clone();
                }
                self.compared += 1;
                play
            }
        }

        let state = amber_valley(false, crate::rng::Rng::mix(9));
        let mut session = Session::new(state.clone());
        let mut entropy = crate::rng::Rng::from_seed(17);
        let mut checker = Checker {
            inner: GreedyAgent::from_seed(23),
            fresh: CaptureFields::new(),
            contest: ContestMap::new(),
            last: [const { None }; MovementClass::COUNT],
            decay: Vec::new(),
            occupant: Vec::new(),
            compared: 0,
            reused: 0,
        };
        let mut opponent = GreedyAgent::from_seed(29);
        {
            let mut agents: [&mut dyn Agent; 2] = [&mut checker, &mut opponent];
            crate::harness::play(
                state,
                &mut session,
                &mut agents,
                &mut entropy,
                crate::harness::Limits::DEFAULT,
            )
        };

        assert!(
            checker.compared > 50,
            "the match offered {} decisions to compare",
            checker.compared
        );
        assert!(
            checker.reused > 0,
            "the cache never hit, so the comparison proves nothing"
        );
    }

    /// The key names every input, and a real rule change moves it.
    ///
    /// The fixture match never changes the weather, the commander or an
    /// allowance, so the match-long comparison above can only prove the
    /// target set. These two pin the rest: the first that each field of the
    /// key is compared at all, the second that a rule change outside this
    /// crate — snow, which reprices every tile — actually reaches the key and
    /// throws the field away.
    #[test]
    fn a_capture_key_names_every_input_it_reads() {
        let built = Built {
            allowance: 3,
            costs: vec![Some(1), None],
            targets: vec![(2.0, vec![Pos { x: 1, y: 1 }])],
            decay: vec![1.0, 0.75],
        };
        assert!(built.still_answers(3, &built.costs, &built.targets, &built.decay));
        assert!(
            !built.still_answers(4, &built.costs, &built.targets, &built.decay),
            "the allowance divides the search into turns"
        );
        assert!(
            !built.still_answers(3, &[Some(2), None], &built.targets, &built.decay),
            "the entry costs are what the search walks"
        );
        assert!(
            !built.still_answers(3, &built.costs, &[(2.0, vec![])], &built.decay),
            "the targets are what the search is seeded with"
        );
        assert!(
            !built.still_answers(3, &built.costs, &built.targets, &[1.0, 0.5]),
            "the decay turns the search into the pull that is stored"
        );
    }

    #[test]
    fn snow_throws_away_a_capture_field_built_under_clear_skies() {
        let build = |state: State, fields: &mut CaptureFields| {
            let session = Session::new(state);
            let state = session.state();
            let seat = state
                .players
                .seats()
                .nth(1)
                .map(|(seat, _)| seat)
                .expect("the second seat holds the predeployed unit");
            let weights = Weights::DEFAULT;
            let board = Board {
                state,
                seat,
                weights: &weights,
            };
            let (mut decay, mut occupant) = (Vec::new(), Vec::new());
            board.decay_table(&mut decay);
            board.occupants(&mut occupant);
            fields.build(&session, &board, &decay, &occupant, &ContestMap::new());
            fields.of(MovementClass::Foot).to_vec()
        };

        let clear = amber_valley(false, 5);
        let mut snowy = clear.clone();
        snowy.weather.kind = WeatherKind::Snow;

        let mut kept = CaptureFields::new();
        let under_clear = build(clear, &mut kept);
        // The same fields again, now over snow. A key that did not carry the
        // entry costs would hand back the clear-weather answer here.
        let after_snow = build(snowy.clone(), &mut kept);
        let from_nothing = build(snowy, &mut CaptureFields::new());

        assert_eq!(after_snow, from_nothing, "a kept field survived the thaw");
        assert_ne!(
            under_clear, after_snow,
            "snow reprices the board, so the field must move"
        );
    }

    /// A property the enemy reaches first pulls less than the same property
    /// on our own half of the board.
    #[test]
    fn the_contest_map_discounts_the_properties_behind_the_enemy() {
        let state = amber_valley(false, 5);
        let session = Session::new(state);
        let state = session.state();
        let seat = state
            .players
            .seats()
            .nth(1)
            .map(|(seat, _)| seat)
            .expect("the second seat holds the predeployed unit");

        let field = |contest_decay: f64| {
            let weights = Weights {
                contest_decay,
                ..Weights::DEFAULT
            };
            let board = Board {
                state,
                seat,
                weights: &weights,
            };
            let (mut decay, mut occupant) = (Vec::new(), Vec::new());
            board.decay_table(&mut decay);
            board.occupants(&mut occupant);
            let mut contest = ContestMap::new();
            if weights.contest_decay != 1.0 {
                contest.build(state, seat);
            }
            let mut fields = CaptureFields::new();
            fields.build(&session, &board, &decay, &occupant, &contest);
            fields.of(MovementClass::Foot).to_vec()
        };

        let flat = field(1.0);
        let contested = field(0.5);
        let discounted = flat
            .iter()
            .zip(contested.iter())
            .filter(|(open, held)| *held < *open)
            .count();
        let raised = flat
            .iter()
            .zip(contested.iter())
            .filter(|(open, held)| *held > *open)
            .count();
        assert!(
            discounted > 0,
            "no tile of the board reads the enemy as nearer to anything"
        );
        assert_eq!(raised, 0, "a discount may not make a property pull harder");
        assert_eq!(
            flat,
            field(1.0),
            "a contest at no discount reads what no contest reads"
        );
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
        // The Amber Valley baseline does not send every new unit toward the
        // enemy headquarters from turn one. HQ capture still outranks every
        // ordinary objective once it can actually be completed.
        assert!(weights.hq_approach < weights.land);
    }

    /// The two capture bonuses, in the order the priorities put them.
    #[test]
    fn a_capture_is_worth_more_the_sooner_it_finishes() {
        let weights = Weights {
            capture_completion: 0.8,
            capture_two_turn: 0.5,
            ..Weights::DEFAULT
        };
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
            .act(&view, NodeBudget::FOUR)
            .expect("the opening position offers a play");

        let OrderKind::Produce(kind) = play.kind() else {
            panic!("the opening play was {:?}, not a build", play.kind());
        };
        assert!(
            ruleset::profile(kind).can_capture,
            "the opening build was a {kind:?}, which captures nothing"
        );
    }

    /// The whole reason for measuring turns: terrain is what a route costs.
    ///
    /// Manhattan distance says a mountain and a road are the same distance
    /// apart, so a property behind a mountain range pulled exactly as hard as
    /// one along a road. The field now asks what the route costs, so a tile
    /// the terrain puts further away is further away.
    ///
    /// This drives the real board rather than a fixture, because the claim is
    /// about a field built from a whole position. It asserts the two things
    /// that separate the new field from the old one: it disagrees with a
    /// straight line somewhere, and a foot unit and a boot unit — three points
    /// a turn against two — do not read the same field.
    #[test]
    fn the_capture_field_measures_the_route_and_not_the_line() {
        let mut state = amber_valley(false, 1);
        let seat = state
            .players
            .seats()
            .nth(1)
            .map(|(seat, _)| seat)
            .expect("the second seat holds the predeployed unit");
        // The opening position fields no mech, and a class we hold nothing of
        // is skipped rather than searched for. Give the seat one, so that both
        // fields are built and the two allowances can be compared.
        let home = state
            .units
            .iter()
            .find(|unit| unit.owner == seat)
            .and_then(|unit| match unit.location {
                Location::Board { position } => Some(position),
                Location::Cargo { .. } => None,
            })
            .expect("the seat opens with a unit on the board");
        let mut mech = test_unit(UnitKind::Mech, home, seat);
        mech.id = UnitId::new(u32::from(u16::MAX));
        state.units.push(mech);
        let session = Session::new(state);
        let weights = Weights::DEFAULT;
        let board = Board {
            state: session.state(),
            seat,
            weights: &weights,
        };
        let mut decay = Vec::new();
        let mut occupant = Vec::new();
        board.decay_table(&mut decay);
        board.occupants(&mut occupant);
        let mut fields = CaptureFields::new();
        fields.build(&session, &board, &decay, &occupant, &ContestMap::new());

        let foot = fields.of(MovementClass::Foot);
        let boot = fields.of(MovementClass::Boot);
        assert_eq!(foot.len(), board.cells(), "a foot field covers the board");
        assert_eq!(boot.len(), board.cells(), "a boot field covers the board");
        assert!(
            foot.iter().any(|pull| *pull > 0.0),
            "a board with open property pulls somewhere"
        );

        // Two classes, two allowances, two fields. Equal fields would mean the
        // allowance is not being read at all.
        assert!(
            foot != boot,
            "foot spends three points a turn and boot two, so the two classes \
             are not the same number of turns from the same property"
        );

        // And the field is not the old one under another name. A tile whose
        // best pull differs from what a Manhattan field would give is a tile
        // the terrain moved, which is the entire change.
        let mut manhattan = vec![0.0; board.cells()];
        for (target, tile) in board.state.board.iter() {
            if !board.capturable(tile) {
                continue;
            }
            let Some(cell) = board.cell(target) else {
                continue;
            };
            if occupant[cell].is_some_and(|index| board.is_our_capturer(index)) {
                continue;
            }
            let weight = board.approach_weight(tile.terrain);
            for (from, value) in board.state.board.positions().zip(&mut manhattan) {
                let pull = weight * decay[target.distance(from) as usize];
                if pull > *value {
                    *value = pull;
                }
            }
        }
        let moved = foot
            .iter()
            .zip(manhattan.iter())
            .filter(|(route, line)| (*route - *line).abs() > f64::EPSILON)
            .count();
        assert!(
            moved > 0,
            "the route field agreed with a straight line everywhere"
        );
    }

    #[test]
    fn travel_rejects_an_entry_cost_above_the_turn_allowance() {
        let mut state = amber_valley(false, 1);
        state.weather.kind = awvm::ruleset::WeatherKind::Snow;
        let seat = state
            .players
            .seat(&state.turn.active_player)
            .expect("the active player holds a seat");
        let mountain = state
            .board
            .iter()
            .find_map(|(position, tile)| (tile.terrain == Terrain::Mountain).then_some(position))
            .expect("Amber Valley contains a mountain");
        let dimensions = state.board.dimensions();
        let cell = dimensions
            .cell_index(mountain)
            .expect("the mountain is on the board");
        let mut travel = query::Travel::open(&state, seat).expect("the seat has travel tables");
        let mut points = Vec::new();

        // Foot movement pays four points for a snowy mountain. Infantry can
        // spend only three, so waiting for another turn cannot make it enter.
        travel.points_to(MovementClass::Foot, 3, [mountain], &mut points);
        assert_eq!(points[usize::from(cell.get())], None);

        travel.points_to(MovementClass::Foot, 4, [mountain], &mut points);
        assert_eq!(points[usize::from(cell.get())], Some(0));
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
            let Some(play) = agent.act(&view, NodeBudget::FOUR) else {
                break;
            };
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

    /// The build table answers the army in front of it, and not the price
    /// list.
    ///
    /// Against an army of soldiers an anti-air takes a whole one off in a
    /// strike, and a missile carries no weapon that reaches one at all — so
    /// the missile, which costs half again as much, must read a loss and the
    /// anti-air a gain. Pricing a build by its cost ranks them the other way
    /// round, which is what this pins.
    #[test]
    fn a_build_is_worth_what_it_answers_and_not_what_it_costs() {
        let (state, ours, _) = army_of(UnitKind::Infantry, 3);
        let weights = Weights::COUNTER;
        let board = Board {
            state: &state,
            seat: ours,
            weights: &weights,
        };
        let mut table = CounterTable::new();
        table.build(&board);

        assert!(
            table.of(UnitKind::AntiAir) > 0.0,
            "an anti-air answers an army of soldiers"
        );
        assert!(
            table.of(UnitKind::Missile) < 0.0,
            "a missile has no weapon that reaches a soldier"
        );
        assert!(table.of(UnitKind::AntiAir) > table.of(UnitKind::Missile));
    }

    /// What is not seen is assumed to look like us.
    ///
    /// Under fog the enemy in sight is a handful of what they hold, and a
    /// table built on a handful reads the same value for every kind, which
    /// leaves the factory drawing at random. The army our side fields fills
    /// the rest in.
    #[test]
    fn an_unseen_enemy_is_assumed_to_look_like_the_army_we_hold() {
        let (mut state, ours, _) = army_of(UnitKind::Infantry, 1);
        for index in 0..4 {
            let mut ours = test_unit(UnitKind::MdTank, Pos { x: 5, y: 5 }, ours);
            ours.id = UnitId::new(500 + index);
            state.units.push(ours);
        }
        let weights = Weights::COUNTER;
        let board = Board {
            state: &state,
            seat: ours,
            weights: &weights,
        };

        let mut army = Vec::new();
        board.enemy_army(&mut army);
        assert!(
            army.iter().any(|(kind, _)| *kind == UnitKind::MdTank),
            "the army we cannot see was left empty rather than guessed at"
        );
        let seen = army
            .iter()
            .find(|(kind, _)| *kind == UnitKind::Infantry)
            .map(|(_, health)| *health);
        assert_eq!(seen, Some(1.0), "the enemy in sight is counted as it is");
    }

    /// A board holding `count` enemy units of one kind, and nothing else,
    /// with the seat that is asking and the seat it is at war with.
    fn army_of(kind: UnitKind, count: u32) -> (State, PlayerIdx, PlayerIdx) {
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
        state.units.retain(|_| false);
        for index in 0..count {
            let mut unit = test_unit(kind, Pos { x: 3, y: 3 }, theirs);
            unit.id = UnitId::new(400 + index);
            state.units.push(unit);
        }
        (state, ours, theirs)
    }

    /// The garrison weighting keeps a body on the headquarters. The shipped
    /// one walks off it.
    ///
    /// The position holds an enemy soldier two tiles from our headquarters
    /// and one soldier of ours standing on it. Denial cannot answer this:
    /// nothing is capturing yet, so there is nothing to strike. The only play
    /// that saves the headquarters is staying, because a property with a unit
    /// on it cannot be captured at all.
    #[test]
    fn a_garrison_holds_the_headquarters_an_enemy_capturer_is_near() {
        /// Where the guard's turn takes it, or `None` for a guard that
        /// never left the headquarters.
        fn guard_destination(weights: Weights) -> Option<CellIdx> {
            let (mut session, guard, hq) = headquarters_under_approach();
            let mut agent = GreedyAgent::with_weights(1, weights);
            let mut entropy = Rng::from_seed(7);
            let hq = session
                .state()
                .board
                .dimensions()
                .cell_index(hq)
                .expect("the headquarters is on the board");

            for _ in 0..12 {
                let state = session.state();
                let Ok(view) = observe(&AwbwVisibility, state, &state.turn.active_player) else {
                    break;
                };
                // The turn ends with the guard where it stands, which is one
                // of the two ways of holding the tile.
                let Some(play) = agent.act(&view, NodeBudget::FOUR) else {
                    break;
                };
                if play.unit() == Some(guard) && play.destination() != hq {
                    return Some(play.destination());
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
            None
        }

        assert!(
            guard_destination(Weights::DENY).is_some(),
            "the shipped weighting already holds the headquarters, so this \
             measures nothing"
        );
        assert_eq!(
            guard_destination(Weights::DEFEND),
            None,
            "the garrison weighting walked off the headquarters"
        );
    }

    /// Our headquarters with one soldier of ours on it, and an enemy capturer
    /// two tiles from it.
    fn headquarters_under_approach() -> (Session, UnitId, Pos) {
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
        let approach = state
            .board
            .positions()
            .find(|position| {
                hq.distance(*position) == 2
                    && state.board.get(*position).is_some_and(|tile| {
                        ruleset::movement_cost(
                            tile.terrain,
                            state.weather.kind,
                            ruleset::profile(UnitKind::Infantry).movement_class,
                        )
                        .is_some()
                    })
            })
            .expect("the headquarters has a walkable tile two steps from it");

        let guard = UnitId::new(9_004);
        state.units.retain(|_| false);
        let mut raider = test_unit(UnitKind::Infantry, approach, theirs);
        raider.id = UnitId::new(9_003);
        state.units.push(raider);
        let mut ours_soldier = test_unit(UnitKind::Infantry, hq, ours);
        ours_soldier.id = guard;
        state.units.push(ours_soldier);

        (Session::new(state), guard, hq)
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
    fn the_vision_terms_decide_nothing_with_fog_off() {
        use crate::harness::{Limits, play};
        use awvm::session::Session;

        fn game(weights: Weights, fog: bool) -> String {
            let mut first = GreedyAgent::with_weights(1, weights);
            let mut second = GreedyAgent::with_weights(2, weights);
            let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];
            let state = amber_valley(fog, 1);
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

        // With the board lit there is no dark tile to disclose and no terrain
        // that hides anything, so the whole of the vision term is zero however
        // it is weighted. The map is not built there, and this is what says the
        // gate holds.
        assert_eq!(
            game(Weights::SCOUT, false),
            game(Weights::COUNTER, false),
            "the vision terms are fog only"
        );
        // The same weighting under fog does not play the same game, which is
        // what makes the equality above a gate and not a term that reads
        // nothing anywhere.
        assert_ne!(game(Weights::SCOUT, true), game(Weights::COUNTER, true));
    }

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
            ..Weights::TIER1
        });
        assert_eq!(game(Weights::TIER1), read);
    }

    /// A board with nothing left to take is a board that builds soldiers no
    /// faster than it needs them.
    #[test]
    fn production_stops_favouring_capturers_once_the_board_is_covered() {
        let weights = Weights {
            capturer_shortfall: 150.0,
            funds_efficiency: 1.0,
            ..Weights::DEFAULT
        };
        let with_shortfall = |kind| {
            weights.capturer_shortfall
                * 4.0
                * (1000.0 / cost(kind).max(1.0)).powf(weights.funds_efficiency)
        };
        // An infantry against a medium tank, with four properties open and
        // with none.
        let soldier = weights.funds * cost(UnitKind::Infantry) + weights.unit_count;
        let tank = weights.funds * cost(UnitKind::MdTank) + weights.unit_count;
        assert!(
            soldier + with_shortfall(UnitKind::Infantry) > tank + with_shortfall(UnitKind::MdTank),
            "open properties buy soldiers"
        );
        assert!(tank > soldier, "a covered board buys what fights");
    }
}
