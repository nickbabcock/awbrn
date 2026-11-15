//! The second move model: boosted trees over the move features, the unit
//! group, the other orders of the unit, the turn so far and the board near
//! each destination, and a model of which unit acts next.
//!
//! One decision is the orders of one unit, as for the first model
//! ([`super::move_policy`]). The candidate orders are those that the greedy
//! policy plays: wait, capture, attack, load, join and supply. The tool that
//! extracts the human decisions for the fit (`awbrn-ai-diagnostics`, example
//! `move_policy`, `extract-policy2`) calls
//! [`super::GreedyAgent::policy2_candidates`], which runs the code of play.
//! The extractor reads the greedy weights of `planner-v8` and has no fog
//! estimate, so a policy with other greedy weights, or with a fog estimate,
//! computes some move features from other values than the fit saw.
//!
//! The features of one order ([`POLICY2_FEATURE_COUNT`] in all, in this
//! order):
//!
//! - **b**: the 50 move features ([`super::move_policy::MOVE_FEATURES`]).
//! - **l**: 18 features of the board near the destination
//!   ([`LOCAL_FEATURES`]). They read the units in the position of the
//!   decision, which is the observation of the acting player.
//! - **g**: the unit group, one-hot.
//! - **r**: each order against the other orders of the unit: the difference
//!   to the highest, the lowest and the mean value of 16 move features, the
//!   rank of the greedy score, its share of the order count, and the
//!   logarithm of the order count.
//! - **t**: the turn so far ([`TURN_FEATURES`]), from the [`TurnLog`] of the
//!   decision and the counts of units.
//!
//! The turn so far is the unit orders that the acting player gave in this
//! turn before the decision: wait, capture, attack, load, join, supply,
//! hide, reveal, repair and explode. Unloads, builds and powers do not count.
//! In a game the agent keeps the log of the orders that it played; in a
//! planner simulation the log is the orders of the simulated line.

use std::sync::LazyLock;

use awvm::ruleset::UnitKind;
use awvm::semantic::{Location, PlayerIdx, Pos, State, Unit, UnitAction, UnitId};
use awvm::session::{Order, OrderKind};

use super::move_policy::{MOVE_FEATURE_COUNT, MoveFeatures, MoveGroup};

/// The board features of one order.
pub const LOCAL_FEATURES: [&str; 18] = [
    "ctx_dist_od",
    "ctx_friend_d1",
    "ctx_friend_d2",
    "ctx_friend_d3",
    "ctx_moved_d2",
    "ctx_ready_d2",
    "ctx_enemy_d1",
    "ctx_enemy_d2",
    "ctx_enemy_d3",
    "ctx_enemy_d5",
    "ctx_enemy_hp_d3",
    "ctx_enemy_indirect_d6",
    "ctx_last_dest_dist",
    "ctx_target_attacked",
    "ctx_attacked_adjacent",
    "ctx_dest_enemy_property",
    "ctx_dest_neutral_property",
    "ctx_orders_same_dest",
];

/// The move features that the sibling comparison reads, by index in
/// [`super::move_policy::MOVE_FEATURES`].
const RELATIVE: [usize; 16] = [
    0,  // greedy
    12, // dealt_value
    13, // taken_value
    24, // threat_value
    22, // threat_share
    37, // enemy_distance
    36, // enemy_distance_change
    38, // hq_distance_change
    39, // property_distance_change
    25, // defense
    42, // friends_near
    40, // friends_adjacent
    41, // enemies_adjacent
    35, // pull_gain
    11, // move_share
    21, // threat_reduction
];

/// The turn features of one decision. They are the same for each order.
pub const TURN_FEATURES: [&str; 25] = [
    "d_step",
    "d_prev_wait",
    "d_prev_capture",
    "d_prev_attack",
    "d_prev_other",
    "d_prev_foot",
    "d_prev_direct",
    "d_prev_indirect",
    "d_prev_othergrp",
    "d_last_none",
    "d_last_wait",
    "d_last_capture",
    "d_last_attack",
    "d_last_other",
    "d_lastgrp_foot",
    "d_lastgrp_direct",
    "d_lastgrp_indirect",
    "d_lastgrp_other",
    "d_same_kind_prev",
    "d_day",
    "d_count",
    "d_units_ready",
    "d_moved",
    "d_own_units",
    "d_enemy_units",
];

const LOCAL_START: usize = MOVE_FEATURE_COUNT;
const GROUP_START: usize = LOCAL_START + LOCAL_FEATURES.len();
const RELATIVE_START: usize = GROUP_START + 4;
const TURN_START: usize = RELATIVE_START + 3 * RELATIVE.len() + 3;

/// The number of features of one order.
pub const POLICY2_FEATURE_COUNT: usize = TURN_START + TURN_FEATURES.len();

/// The features of one order ([`policy2_feature_names`]).
pub type Policy2Features = [f32; POLICY2_FEATURE_COUNT];

/// The names of the features of one order, in order.
pub fn policy2_feature_names() -> Vec<String> {
    let moves = super::move_policy::MOVE_FEATURES;
    let mut names: Vec<String> = moves.iter().map(|name| (*name).to_owned()).collect();
    names.extend(LOCAL_FEATURES.iter().map(|name| (*name).to_owned()));
    names.extend(
        MoveGroup::ALL
            .iter()
            .map(|group| format!("grp_{}", group.name())),
    );
    for index in RELATIVE {
        let name = moves[index];
        names.push(format!("rel_max_{name}"));
        names.push(format!("rel_min_{name}"));
        names.push(format!("rel_mean_{name}"));
    }
    names.extend(
        ["rel_greedy_rank", "rel_greedy_rank_share", "log_count"]
            .iter()
            .map(|name| (*name).to_owned()),
    );
    names.extend(TURN_FEATURES.iter().map(|name| (*name).to_owned()));
    debug_assert_eq!(names.len(), POLICY2_FEATURE_COUNT);
    names
}

/// The per-unit fields of the next-unit model, before the three blocks of
/// move features (the highest value over the orders of the unit, the order
/// with the best greedy score, and the order that stays).
pub const UNIT_FIELDS: [&str; 14] = [
    "u_foot",
    "u_direct",
    "u_indirect",
    "u_other",
    "u_hp",
    "u_max_greedy",
    "u_orders",
    "u_friend_o2",
    "u_enemy_o3",
    "u_enemy_o6",
    "u_last_dest_dist",
    "u_has_stay",
    "u_nearest_enemy",
    "u_attacked_adjacent",
];

/// The comparisons of each unit with the other units of the decision.
const UNIT_RELATIVE: [&str; 6] = [
    "rel_greedy_gap",
    "greedy_rank",
    "greedy_rank_share",
    "list_pos",
    "list_pos_share",
    "log_units",
];

/// The move features of the highest-value block that the unit comparison
/// reads, by index in [`super::move_policy::MOVE_FEATURES`].
const UNIT_RELATIVE_MAX: [usize; 6] = [
    12, // dealt_value
    14, // kill_certain
    30, // capture_completes
    24, // threat_value
    37, // enemy_distance
    34, // capture_continue
];

/// The turn features that the next-unit model reads: all but the order
/// count of the acting unit and whether its kind repeats the last one, which
/// name the unit that acts.
const UNIT_TURN: [usize; 23] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 19, 21, 22, 23, 24,
];

const UNIT_BLOCKS: usize = UNIT_FIELDS.len();
const UNIT_RELATIVE_START: usize = UNIT_BLOCKS + 3 * MOVE_FEATURE_COUNT;
const UNIT_TURN_START: usize = UNIT_RELATIVE_START + UNIT_RELATIVE.len() + UNIT_RELATIVE_MAX.len();

/// The number of features of one unit.
pub const UNIT_FEATURE_COUNT: usize = UNIT_TURN_START + UNIT_TURN.len();

/// The features of one unit ([`unit_feature_names`]).
pub type UnitFeatures = [f32; UNIT_FEATURE_COUNT];

/// The names of the features of one unit, in order.
pub fn unit_feature_names() -> Vec<String> {
    let moves = super::move_policy::MOVE_FEATURES;
    let mut names: Vec<String> = UNIT_FIELDS.iter().map(|name| (*name).to_owned()).collect();
    for block in ["max", "best", "stay"] {
        names.extend(moves.iter().map(|name| format!("{block}_{name}")));
    }
    names.extend(UNIT_RELATIVE.iter().map(|name| (*name).to_owned()));
    names.extend(
        UNIT_RELATIVE_MAX
            .iter()
            .map(|index| format!("rel_max_{}", moves[*index])),
    );
    names.extend(
        UNIT_TURN
            .iter()
            .map(|index| TURN_FEATURES[*index].to_owned()),
    );
    debug_assert_eq!(names.len(), UNIT_FEATURE_COUNT);
    names
}

/// The cap of the distances of the board features.
const FAR: u32 = 40;

/// The kind of action of one order of the turn so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionClass {
    Wait,
    Capture,
    Attack,
    /// Load, join, supply, hide, reveal, repair and explode.
    Other,
}

impl ActionClass {
    /// The class of an order kind, or `None` for an order that the log does
    /// not keep.
    pub const fn of(kind: OrderKind) -> Option<Self> {
        Some(match kind {
            OrderKind::Wait => Self::Wait,
            OrderKind::Capture => Self::Capture,
            OrderKind::Attack(_) => Self::Attack,
            OrderKind::Load
            | OrderKind::Join
            | OrderKind::Supply
            | OrderKind::Hide
            | OrderKind::Reveal
            | OrderKind::Repair(_)
            | OrderKind::Explode => Self::Other,
            _ => return None,
        })
    }

    /// The class of the action name of a replay row (`wait`, `attack`, ...).
    pub fn of_name(name: &str) -> Self {
        match name {
            "wait" => Self::Wait,
            "capture" => Self::Capture,
            "attack" => Self::Attack,
            _ => Self::Other,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Wait => 0,
            Self::Capture => 1,
            Self::Attack => 2,
            Self::Other => 3,
        }
    }
}

/// One unit order of the turn so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LogEntry {
    pub class: ActionClass,
    pub kind: UnitKind,
    pub destination: Pos,
    /// The tile that an attack targets.
    pub target: Option<Pos>,
}

impl LogEntry {
    /// The entry of `order` in the position `state` before the order, or
    /// `None` for an order that the log does not keep.
    pub fn of_order(state: &State, order: Order) -> Option<Self> {
        let unit = state.units.at(usize::from(order.unit()?.get()))?;
        Self::of_kind(state, unit.kind, order.destination(), order.kind())
    }

    /// The entry of `play` in the position `state` before the play.
    pub fn of_play(state: &State, play: &crate::agent::Play) -> Option<Self> {
        let unit = state.units.get(play.unit()?)?;
        Self::of_kind(state, unit.kind, play.destination(), play.kind())
    }

    fn of_kind(
        state: &State,
        kind: UnitKind,
        destination: awvm::semantic::CellIdx,
        order: OrderKind,
    ) -> Option<Self> {
        let class = ActionClass::of(order)?;
        let dimensions = state.board.dimensions();
        let target = match order {
            OrderKind::Attack(cell) => dimensions.position_of(cell),
            _ => None,
        };
        Some(Self {
            class,
            kind,
            destination: dimensions.position_of(destination)?,
            target,
        })
    }
}

/// An empty turn so far, for tests that build a planner by hand.
#[cfg(test)]
pub(crate) static EMPTY_TURN_LOG: TurnLog = TurnLog::new();

/// The unit orders of the acting player in this turn so far, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnLog {
    entries: Vec<LogEntry>,
}

impl TurnLog {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn push(&mut self, entry: LogEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A key of the log for the selection memory of the greedy policy.
    pub(super) fn key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::hash::DefaultHasher::new();
        self.entries.hash(&mut hasher);
        // Zero is the key of a policy without the log.
        hasher.finish() | 1
    }

    fn last_destination(&self) -> Option<Pos> {
        self.entries.last().map(|entry| entry.destination)
    }

    fn attacked(&self, position: Pos) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.target == Some(position))
    }
}

fn manhattan(a: Pos, b: Pos) -> u32 {
    u32::from(a.x.abs_diff(b.x)) + u32::from(a.y.abs_diff(b.y))
}

/// The units of the position that the board and turn features read, built
/// once for each position when a unit first needs them.
#[derive(Debug, Default)]
pub(super) struct Policy2Context {
    built: bool,
    /// Our units on the board: position, identity and whether it is ready.
    own: Vec<(Pos, UnitId, bool)>,
    /// The enemy units on the board: position, health in tenths and whether
    /// the unit is in the indirect group.
    enemy: Vec<(Pos, f32, bool)>,
    ready: u32,
    day: u64,
}

impl Policy2Context {
    pub(super) const fn new() -> Self {
        Self {
            built: false,
            own: Vec::new(),
            enemy: Vec::new(),
            ready: 0,
            day: 0,
        }
    }

    pub(super) fn clear(&mut self) {
        self.built = false;
    }

    pub(super) fn prepare(&mut self, state: &State, seat: PlayerIdx) {
        if self.built {
            return;
        }
        self.built = true;
        self.own.clear();
        self.enemy.clear();
        self.ready = 0;
        self.day = state.turn.day;
        for unit in state.units.iter() {
            let Location::Board { position } = unit.location else {
                continue;
            };
            if unit.owner == seat {
                let ready = unit.action == UnitAction::Ready;
                self.ready += u32::from(ready);
                self.own.push((position, unit.id, ready));
            } else if crate::threat::hostile(state, seat, unit.owner) {
                self.enemy.push((
                    position,
                    f32::from(unit.hp) / 10.0,
                    MoveGroup::of(unit.kind) == MoveGroup::Indirect,
                ));
            }
        }
    }
}

/// Whether the policy plays an order of this kind, so that the order is one
/// candidate of the second model.
pub(super) const fn is_candidate(kind: OrderKind) -> bool {
    matches!(
        kind,
        OrderKind::Wait
            | OrderKind::Capture
            | OrderKind::Attack(_)
            | OrderKind::Load
            | OrderKind::Join
            | OrderKind::Supply
    )
}

/// The turn features of a decision of `unit` with `count` orders.
fn turn_features(
    context: &Policy2Context,
    log: &TurnLog,
    unit: &Unit,
    count: usize,
) -> [f32; TURN_FEATURES.len()] {
    let mut d = [0.0_f32; TURN_FEATURES.len()];
    d[0] = log.len() as f32;
    for entry in log.entries() {
        d[1 + entry.class.index()] += 1.0;
        d[5 + MoveGroup::of(entry.kind) as usize] += 1.0;
    }
    match log.entries().last() {
        None => d[9] = 1.0,
        Some(last) => {
            d[10 + last.class.index()] = 1.0;
            d[14 + MoveGroup::of(last.kind) as usize] = 1.0;
            d[18] = f32::from(last.kind == unit.kind);
        }
    }
    d[19] = context.day as f32;
    d[20] = count as f32;
    d[21] = context.ready as f32;
    d[22] = context.own.iter().filter(|(_, _, ready)| !ready).count() as f32;
    d[23] = context.own.len() as f32;
    d[24] = context.enemy.len() as f32;
    d
}

/// The features of the orders of one unit. `orders` and `moves` are the
/// candidate orders of the unit and their move features (the greedy score
/// features set), in the order of the legal orders. The features go to
/// `out`, one entry for each order.
#[allow(clippy::too_many_arguments)]
pub(super) fn order_features(
    context: &Policy2Context,
    log: &TurnLog,
    state: &State,
    seat: PlayerIdx,
    unit: &Unit,
    origin: Pos,
    orders: &[Order],
    moves: &[MoveFeatures],
    out: &mut Vec<Policy2Features>,
) {
    out.clear();
    let count = orders.len();
    if count == 0 {
        return;
    }
    let dimensions = state.board.dimensions();
    let turn = turn_features(context, log, unit, count);
    let last = log.last_destination();
    let group = MoveGroup::of(unit.kind) as usize;
    for (order, features) in orders.iter().zip(moves) {
        let mut x = [0.0_f32; POLICY2_FEATURE_COUNT];
        for (slot, value) in x.iter_mut().zip(features.iter()) {
            *slot = *value as f32;
        }
        if let Some(destination) = dimensions.position_of(order.destination()) {
            let l = &mut x[LOCAL_START..GROUP_START];
            l[0] = manhattan(origin, destination) as f32;
            for (position, id, ready) in &context.own {
                if *id == unit.id {
                    continue;
                }
                let d = manhattan(*position, destination);
                l[1] += f32::from(d <= 1);
                l[2] += f32::from(d <= 2);
                l[3] += f32::from(d <= 3);
                l[4] += f32::from(d <= 2 && !ready);
                l[5] += f32::from(d <= 2 && *ready);
            }
            for (position, hp, indirect) in &context.enemy {
                let d = manhattan(*position, destination);
                l[6] += f32::from(d <= 1);
                l[7] += f32::from(d <= 2);
                l[8] += f32::from(d <= 3);
                l[9] += f32::from(d <= 5);
                if d <= 3 {
                    l[10] += hp;
                }
                if d <= 6 && *indirect {
                    l[11] += 1.0;
                }
                if d <= 1 && log.attacked(*position) {
                    l[14] += 1.0;
                }
            }
            l[12] = last.map_or(FAR as f32, |last| {
                manhattan(last, destination).min(FAR) as f32
            });
            if let OrderKind::Attack(cell) = order.kind()
                && let Some(target) = dimensions.position_of(cell)
            {
                l[13] = f32::from(log.attacked(target));
            }
            if let Some(tile) = state.board.get(destination) {
                match tile.owner {
                    awvm::semantic::TileOwner::Owned(owner)
                        if crate::threat::hostile(state, seat, owner) =>
                    {
                        l[15] = 1.0;
                    }
                    awvm::semantic::TileOwner::Neutral => l[16] = 1.0,
                    _ => {}
                }
            }
            l[17] = orders
                .iter()
                .filter(|other| other.destination() == order.destination())
                .count() as f32;
        }
        x[GROUP_START + group] = 1.0;
        x[TURN_START..].copy_from_slice(&turn);
        out.push(x);
    }
    // The sibling comparison, in single precision as the fit computed it.
    for (slot, index) in RELATIVE.iter().enumerate() {
        let column = RELATIVE_START + 3 * slot;
        let mut high = f32::NEG_INFINITY;
        let mut low = f32::INFINITY;
        let mut sum = 0.0_f64;
        for x in out.iter() {
            let value = x[*index];
            high = high.max(value);
            low = low.min(value);
            sum += f64::from(value);
        }
        let mean = (sum / count as f64) as f32;
        for x in out.iter_mut() {
            let value = x[*index];
            x[column] = value - high;
            x[column + 1] = value - low;
            x[column + 2] = value - mean;
        }
    }
    // The rank of the greedy score, highest first; a tie keeps the order of
    // the legal orders.
    let mut ranked: Vec<usize> = (0..count).collect();
    ranked.sort_by(|left, right| {
        out[*right][0]
            .partial_cmp(&out[*left][0])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let at = RELATIVE_START + 3 * RELATIVE.len();
    let log_count = (count as f64).ln() as f32;
    for (rank, index) in ranked.into_iter().enumerate() {
        let x = &mut out[index];
        x[at] = rank as f32;
        x[at + 1] = (rank as f64 / count as f64) as f32;
        x[at + 2] = log_count;
    }
}

/// The fields and move feature blocks of one unit for the next-unit model.
/// `totals` are the greedy scores of the orders. The comparisons with the
/// other units and the turn features are set by [`finish_unit_rows`].
#[allow(clippy::too_many_arguments)]
pub(super) fn unit_row(
    context: &Policy2Context,
    log: &TurnLog,
    unit: &Unit,
    origin: Pos,
    orders: &[Order],
    moves: &[MoveFeatures],
    totals: &[f64],
    origin_cell: awvm::semantic::CellIdx,
) -> UnitFeatures {
    let mut row = [0.0_f32; UNIT_FEATURE_COUNT];
    row[MoveGroup::of(unit.kind) as usize] = 1.0;
    row[4] = f32::from(unit.hp) / 10.0;
    let base = UNIT_BLOCKS;
    let mut best = f64::NEG_INFINITY;
    let mut best_index = None;
    let mut stay_index = None;
    let mut high = [f32::NEG_INFINITY; MOVE_FEATURE_COUNT];
    for (index, ((order, features), total)) in orders.iter().zip(moves).zip(totals).enumerate() {
        for (slot, value) in high.iter_mut().zip(features.iter()) {
            *slot = slot.max(*value as f32);
        }
        if *total > best {
            best = *total;
            best_index = Some(index);
        }
        if order.kind() == OrderKind::Wait && order.destination() == origin_cell {
            stay_index = Some(index);
        }
    }
    row[5] = best as f32;
    row[6] = orders.len() as f32;
    row[base..base + MOVE_FEATURE_COUNT].copy_from_slice(&high);
    if let Some(index) = best_index {
        for (slot, value) in row[base + MOVE_FEATURE_COUNT..base + 2 * MOVE_FEATURE_COUNT]
            .iter_mut()
            .zip(moves[index].iter())
        {
            *slot = *value as f32;
        }
    }
    if let Some(index) = stay_index {
        row[11] = 1.0;
        for (slot, value) in row[base + 2 * MOVE_FEATURE_COUNT..base + 3 * MOVE_FEATURE_COUNT]
            .iter_mut()
            .zip(moves[index].iter())
        {
            *slot = *value as f32;
        }
    }
    row[7] = context
        .own
        .iter()
        .filter(|(position, id, _)| *id != unit.id && manhattan(*position, origin) <= 2)
        .count() as f32;
    let mut nearest = FAR;
    for (position, _, _) in &context.enemy {
        let d = manhattan(*position, origin);
        row[8] += f32::from(d <= 3);
        row[9] += f32::from(d <= 6);
        nearest = nearest.min(d);
        if d <= 1 && log.attacked(*position) {
            row[13] += 1.0;
        }
    }
    row[12] = nearest as f32;
    row[10] = log
        .last_destination()
        .map_or(FAR as f32, |last| manhattan(last, origin).min(FAR) as f32);
    row
}

/// Set the comparisons of each unit with the other units of the decision,
/// and the turn features. `rows` are the units in the order of the legal
/// orders.
pub(super) fn finish_unit_rows(context: &Policy2Context, log: &TurnLog, rows: &mut [UnitFeatures]) {
    let count = rows.len();
    if count == 0 {
        return;
    }
    let units = count as f32;
    let high = rows
        .iter()
        .map(|row| row[5])
        .fold(f32::NEG_INFINITY, f32::max);
    let mut ranked: Vec<usize> = (0..count).collect();
    ranked.sort_by(|left, right| {
        rows[*right][5]
            .partial_cmp(&rows[*left][5])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut ranks = vec![0.0_f32; count];
    for (rank, index) in ranked.into_iter().enumerate() {
        ranks[index] = rank as f32;
    }
    let mut highs = [f32::NEG_INFINITY; UNIT_RELATIVE_MAX.len()];
    for row in rows.iter() {
        for (slot, index) in highs.iter_mut().zip(UNIT_RELATIVE_MAX) {
            *slot = slot.max(row[UNIT_BLOCKS + index]);
        }
    }
    // The turn features; the order count and the kind of the acting unit
    // are not read.
    let mut turn = [0.0_f32; TURN_FEATURES.len()];
    turn[0] = log.len() as f32;
    for entry in log.entries() {
        turn[1 + entry.class.index()] += 1.0;
        turn[5 + MoveGroup::of(entry.kind) as usize] += 1.0;
    }
    match log.entries().last() {
        None => turn[9] = 1.0,
        Some(last) => {
            turn[10 + last.class.index()] = 1.0;
            turn[14 + MoveGroup::of(last.kind) as usize] = 1.0;
        }
    }
    turn[19] = context.day as f32;
    turn[21] = context.ready as f32;
    turn[22] = context.own.iter().filter(|(_, _, ready)| !ready).count() as f32;
    turn[23] = context.own.len() as f32;
    turn[24] = context.enemy.len() as f32;
    for (position, row) in rows.iter_mut().enumerate() {
        let at = UNIT_RELATIVE_START;
        row[at] = row[5] - high;
        row[at + 1] = ranks[position];
        row[at + 2] = ranks[position] / units;
        row[at + 3] = position as f32;
        row[at + 4] = position as f32 / units;
        row[at + 5] = units.ln();
        for (slot, index) in UNIT_RELATIVE_MAX.iter().enumerate() {
            row[at + 6 + slot] = row[UNIT_BLOCKS + index] - highs[slot];
        }
        for (slot, index) in UNIT_TURN.iter().enumerate() {
            row[UNIT_TURN_START + slot] = turn[*index];
        }
    }
}

/// A sum of regression trees, as LightGBM fits it.
///
/// A split sends a value that is at most its threshold to the left. The
/// features are single precision, as in the fit. LightGBM compares them in
/// double precision with a double threshold; for a single precision value
/// `x`, `f64(x) <= t` holds exactly when `x` is at most the largest single
/// precision value at or below `t`, so the trees keep that value and compare
/// in single precision with the same result.
///
/// Each tree keeps its nodes together, with child indices inside the tree,
/// so that one tree is a few hundred bytes. [`TreeEnsemble::score_rows`]
/// walks each tree for all the orders of a unit before the next tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeEnsemble {
    features: usize,
    trees: Vec<Tree>,
    nodes: Vec<Node>,
    leaves: Vec<f64>,
    /// The same trees as complete binary trees of one depth, when no tree
    /// is deeper than [`COMPLETE_DEPTH`]: a walk is then a fixed number of
    /// steps with no branch.
    complete: Option<Complete>,
}

/// The deepest trees that [`Complete`] holds.
const COMPLETE_DEPTH: usize = 8;

/// Complete binary trees of depth `depth`, in level order. A leaf above the
/// last level is a node that always goes left (threshold infinity), down to
/// a leaf with its value.
#[derive(Clone, Debug, PartialEq)]
struct Complete {
    depth: usize,
    /// For each tree, `2^depth - 1` features and thresholds.
    features: Vec<u16>,
    thresholds: Vec<f32>,
    /// For each tree, `2^depth` leaves.
    leaves: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Tree {
    /// The first node and the first leaf of the tree.
    nodes: u32,
    leaves: u32,
    /// The root: a node of the tree, or the complement of a leaf of the
    /// tree for a tree with no split.
    root: i16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Node {
    threshold: f32,
    feature: u16,
    /// A node of the tree, or the complement of a leaf of the tree.
    left: i16,
    right: i16,
}

/// The largest single precision value at or below `value`.
fn floor_f32(value: f64) -> f32 {
    let mut out = value as f32;
    if f64::from(out) > value {
        out = out.next_down();
    }
    out
}

impl TreeEnsemble {
    /// Build the trees from the roots, nodes and leaves of a model file,
    /// whose children are indices into all nodes and all leaves.
    fn new(
        features: usize,
        roots: &[i32],
        nodes: &[(u16, f64, i32, i32)],
        leaves: &[f64],
    ) -> Result<Self, String> {
        let mut out = Self {
            features,
            trees: Vec::with_capacity(roots.len()),
            nodes: Vec::with_capacity(nodes.len()),
            leaves: Vec::with_capacity(leaves.len()),
            complete: None,
        };
        for root in roots {
            let tree = Tree {
                nodes: out.nodes.len() as u32,
                leaves: out.leaves.len() as u32,
                root: 0,
            };
            let root = out.copy(&tree, *root, nodes, leaves, 0)?;
            out.trees.push(Tree { root, ..tree });
        }
        let depth = out
            .trees
            .iter()
            .map(|tree| out.depth(tree, tree.root))
            .max()
            .unwrap_or(0);
        if depth <= COMPLETE_DEPTH {
            out.complete = Some(out.complete_trees(depth.max(1)));
        }
        Ok(out)
    }

    fn depth(&self, tree: &Tree, at: i16) -> usize {
        if at < 0 {
            return 0;
        }
        let node = &self.nodes[tree.nodes as usize + at as usize];
        1 + self
            .depth(tree, node.left)
            .max(self.depth(tree, node.right))
    }

    fn complete_trees(&self, depth: usize) -> Complete {
        let inner = (1 << depth) - 1;
        let mut complete = Complete {
            depth,
            features: vec![0; inner * self.trees.len()],
            thresholds: vec![f32::INFINITY; inner * self.trees.len()],
            leaves: vec![0.0; (inner + 1) * self.trees.len()],
        };
        for (index, tree) in self.trees.iter().enumerate() {
            self.fill(tree, tree.root, 0, 0, depth, index, &mut complete);
        }
        complete
    }

    /// Put the subtree at `at` of `tree` at `slot` (level order, at `level`)
    /// of complete tree `index`.
    #[allow(clippy::too_many_arguments)]
    fn fill(
        &self,
        tree: &Tree,
        at: i16,
        slot: usize,
        level: usize,
        depth: usize,
        index: usize,
        complete: &mut Complete,
    ) {
        let inner = (1 << depth) - 1;
        if at < 0 {
            // Every leaf below `slot`; the walk reaches the leftmost one,
            // because the nodes below keep the infinite threshold.
            let value = self.leaves[tree.leaves as usize + !at as usize];
            let mut first = slot;
            let mut last = slot;
            for _ in level..depth {
                first = 2 * first + 1;
                last = 2 * last + 2;
            }
            for leaf in first..=last {
                complete.leaves[index * (inner + 1) + leaf - inner] = value;
            }
            return;
        }
        let node = &self.nodes[tree.nodes as usize + at as usize];
        complete.features[index * inner + slot] = node.feature;
        complete.thresholds[index * inner + slot] = node.threshold;
        self.fill(
            tree,
            node.left,
            2 * slot + 1,
            level + 1,
            depth,
            index,
            complete,
        );
        self.fill(
            tree,
            node.right,
            2 * slot + 2,
            level + 1,
            depth,
            index,
            complete,
        );
    }

    /// Copy the subtree at `at` (a global node, or the complement of a
    /// global leaf) into `tree`, and return its index in the tree.
    fn copy(
        &mut self,
        tree: &Tree,
        at: i32,
        nodes: &[(u16, f64, i32, i32)],
        leaves: &[f64],
        depth: usize,
    ) -> Result<i16, String> {
        if depth > 64 {
            return Err("a tree is too deep".into());
        }
        let local = |count: usize, start: u32| {
            i16::try_from(count - start as usize).map_err(|_| "a tree is too large".to_owned())
        };
        if at < 0 {
            let leaf = *leaves
                .get(!at as usize)
                .ok_or("a tree refers to a missing leaf")?;
            let index = local(self.leaves.len(), tree.leaves)?;
            self.leaves.push(leaf);
            return Ok(!index);
        }
        let (feature, threshold, left, right) = *nodes
            .get(at as usize)
            .ok_or("a tree refers to a missing node")?;
        if usize::from(feature) >= self.features {
            return Err("a tree refers to a missing feature".into());
        }
        let index = local(self.nodes.len(), tree.nodes)?;
        self.nodes.push(Node {
            threshold: floor_f32(threshold),
            feature,
            left: 0,
            right: 0,
        });
        let left = self.copy(tree, left, nodes, leaves, depth + 1)?;
        let right = self.copy(tree, right, nodes, leaves, depth + 1)?;
        let node = &mut self.nodes[tree.nodes as usize + index as usize];
        node.left = left;
        node.right = right;
        Ok(index)
    }

    #[inline]
    fn leaf(&self, tree: &Tree, x: &[f32]) -> f64 {
        let nodes = &self.nodes[tree.nodes as usize..];
        let mut at = tree.root;
        while at >= 0 {
            let node = &nodes[at as usize];
            at = if x[usize::from(node.feature)] <= node.threshold {
                node.left
            } else {
                node.right
            };
        }
        self.leaves[tree.leaves as usize + !at as usize]
    }

    /// The raw score of one row, by the walk of the nodes.
    #[cfg(test)]
    fn score(&self, x: &[f32]) -> f64 {
        debug_assert_eq!(x.len(), self.features);
        let mut sum = 0.0;
        for tree in &self.trees {
            sum += self.leaf(tree, x);
        }
        sum
    }

    /// The raw score of each row, the same values as [`TreeEnsemble::score`].
    pub fn score_rows<const N: usize>(&self, rows: &[[f32; N]], out: &mut Vec<f64>) {
        debug_assert_eq!(N, self.features);
        // A split sends NaN to the right, where LightGBM, with a missing value
        // rule, could send it left. The features are always finite.
        debug_assert!(
            rows.iter().flatten().all(|value| value.is_finite()),
            "a feature is not finite"
        );
        out.clear();
        out.resize(rows.len(), 0.0);
        if let Some(complete) = &self.complete {
            complete.score_rows(rows, out);
            return;
        }
        for tree in &self.trees {
            for (sum, x) in out.iter_mut().zip(rows) {
                *sum += self.leaf(tree, x);
            }
        }
    }

    /// The raw score of each row by the walk of the nodes, for a test of
    /// the complete trees.
    #[cfg(test)]
    fn score_rows_by_nodes<const N: usize>(&self, rows: &[[f32; N]]) -> Vec<f64> {
        rows.iter().map(|x| self.score(x)).collect()
    }

    /// The number of trees.
    pub fn trees(&self) -> usize {
        self.trees.len()
    }
}

impl Complete {
    /// Add the score of each tree to each row, tree by tree, so that one
    /// tree stays in the cache for all rows. Four rows walk together.
    fn score_rows<const N: usize>(&self, rows: &[[f32; N]], out: &mut [f64]) {
        let depth = self.depth;
        let inner = (1 << depth) - 1;
        let trees = self.thresholds.len() / inner;
        for tree in 0..trees {
            let features = &self.features[tree * inner..(tree + 1) * inner];
            let thresholds = &self.thresholds[tree * inner..(tree + 1) * inner];
            let leaves = &self.leaves[tree * (inner + 1)..(tree + 1) * (inner + 1)];
            let step = |x: &[f32; N], at: usize| {
                2 * at + 1 + usize::from(x[usize::from(features[at])] > thresholds[at])
            };
            let (chunks, rest) = rows.as_chunks::<4>();
            let (sums, rest_sums) = out.as_chunks_mut::<4>();
            for (x, sum) in chunks.iter().zip(sums) {
                let (mut a, mut b, mut c, mut d) = (0, 0, 0, 0);
                for _ in 0..depth {
                    a = step(&x[0], a);
                    b = step(&x[1], b);
                    c = step(&x[2], c);
                    d = step(&x[3], d);
                }
                sum[0] += leaves[a - inner];
                sum[1] += leaves[b - inner];
                sum[2] += leaves[c - inner];
                sum[3] += leaves[d - inner];
            }
            for (x, sum) in rest.iter().zip(rest_sums) {
                let mut at = 0;
                for _ in 0..depth {
                    at = step(x, at);
                }
                *sum += leaves[at - inner];
            }
        }
    }
}

/// A reader of the little-endian model files.
struct Reader<'a> {
    bytes: &'a [u8],
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8], String> {
        if self.bytes.len() < count {
            return Err("the model file ends early".into());
        }
        let (head, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(head)
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> Result<f64, String> {
        let value = f64::from_le_bytes(self.take(8)?.try_into().unwrap());
        if value.is_finite() {
            Ok(value)
        } else {
            Err("a model value is not finite".into())
        }
    }

    /// A count of items of at least `size` bytes each. A count that the
    /// rest of the file cannot hold is an error, so that a damaged file does
    /// not ask for a large allocation.
    fn count(&mut self, size: usize) -> Result<usize, String> {
        let count = self.u32()? as usize;
        if count.saturating_mul(size) > self.bytes.len() {
            return Err("the model file ends early".into());
        }
        Ok(count)
    }
}

/// A fitted model file: the feature names and the trees.
///
/// The layout, all little-endian: the magic `AWT1`; the feature count and
/// each name (a `u16` length and UTF-8); the extra value count and each
/// value (`f64`), which the models do not read; the tree count and the root
/// of each tree (`i32`); the node count and each node (feature `u16`,
/// threshold `f64`, left `i32`, right `i32`); the leaf count and each leaf
/// value (`f64`). A child or root that is negative is the complement of a
/// leaf index.
struct ModelFile {
    trees: TreeEnsemble,
}

impl ModelFile {
    fn parse(bytes: &[u8], expected: &[String]) -> Result<Self, String> {
        let mut reader = Reader { bytes };
        if reader.take(4)? != b"AWT1" {
            return Err("not a tree model file".into());
        }
        let count = reader.count(2)?;
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let length = usize::from(reader.u16()?);
            let name = std::str::from_utf8(reader.take(length)?)
                .map_err(|error| error.to_string())?
                .to_owned();
            names.push(name);
        }
        if names != expected {
            return Err("the model features are not the features of this policy".into());
        }
        for _ in 0..reader.count(8)? {
            reader.f64()?;
        }
        let roots = (0..reader.count(4)?)
            .map(|_| reader.i32())
            .collect::<Result<Vec<_>, _>>()?;
        // A node is a feature (2 bytes), a threshold (8) and two children (4
        // each).
        let node_count = reader.count(18)?;
        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let feature = reader.u16()?;
            let threshold = reader.f64()?;
            let left = reader.i32()?;
            let right = reader.i32()?;
            nodes.push((feature, threshold, left, right));
        }
        let leaves = (0..reader.count(8)?)
            .map(|_| reader.f64())
            .collect::<Result<Vec<_>, _>>()?;
        if !reader.bytes.is_empty() {
            return Err("the model file has trailing bytes".into());
        }
        let trees = TreeEnsemble::new(count, &roots, &nodes, &leaves)?;
        Ok(Self { trees })
    }
}

/// The second move model: trees over [`policy2_feature_names`]. The
/// utility of an order is its tree score, the log-odds of the human choice.
#[derive(Clone, Debug, PartialEq)]
pub struct Move2Model {
    trees: TreeEnsemble,
}

impl Move2Model {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let file = ModelFile::parse(bytes, &policy2_feature_names())?;
        Ok(Self { trees: file.trees })
    }

    /// The utility of each order of a unit.
    pub fn utilities(&self, rows: &[Policy2Features], out: &mut Vec<f64>) {
        self.trees.score_rows(rows, out);
    }
}

/// The next-unit model: trees over [`unit_feature_names`].
#[derive(Clone, Debug, PartialEq)]
pub struct UnitModel {
    trees: TreeEnsemble,
}

impl UnitModel {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let file = ModelFile::parse(bytes, &unit_feature_names())?;
        Ok(Self { trees: file.trees })
    }

    /// The utility of each unit.
    pub fn utilities(&self, rows: &[UnitFeatures], out: &mut Vec<f64>) {
        self.trees.score_rows(rows, out);
    }
}

macro_rules! asset {
    ($name:literal) => {
        concat!("../../../../assets/ai-diagnostics/move-policy2/", $name)
    };
}

/// The tree model for games without fog.
pub fn tree_model() -> &'static Move2Model {
    static MODEL: LazyLock<Move2Model> = LazyLock::new(|| {
        Move2Model::from_bytes(include_bytes!(asset!("move2-nofog.awt")))
            .expect("the move2 model is valid")
    });
    &MODEL
}

/// The tree model for games with fog.
pub fn fog_tree_model() -> &'static Move2Model {
    static MODEL: LazyLock<Move2Model> = LazyLock::new(|| {
        Move2Model::from_bytes(include_bytes!(asset!("move2-fog.awt")))
            .expect("the fog move2 model is valid")
    });
    &MODEL
}

/// The next-unit model for games without fog.
pub fn unit_model() -> &'static UnitModel {
    static MODEL: LazyLock<UnitModel> = LazyLock::new(|| {
        UnitModel::from_bytes(include_bytes!(asset!("unit2-nofog.awt")))
            .expect("the next-unit model is valid")
    });
    &MODEL
}

/// The next-unit model for games with fog.
pub fn fog_unit_model() -> &'static UnitModel {
    static MODEL: LazyLock<UnitModel> = LazyLock::new(|| {
        UnitModel::from_bytes(include_bytes!(asset!("unit2-fog.awt")))
            .expect("the fog next-unit model is valid")
    });
    &MODEL
}

const _: () = assert!(POLICY2_FEATURE_COUNT == 148);
const _: () = assert!(UNIT_FEATURE_COUNT == 14 + 150 + 12 + 23);

/// One candidate order of a unit with the features of the second model.
#[derive(Clone, Debug, PartialEq)]
pub struct Move2Candidate {
    pub order: Order,
    pub unit: UnitId,
    pub features: Policy2Features,
}

/// The features of the second model in one position: the candidate orders
/// of each unit, and the next-unit features of each unit with a candidate
/// order, in the order of the legal orders.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Policy2Outputs {
    pub orders: Vec<Move2Candidate>,
    pub units: Vec<(UnitId, UnitFeatures)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::agent::Agent;
    use crate::agents::GreedyAgent;
    use crate::board::amber_valley;
    use crate::rng::Rng;
    use awvm::semantic::{AwbwVisibility, Match, observe};
    use awvm::session::Session;

    /// The rows and raw LightGBM scores of a fixture that the fit wrote
    /// (`policy2/fit.py --fixture`).
    fn fixture(name: &str) -> (usize, Vec<f32>, Vec<f64>) {
        let path = format!(
            "{}/../../assets/ai-diagnostics/move-policy2/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes = std::fs::read(&path).expect("the fixture is present");
        let rows = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let columns = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let mut at = 8;
        let x = (0..rows * columns)
            .map(|_| {
                let value = f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
                at += 4;
                value
            })
            .collect();
        let raw = (0..rows)
            .map(|_| {
                let value = f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
                at += 8;
                value
            })
            .collect();
        assert_eq!(at, bytes.len());
        (columns, x, raw)
    }

    fn assert_scores(trees: &TreeEnsemble, name: &str) {
        let (columns, x, raw) = fixture(name);
        assert!(raw.len() >= 50, "{name} has enough rows");
        let mut batch = Vec::new();
        match columns {
            POLICY2_FEATURE_COUNT => {
                let rows: Vec<Policy2Features> = x
                    .chunks(columns)
                    .map(|row| row.try_into().unwrap())
                    .collect();
                trees.score_rows(&rows, &mut batch);
            }
            UNIT_FEATURE_COUNT => {
                let rows: Vec<UnitFeatures> = x
                    .chunks(columns)
                    .map(|row| row.try_into().unwrap())
                    .collect();
                trees.score_rows(&rows, &mut batch);
            }
            _ => panic!("{name} has {columns} columns"),
        }
        if columns == POLICY2_FEATURE_COUNT {
            let rows: Vec<Policy2Features> = x
                .chunks(columns)
                .map(|row| row.try_into().unwrap())
                .collect();
            let walked = trees.score_rows_by_nodes(&rows);
            assert!(
                walked
                    .iter()
                    .zip(&batch)
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
                "{name}: the complete trees score as the nodes"
            );
        }
        for ((row, expected), batched) in x.chunks(columns).zip(&raw).zip(&batch) {
            let score = trees.score(row);
            assert_eq!(
                score.to_bits(),
                batched.to_bits(),
                "{name}: batch and row agree"
            );
            assert!(
                (score - expected).abs() <= 1e-9 * expected.abs().max(1.0),
                "{name}: {score} against LightGBM {expected}"
            );
        }
    }

    /// The Rust trees give the raw scores of LightGBM on held-out rows.
    #[test]
    fn the_trees_score_as_lightgbm() {
        assert_scores(&tree_model().trees, "move2-nofog.rows");
        assert_scores(&fog_tree_model().trees, "move2-fog.rows");
        assert_scores(&unit_model().trees, "unit2-nofog.rows");
        assert_scores(&fog_unit_model().trees, "unit2-fog.rows");
    }

    #[test]
    fn the_models_load() {
        assert!(tree_model().trees.trees() > 0);
        assert!(fog_tree_model().trees.trees() > 0);
        assert!(unit_model().trees.trees() > 0);
        assert!(fog_unit_model().trees.trees() > 0);
        assert_ne!(tree_model(), fog_tree_model());
    }

    #[test]
    fn a_bad_model_file_is_refused() {
        Move2Model::from_bytes(b"AWT0").unwrap_err();
        UnitModel::from_bytes(include_bytes!(asset!("move2-nofog.awt"))).unwrap_err();
    }

    /// The bytes of a model file in the layout of [`ModelFile`].
    fn model_file(
        names: &[&str],
        roots: &[i32],
        nodes: &[(u16, f64, i32, i32)],
        leaves: &[f64],
    ) -> Vec<u8> {
        let mut out = b"AWT1".to_vec();
        out.extend((names.len() as u32).to_le_bytes());
        for name in names {
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(name.as_bytes());
        }
        out.extend(1_u32.to_le_bytes());
        out.extend(0.5_f64.to_le_bytes());
        out.extend((roots.len() as u32).to_le_bytes());
        for root in roots {
            out.extend(root.to_le_bytes());
        }
        out.extend((nodes.len() as u32).to_le_bytes());
        for (feature, threshold, left, right) in nodes {
            out.extend(feature.to_le_bytes());
            out.extend(threshold.to_le_bytes());
            out.extend(left.to_le_bytes());
            out.extend(right.to_le_bytes());
        }
        out.extend((leaves.len() as u32).to_le_bytes());
        for leaf in leaves {
            out.extend(leaf.to_le_bytes());
        }
        out
    }

    /// A damaged model file gives an error, never a panic or a large
    /// allocation: every truncation, trailing bytes, a child, root or
    /// feature out of range, a node cycle and a huge count.
    #[test]
    fn a_damaged_model_file_is_refused() {
        let names = ["a", "b"];
        let expected: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        let parse = |bytes: &[u8]| ModelFile::parse(bytes, &expected).map(|file| file.trees);
        // Two trees: a split on feature 1 with two leaves, and a single leaf.
        let good = model_file(&names, &[0, !2], &[(1, 0.5, !0, !1)], &[1.0, 2.0, 3.0]);
        let trees = parse(&good).expect("the file is valid");
        assert_eq!(trees.score(&[0.0, 0.0]), 1.0 + 3.0);
        assert_eq!(trees.score(&[0.0, 1.0]), 2.0 + 3.0);
        for end in 0..good.len() {
            assert!(parse(&good[..end]).is_err(), "truncated at {end}");
        }
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(parse(&trailing).is_err(), "trailing bytes");
        for (roots, nodes, what) in [
            (
                vec![0, !2],
                vec![(1, 0.5, 3, !1)],
                "a child node out of range",
            ),
            (
                vec![0, !2],
                vec![(1, 0.5, !0, !7)],
                "a child leaf out of range",
            ),
            (vec![4, !2], vec![(1, 0.5, !0, !1)], "a root out of range"),
            (
                vec![0, !2],
                vec![(2, 0.5, !0, !1)],
                "a feature out of range",
            ),
            (vec![0, !2], vec![(1, 0.5, 0, !1)], "a node cycle"),
        ] {
            let bytes = model_file(&names, &roots, &nodes, &[1.0, 2.0, 3.0]);
            assert!(parse(&bytes).is_err(), "{what}");
        }
        let mut huge = good.clone();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&huge).is_err(), "a huge count");
    }

    /// Play a short game between two greedy agents that use the second
    /// model and keep their own turn so far. At each decision:
    ///
    /// - the turn so far of the agent is the log of the orders that it
    ///   played in this turn and the host accepted, which the test keeps;
    /// - the features that play calculated for every unit are the features
    ///   that the extractor reads for the same position and log, bit for bit.
    fn check_game(fog: bool, weights: crate::agents::Weights) -> usize {
        let mut agents = [
            GreedyAgent::with_generic_tactical_policy(1, weights),
            GreedyAgent::with_generic_tactical_policy(2, weights),
        ];
        let mut extractor = crate::profile::V8_GREEDY_CONFIG.build_greedy(9);
        let mut session = Session::new(amber_valley(fog, 1));
        let mut entropy = Rng::from_seed(3);
        let mut log = TurnLog::new();
        let mut turn = None;
        let mut compared = 0;
        let mut refused = 0;
        for _ in 0..600 {
            let state = session.state().clone();
            if !matches!(state.match_state, Match::Active { .. }) || state.turn.day > 6 {
                break;
            }
            let player = state.turn.active_player.clone();
            let key = (player.clone(), state.turn.day);
            if turn.as_ref() != Some(&key) {
                turn = Some(key);
                log.clear();
            }
            let seat = state.players.seat(&player).expect("seat").get();
            let view = observe(&AwbwVisibility, &state, &player).expect("observe");
            let agent = &mut agents[seat];
            agent.capture_policy2();
            let play = agent.act(&view, crate::agent::NodeBudget::ONE);
            let played = agent.take_policy2_capture().expect("captured");
            assert_eq!(agent.turn_log(), &log, "the agent keeps the turn so far");
            let (_, extracted) = extractor
                .policy2_candidates(&view, &log)
                .expect("candidates");
            for candidate in &played.orders {
                let found = extracted
                    .orders
                    .iter()
                    .find(|other| other.order == candidate.order && other.unit == candidate.unit)
                    .expect("the extractor has the order");
                assert!(
                    found
                        .features
                        .iter()
                        .zip(&candidate.features)
                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                    "features of {:?}",
                    candidate.order
                );
                compared += 1;
            }
            if !played.units.is_empty() {
                assert_eq!(played.units, extracted.units, "next-unit features");
            }
            let command = match play {
                Some(play) => {
                    let entry = LogEntry::of_play(&state, &play);
                    // Refuse every seventh unit order the way the server does:
                    // no reject and no refresh, and the host asks again. The
                    // refused order must not join the turn so far.
                    if entry.is_some() && (compared + refused) % 7 == 3 {
                        refused += 1;
                        continue;
                    }
                    if let Some(entry) = entry {
                        log.push(entry);
                    }
                    play.command(&session).expect("command")
                }
                None => awvm::transition::Command::EndTurn {
                    player: player.clone(),
                },
            };
            let order = session.resolve(&command).expect("legal");
            session
                .apply(order, &mut entropy, &mut ())
                .expect("applies");
            // The host reports an accepted order that does not end the turn.
            let after = session.state();
            if matches!(after.match_state, Match::Active { .. })
                && after.turn.active_player == player
            {
                let view = observe(&AwbwVisibility, after, &player).expect("observe");
                agents[seat].refresh(&view);
            }
        }
        assert!(refused > 0, "the game has refused orders");
        compared
    }

    #[test]
    fn play_reads_the_features_that_the_extractor_writes() {
        let base = crate::profile::V8_GREEDY_CONFIG.weights;
        for weights in [
            crate::agents::Weights {
                move2: true,
                ..base
            },
            crate::agents::Weights {
                move2: true,
                move2_order: true,
                ..base
            },
        ] {
            assert!(check_game(false, weights) > 1_000);
            assert!(check_game(true, weights) > 1_000);
        }
    }
}
