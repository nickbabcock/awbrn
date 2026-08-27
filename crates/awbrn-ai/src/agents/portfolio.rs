//! Deterministic complete-turn plans from a small script portfolio.
//!
//! Each script uses the greedy agent as a repair policy. After each selected
//! order, the planner applies the order, observes the resulting position, and
//! selects again. The result is therefore a coherent turn instead of a batch
//! of orders that can invalidate one another.

use awvm::random::Entropy;
use awvm::semantic::{AwbwVisibility, Match, Observation, observe_into};
use awvm::session::{OrderKind, Session};
use awvm::transition::Command;

use crate::agent::{Agent, NodeBudget, Play};
use crate::agents::{GreedyAgent, Weights};
use crate::rng::Rng;

/// A deterministic unit-level policy used to generate a complete turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    /// Commit capturers to valuable properties and finish captures first.
    CaptureCommitment,
    /// Prefer profitable attacks and avoid exposed exchanges.
    FavorableCombat,
    /// Advance toward the opponent while limiting reply damage.
    SafePressure,
    /// Deny captures and hold threatened owned properties.
    ObjectiveDefense,
}

impl Script {
    /// The scripts included in the first portfolio coverage experiment.
    pub const ALL: [Self; 4] = [
        Self::CaptureCommitment,
        Self::FavorableCombat,
        Self::SafePressure,
        Self::ObjectiveDefense,
    ];

    /// A stable name for reports and serialized diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            Self::CaptureCommitment => "capture-commitment",
            Self::FavorableCombat => "favorable-combat",
            Self::SafePressure => "safe-pressure",
            Self::ObjectiveDefense => "objective-defense",
        }
    }

    pub(crate) const fn weights(self) -> Weights {
        match self {
            // Capture and approach dominate combat. The existing capture
            // fields stop targeting a property after one of our capturers
            // occupies it, which provides deterministic target reservation.
            Self::CaptureCommitment => Weights {
                hq_approach: 4_000.0,
                land: 3_000.0,
                air: 2_400.0,
                income: 2_000.0,
                naval: 800.0,
                other_property: 1_200.0,
                capture: 2.0,
                capture_completion: 3.0,
                capture_two_turn: 1.0,
                proximity_decay: 0.8,
                funds: 0.005,
                unit_count: 5.0,
                deny: 0.25,
                advance: 0.0,
                build_cost: 0.005,
                counter: 0.0,
                funds_efficiency: 1.0,
                threat: 0.01,
                ..Weights::BASELINE
            },
            // Remove general approach pull. Units act for concrete exchanges,
            // capture denial, and production rather than positional motion.
            Self::FavorableCombat => Weights {
                hq_approach: 0.0,
                land: 0.0,
                air: 0.0,
                income: 0.0,
                naval: 0.0,
                other_property: 0.0,
                capture: 0.0,
                capture_completion: 0.0,
                capture_two_turn: 0.0,
                funds: 0.08,
                unit_count: 100.0,
                advance: 0.0,
                deny: 4.0,
                hold: 0.0,
                build_cost: 0.01,
                counter: 10.0,
                funds_efficiency: 1.0,
                threat: 0.08,
                ..Weights::BASELINE
            },
            // Advance is the main positive signal, but immediate and deferred
            // threat cost more than in the baseline policy.
            Self::SafePressure => Weights {
                hq_approach: 200.0,
                land: 300.0,
                air: 240.0,
                income: 180.0,
                naval: 60.0,
                other_property: 90.0,
                capture: 0.5,
                capture_completion: 0.5,
                capture_two_turn: 0.1,
                proximity_decay: 0.85,
                funds: 0.015,
                unit_count: 15.0,
                advance: 0.08,
                deny: 1.0,
                hold: 0.25,
                threat: 0.06,
                deferred_threat: 0.7,
                ..Weights::BASELINE
            },
            // Holding and denying objectives dominate ordinary exchanges.
            Self::ObjectiveDefense => Weights {
                hq_approach: 0.0,
                land: 1_500.0,
                air: 1_200.0,
                income: 900.0,
                naval: 300.0,
                other_property: 450.0,
                capture: 0.25,
                capture_completion: 0.5,
                capture_two_turn: 0.0,
                funds: 0.01,
                unit_count: 10.0,
                advance: 0.0,
                deny: 5.0,
                deny_neutral: 0.25,
                deny_decay: 0.75,
                hold: 3.0,
                hold_decay: 0.75,
                threat: 0.03,
                ..Weights::BASELINE
            },
        }
    }
}

/// One named, complete, legal turn from a portfolio script.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptPlan {
    pub script: Script,
    pub plays: Vec<Play>,
}

/// Generate all portfolio plans from the same visible root and seed.
///
/// Search currently supports standard games only. This function follows the
/// same boundary because a speculative turn cannot update a fog belief from
/// events that the authoritative game has not emitted.
pub fn generate_plans(view: &Observation, seed: u64) -> Vec<ScriptPlan> {
    Script::ALL
        .into_iter()
        .filter_map(|script| {
            complete_turn(view, seed, script.weights()).map(|plays| ScriptPlan { script, plays })
        })
        .collect()
}

/// Generate one complete turn with the supplied weighting.
pub fn generate_plan(view: &Observation, seed: u64, weights: Weights) -> Option<Vec<Play>> {
    complete_turn(view, seed, weights)
}

fn complete_turn(view: &Observation, seed: u64, weights: Weights) -> Option<Vec<Play>> {
    let mut session = Session::from_observation(view).ok()?;
    if !session.is_commandable() || session.state().settings.fog {
        return None;
    }
    let player = session.state().turn.active_player.clone();
    let mut observation = view.clone();
    let mut agent = GreedyAgent::with_weights(seed, weights);
    let mut entropy = Rng::from_seed(Rng::mix(seed ^ 0x051c_71f7));
    let mut plays = Vec::new();

    while session.state().turn.active_player == player
        && matches!(session.state().match_state, Match::Active { .. })
    {
        observe_into(&AwbwVisibility, session.state(), &player, &mut observation).ok()?;
        let Some(play) = agent.act(&observation, NodeBudget::ONE) else {
            apply_end_turn(&mut session, &player, &mut entropy)?;
            break;
        };
        let command = play.command(&session)?;
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
        if order.kind() != OrderKind::EndTurn {
            plays.push(play);
        }
    }
    Some(plays)
}

fn apply_end_turn(
    session: &mut Session,
    player: &awvm::semantic::PlayerId,
    entropy: &mut impl Entropy,
) -> Option<()> {
    let command = Command::EndTurn {
        player: player.clone(),
    };
    let order = session.resolve(&command).ok()?;
    session.apply(order, entropy, &mut ()).ok()?;
    Some(())
}

#[cfg(test)]
mod tests {
    use awvm::semantic::observe;
    use awvm::transition::{ExecuteOutcome, execute};

    use super::*;
    use crate::board::arena;

    fn view() -> Observation {
        let mut state = arena(false, 1);
        let player = state.turn.active_player.clone();
        state = match execute(&state, Command::EndTurn { player }, &[]) {
            Ok(ExecuteOutcome::Accepted(execution)) => execution.state,
            other => panic!("end turn did not execute: {other:?}"),
        };
        observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player observes the arena")
    }

    #[test]
    fn portfolio_is_complete_and_repeatable() {
        let view = view();
        let first = generate_plans(&view, 29);
        let second = generate_plans(&view, 29);

        assert_eq!(first, second);
        assert_eq!(first.len(), Script::ALL.len());
        assert!(first.iter().all(|plan| !plan.plays.is_empty()));
    }

    #[test]
    fn portfolio_contains_different_turns() {
        let plans = generate_plans(&view(), 31);
        let distinct = plans
            .iter()
            .enumerate()
            .filter(|(index, plan)| {
                plans[..*index]
                    .iter()
                    .all(|prior| prior.plays != plan.plays)
            })
            .count();

        assert!(distinct >= 3, "expected at least three distinct plans");
    }
}
