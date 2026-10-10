use super::*;
use crate::harness::run_agent_turn_unmeasured;
use awvm::ruleset;
use awvm::semantic::{AwbwVisibility, ObservedPlayer, observe};

fn day6() -> State {
    let path = format!(
        "{}/tests/fixtures/replay_regressions/amber-valley-day06.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn short_plays(state: &State, plays: &[Play]) -> Vec<String> {
    let dimensions = state.board.dimensions();
    plays
        .iter()
        .map(|play| {
            let destination = dimensions.position_of(play.destination());
            format!(
                "unit={:?} to={destination:?} {:?}",
                play.unit(),
                play.kind()
            )
        })
        .collect()
}

fn short_commands(commands: &[Command]) -> Vec<String> {
    commands
        .iter()
        .map(|command| match command {
            Command::MoveWait { unit, path, .. } => {
                format!("unit={unit:?} to={:?} Wait", path.last())
            }
            Command::MoveCapture { unit, path, .. } => {
                format!("unit={unit:?} to={:?} Capture", path.last())
            }
            Command::MoveAttack {
                unit, path, target, ..
            } => format!("unit={unit:?} to={:?} Attack({target:?})", path.last()),
            Command::ProduceUnit { position, kind, .. } => {
                format!("produce={kind:?} at={position:?}")
            }
            Command::EndTurn { .. } => "EndTurn".to_owned(),
            _ => format!("{command:?}"),
        })
        .collect()
}

struct ForcedLine {
    plays: Vec<Play>,
    next: usize,
}

impl Agent for ForcedLine {
    fn act(&mut self, _view: &Observation, _budget: NodeBudget) -> Option<Play> {
        let play = self.plays.get(self.next).copied();
        self.next += 1;
        play
    }
}

fn army_balance(state: &State, seat: PlayerIdx) -> f64 {
    state
        .units
        .iter()
        .map(|unit| {
            let value = ruleset::profile(unit.kind).cost as f64 * f64::from(unit.hp) / 100.0;
            if unit.owner == seat { value } else { -value }
        })
        .sum()
}

fn external_swing(state: &State, seat: PlayerIdx, plays: Vec<Play>) -> f64 {
    let mut forced = ForcedLine { plays, next: 0 };
    let after_own = run_agent_turn_unmeasured(
        state.clone(),
        &mut forced,
        &mut MeanLuck,
        NodeBudget::SIXTEEN,
    )
    .unwrap()
    .state;
    let mut reply = PlannerAgent::with_config(0x5eed, PlannerConfig::V3);
    let after_reply =
        run_agent_turn_unmeasured(after_own, &mut reply, &mut MeanLuck, NodeBudget::SIXTEEN)
            .unwrap()
            .state;
    army_balance(&after_reply, seat) - army_balance(state, seat)
}

fn replay_swing(
    state: &State,
    seat: PlayerIdx,
    seed: u64,
    config: PlannerConfig,
) -> (f64, Vec<Command>, PlannerStats) {
    let mut candidate = PlannerAgent::with_config(seed, config);
    candidate.start_match();
    let own_turn = run_agent_turn_unmeasured(
        state.clone(),
        &mut candidate,
        &mut MeanLuck,
        NodeBudget::THIRTY_TWO,
    )
    .unwrap();
    let commands = own_turn.commands;
    let stats = candidate.planner_stats().unwrap();
    let mut reply = PlannerAgent::with_config(0x5eed, PlannerConfig::V3);
    reply.start_match();
    let after_reply = run_agent_turn_unmeasured(
        own_turn.state,
        &mut reply,
        &mut MeanLuck,
        NodeBudget::SIXTEEN,
    )
    .unwrap()
    .state;
    (
        army_balance(&after_reply, seat) - army_balance(state, seat),
        commands,
        stats,
    )
}

#[test]
#[ignore = "prints the day-six V6 and V7_DAY6 plan and reply values"]
fn trace_day6_v6_and_v7_clearance() {
    let authoritative = day6();
    let active_player = authoritative.turn.active_player.clone();
    let seat = authoritative.players.seat(&active_player).unwrap();
    let observation = observe(&AwbwVisibility, &authoritative, &active_player).unwrap();
    let observed = Session::from_observation(&observation).unwrap();
    let observed_state = observed.state().clone();
    let actual_funds: Vec<_> = authoritative
        .players
        .iter()
        .map(|player| (player.id().clone(), player.funds))
        .collect();
    let visible_funds: Vec<_> = observation
        .players
        .iter()
        .map(|player| match player {
            ObservedPlayer::Private { id, funds, .. } => (id.clone(), Some(*funds)),
            ObservedPlayer::Public { id, .. } => (id.clone(), None),
        })
        .collect();
    let observed_funds: Vec<_> = observed_state
        .players
        .iter()
        .map(|player| (player.id().clone(), player.funds))
        .collect();
    println!(
        "planning_view=observation actual_funds={actual_funds:?} visible_funds={visible_funds:?} reified_funds={observed_funds:?}"
    );

    for (name, config) in [
        ("V6", PlannerConfig::V6),
        ("V7_DAY6", PlannerConfig::V7_DAY6),
    ] {
        for agent_seed in [1, 2, 3] {
            let mut seed_evaluator = Evaluator::new(config.eval_weights);
            let mut seed_replay = config.replay_score.map(ReplayReader::new);
            let mut seed_planner = TurnPlanner {
                config: &config,
                seed: Rng::mix(agent_seed),
                seat,
                evaluator: &mut seed_evaluator,
                replay: seed_replay.as_mut(),
                candidates: 0,
                work: 0,
                work_left: config.turn_work,
                nodes_left: 32,
            };
            let mut session = Session::new(observed_state.clone());
            let seed_score = seed_planner
                .line(&mut session, Generator::Seed, &[])
                .unwrap()
                .score;

            let mut evaluator = Evaluator::new(config.eval_weights);
            let mut replay = config.replay_score.map(ReplayReader::new);
            let mut planner = TurnPlanner {
                config: &config,
                seed: Rng::mix(agent_seed),
                seat,
                evaluator: &mut evaluator,
                replay: replay.as_mut(),
                candidates: 0,
                work: 0,
                work_left: config.turn_work,
                nodes_left: 32,
            };
            let winner = planner.plan(&session).unwrap();
            let work = planner.work;
            let nodes_left = planner.nodes_left;

            let mut reply_evaluator = Evaluator::new(config.eval_weights);
            let mut reply_replay = config.replay_score.map(ReplayReader::new);
            let mut reply_planner = TurnPlanner {
                config: &config,
                seed: Rng::mix(agent_seed),
                seat,
                evaluator: &mut reply_evaluator,
                replay: reply_replay.as_mut(),
                candidates: 0,
                work: 0,
                work_left: config.turn_work,
                nodes_left: 32,
            };
            let after_reply = reply_planner.replied_value(&mut session, &winner).unwrap();
            let external = external_swing(&authoritative, seat, winner.plays.clone());
            let (replay, replay_commands, replay_stats) =
                replay_swing(&authoritative, seat, agent_seed, config);
            println!(
                "config={name} agent_seed={agent_seed} generator={:?} seed_score={seed_score:.0} line_score={:.0} reply_loss={:.0} after_hard={after_reply:.0} fixed_line_v3_swing={external:.0} replay_v3_swing={replay:.0} replay_mismatches={} replay_plans={} replay_chose_clearance={} replay_commands={:?} work={work} nodes_left={nodes_left} plays={:?}",
                winner.generator,
                winner.score,
                winner.reply.total(),
                replay_stats.mismatches,
                replay_stats.plans,
                replay_stats.chose_clearance,
                short_commands(&replay_commands),
                short_plays(&authoritative, &winner.plays),
            );
        }
    }
}
