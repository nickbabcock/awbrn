//! A unit order policy fitted to the unit orders of human players.
//!
//! One decision is one order of one unit: a destination and an action there
//! (wait, attack a target, capture, load, join, supply, unload). The choices
//! are all the legal orders of that unit. The model is a conditional logit
//! over the features of each order ([`MOVE_FEATURES`]). The greedy score of
//! the order is one group of features, so the model can keep the greedy
//! choice where the other features do not disagree.
//!
//! The greedy policy uses the model in this way: the greedy scores still
//! decide which unit acts next and when the turn ends, and the model decides
//! the order of the unit. The order that the model chooses gets the highest
//! greedy score of the orders of the unit, and the other orders of the unit
//! get zero. This is the same method as the build model
//! ([`crate::build_policy`]).
//!
//! The tool that extracts the human orders for the fit (`awbrn-ai-diagnostics`,
//! example `move_policy`) calls [`GreedyAgent::move_candidates`], so the fit
//! and the play read the same values.
//!
//! [`GreedyAgent::move_candidates`]: super::GreedyAgent::move_candidates

use std::sync::LazyLock;

use awvm::combat::Forecast;
use awvm::ruleset::{self, Domain, FireMode, TerrainTrait, UnitKind};
use awvm::semantic::{Location, Pos, TileOwner, UnitId};
use awvm::session::{Order, OrderKind};

use super::GreedyScoreBreakdown;
use super::greedy::{Board, CaptureTurn, Scorer, WHOLE_PROPERTY, capture_strength, cost};

/// The names of the features of one unit order, in order.
pub const MOVE_FEATURES: [&str; 50] = [
    // The greedy score. The extractor and the policy set these three after
    // all the orders of the unit are scored.
    "greedy",
    "greedy_gap",
    "greedy_top",
    // The action. Wait is the reference.
    "act_capture",
    "act_attack",
    "act_load",
    "act_join",
    "act_supply",
    "act_unload",
    "act_other",
    // The movement.
    "stay",
    "move_share",
    // The attack.
    "dealt_value",
    "taken_value",
    "kill_certain",
    "kill_possible",
    "attacker_lost",
    "denial",
    "target_indirect",
    "target_hp",
    "no_counter",
    "threat_reduction",
    // The threat at the destination.
    "threat_share",
    "deferred_share",
    "threat_value",
    // The destination tile.
    "defense",
    "own_property",
    "own_factory",
    "other_property",
    "heal",
    // The capture.
    "capture_completes",
    "capture_hq",
    "capture_factory",
    "capture_income",
    "capture_continue",
    // The distances.
    "pull_gain",
    "enemy_distance_change",
    "enemy_distance",
    "hq_distance_change",
    "property_distance_change",
    // The support.
    "friends_adjacent",
    "enemies_adjacent",
    "friends_near",
    // The interactions.
    "indirect_moved",
    "foot_threat",
    "lead_threat",
    "lead_dealt",
    "late_threat",
    "guard",
    "unit_hp",
];

/// The number of move features.
pub const MOVE_FEATURE_COUNT: usize = MOVE_FEATURES.len();

/// The features of one order.
pub type MoveFeatures = [f64; MOVE_FEATURE_COUNT];

const GREEDY: usize = 0;
const GREEDY_GAP: usize = 1;
const GREEDY_TOP: usize = 2;
/// The first feature that is not a greedy score feature.
const OWN_START: usize = 3;

/// The scale of the greedy score in the greedy features, in score points.
const GREEDY_SCALE: f64 = 50.0;

/// One legal order of a unit with its greedy score and its features.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveCandidate {
    /// The legal order.
    pub order: Order,
    /// The unit that acts.
    pub unit: UnitId,
    /// The greedy score and its terms.
    pub breakdown: GreedyScoreBreakdown,
    /// The features ([`MOVE_FEATURES`]).
    pub features: MoveFeatures,
}

/// The values of a combat forecast that the features read. A forecast holds
/// a vector, so the policy keeps these values and not the forecast.
#[derive(Clone, Copy, Debug)]
pub(super) struct ForecastValues {
    attack_low: u16,
    attack_high: u16,
    counter: Option<(u16, u16)>,
    attacker_hp: u8,
    target_hp: u8,
}

impl From<&Forecast> for ForecastValues {
    fn from(forecast: &Forecast) -> Self {
        Self {
            attack_low: forecast.attack.low,
            attack_high: forecast.attack.high,
            counter: forecast.counter.map(|counter| (counter.low, counter.high)),
            attacker_hp: forecast.attacker_hp,
            target_hp: forecast.target_hp,
        }
    }
}

/// The board values that the features read, calculated once for each
/// position when a unit first needs them ([`MoveContext::prepare`]).
///
/// The vectors stay in place from one position to the next, so that a new
/// position does not allocate.
#[derive(Debug)]
pub(super) struct MoveContext {
    /// Whether the values other than `property_distance` are for the
    /// current position.
    built: bool,
    /// Whether `property_distance` is for the current position.
    properties_built: bool,
    width: usize,
    height: usize,
    /// The distance from each tile to the nearest enemy unit, capped.
    enemy_distance: Vec<u8>,
    /// The distance from each tile to the nearest enemy headquarters or, on
    /// a map with no headquarters, the nearest enemy factory, capped.
    hq_distance: Vec<u8>,
    /// The distance from each tile to the nearest property that we can
    /// capture, capped. Only the features of a unit that can capture read
    /// it.
    property_distance: Vec<u8>,
    /// The number of our units next to each tile.
    friends_adjacent: Vec<u8>,
    /// The number of our units at most three tiles from each tile.
    friends_near: Vec<u8>,
    /// The number of enemy units next to each tile.
    enemies_adjacent: Vec<u8>,
    /// Our share of the army value on the board, less one half.
    lead: f64,
    /// Whether the day is 15 or later.
    late: bool,
    /// The positions of the enemy units.
    enemies: Vec<Pos>,
    /// The positions of the enemy units that can capture.
    capturers: Vec<Pos>,
    /// The positions that the other distance fields start from.
    headquarters: Vec<Pos>,
    factories: Vec<Pos>,
    properties: Vec<Pos>,
}

/// The cap of the distance fields.
const FAR: u8 = 40;

impl MoveContext {
    pub(super) const fn new() -> Self {
        Self {
            built: false,
            properties_built: false,
            width: 0,
            height: 0,
            enemy_distance: Vec::new(),
            hq_distance: Vec::new(),
            property_distance: Vec::new(),
            friends_adjacent: Vec::new(),
            friends_near: Vec::new(),
            enemies_adjacent: Vec::new(),
            lead: 0.0,
            late: false,
            enemies: Vec::new(),
            capturers: Vec::new(),
            headquarters: Vec::new(),
            factories: Vec::new(),
            properties: Vec::new(),
        }
    }

    /// Mark the values as out of date, for a new position.
    pub(super) fn clear(&mut self) {
        self.built = false;
        self.properties_built = false;
    }

    /// Calculate the values that the features of a unit read, if they are
    /// out of date. `capturer` tells whether the unit can capture.
    pub(super) fn prepare(&mut self, board: &Board<'_>, capturer: bool) {
        if !self.built {
            self.build(board);
            self.built = true;
        }
        if capturer && !self.properties_built {
            distance_field(
                &self.properties,
                self.width,
                self.height,
                &mut self.property_distance,
            );
            self.properties_built = true;
        }
    }

    fn build(&mut self, board: &Board<'_>) {
        let state = board.state;
        let seat = board.seat;
        let cells = state.board.dimensions().len();
        let width = usize::from(state.board.width());
        let height = usize::from(state.board.height());
        self.width = width;
        self.height = height;
        for field in [
            &mut self.friends_adjacent,
            &mut self.friends_near,
            &mut self.enemies_adjacent,
        ] {
            field.clear();
            field.resize(cells, 0);
        }
        self.enemies.clear();
        self.capturers.clear();
        let (mut ours, mut theirs) = (0.0, 0.0);
        for unit in state.units.iter() {
            let Location::Board { position } = unit.location else {
                continue;
            };
            let value = cost(unit.kind) * f64::from(unit.hp) / 100.0;
            if unit.owner == seat {
                ours += value;
                count(&mut self.friends_adjacent, width, height, position, 1, true);
                count(&mut self.friends_near, width, height, position, 3, false);
            } else if board.hostile(unit.owner) {
                self.enemies.push(position);
                theirs += value;
                count(&mut self.enemies_adjacent, width, height, position, 1, true);
                if ruleset::profile(unit.kind).can_capture {
                    self.capturers.push(position);
                }
            }
        }
        self.lead = if ours + theirs > 0.0 {
            ours / (ours + theirs) - 0.5
        } else {
            0.0
        };
        self.late = state.turn.day >= 15;
        self.headquarters.clear();
        self.factories.clear();
        self.properties.clear();
        for (position, tile) in state.board.iter() {
            if !ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable) {
                continue;
            }
            let enemy = matches!(tile.owner, TileOwner::Owned(holder) if board.hostile(holder));
            if enemy && ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner) {
                self.headquarters.push(position);
            }
            if enemy && ruleset::terrain_has(tile.terrain, TerrainTrait::ProducesGround) {
                self.factories.push(position);
            }
            if board.capturable(tile) {
                self.properties.push(position);
            }
        }
        let goals = if self.headquarters.is_empty() {
            &self.factories
        } else {
            &self.headquarters
        };
        distance_field(&self.enemies, width, height, &mut self.enemy_distance);
        distance_field(goals, width, height, &mut self.hq_distance);
    }

    /// Whether an enemy unit that can capture is at most three tiles from
    /// `position`.
    fn capturer_near(&self, position: Pos) -> bool {
        self.capturers
            .iter()
            .any(|capturer| capturer.distance(position) <= 3)
    }

    fn at(field: &[u8], cell: usize) -> f64 {
        f64::from(field.get(cell).copied().unwrap_or(FAR))
    }
}

/// Add one to each tile at most `radius` tiles from `center`. With `ring`,
/// add only to the tiles at exactly `radius` tiles.
fn count(field: &mut [u8], width: usize, height: usize, center: Pos, radius: i32, ring: bool) {
    let (cx, cy) = (i32::from(center.x), i32::from(center.y));
    for dy in -radius..=radius {
        let rest = radius - dy.abs();
        for dx in -rest..=rest {
            if ring && dx.abs() + dy.abs() != radius {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x as usize >= width || y as usize >= height {
                continue;
            }
            let cell = y as usize * width + x as usize;
            field[cell] = field[cell].saturating_add(1);
        }
    }
}

/// The Manhattan distance from each tile to the nearest target, capped at
/// [`FAR`]. Two passes over the grid give the exact distance. All values stay
/// at or below [`FAR`], so one more step cannot overflow.
fn distance_field(targets: &[Pos], width: usize, height: usize, out: &mut Vec<u8>) {
    out.clear();
    out.resize(width * height, FAR);
    if width == 0 {
        return;
    }
    for target in targets {
        let (x, y) = (usize::from(target.x), usize::from(target.y));
        if x < width && y < height {
            out[y * width + x] = 0;
        }
    }
    // From the top left: the tile above, then the tile to the left.
    for y in 0..height {
        let (done, rest) = out.split_at_mut(y * width);
        let row = &mut rest[..width];
        if y > 0 {
            for (value, above) in row.iter_mut().zip(&done[(y - 1) * width..]) {
                *value = (*value).min(above + 1);
            }
        }
        for x in 1..width {
            row[x] = row[x].min(row[x - 1] + 1);
        }
    }
    // From the bottom right: the tile below, then the tile to the right.
    for y in (0..height).rev() {
        let (rest, done) = out.split_at_mut((y + 1) * width);
        let row = &mut rest[y * width..];
        if y + 1 < height {
            for (value, below) in row.iter_mut().zip(&done[..width]) {
                *value = (*value).min(below + 1);
            }
        }
        for x in (0..width - 1).rev() {
            row[x] = row[x].min(row[x + 1] + 1);
        }
    }
}

/// The values of one unit that all its orders read.
#[derive(Clone, Copy, Debug)]
pub(super) struct UnitBasis {
    origin: Pos,
    origin_cell: usize,
    allowance: f64,
    origin_pull: f64,
}

impl Scorer<'_> {
    /// Whether the move features of `order` are defined: the order has a
    /// unit on the board, and its destination is a tile.
    pub(super) fn has_move_features(&self, order: Order) -> bool {
        let Some(unit) = order.unit().and_then(|index| self.unit(index)) else {
            return false;
        };
        let dimensions = self.board.state.board.dimensions();
        matches!(unit.location, Location::Board { position } if dimensions.cell_index(position).is_some())
            && dimensions
                .position_of(order.destination())
                .and_then(|position| self.board.state.board.get(position))
                .is_some()
    }

    /// The values of the unit of `order` that all its orders read. Call it
    /// only when [`Scorer::has_move_features`] is true for the order.
    pub(super) fn unit_basis(&self, order: Order) -> Option<UnitBasis> {
        let index = order.unit()?;
        let unit = self.unit(index)?;
        let Location::Board { position: origin } = unit.location else {
            return None;
        };
        let dimensions = self.board.state.board.dimensions();
        let origin_index = dimensions.cell_index(origin)?;
        let profile = ruleset::profile(unit.kind);
        let allowance = awvm::commander::effective_move(
            self.board.state,
            unit,
            profile.movement,
            profile.domain,
        )
        .max(1);
        Some(UnitBasis {
            origin,
            origin_cell: usize::from(origin_index.get()),
            allowance: allowance as f64,
            origin_pull: self.pull(Order::new(index, origin_index, OrderKind::Wait)),
        })
    }

    /// The features of one unit order, without the greedy score features.
    /// `None` only when [`Scorer::has_move_features`] is false.
    pub(super) fn move_features(
        &self,
        context: &MoveContext,
        basis: &UnitBasis,
        order: Order,
        forecast: Option<&ForecastValues>,
        breakdown: &GreedyScoreBreakdown,
    ) -> Option<MoveFeatures> {
        let unit = self.unit(order.unit()?)?;
        let origin = basis.origin;
        let dimensions = self.board.state.board.dimensions();
        let destination = dimensions.position_of(order.destination())?;
        let cell = usize::from(order.destination().get());
        let origin_cell = basis.origin_cell;
        let profile = ruleset::profile(unit.kind);
        let unit_cost = cost(unit.kind).max(1.0);
        let hp = f64::from(unit.hp) / 100.0;
        let mut f = [0.0; MOVE_FEATURE_COUNT];
        let flag = |value: bool| if value { 1.0 } else { 0.0 };
        let mut dealt_value = 0.0;
        let mut set = |name: Feature, value: f64| f[name as usize] = value;

        match order.kind() {
            OrderKind::Wait => {}
            OrderKind::Capture => set(Feature::ActCapture, 1.0),
            OrderKind::Attack(_) => set(Feature::ActAttack, 1.0),
            OrderKind::Load => set(Feature::ActLoad, 1.0),
            OrderKind::Join => set(Feature::ActJoin, 1.0),
            OrderKind::Supply => set(Feature::ActSupply, 1.0),
            OrderKind::Unload(_) => set(Feature::ActUnload, 1.0),
            _ => set(Feature::ActOther, 1.0),
        }
        let moved = destination != origin;
        set(Feature::Stay, flag(!moved));
        set(
            Feature::MoveShare,
            origin.distance(destination) as f64 / basis.allowance,
        );

        // The attack.
        if let OrderKind::Attack(target) = order.kind()
            && let Some(defender) = self.unit_at(target)
        {
            let defender_profile = ruleset::profile(defender.kind);
            set(
                Feature::TargetIndirect,
                flag(defender_profile.fire_mode == FireMode::Indirect),
            );
            set(Feature::TargetHp, f64::from(defender.hp) / 100.0);
            if let Some(forecast) = forecast {
                let mean = |low: u16, high: u16| f64::from(low + high) / 2.0;
                let target_hp = f64::from(forecast.target_hp);
                let dealt = mean(forecast.attack_low, forecast.attack_high).min(target_hp) / 100.0;
                let taken = forecast.counter.map_or(0.0, |(low, high)| {
                    mean(low, high).min(f64::from(forecast.attacker_hp)) / 100.0
                });
                dealt_value = dealt * cost(defender.kind) / 1000.0;
                set(Feature::DealtValue, dealt_value);
                set(Feature::TakenValue, taken * unit_cost / 1000.0);
                set(
                    Feature::KillCertain,
                    flag(f64::from(forecast.attack_low) >= target_hp),
                );
                set(
                    Feature::KillPossible,
                    flag(f64::from(forecast.attack_high) >= target_hp),
                );
                set(
                    Feature::AttackerLost,
                    flag(taken * 100.0 >= f64::from(forecast.attacker_hp)),
                );
                set(Feature::NoCounter, flag(forecast.counter.is_none()));
            } else {
                // A strike that the position cannot forecast: the greedy
                // blind value, in thousands of funds.
                dealt_value = self.weights().blind_attack * cost(defender.kind) / 1000.0;
                set(Feature::DealtValue, dealt_value);
            }
            if let Some(attack) = breakdown.attack {
                set(Feature::Denial, flag(attack.capture_denial > 0.0));
                set(
                    Feature::ThreatReduction,
                    (attack.target_threat_reduction / 20.0).asinh(),
                );
            }
        }

        // The threat at the destination. A unit that loads or joins leaves
        // the board or merges, so its own threat is not counted.
        let on_board = !matches!(order.kind(), OrderKind::Load | OrderKind::Join);
        let (immediate, deferred) = if on_board {
            (
                self.threat.immediate(order.destination(), unit.kind),
                self.threat.deferred(order.destination(), unit.kind),
            )
        } else {
            (0.0, 0.0)
        };
        let threat_share = (immediate / unit_cost).min(1.0);
        set(Feature::ThreatShare, threat_share);
        set(Feature::DeferredShare, (deferred / unit_cost).min(1.0));
        set(Feature::ThreatValue, immediate.min(unit_cost) * hp / 1000.0);

        // The destination tile.
        let tile = self.board.state.board.get(destination)?;
        if profile.domain != Domain::Air {
            set(
                Feature::Defense,
                f64::from(ruleset::defense_stars(tile.terrain)) / 4.0,
            );
        }
        let property = ruleset::terrain_has(tile.terrain, TerrainTrait::Capturable);
        let own = property && tile.owner == TileOwner::Owned(self.board.seat);
        set(Feature::OwnProperty, flag(own));
        let factory = [
            TerrainTrait::ProducesGround,
            TerrainTrait::ProducesAir,
            TerrainTrait::ProducesSea,
        ]
        .into_iter()
        .any(|value| ruleset::terrain_has(tile.terrain, value));
        set(Feature::OwnFactory, flag(own && factory));
        set(
            Feature::OtherProperty,
            flag(property && !own && !matches!(order.kind(), OrderKind::Capture)),
        );
        if own && ruleset::terrain_has(tile.terrain, profile.domain.repairs()) && on_board {
            set(Feature::Heal, 1.0 - hp);
        }
        set(
            Feature::Guard,
            flag(own && context.capturer_near(destination)),
        );

        // The capture.
        if matches!(order.kind(), OrderKind::Capture) {
            let points = tile.capture_points.unwrap_or(WHOLE_PROPERTY);
            let strength = capture_strength(
                self.board.state,
                unit,
                unit.hp.div_ceil(10),
                CaptureTurn::Now,
            );
            set(Feature::CaptureCompletes, flag(strength >= points));
            let has = |value| ruleset::terrain_has(tile.terrain, value);
            if has(TerrainTrait::CaptureDefeatsOwner) {
                set(Feature::CaptureHq, 1.0);
            } else if factory {
                set(Feature::CaptureFactory, 1.0);
            } else {
                set(Feature::CaptureIncome, 1.0);
            }
            set(
                Feature::CaptureContinue,
                flag(!moved && points < WHOLE_PROPERTY),
            );
        }

        // The distances.
        set(
            Feature::PullGain,
            ((self.pull(order) - basis.origin_pull) / 20.0).asinh(),
        );
        let enemy_after = MoveContext::at(&context.enemy_distance, cell);
        let enemy_before = MoveContext::at(&context.enemy_distance, origin_cell);
        set(
            Feature::EnemyDistanceChange,
            (enemy_after - enemy_before) / 10.0,
        );
        set(Feature::EnemyDistance, enemy_after.min(20.0) / 10.0);
        set(
            Feature::HqDistanceChange,
            (MoveContext::at(&context.hq_distance, cell)
                - MoveContext::at(&context.hq_distance, origin_cell))
                / 10.0,
        );
        if profile.can_capture {
            set(
                Feature::PropertyDistanceChange,
                (MoveContext::at(&context.property_distance, cell)
                    - MoveContext::at(&context.property_distance, origin_cell))
                    / 10.0,
            );
        }

        // The support.
        // The counts include the unit itself at its origin, so it is taken
        // out where its origin is in the range.
        let gap = origin.distance(destination);
        let at = |field: &[u8]| f64::from(field.get(cell).copied().unwrap_or(0));
        let adjacent = at(&context.friends_adjacent) - if gap == 1 { 1.0 } else { 0.0 };
        let near = at(&context.friends_near) - if gap <= 3 { 1.0 } else { 0.0 };
        let enemies_adjacent = at(&context.enemies_adjacent);
        set(Feature::FriendsAdjacent, adjacent / 4.0);
        set(Feature::EnemiesAdjacent, enemies_adjacent / 4.0);
        set(Feature::FriendsNear, (near / 8.0).min(2.0));

        // The interactions.
        set(
            Feature::IndirectMoved,
            flag(profile.fire_mode == FireMode::Indirect && moved),
        );
        set(
            Feature::FootThreat,
            if profile.can_capture {
                threat_share
            } else {
                0.0
            },
        );
        set(Feature::LeadThreat, context.lead * threat_share);
        set(Feature::LeadDealt, context.lead * dealt_value);
        set(
            Feature::LateThreat,
            if context.late { threat_share } else { 0.0 },
        );
        set(Feature::UnitHp, hp);
        Some(f)
    }
}

/// The index of each feature in [`MOVE_FEATURES`].
#[derive(Clone, Copy)]
#[repr(usize)]
enum Feature {
    ActCapture = 3,
    ActAttack,
    ActLoad,
    ActJoin,
    ActSupply,
    ActUnload,
    ActOther,
    Stay,
    MoveShare,
    DealtValue,
    TakenValue,
    KillCertain,
    KillPossible,
    AttackerLost,
    Denial,
    TargetIndirect,
    TargetHp,
    NoCounter,
    ThreatReduction,
    ThreatShare,
    DeferredShare,
    ThreatValue,
    Defense,
    OwnProperty,
    OwnFactory,
    OtherProperty,
    Heal,
    CaptureCompletes,
    CaptureHq,
    CaptureFactory,
    CaptureIncome,
    CaptureContinue,
    PullGain,
    EnemyDistanceChange,
    EnemyDistance,
    HqDistanceChange,
    PropertyDistanceChange,
    FriendsAdjacent,
    EnemiesAdjacent,
    FriendsNear,
    IndirectMoved,
    FootThreat,
    LeadThreat,
    LeadDealt,
    LateThreat,
    Guard,
    UnitHp,
}

/// Set the greedy score features of one order from its greedy score and the
/// highest greedy score of the orders of its unit.
pub fn set_greedy_features(best: f64, score: f64, features: &mut MoveFeatures) {
    features[GREEDY] = (score / GREEDY_SCALE).asinh();
    features[GREEDY_GAP] = ((best - score) / GREEDY_SCALE).asinh();
    features[GREEDY_TOP] = if score >= best { 1.0 } else { 0.0 };
}

/// Which unit group a coefficient vector of the model is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveGroup {
    /// Infantry and mech.
    Foot,
    /// Direct ground units that cannot capture.
    Direct,
    /// Indirect units.
    Indirect,
    /// Air and naval units, and transports.
    Other,
}

impl MoveGroup {
    const ALL: [Self; 4] = [Self::Foot, Self::Direct, Self::Indirect, Self::Other];

    const fn name(self) -> &'static str {
        match self {
            Self::Foot => "foot",
            Self::Direct => "direct",
            Self::Indirect => "indirect",
            Self::Other => "other",
        }
    }

    fn of(kind: UnitKind) -> Self {
        let profile = ruleset::profile(kind);
        if profile.can_capture {
            Self::Foot
        } else if profile.domain != Domain::Ground || profile.transport.is_some() {
            Self::Other
        } else if profile.fire_mode == FireMode::Indirect {
            Self::Indirect
        } else {
            Self::Direct
        }
    }
}

/// The name of the unit group of `kind` that gives the coefficients of its
/// orders: `foot`, `direct`, `indirect` or `other`. The extractor writes this
/// name in each row, so that the fit uses the same groups as the policy.
pub fn group_name(kind: UnitKind) -> &'static str {
    MoveGroup::of(kind).name()
}

/// A fitted move model: one coefficient vector over [`MOVE_FEATURES`] for
/// each unit group.
#[derive(Clone, Debug, PartialEq)]
pub struct MoveModel {
    groups: [MoveFeatures; 4],
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveModelFile {
    schema_version: u32,
    features: Vec<String>,
    /// The coefficients of each group, by group name. A group that the file
    /// does not name uses `all`.
    groups: std::collections::BTreeMap<String, Vec<f64>>,
    #[serde(default)]
    #[allow(dead_code)]
    notes: serde_json::Value,
}

impl MoveModel {
    /// Read a model file. A feature that the file does not name has a
    /// coefficient of zero.
    fn from_json(text: &str) -> Result<Self, String> {
        let file: MoveModelFile = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if file.schema_version != 1 {
            return Err(format!("unknown schema version {}", file.schema_version));
        }
        let mut index = Vec::with_capacity(file.features.len());
        for name in &file.features {
            let position = MOVE_FEATURES
                .iter()
                .position(|known| known == name)
                .ok_or_else(|| format!("unknown move feature {name}"))?;
            index.push(position);
        }
        let read = |values: &Vec<f64>| -> Result<MoveFeatures, String> {
            if values.len() != index.len() {
                return Err("a coefficient vector has the wrong length".into());
            }
            let mut out = [0.0; MOVE_FEATURE_COUNT];
            for (value, position) in values.iter().zip(&index) {
                if !value.is_finite() {
                    return Err("a coefficient is not finite".into());
                }
                out[*position] = *value;
            }
            Ok(out)
        };
        let all = file.groups.get("all").map(read).transpose()?;
        let mut groups = [[0.0; MOVE_FEATURE_COUNT]; 4];
        for (slot, group) in groups.iter_mut().zip(MoveGroup::ALL) {
            *slot = match file.groups.get(group.name()) {
                Some(values) => read(values)?,
                None => all.ok_or_else(|| format!("no coefficients for {}", group.name()))?,
            };
        }
        Ok(Self { groups })
    }

    /// The utility of one order. `strength` multiplies the part of the
    /// utility that does not come from the greedy score.
    pub fn utility(&self, kind: UnitKind, features: &MoveFeatures, strength: f64) -> f64 {
        let coefficients = &self.groups[MoveGroup::of(kind) as usize];
        let greedy: f64 = coefficients[..OWN_START]
            .iter()
            .zip(&features[..OWN_START])
            .map(|(beta, value)| beta * value)
            .sum();
        let own: f64 = coefficients[OWN_START..]
            .iter()
            .zip(&features[OWN_START..])
            .map(|(beta, value)| beta * value)
            .sum();
        greedy + strength * own
    }
}

/// The move model of this crate (`human-move-v1-blend.json`).
pub fn model() -> &'static MoveModel {
    static MODEL: LazyLock<MoveModel> =
        LazyLock::new(|| MoveModel::from_json(MODEL_JSON).expect("the move model is valid"));
    &MODEL
}

const MODEL_JSON: &str =
    include_str!("../../../../assets/ai-diagnostics/move-policy/human-move-v1-blend.json");

// Each feature index agrees with its name in `MOVE_FEATURES`.
const _: () = assert!(Feature::ActCapture as usize == OWN_START);
const _: () = assert!(Feature::DealtValue as usize == 12);
const _: () = assert!(Feature::PullGain as usize == 35);
const _: () = assert!(Feature::UnitHp as usize == MOVE_FEATURE_COUNT - 1);

#[cfg(test)]
mod tests {
    use super::*;

    /// The distance field is the capped Manhattan distance to the nearest
    /// target.
    #[test]
    fn the_distance_field_is_the_capped_distance_to_the_nearest_target() {
        type Grid = (usize, usize, &'static [(u8, u8)]);
        let grids: [Grid; 4] = [
            (1, 1, &[]),
            (7, 3, &[(0, 0)]),
            (22, 18, &[(3, 4), (20, 1), (11, 17)]),
            (60, 50, &[(0, 0)]),
        ];
        let mut field = Vec::new();
        for (width, height, targets) in grids {
            let targets: Vec<Pos> = targets.iter().map(|(x, y)| Pos::new(*x, *y)).collect();
            distance_field(&targets, width, height, &mut field);
            for y in 0..height {
                for x in 0..width {
                    let here = Pos::new(x as u8, y as u8);
                    let expected = targets
                        .iter()
                        .map(|target| target.distance(here))
                        .min()
                        .map_or(FAR, |distance| distance.min(u64::from(FAR)) as u8);
                    assert_eq!(
                        field[y * width + x],
                        expected,
                        "{width}x{height} at {x},{y}"
                    );
                }
            }
        }
    }
}
