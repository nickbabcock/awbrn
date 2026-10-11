//! Bounded composition of compatible two-order prefixes.

use super::{MeanLuck, play_order};
use crate::agent::Play;
use awvm::session::Session;

pub(super) const MAX_PLANS: usize = 3;

pub(super) fn pairs(
    session: &Session,
    anchors: &[Play],
    holds: &[Play],
    limit: usize,
) -> Vec<Vec<Play>> {
    let limit = limit.min(MAX_PLANS);
    let mut result = Vec::new();
    for anchor in anchors.iter().take(MAX_PLANS) {
        for hold in holds.iter().take(MAX_PLANS) {
            if result.len() == limit {
                return result;
            }
            if anchor.unit() == hold.unit() {
                continue;
            }
            let prefix = vec![*anchor, *hold];
            if result.contains(&prefix) {
                continue;
            }
            let mut probe = Session::new(session.state().clone());
            let mut entropy = MeanLuck;
            if prefix.iter().all(|play| {
                play_order(&probe, play)
                    .is_some_and(|order| probe.apply(order, &mut entropy, &mut ()).is_ok())
            }) {
                result.push(prefix);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::hold;
    use awvm::session::OrderKind;

    #[test]
    fn pairs_skip_incompatible_orders_and_deduplicate_compatible_prefixes() {
        let session = Session::new(crate::puzzles::suite().remove(0).state);
        let ours = session
            .state()
            .players
            .seat(&session.state().turn.active_player)
            .unwrap();
        let holds: Vec<_> = session
            .state()
            .units
            .iter()
            .filter(|unit| unit.owner == ours)
            .filter_map(|unit| hold(&session, unit.id))
            .take(2)
            .collect();
        assert_eq!(holds.len(), 2);
        let blocked = Play::new(
            holds[0].unit().unwrap(),
            holds[1].destination(),
            OrderKind::Wait,
        );
        let result = pairs(
            &session,
            &[blocked, holds[0], holds[0]],
            &[holds[0], holds[1], holds[1]],
            99,
        );
        assert_eq!(result, vec![vec![holds[0], holds[1]]]);
        assert!(pairs(&session, &holds, &holds, 0).is_empty());
    }
}
