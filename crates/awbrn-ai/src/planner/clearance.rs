//! Plans that move a cheap friendly unit so another unit can attack from its tile.

use awvm::ruleset;
use awvm::semantic::{Location, PlayerIdx, UnitId};
use awvm::session::{Order, OrderKind, Session};

use crate::agent::Play;

use super::{Line, TurnPlanner};

const MAX_BLOCKERS: usize = 3;
const VACATING_ORDERS_PER_BLOCKER: usize = 24;

struct Blocker {
    unit: UnitId,
    cell: awvm::semantic::CellIdx,
    gain: u8,
    cost: u64,
}

struct VacatingOrder {
    order: Order,
    play: Play,
    score: f64,
    gain: u8,
    cost: u64,
}

pub(super) fn plans(
    planner: &mut TurnPlanner<'_>,
    session: &Session,
    seed: &Line,
    safety_holds: &[Play],
    limit: usize,
) -> Vec<Vec<Play>> {
    if limit == 0 || planner.out_of_work() {
        return Vec::new();
    }

    let state = session.state();
    let Some(seat) = state.players.seat(&state.turn.active_player) else {
        return Vec::new();
    };
    let exposed: Vec<(UnitId, u64, u8)> = seed
        .reply
        .exposed
        .iter()
        .filter_map(|id| {
            let unit = state.units.get(*id)?;
            let seed_play = seed.plays.iter().find(|play| play.unit() == Some(*id));
            let cell = seed_play.map_or_else(
                || position_cell(state, unit.location),
                |play| Some(play.destination()),
            )?;
            let cover = tile_defense(state, cell)?;
            Some((*id, ruleset::profile(unit.kind).cost, cover))
        })
        .collect();
    if exposed.is_empty() {
        return Vec::new();
    }

    let mut blockers = blockers(session, seed, seat, &exposed);
    blockers.sort_by(|left, right| {
        right
            .gain
            .cmp(&left.gain)
            .then_with(|| left.cost.cmp(&right.cost))
            .then_with(|| left.unit.cmp(&right.unit))
    });

    let friendly = state.turn.active_player.clone();
    let mut legal_orders = Vec::new();
    session.legal().orders(&mut legal_orders);
    let mut candidates = Vec::new();
    let mut screen_session = Session::new(state.clone());
    let blocker_limit = limit.min(MAX_BLOCKERS);

    for blocker in blockers.into_iter().take(blocker_limit) {
        if planner.out_of_work() {
            break;
        }
        planner.work += 1;
        if !opens_attack(session, blocker.cell, blocker.unit, blocker.cost, &exposed) {
            continue;
        }

        let mut screened = 0;
        for order in legal_orders.iter().copied().filter(|order| {
            session.unit_of(*order) == Some(blocker.unit)
                && order.destination() != blocker.cell
                && matches!(order.kind(), OrderKind::Wait | OrderKind::Attack(_))
        }) {
            if screened >= VACATING_ORDERS_PER_BLOCKER || planner.out_of_work() {
                break;
            }
            let Some(play) = Play::from_order(session, order) else {
                continue;
            };
            if candidates
                .iter()
                .any(|candidate: &VacatingOrder| candidate.play == play)
            {
                continue;
            }
            screened += 1;
            planner.work += 1;
            let Some(score) = planner.screen(&mut screen_session, order, &friendly) else {
                continue;
            };
            candidates.push(VacatingOrder {
                order,
                play,
                score,
                gain: blocker.gain,
                cost: blocker.cost,
            });
        }
    }

    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| right.gain.cmp(&left.gain))
            .then_with(|| left.cost.cmp(&right.cost))
            .then_with(|| left.play.unit().cmp(&right.play.unit()))
            .then_with(|| left.play.destination().cmp(&right.play.destination()))
            .then_with(|| left.play.kind().cmp(&right.play.kind()))
            .then_with(|| left.order.cmp(&right.order))
    });

    let mut prefixes = Vec::new();
    for candidate in &candidates {
        if prefixes.len() >= limit {
            break;
        }
        if safety_holds.is_empty() {
            prefixes.push(vec![candidate.play]);
            continue;
        }
        for hold in safety_holds {
            if prefixes.len() >= limit {
                break;
            }
            if hold.unit() != candidate.play.unit() {
                let prefix = vec![candidate.play, *hold];
                if !prefixes.contains(&prefix) {
                    prefixes.push(prefix);
                }
            }
        }
    }

    if prefixes.len() < limit {
        for candidate in &candidates {
            if prefixes.len() >= limit {
                break;
            }
            let prefix = vec![candidate.play];
            if !prefixes.contains(&prefix) {
                prefixes.push(prefix);
            }
        }
    }
    prefixes
}

fn blockers(
    session: &Session,
    seed: &Line,
    seat: PlayerIdx,
    exposed: &[(UnitId, u64, u8)],
) -> Vec<Blocker> {
    let state = session.state();
    state
        .units
        .iter()
        .filter_map(|unit| {
            if unit.owner != seat || unit.action != awvm::semantic::UnitAction::Ready {
                return None;
            }
            let location = position_cell(state, unit.location)?;
            let seed_keeps_unit = seed
                .plays
                .iter()
                .any(|play| play.unit() == Some(unit.id) && play.destination() == location);
            if !seed_keeps_unit {
                return None;
            }
            let cover = tile_defense(state, location)?;
            let gain = exposed
                .iter()
                .filter(|(id, cost, seed_cover)| {
                    *id != unit.id
                        && *cost > ruleset::profile(unit.kind).cost
                        && cover > *seed_cover
                })
                .map(|(_, _, seed_cover)| cover - seed_cover)
                .max()?;
            Some(Blocker {
                unit: unit.id,
                cell: location,
                gain,
                cost: ruleset::profile(unit.kind).cost,
            })
        })
        .collect()
}

fn opens_attack(
    session: &Session,
    cell: awvm::semantic::CellIdx,
    blocker: UnitId,
    blocker_cost: u64,
    exposed: &[(UnitId, u64, u8)],
) -> bool {
    let Some(cell_cover) = tile_defense(session.state(), cell) else {
        return false;
    };
    let mut state = session.state().clone();
    let Some(index) = state.units.index_of(blocker) else {
        return false;
    };
    state.units.remove(index);
    let vacant = Session::new(state);
    let mut orders = Vec::new();
    vacant.legal().orders(&mut orders);
    orders.into_iter().any(|order| {
        if !matches!(order.kind(), OrderKind::Attack(_)) || order.destination() != cell {
            return false;
        }
        let Some(attacker) = vacant.unit_of(order) else {
            return false;
        };
        exposed.iter().any(|(id, cost, seed_cover)| {
            *id == attacker && *cost > blocker_cost && cell_cover > *seed_cover
        })
    })
}

fn position_cell(
    state: &awvm::semantic::State,
    location: Location,
) -> Option<awvm::semantic::CellIdx> {
    match location {
        Location::Board { position } => state.board.dimensions().cell_index(position),
        Location::Cargo { .. } => None,
    }
}

fn tile_defense(state: &awvm::semantic::State, cell: awvm::semantic::CellIdx) -> Option<u8> {
    let position = state.board.dimensions().position_of(cell)?;
    Some(ruleset::defense_stars(state.board.tile(position).terrain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::Evaluator;
    use crate::planner::{Generator, MeanLuck, PlannerConfig, ReplayReader, Rng, hold, play_order};
    use awvm::semantic::Pos;

    fn day_six() -> awvm::semantic::State {
        let path = format!(
            "{}/tests/fixtures/replay_regressions/amber-valley-day06.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn clearance_configuration_keeps_the_v6_fingerprint() {
        assert_eq!(PlannerConfig::V6.fingerprint(), "26569da0906e9bc4");
        assert!(
            serde_json::to_value(PlannerConfig::V6)
                .unwrap()
                .get("clearance_plans")
                .is_none()
        );
    }

    #[test]
    fn clearance_prefixes_are_bounded_and_legal() {
        let state = day_six();
        let config = PlannerConfig::V7_DAY6;
        let seat = state.players.seat(&state.turn.active_player).unwrap();
        let mut evaluator = Evaluator::new(config.eval_weights);
        let mut replay = config.replay_score.map(ReplayReader::new);
        let mut planner = TurnPlanner {
            config: &config,
            seed: Rng::mix(1),
            seat,
            evaluator: &mut evaluator,
            replay: replay.as_mut(),
            candidates: 0,
            work: 0,
            work_left: config.turn_work,
            nodes_left: 32,
        };
        let mut session = Session::new(state.clone());
        let seed = planner
            .line(&mut session, Generator::Seed, &[])
            .expect("the seed line plays");
        let work_before = planner.work;
        let safety_holds: Vec<Play> = seed
            .reply
            .destroyed
            .iter()
            .take(config.safety_plans)
            .filter_map(|unit| hold(&session, *unit))
            .collect();
        let prefixes = plans(
            &mut planner,
            &session,
            &seed,
            &safety_holds,
            config.clearance_plans,
        );

        assert!(!prefixes.is_empty());
        assert!(prefixes.len() <= config.clearance_plans);
        let blocker = state
            .units
            .iter()
            .find(|unit| {
                unit.location
                    == Location::Board {
                        position: Pos::new(16, 8),
                    }
            })
            .unwrap()
            .id;
        assert!(prefixes.iter().any(|prefix| {
            prefix
                .first()
                .is_some_and(|play| play.unit() == Some(blocker))
                && prefix.len() == 2
                && safety_holds.contains(&prefix[1])
        }));
        assert!(
            planner.work - work_before
                <= (config.clearance_plans * (VACATING_ORDERS_PER_BLOCKER + 1)) as u64
        );
        for prefix in &prefixes {
            let mut probe = Session::new(state.clone());
            let mut entropy = MeanLuck;
            for play in prefix {
                let order = play_order(&probe, play).expect("prefix order is legal");
                probe
                    .apply(order, &mut entropy, &mut ())
                    .expect("prefix order applies");
            }
        }

        let spent = planner.work;
        assert!(plans(&mut planner, &session, &seed, &safety_holds, 0).is_empty());
        planner.work_left = Some(spent);
        assert!(
            plans(
                &mut planner,
                &session,
                &seed,
                &safety_holds,
                config.clearance_plans
            )
            .is_empty()
        );
        assert_eq!(planner.work, spent);
    }
}
