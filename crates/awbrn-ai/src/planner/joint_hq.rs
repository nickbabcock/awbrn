//! Plans that block a headquarters and hold another exposed unit.

use awvm::ruleset::{self, TerrainTrait};
use awvm::session::Session;

use crate::agent::Play;
use awvm::semantic::UnitId;

use super::{hold, joint_orders};

const MAX_PLANS: usize = 3;

/// Pair a headquarters block with a hold for one exposed unit.
pub(super) fn plans(
    session: &Session,
    blocks: &[Play],
    exposed: &[UnitId],
    limit: usize,
) -> Vec<Vec<Play>> {
    let limit = limit.min(MAX_PLANS);
    if limit == 0 {
        return Vec::new();
    }

    let hq_blocks: Vec<Play> = blocks
        .iter()
        .copied()
        .filter(|play| is_hq(session, play))
        .collect();
    let mut result = Vec::new();
    for block in hq_blocks {
        let Some(block_unit) = block.unit() else {
            continue;
        };
        let mut units: Vec<(u64, usize, UnitId)> = exposed
            .iter()
            .enumerate()
            .filter_map(|(order, unit)| {
                if *unit == block_unit {
                    return None;
                }
                session
                    .state()
                    .units
                    .get(*unit)
                    .map(|info| (ruleset::profile(info.kind).cost, order, *unit))
            })
            .collect();
        units.sort_unstable();
        units.dedup_by_key(|(_, _, unit)| *unit);
        let holds: Vec<Play> = units
            .into_iter()
            .filter_map(|(_, _, unit)| hold(session, unit))
            .take(limit)
            .collect();
        for prefix in joint_orders::pairs(session, &[block], &holds, limit) {
            if !result.contains(&prefix) {
                result.push(prefix);
                if result.len() == limit {
                    return result;
                }
            }
        }
    }
    result
}

fn is_hq(session: &Session, play: &Play) -> bool {
    let Some(position) = session
        .state()
        .board
        .dimensions()
        .position_of(play.destination())
    else {
        return false;
    };
    let Some(tile) = session.state().board.get(position) else {
        return false;
    };
    ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::PlannerConfig;
    use awvm::semantic::Pos;
    use awvm::session::OrderKind;

    fn day11() -> awvm::semantic::State {
        let path = format!(
            "{}/tests/fixtures/replay_regressions/amber-valley-day11.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    fn cell(session: &Session, x: u8, y: u8) -> awvm::semantic::CellIdx {
        session
            .state()
            .board
            .dimensions()
            .cell_index(Pos { x, y })
            .unwrap()
    }

    #[test]
    fn configuration_keeps_v6_stable_and_limits_v7_to_three_plans() {
        assert_eq!(PlannerConfig::V6.fingerprint(), "26569da0906e9bc4");
        assert!(
            serde_json::to_value(PlannerConfig::V6)
                .unwrap()
                .get("joint_hq_plans")
                .is_none()
        );
        assert_eq!(PlannerConfig::V7.identifier, "planner-v7");
        assert_eq!(PlannerConfig::V7.joint_hq_plans, 3);
    }

    #[test]
    fn plans_pair_a_headquarters_block_with_the_cheapest_exposed_holds() {
        let session = Session::new(day11());
        let block = Play::new(UnitId::new(37), cell(&session, 14, 6), OrderKind::Wait);
        let exposed = [
            UnitId::new(37),
            UnitId::new(43),
            UnitId::new(32),
            UnitId::new(21),
            UnitId::new(17),
        ];
        let expected = [UnitId::new(21), UnitId::new(17), UnitId::new(32)];

        let candidates = plans(&session, &[block], &exposed, 3);
        assert_eq!(candidates.len(), expected.len());
        for (prefix, unit) in candidates.iter().zip(expected) {
            assert_eq!(prefix[0], block);
            assert_eq!(prefix[1].unit(), Some(unit));
            assert_eq!(prefix[1].kind(), OrderKind::Wait);
        }
        assert!(plans(&session, &[block], &[UnitId::new(37)], 3).is_empty());
    }

    #[test]
    fn plans_ignore_non_hq_blocks_and_stop_at_three() {
        let session = Session::new(day11());
        let hq = Play::new(UnitId::new(37), cell(&session, 14, 6), OrderKind::Wait);
        let city = Play::new(UnitId::new(37), cell(&session, 6, 8), OrderKind::Wait);
        let exposed = [UnitId::new(21), UnitId::new(17), UnitId::new(32)];

        assert!(plans(&session, &[city], &exposed, 3).is_empty());
        let candidates = plans(&session, &[hq], &exposed, 99);
        assert_eq!(candidates.len(), 3);
        assert!(candidates.iter().all(|prefix| prefix[0] == hq));
        assert_eq!(candidates, plans(&session, &[hq], &exposed, 99));
    }
}
