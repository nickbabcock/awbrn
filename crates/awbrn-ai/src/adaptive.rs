//! Select a turn with a short horizon and an uncertainty extension.

use awvm::semantic::{AwbwVisibility, Match, Outcome, State, TeamId, observe, observe_into};
use awvm::session::Session;
use awvm::transition::Command;

use crate::agent::{Agent, NodeBudget, Play};
use crate::agents::{GreedyAgent, Weights};
use crate::eval::{EvalWeights, Evaluator};
use crate::rng::Rng;

/// The salt that seeds a replay's entropy, so that a diagnostic replaying the
/// same root outside this module draws the same rolls.
pub const ENTROPY_SALT: u64 = 0x051c_71f7;
/// The salt that seeds each replied turn.
pub const REPLY_SALT: u64 = 0x072a_94d3;
/// The turn count a replay stops at, in case a match never ends.
pub const MAX_TURNS: u32 = 1_000;

/// The horizon policy used to rank candidate turns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionPolicy {
    /// Rank every candidate after four rounds.
    StandardFour,
    /// Rank after four rounds, then extend uncertain candidates to eight rounds.
    Adaptive,
    /// Rank every candidate after eight rounds.
    AlwaysEight,
}

/// The result of one candidate selection pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionResult {
    /// The selected candidate index.
    pub selected_index: usize,
    /// The score of candidate zero, which is the baseline in arena searches.
    pub baseline_score: f64,
    /// The score of the selected candidate at the selection horizon.
    pub selected_score: f64,
    /// The number of candidates replayed for four rounds.
    pub four_round_replays: usize,
    /// The number of candidates replayed for the eight-round extension.
    pub eight_round_replays: usize,
    /// Whether the two four-round evaluators selected different candidates.
    pub disagreement: bool,
}

/// Select one candidate from a root position.
///
/// Candidate generation is outside this function. This keeps the measurement
/// of horizon selection separate from the cost of making candidate turns.
/// `P` can be a `Vec<Play>` or a wrapper that exposes its plays with `AsRef`.
pub fn select<P: AsRef<[Play]>>(
    root: &State,
    candidates: &[P],
    seed: u64,
    days: u32,
    policy: SelectionPolicy,
) -> Option<SelectionResult> {
    let friendly = root.turn.active_player.clone();
    let friendly_seat = root.players.seat(&friendly)?;

    match policy {
        SelectionPolicy::StandardFour => {
            select_standard_four(root, candidates, seed, days, friendly_seat, &friendly)
        }
        SelectionPolicy::Adaptive => {
            select_adaptive(root, candidates, seed, days, friendly_seat, &friendly)
        }
        SelectionPolicy::AlwaysEight => {
            select_always_eight(root, candidates, seed, days, friendly_seat, &friendly)
        }
    }
}

#[derive(Clone, Copy)]
struct AdaptiveLine {
    standard_score: f64,
    conservative_score: f64,
}

struct HorizonReplay {
    session: Session,
    entropy: Rng,
    turns: u32,
    terminal_result: Option<f64>,
}

struct CandidateLine {
    score: AdaptiveLine,
    checkpoint: HorizonReplay,
}

fn select_standard_four<P: AsRef<[Play]>>(
    root: &State,
    candidates: &[P],
    seed: u64,
    days: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    friendly: &awvm::semantic::PlayerId,
) -> Option<SelectionResult> {
    if candidates.is_empty() {
        return None;
    }
    let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
    let lines: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            evaluate(
                root,
                candidate.as_ref(),
                seed,
                days,
                8,
                friendly_seat,
                friendly,
                &mut evaluator,
            )
        })
        .collect();
    let baseline_score = lines.first().and_then(|line| *line)?;
    let (selected_index, selected_score) = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.map(|line| (index, line)))
        .max_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| right.0.cmp(&left.0))
        })?;
    Some(SelectionResult {
        selected_index,
        baseline_score,
        selected_score,
        four_round_replays: candidates.len(),
        eight_round_replays: 0,
        disagreement: false,
    })
}

fn select_adaptive<P: AsRef<[Play]>>(
    root: &State,
    candidates: &[P],
    seed: u64,
    days: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    friendly: &awvm::semantic::PlayerId,
) -> Option<SelectionResult> {
    if candidates.is_empty() {
        return None;
    }
    let mut standard_evaluator = Evaluator::new(EvalWeights::STANDARD);
    let mut conservative_evaluator = Evaluator::new(conservative_weights());
    let mut four_lines: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            evaluate_both_checkpoint(
                root,
                candidate.as_ref(),
                seed,
                days,
                8,
                friendly_seat,
                friendly,
                &mut standard_evaluator,
                &mut conservative_evaluator,
            )
        })
        .collect();
    let four_scores: Vec<_> = four_lines
        .iter()
        .map(|line| line.as_ref().map(|line| line.score))
        .collect();
    let standard_top = top_two(&four_scores, true);
    let conservative_top = top_two(&four_scores, false);
    let standard_selected = standard_top.first().copied()?;
    let conservative_selected = conservative_top.first().copied()?;
    let baseline_score = joint_score(four_scores[0]?);

    // The scores reported for the selection are read at the horizon the
    // selection was made at, baseline and selection together, so that the
    // margin between them compares two lines of the same length: the
    // four-round scores when both weightings agreed, and the eight-round
    // scores when the extension broke the tie.
    let (selected_index, eight_round_replays, extended_scores) =
        if standard_selected == conservative_selected {
            let selected_score = joint_score(four_scores[standard_selected]?);
            let selected_index = if selected_score > baseline_score {
                standard_selected
            } else {
                0
            };
            (selected_index, 0, None)
        } else {
            let mut extended = standard_top;
            for index in conservative_top {
                if !extended.contains(&index) {
                    extended.push(index);
                }
            }
            if !extended.contains(&0) {
                extended.push(0);
            }
            extended.sort_unstable();
            let mut standard_evaluator = Evaluator::new(EvalWeights::STANDARD);
            let mut conservative_evaluator = Evaluator::new(conservative_weights());
            let eight_lines: Vec<_> = extended
                .iter()
                .map(|index| {
                    let checkpoint = four_lines[*index].take()?;
                    evaluate_both_from_checkpoint(
                        checkpoint.checkpoint,
                        seed,
                        16,
                        friendly_seat,
                        &mut standard_evaluator,
                        &mut conservative_evaluator,
                    )
                })
                .collect();
            let (selected_index, selected_score) = eight_lines
                .iter()
                .enumerate()
                .filter_map(|(position, line)| line.map(|line| (position, line)))
                .max_by(|left, right| {
                    joint_score(left.1)
                        .total_cmp(&joint_score(right.1))
                        .then_with(|| right.0.cmp(&left.0))
                })
                .map(|(position, line)| (extended[position], joint_score(line)))?;
            // The baseline is always extended, so the pair is ordinarily read
            // at eight rounds. A baseline whose extension failed leaves no
            // pair to read there, and both scores fall back to four rounds
            // rather than being compared across horizons.
            let extended_baseline = extended
                .iter()
                .position(|index| *index == 0)
                .and_then(|position| eight_lines[position])
                .map(joint_score);
            let scores = extended_baseline.map(|baseline| (baseline, selected_score));
            (selected_index, extended.len(), scores)
        };

    let (baseline_score, selected_score) = match extended_scores {
        Some(scores) => scores,
        None => (baseline_score, joint_score(four_scores[selected_index]?)),
    };
    Some(SelectionResult {
        selected_index,
        baseline_score,
        selected_score,
        four_round_replays: candidates.len(),
        eight_round_replays,
        disagreement: standard_selected != conservative_selected,
    })
}

fn select_always_eight<P: AsRef<[Play]>>(
    root: &State,
    candidates: &[P],
    seed: u64,
    days: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    friendly: &awvm::semantic::PlayerId,
) -> Option<SelectionResult> {
    if candidates.is_empty() {
        return None;
    }
    let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
    let lines: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            evaluate(
                root,
                candidate.as_ref(),
                seed,
                days,
                16,
                friendly_seat,
                friendly,
                &mut evaluator,
            )
        })
        .collect();
    let baseline_score = lines.first().and_then(|line| *line)?;
    let (selected_index, selected_score) = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.map(|line| (index, line)))
        .max_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| right.0.cmp(&left.0))
        })?;
    Some(SelectionResult {
        selected_index,
        baseline_score,
        selected_score,
        four_round_replays: 0,
        eight_round_replays: candidates.len(),
        disagreement: false,
    })
}

fn conservative_weights() -> EvalWeights {
    let mut weights = EvalWeights::STANDARD;
    weights.exposure *= 0.25;
    weights.front *= 0.25;
    weights
}

fn top_two(lines: &[Option<AdaptiveLine>], standard: bool) -> Vec<usize> {
    let mut indices: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.map(|_| index))
        .collect();
    indices.sort_by(|left, right| {
        let left_score = adaptive_score(lines[*left], standard);
        let right_score = adaptive_score(lines[*right], standard);
        right_score
            .total_cmp(&left_score)
            .then_with(|| left.cmp(right))
    });
    indices.truncate(2);
    indices
}

fn adaptive_score(line: Option<AdaptiveLine>, standard: bool) -> f64 {
    let line = line.expect("adaptive score needs a valid line");
    if standard {
        line.standard_score
    } else {
        line.conservative_score
    }
}

fn joint_score(line: AdaptiveLine) -> f64 {
    (line.standard_score + line.conservative_score) / 2.0
}

#[allow(clippy::too_many_arguments)]
fn evaluate(
    root: &State,
    plays: &[Play],
    seed: u64,
    days: u32,
    turns: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    friendly: &awvm::semantic::PlayerId,
    evaluator: &mut Evaluator,
) -> Option<f64> {
    let replay = replay_horizon(root, plays, seed, days, turns, friendly_seat, friendly)?;
    Some(
        replay
            .terminal_result
            .unwrap_or_else(|| evaluator.value(replay.session.state(), friendly_seat)),
    )
}

#[allow(clippy::too_many_arguments)]
fn evaluate_both_checkpoint(
    root: &State,
    plays: &[Play],
    seed: u64,
    days: u32,
    turns: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    friendly: &awvm::semantic::PlayerId,
    standard: &mut Evaluator,
    conservative: &mut Evaluator,
) -> Option<CandidateLine> {
    let checkpoint = replay_horizon(root, plays, seed, days, turns, friendly_seat, friendly)?;
    let score = score_both(&checkpoint, friendly_seat, standard, conservative);
    Some(CandidateLine { score, checkpoint })
}

fn evaluate_both_from_checkpoint(
    checkpoint: HorizonReplay,
    seed: u64,
    turns: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    standard: &mut Evaluator,
    conservative: &mut Evaluator,
) -> Option<AdaptiveLine> {
    let replay = continue_replay(checkpoint, seed, turns, friendly_seat)?;
    Some(score_both(&replay, friendly_seat, standard, conservative))
}

fn score_both(
    replay: &HorizonReplay,
    friendly_seat: awvm::semantic::PlayerIdx,
    standard: &mut Evaluator,
    conservative: &mut Evaluator,
) -> AdaptiveLine {
    let standard_score = replay
        .terminal_result
        .unwrap_or_else(|| standard.value(replay.session.state(), friendly_seat));
    let conservative_score = replay
        .terminal_result
        .unwrap_or_else(|| conservative.value(replay.session.state(), friendly_seat));
    AdaptiveLine {
        standard_score,
        conservative_score,
    }
}

fn replay_horizon(
    root: &State,
    plays: &[Play],
    seed: u64,
    days: u32,
    turns: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
    friendly: &awvm::semantic::PlayerId,
) -> Option<HorizonReplay> {
    let mut state = root.clone();
    state.settings.day_limit = Some(u64::from(days));
    let mut session = Session::new(state);
    let mut entropy = Rng::from_seed(Rng::mix(seed ^ ENTROPY_SALT));

    for play in plays {
        if session.state().turn.active_player != *friendly
            || !matches!(session.state().match_state, Match::Active { .. })
        {
            return None;
        }
        let command = play.command(&session)?;
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }
    if session.state().turn.active_player == *friendly
        && matches!(session.state().match_state, Match::Active { .. })
    {
        let command = Command::EndTurn {
            player: friendly.clone(),
        };
        let order = session.resolve(&command).ok()?;
        session.apply(order, &mut entropy, &mut ()).ok()?;
    }

    continue_replay(
        HorizonReplay {
            session,
            entropy,
            turns: 0,
            terminal_result: None,
        },
        seed,
        turns,
        friendly_seat,
    )
}

fn continue_replay(
    mut replay: HorizonReplay,
    seed: u64,
    turns: u32,
    friendly_seat: awvm::semantic::PlayerIdx,
) -> Option<HorizonReplay> {
    // The horizon is a count of player turns, which is what the caller names
    // and what `replay.turns` counts. A round of a two-player match is two of
    // them, so four rounds is a limit of eight.
    while matches!(replay.session.state().match_state, Match::Active { .. })
        && replay.turns < MAX_TURNS
        && replay.turns < turns
    {
        let turn_seed = Rng::mix(seed ^ REPLY_SALT ^ ((u64::from(replay.turns)) << 32));
        greedy_turn(&mut replay.session, turn_seed, &mut replay.entropy)?;
        replay.turns += 1;
    }

    replay.terminal_result = match &replay.session.state().match_state {
        Match::Finished { outcome } => Some(outcome_score(
            outcome,
            &replay.session.state().player(friendly_seat).team,
        )),
        Match::Active { .. } => None,
    };
    Some(replay)
}

fn greedy_turn(session: &mut Session, seed: u64, entropy: &mut Rng) -> Option<()> {
    let player = session.state().turn.active_player.clone();
    let mut agent = GreedyAgent::with_weights(seed, Weights::BASELINE);
    let direct = rollout_fully_disclosed(session.state());
    let mut view = (!direct)
        .then(|| observe(&AwbwVisibility, session.state(), &player).ok())
        .flatten();
    while session.state().turn.active_player == player
        && matches!(session.state().match_state, Match::Active { .. })
    {
        let play = if direct {
            agent.act_in_session(session)
        } else {
            let view = view.as_mut()?;
            observe_into(&AwbwVisibility, session.state(), &player, view).ok()?;
            agent.act(view, NodeBudget::ONE)
        };
        let command = play
            .and_then(|play| play.command(session))
            .unwrap_or_else(|| Command::EndTurn {
                player: player.clone(),
            });
        let order = session.resolve(&command).ok()?;
        session.apply(order, entropy, &mut ()).ok()?;
    }
    Some(())
}

/// Whether the authoritative position contains no fact hidden from Greedy.
///
/// Fog can hide units and terrain. A commander can also hide enemy HP when
/// fog is off. Keep those positions on the observation path.
fn rollout_fully_disclosed(state: &State) -> bool {
    if state.settings.fog {
        return false;
    }
    let Some(active) = state.players.seat(&state.turn.active_player) else {
        return false;
    };
    let active_team = &state.player(active).team;
    state.players.seats().all(|(seat, player)| {
        player.team == *active_team || !awvm::commander::hides_hp(state, Some(seat))
    })
}

fn outcome_score(outcome: &Outcome, team: &TeamId) -> f64 {
    match outcome {
        Outcome::Victory { winners, .. } => f64::from(u8::from(winners.contains(team))),
        Outcome::Draw { .. } | Outcome::Cancelled { .. } => 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::generate_plan;
    use crate::board::arena;

    fn root_and_plays(seed: u64) -> (State, Vec<Play>) {
        let state = arena(false, seed);
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the test root observes");
        let plays =
            generate_plan(&view, seed, Weights::BASELINE).expect("the test root has a plan");
        (state, plays)
    }

    #[test]
    fn resumed_replay_matches_replay_from_root() {
        let seed = 7;
        let (root, plays) = root_and_plays(seed);
        let friendly = root.turn.active_player.clone();
        let friendly_seat = root
            .players
            .seat(&friendly)
            .expect("the active player has a seat");

        let four = replay_horizon(&root, &plays, seed, 35, 8, friendly_seat, &friendly)
            .expect("the four-round replay completes");
        let mut resumed =
            continue_replay(four, seed, 16, friendly_seat).expect("the checkpoint resumes");
        let mut fresh = replay_horizon(&root, &plays, seed, 35, 16, friendly_seat, &friendly)
            .expect("the eight-round replay completes");

        assert_eq!(resumed.turns, fresh.turns);
        assert_eq!(resumed.terminal_result, fresh.terminal_result);
        assert_eq!(resumed.session.state(), fresh.session.state());
        assert_eq!(resumed.entropy.next_u64(), fresh.entropy.next_u64());
    }

    #[test]
    fn direct_policy_matches_the_observation_policy() {
        let seed = 11;
        let (state, _) = root_and_plays(seed);
        assert!(rollout_fully_disclosed(&state));
        let session = Session::new(state.clone());
        let view = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the direct-policy root observes");
        let mut direct = GreedyAgent::with_weights(seed, Weights::BASELINE);
        let mut observed = GreedyAgent::with_weights(seed, Weights::BASELINE);

        assert_eq!(
            direct.act_in_session(&session),
            observed.act(&view, NodeBudget::ONE)
        );
    }
}
