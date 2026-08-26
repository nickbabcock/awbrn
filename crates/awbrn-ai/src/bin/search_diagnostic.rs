//! Audit search choices and replay them against the same seeded policies.

use std::fmt::Write as _;

use anyhow::Result;
use awbrn_ai::agent::{Agent, NodeBudget};
use awbrn_ai::agents::{GreedyAgent, SearchAgent, SearchAudit, Weights, audit};
use awbrn_ai::board::arena;
use awbrn_ai::eval::{EvalWeights, Evaluator};
use awbrn_ai::harness::{Limits, play_observed};
use awbrn_ai::rng::Rng;
use awvm::semantic::{AwbwVisibility, Match, Outcome, State, observe, observe_into};
use awvm::session::{Order, Session};
use awvm::transition::Command;

const DEFAULT_SAMPLES: usize = 50;
const DEFAULT_ROOTS: usize = 300;
const DEFAULT_GAMES: usize = 20;
const MAX_TURNS: u32 = 1_000;

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(error) = run(options) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

const USAGE: &str = "\
usage: search-diagnostic [--seed N] [--games N] [--roots N] [--samples N] [--nodes N]

  --seed N       Run seed. Default 101.
  --games N      Paired game count. Default 20.
  --roots N      Root positions to audit. Default 300.
  --samples N    Changed leaves to replay. Default 50.
  --nodes N      Search budget. Default 4.
";

struct Options {
    seed: u64,
    games: usize,
    roots: usize,
    samples: usize,
    nodes: NodeBudget,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            seed: 101,
            games: DEFAULT_GAMES,
            roots: DEFAULT_ROOTS,
            samples: DEFAULT_SAMPLES,
            nodes: NodeBudget::FOUR,
        };
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            let mut value = || {
                arguments
                    .next()
                    .ok_or_else(|| format!("{argument} needs a value"))
            };
            match argument.as_str() {
                "--seed" => options.seed = number(&value()?)?,
                "--games" => options.games = number(&value()?)?,
                "--roots" => options.roots = number(&value()?)?,
                "--samples" => options.samples = number(&value()?)?,
                "--nodes" => {
                    options.nodes = NodeBudget::new(number(&value()?)?)
                        .ok_or_else(|| "--nodes must be at least 1".to_owned())?;
                }
                "--help" | "-h" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        if options.games == 0 || options.roots == 0 || options.samples == 0 {
            return Err("--games, --roots, and --samples must be at least 1".to_owned());
        }
        Ok(options)
    }
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{text} is not a number this argument accepts"))
}

#[derive(Clone)]
struct Sample {
    audit: SearchAudit,
    seed_forward: Forward,
    selected_forward: Forward,
    material_seed: f64,
    material_selected: f64,
    no_front_seed: f64,
    no_front_selected: f64,
}

#[derive(Clone, Copy)]
struct Forward {
    score: f64,
    turns: u32,
    replayed: bool,
}

fn run(options: Options) -> Result<()> {
    let (pairs, roots, changed, terminal, mut samples) = collect(&options)?;
    for sample in &mut samples {
        sample.seed_forward = forward(&sample.audit, &sample.audit.seed_plan);
        sample.selected_forward = forward(&sample.audit, &sample.audit.selected_plan);
        let material = material_weights();
        let no_front = no_front_weights();
        sample.material_seed = score(
            &sample.audit.seed_state,
            sample.audit.friendly_seat,
            material,
        );
        sample.material_selected = score(
            &sample.audit.selected_state,
            sample.audit.friendly_seat,
            material,
        );
        sample.no_front_seed = score(
            &sample.audit.seed_state,
            sample.audit.friendly_seat,
            no_front,
        );
        sample.no_front_selected = score(
            &sample.audit.selected_state,
            sample.audit.friendly_seat,
            no_front,
        );
    }

    println!(
        "search diagnostic: seed {} nodes {}  roots audited {}  changed leaves {}",
        options.seed,
        options.nodes.get(),
        roots,
        samples.len()
    );
    println!(
        "changed leaves seen {}  terminal leaves skipped {}",
        changed, terminal
    );
    println!(
        "games {} paired, both seat orders; each changed leaf is replayed to completion",
        pairs
    );
    let excluded = samples
        .iter()
        .filter(|sample| !sample.seed_forward.replayed || !sample.selected_forward.replayed)
        .count();
    println!("forward replays excluded {excluded}");
    println!();
    component_report(&samples);
    ranking_report(&samples);
    threshold_report(&samples);
    agreement_report(&samples);
    sample_report(&samples);
    Ok(())
}

fn collect(options: &Options) -> Result<(usize, usize, usize, usize, Vec<Sample>)> {
    let mut pairs = 0;
    let mut roots = 0;
    let mut changed = 0;
    let mut terminal = 0;
    let mut samples = Vec::new();
    let mut sampled_leaves = 0_u64;
    let mut sampling = Rng::from_seed(Rng::mix(options.seed ^ 0x7361_6d70_6c65));
    let mut session = Session::new(arena(false, options.seed));
    let weights = Weights::BASELINE;

    for pair in 0..options.games {
        for search_first in [true, false] {
            let game = Rng::mix(options.seed ^ ((pair as u64) << 32));
            let mut entropy = Rng::from_seed(Rng::mix(game ^ 0x1));
            let search_seed = Rng::mix(game ^ 0x2);
            let greedy_seed = Rng::mix(game ^ 0x3);
            let search_seat = usize::from(!search_first);
            let mut search_agent = SearchAgent::with_weights_and_evaluator(
                search_seed,
                weights,
                EvalWeights::STANDARD,
            );
            let mut greedy_agent = GreedyAgent::with_weights(greedy_seed, weights);
            let mut agents: [&mut dyn Agent; 2] = if search_first {
                [&mut search_agent, &mut greedy_agent]
            } else {
                [&mut greedy_agent, &mut search_agent]
            };
            let state = arena(false, game);
            let mut observer = |state: &State, command: Option<&Command>| {
                let at_turn_start = command.is_none()
                    || command.is_some_and(|command| matches!(command, Command::EndTurn { .. }));
                let active_seat = state
                    .players
                    .seat(&state.turn.active_player)
                    .map(|seat| seat.get());
                if !at_turn_start || active_seat != Some(search_seat) || roots >= options.roots {
                    return;
                }
                roots += 1;
                let player = state.turn.active_player.clone();
                let Some(view) = observe(&AwbwVisibility, state, &player).ok() else {
                    return;
                };
                let Some(audit) = audit(
                    &view,
                    search_seed,
                    weights,
                    EvalWeights::STANDARD,
                    options.nodes,
                ) else {
                    return;
                };
                if audit.changes.is_empty() {
                    return;
                }
                changed += 1;
                if !matches!(audit.seed_state.match_state, Match::Active { .. })
                    || !matches!(audit.selected_state.match_state, Match::Active { .. })
                {
                    terminal += 1;
                    return;
                }
                sampled_leaves += 1;
                let replacement = if samples.len() < options.samples {
                    Some(samples.len())
                } else {
                    let index = sampling.below(sampled_leaves);
                    (index < options.samples as u64).then_some(index as usize)
                };
                if let Some(index) = replacement {
                    let sample = Sample {
                        audit,
                        seed_forward: Forward {
                            score: 0.5,
                            turns: 0,
                            replayed: false,
                        },
                        selected_forward: Forward {
                            score: 0.5,
                            turns: 0,
                            replayed: false,
                        },
                        material_seed: 0.0,
                        material_selected: 0.0,
                        no_front_seed: 0.0,
                        no_front_selected: 0.0,
                    };
                    if index == samples.len() {
                        samples.push(sample);
                    } else {
                        samples[index] = sample;
                    }
                }
            };
            play_observed(
                state,
                &mut session,
                &mut agents,
                &mut entropy,
                Limits {
                    nodes: options.nodes,
                    ..Limits::DEFAULT
                },
                &mut observer,
            );
        }
        pairs += 1;
        if roots >= options.roots && samples.len() >= options.samples {
            break;
        }
    }

    Ok((pairs, roots, changed, terminal, samples))
}

fn forward(audit: &SearchAudit, plan: &[Order]) -> Forward {
    let mut session = Session::new(audit.root.clone());
    let mut entropy = Rng::from_seed(audit.entropy_seed);
    for order in plan.iter().copied() {
        if session.apply(order, &mut entropy, &mut ()).is_err() {
            return Forward {
                score: 0.5,
                turns: 0,
                replayed: false,
            };
        }
    }
    let mut turns = 0;
    while turns < MAX_TURNS && matches!(session.state().match_state, Match::Active { .. }) {
        let turn_seed = if turns == 0 {
            audit.reply_seed
        } else {
            Rng::mix(audit.reply_seed ^ ((u64::from(turns)) << 32))
        };
        if greedy_turn(&mut session, audit.weights, turn_seed, &mut entropy).is_none() {
            return Forward {
                score: 0.5,
                turns,
                replayed: false,
            };
        }
        turns += 1;
    }
    let team = session.state().player(audit.friendly_seat).team.clone();
    let score = match &session.state().match_state {
        Match::Finished { outcome } => outcome_score(outcome, &team),
        Match::Active { .. } => 0.5,
    };
    Forward {
        score,
        turns,
        replayed: true,
    }
}

fn greedy_turn(
    session: &mut Session,
    weights: Weights,
    seed: u64,
    entropy: &mut Rng,
) -> Option<()> {
    let player = session.state().turn.active_player.clone();
    let mut agent = GreedyAgent::with_weights(seed, weights);
    let mut view = observe(&AwbwVisibility, session.state(), &player).ok()?;
    while session.state().turn.active_player == player
        && matches!(session.state().match_state, Match::Active { .. })
    {
        observe_into(&AwbwVisibility, session.state(), &player, &mut view).ok()?;
        let command = agent
            .act(&view, NodeBudget::ONE)
            .and_then(|play| play.command(session))
            .unwrap_or_else(|| Command::EndTurn {
                player: player.clone(),
            });
        let order = session.resolve(&command).ok()?;
        session.apply(order, entropy, &mut ()).ok()?;
    }
    Some(())
}

fn outcome_score(outcome: &Outcome, team: &awvm::semantic::TeamId) -> f64 {
    match outcome {
        Outcome::Victory { winners, .. } => f64::from(u8::from(winners.contains(team))),
        Outcome::Draw { .. } | Outcome::Cancelled { .. } => 0.5,
    }
}

fn material_weights() -> EvalWeights {
    EvalWeights {
        exposure: 0.0,
        contest: 0.0,
        front: 0.0,
        ..EvalWeights::STANDARD
    }
}

fn no_front_weights() -> EvalWeights {
    EvalWeights {
        front: 0.0,
        ..EvalWeights::STANDARD
    }
}

fn score(state: &State, seat: awvm::semantic::PlayerIdx, weights: EvalWeights) -> f64 {
    Evaluator::new(weights).value(state, seat)
}

fn component_report(samples: &[Sample]) {
    if samples.is_empty() {
        println!("no changed leaves were sampled");
        return;
    }
    let mut totals = [0.0; 6];
    for sample in samples {
        let seed = sample.audit.seed_breakdown;
        let selected = sample.audit.selected_breakdown;
        totals[0] += selected.score - seed.score;
        totals[1] += selected.army - seed.army;
        totals[2] += selected.income - seed.income;
        totals[3] += selected.exposure - seed.exposure;
        totals[4] += selected.contest - seed.contest;
        totals[5] += selected.front - seed.front;
    }
    let count = samples.len() as f64;
    println!(
        "component deltas: selected minus seed, mean over {} leaves",
        samples.len()
    );
    println!(
        "  score {:+.1}  army {:+.1}  income {:+.1}  exposure {:+.1}  contest {:+.1}  front {:+.1}",
        totals[0] / count,
        totals[1] / count,
        totals[2] / count,
        totals[3] / count,
        totals[4] / count,
        totals[5] / count,
    );
    println!();
}

fn ranking_report(samples: &[Sample]) {
    let samples: Vec<_> = samples
        .iter()
        .filter(|sample| sample.seed_forward.replayed && sample.selected_forward.replayed)
        .collect();
    let mut selected_better = 0;
    let mut seed_better = 0;
    let mut ties = 0;
    for sample in &samples {
        match sample
            .selected_forward
            .score
            .total_cmp(&sample.seed_forward.score)
        {
            std::cmp::Ordering::Greater => selected_better += 1,
            std::cmp::Ordering::Less => seed_better += 1,
            std::cmp::Ordering::Equal => ties += 1,
        }
    }
    let non_ties = selected_better + seed_better;
    let accuracy = if non_ties == 0 {
        0.0
    } else {
        selected_better as f64 / non_ties as f64
    };
    let mean_seed_turns = samples
        .iter()
        .map(|sample| sample.seed_forward.turns)
        .sum::<u32>() as f64
        / samples.len().max(1) as f64;
    let mean_selected_turns = samples
        .iter()
        .map(|sample| sample.selected_forward.turns)
        .sum::<u32>() as f64
        / samples.len().max(1) as f64;
    println!("counterfactual ranking");
    println!(
        "  selected branch better {}  seed better {}  tie {}",
        selected_better, seed_better, ties
    );
    println!(
        "  evaluator ranking accuracy among non-ties {:.1}%",
        accuracy * 100.0
    );
    println!(
        "  mean forward turns seed {:.1}  selected {:.1}",
        mean_seed_turns, mean_selected_turns
    );
    println!();
}

fn threshold_report(samples: &[Sample]) {
    let replayed = samples
        .iter()
        .filter(|sample| sample.seed_forward.replayed && sample.selected_forward.replayed)
        .count();
    println!("improvement thresholds");
    println!("  threshold   accepted   selected-better   seed-better   ties   net score");
    for threshold in [0.0, 500.0, 1_000.0, 2_500.0, 5_000.0, 10_000.0] {
        let accepted: Vec<_> = samples
            .iter()
            .filter(|sample| {
                sample.seed_forward.replayed
                    && sample.selected_forward.replayed
                    && sample.audit.selected_score - sample.audit.seed_score >= threshold
            })
            .collect();
        let (net, selected_better, seed_better, ties) = tally(&accepted, replayed);
        println!(
            "  {:>8.0} {:>10} {:>17} {:>13} {:>7} {:>10.3}",
            threshold,
            accepted.len(),
            selected_better,
            seed_better,
            ties,
            net
        );
    }
    println!();
}

fn agreement_report(samples: &[Sample]) {
    let replayed = samples
        .iter()
        .filter(|sample| sample.seed_forward.replayed && sample.selected_forward.replayed)
        .count();
    println!("simpler evaluator agreement");
    println!(
        "  gate                         accepted   selected-better   seed-better   ties   net score"
    );
    type Gate = (&'static str, fn(&Sample) -> bool);
    let gates: [Gate; 3] = [
        ("material", |sample: &Sample| {
            sample.material_selected > sample.material_seed
        }),
        ("no-front", |sample: &Sample| {
            sample.no_front_selected > sample.no_front_seed
        }),
        ("material + no-front", |sample: &Sample| {
            sample.material_selected > sample.material_seed
                && sample.no_front_selected > sample.no_front_seed
        }),
    ];
    for (name, accepts) in gates {
        let accepted: Vec<_> = samples
            .iter()
            .filter(|sample| {
                sample.seed_forward.replayed && sample.selected_forward.replayed && accepts(sample)
            })
            .collect();
        let (net, selected_better, seed_better, ties) = tally(&accepted, replayed);
        println!(
            "  {:<26} {:>9} {:>17} {:>13} {:>7} {:>10.3}",
            name,
            accepted.len(),
            selected_better,
            seed_better,
            ties,
            net
        );
    }
    println!();
}

fn tally(accepted: &[&Sample], total: usize) -> (f64, usize, usize, usize) {
    let mut net = 0.0;
    let mut selected_better = 0;
    let mut seed_better = 0;
    let mut ties = 0;
    for sample in accepted {
        net += sample.selected_forward.score - sample.seed_forward.score;
        match sample
            .selected_forward
            .score
            .total_cmp(&sample.seed_forward.score)
        {
            std::cmp::Ordering::Greater => selected_better += 1,
            std::cmp::Ordering::Less => seed_better += 1,
            std::cmp::Ordering::Equal => ties += 1,
        }
    }
    let net = if total == 0 { 0.0 } else { net / total as f64 };
    (net, selected_better, seed_better, ties)
}

fn sample_report(samples: &[Sample]) {
    println!("sampled changed leaves");
    for (index, sample) in samples.iter().enumerate() {
        let seed = sample.audit.seed_breakdown;
        let selected = sample.audit.selected_breakdown;
        let change = sample
            .audit
            .changes
            .first()
            .map(|change| {
                format!(
                    "unit {:?}: {:?} -> {:?}",
                    change.unit, change.seed, change.selected
                )
            })
            .unwrap_or_else(|| "no change".to_owned());
        let mut line = String::new();
        let _ = write!(
            line,
            concat!(
                "  {:>3} eval {:.1}->{:.1} forward {:.1}->{:.1} ",
                "army {:.1}->{:.1} income {:.1}->{:.1} ",
                "exposure {:.1}->{:.1} contest {:.1}->{:.1} front {:.1}->{:.1} ",
            ),
            index + 1,
            seed.score,
            selected.score,
            sample.seed_forward.score,
            sample.selected_forward.score,
            seed.army,
            selected.army,
            seed.income,
            selected.income,
            seed.exposure,
            selected.exposure,
            seed.contest,
            selected.contest,
            seed.front,
            selected.front,
        );
        line.push_str(&change);
        println!("{line}");
    }
}
