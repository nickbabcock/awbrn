//! Coordinated kills: which of our units destroy which enemy units.
//!
//! The greedy policy scores one attack at a time. An attack that does not
//! destroy its target often scores low, so the attack that sets up a kill is
//! not chosen. This module looks at sets of attackers for each target.
//!
//! 1. Collect every legal attack on an enemy unit, with its forecast and its
//!    destination tile.
//! 2. For each target, list the smallest attacker sets whose low forecast
//!    damage together reaches the target health. These kills do not depend
//!    on luck, except for counter damage to our attackers.
//! 3. Choose disjoint kills across targets with branch and bound. Two kills
//!    must not use the same attacker or the same destination tile.
//!
//! The result is a short list of kill plans. Each plan is an ordered list of
//! attack orders. The planner plays a plan in a simulation, then lets the
//! greedy policy finish the turn, and scores the result.

use awvm::ruleset::{self, UnitKind};
use awvm::semantic::{CellIdx, PlayerIdx, UnitId};
use awvm::session::{AttackCandidate, LegalVisitor, Order, Session};

/// The largest attacker set the solver considers for one target.
const MAX_ATTACKERS: usize = 3;

/// The largest number of kill options kept for one target.
const OPTIONS_PER_TARGET: usize = 6;

/// The largest number of nodes in each combat search phase.
const MAX_NODES: usize = 20_000;

/// One legal attack on an enemy unit.
#[derive(Clone, Copy, Debug)]
pub(super) struct AttackOption {
    pub attacker: UnitId,
    pub attacker_kind: UnitKind,
    pub destination: CellIdx,
    pub target: UnitId,
    pub target_hp: u8,
    pub target_kind: UnitKind,
    pub low: u16,
    pub high: u16,
    /// The largest counter damage, if the target survives this attack.
    pub counter_high: u16,
}

/// One way to destroy one target.
#[derive(Clone, Debug)]
pub(super) struct Kill {
    /// The attacks, in the order to play them.
    pub attacks: Vec<AttackOption>,
    /// The target value removed, less the expected counter damage.
    pub value: f64,
}

/// A set of disjoint kills for one turn.
#[derive(Clone, Debug)]
pub(super) struct KillPlan {
    pub kills: Vec<Kill>,
    pub value: f64,
}

impl KillPlan {
    /// The attack orders of the plan, in play order.
    pub(super) fn orders(&self) -> impl Iterator<Item = &AttackOption> {
        self.kills.iter().flat_map(|kill| kill.attacks.iter())
    }
}

struct Collector {
    ours: PlayerIdx,
    options: Vec<AttackOption>,
}

impl LegalVisitor for Collector {
    const ATTACK_CONTEXT: bool = true;

    fn order(&mut self, _order: Order) {}

    fn attack(&mut self, candidate: AttackCandidate<'_>) {
        let (Some(target), Some(forecast)) = (candidate.target_unit, candidate.forecast) else {
            return;
        };
        if target.owner == self.ours || candidate.attacker.owner != self.ours {
            return;
        }
        self.options.push(AttackOption {
            attacker: candidate.attacker.id,
            attacker_kind: candidate.attacker.kind,
            destination: candidate.order.destination(),
            target: target.id,
            target_hp: target.hp,
            target_kind: target.kind,
            low: forecast.attack.low,
            high: forecast.attack.high,
            counter_high: forecast.counter.map_or(0, |counter| counter.high),
        });
    }
}

/// Every legal attack of the active seat on an enemy unit.
pub(super) fn attack_options(session: &Session) -> Vec<AttackOption> {
    let state = session.state();
    let Some(ours) = state.players.seat(&state.turn.active_player) else {
        return Vec::new();
    };
    let mut collector = Collector {
        ours,
        options: Vec::new(),
    };
    session.legal().visit_orders(&mut collector);
    collector.options
}

fn cost(kind: UnitKind) -> f64 {
    ruleset::profile(kind).cost as f64
}

/// All minimal kills of each target, best first, at most a few per target.
pub(super) fn kills(options: &[AttackOption]) -> Vec<Vec<Kill>> {
    let mut targets: Vec<UnitId> = options.iter().map(|option| option.target).collect();
    targets.sort_unstable();
    targets.dedup();
    let mut result = Vec::new();
    let mut remaining_nodes = MAX_NODES;
    let target_count = targets.len();
    for (index, target) in targets.into_iter().enumerate() {
        let allowance = remaining_nodes / (target_count - index);
        let mut nodes = 0;
        let on_target: Vec<&AttackOption> = options
            .iter()
            .filter(|option| option.target == target)
            .collect();
        let mut found: Vec<Kill> = Vec::new();
        let mut chosen: Vec<&AttackOption> = Vec::new();
        collect_kills(
            &on_target,
            0,
            &mut chosen,
            &mut found,
            &mut nodes,
            allowance,
        );
        remaining_nodes -= nodes;
        found.sort_by(|left, right| right.value.total_cmp(&left.value));
        found.truncate(OPTIONS_PER_TARGET);
        if !found.is_empty() {
            result.push(found);
        }
    }
    result.sort_by(|left, right| right[0].value.total_cmp(&left[0].value));
    result
}

fn collect_kills<'a>(
    options: &[&'a AttackOption],
    start: usize,
    chosen: &mut Vec<&'a AttackOption>,
    found: &mut Vec<Kill>,
    nodes: &mut usize,
    node_limit: usize,
) {
    if *nodes >= node_limit {
        return;
    }
    *nodes += 1;
    if let Some(first) = chosen.first() {
        let low: u32 = chosen.iter().map(|option| u32::from(option.low)).sum();
        if low >= u32::from(first.target_hp) {
            found.push(kill_of(chosen));
            found.sort_by(|left, right| right.value.total_cmp(&left.value));
            found.truncate(OPTIONS_PER_TARGET);
            // A larger set with this one inside it is not minimal.
            return;
        }
    }
    if chosen.len() == MAX_ATTACKERS {
        return;
    }
    for index in start..options.len() {
        if *nodes >= node_limit {
            break;
        }
        let option = options[index];
        if chosen.iter().any(|other| {
            other.attacker == option.attacker || other.destination == option.destination
        }) {
            continue;
        }
        chosen.push(option);
        collect_kills(options, index + 1, chosen, found, nodes, node_limit);
        chosen.pop();
    }
}

/// Order the attacks and price the kill.
///
/// The strongest hit goes first, so later attackers meet a weaker target and
/// take less counter damage. An indirect attack takes no counter, and it can
/// go first for the same reason.
fn kill_of(chosen: &[&AttackOption]) -> Kill {
    let mut attacks: Vec<AttackOption> = chosen.iter().map(|option| **option).collect();
    attacks.sort_by(|left, right| {
        (right.counter_high == 0)
            .cmp(&(left.counter_high == 0))
            .then(right.low.cmp(&left.low))
    });
    let target = attacks[0];
    let mut hp = f64::from(target.target_hp);
    let mut counter_cost = 0.0;
    for (index, attack) in attacks.iter().enumerate() {
        let damage = (f64::from(attack.low) + f64::from(attack.high)) / 2.0;
        let last = index + 1 == attacks.len();
        if !last && damage < hp {
            // The counter scales with the target health that is left.
            let share = (hp - damage) / f64::from(target.target_hp.max(1));
            counter_cost +=
                f64::from(attack.counter_high) * share * cost(attack.attacker_kind) / 100.0;
        }
        hp -= damage;
    }
    let value = f64::from(target.target_hp) * cost(target.target_kind) / 100.0 - counter_cost;
    Kill { attacks, value }
}

/// The best disjoint kill plans, best first, at most `limit` of them.
///
/// Each plan has at least one kill. Branch and bound searches the targets in
/// order of their best kill value. A branch stops when even the best kill of
/// every target that is left cannot beat the worst kept plan.
pub(super) fn plans(kills_by_target: &[Vec<Kill>], limit: usize) -> Vec<KillPlan> {
    let mut best: Vec<KillPlan> = Vec::new();
    if limit == 0 {
        return best;
    }
    let mut bound_after = vec![0.0; kills_by_target.len() + 1];
    for index in (0..kills_by_target.len()).rev() {
        bound_after[index] = bound_after[index + 1] + kills_by_target[index][0].value.max(0.0);
    }
    let mut chosen: Vec<&Kill> = Vec::new();
    let mut nodes = 0;
    search(
        kills_by_target,
        0,
        &mut chosen,
        0.0,
        &bound_after,
        limit,
        &mut best,
        &mut nodes,
    );
    best
}

#[allow(clippy::too_many_arguments)]
fn search<'a>(
    kills_by_target: &'a [Vec<Kill>],
    index: usize,
    chosen: &mut Vec<&'a Kill>,
    value: f64,
    bound_after: &[f64],
    limit: usize,
    best: &mut Vec<KillPlan>,
    nodes: &mut usize,
) {
    if *nodes >= MAX_NODES {
        return;
    }
    *nodes += 1;
    let floor = if best.len() < limit {
        f64::NEG_INFINITY
    } else {
        best.last().map_or(f64::NEG_INFINITY, |plan| plan.value)
    };
    if value + bound_after[index] <= floor {
        return;
    }
    if index == kills_by_target.len() {
        if chosen.is_empty() || value <= 0.0 {
            return;
        }
        best.push(KillPlan {
            kills: chosen.iter().map(|kill| (*kill).clone()).collect(),
            value,
        });
        best.sort_by(|left, right| right.value.total_cmp(&left.value));
        best.truncate(limit);
        return;
    }
    for kill in &kills_by_target[index] {
        if *nodes >= MAX_NODES {
            break;
        }
        let conflicts = chosen.iter().any(|other| {
            other.attacks.iter().any(|used| {
                kill.attacks.iter().any(|attack| {
                    attack.attacker == used.attacker || attack.destination == used.destination
                })
            })
        });
        if conflicts || kill.value <= 0.0 {
            continue;
        }
        chosen.push(kill);
        search(
            kills_by_target,
            index + 1,
            chosen,
            value + kill.value,
            bound_after,
            limit,
            best,
            nodes,
        );
        chosen.pop();
    }
    search(
        kills_by_target,
        index + 1,
        chosen,
        value,
        bound_after,
        limit,
        best,
        nodes,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_bounds_work_and_retained_kills() {
        let options: Vec<_> = (0..100)
            .map(|index| AttackOption {
                attacker: UnitId::new(index + 1),
                attacker_kind: UnitKind::Infantry,
                destination: CellIdx::from_raw(index as u16),
                target: UnitId::new(200),
                target_hp: 100,
                target_kind: UnitKind::Tank,
                low: 34,
                high: 40,
                counter_high: 0,
            })
            .collect();
        let options: Vec<_> = options.iter().collect();
        let mut found = Vec::new();
        let mut nodes = 0;
        collect_kills(
            &options,
            0,
            &mut Vec::new(),
            &mut found,
            &mut nodes,
            MAX_NODES,
        );
        assert_eq!(nodes, MAX_NODES);
        assert_eq!(found.len(), OPTIONS_PER_TARGET);
        assert!(found.iter().all(|kill| kill.attacks.len() <= MAX_ATTACKERS));
    }

    #[test]
    fn an_empty_input_or_zero_limit_produces_no_plan() {
        assert!(plans(&[], 1).is_empty());
        assert!(plans(&[], 0).is_empty());
    }
}
