use super::*;
use crate::harness::run_agent_turn_unmeasured;
use awvm::ruleset;
use awvm::semantic::{AwbwVisibility, observe};

const REPLY_SEED: u64 = 0x5eed;
const SEEDS: [u64; 3] = [1, 2, 3];

fn day11() -> State {
    let path = format!(
        "{}/tests/fixtures/replay_regressions/amber-valley-day11.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
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

fn selected_line(config: PlannerConfig, seed: u64, state: &State, seat: PlayerIdx) -> Line {
    let observation = observe(&AwbwVisibility, state, &state.turn.active_player).unwrap();
    let session = Session::from_observation(&observation).unwrap();
    let mut evaluator = Evaluator::new(config.eval_weights);
    let mut replay = config.replay_score.map(ReplayReader::new);
    let mut planner = TurnPlanner {
        config: &config,
        seed: Rng::mix(seed),
        seat,
        evaluator: &mut evaluator,
        replay: replay.as_mut(),
        candidates: 0,
        work: 0,
        work_left: config.turn_work,
        nodes_left: NodeBudget::THIRTY_TWO.get(),
    };
    planner.plan(&session).unwrap()
}

fn play_text(state: &State, play: &Play) -> String {
    let position = state
        .board
        .dimensions()
        .position_of(play.destination())
        .unwrap();
    format!(
        "unit={:?} {:?} to ({},{})",
        play.unit(),
        play.kind(),
        position.x,
        position.y
    )
}

#[test]
#[ignore = "prints the day 11 V7 stage and reply trace"]
fn compare_v7_day6_and_v7() {
    let state = day11();
    let seat = state.players.seat(&state.turn.active_player).unwrap();
    let starting_balance = army_balance(&state, seat);

    for (name, config) in [
        ("V7_DAY6", PlannerConfig::V7_DAY6),
        ("V7", PlannerConfig::V7),
    ] {
        let mut total_swing = 0.0;
        for seed in SEEDS {
            let line = selected_line(config, seed, &state, seat);
            let prefix = line
                .plays
                .iter()
                .take(2)
                .map(|play| play_text(&state, play))
                .collect::<Vec<_>>();

            let mut agent = PlannerAgent::with_config(seed, config);
            agent.start_match();
            let mut entropy = MeanLuck;
            let own = run_agent_turn_unmeasured(
                state.clone(),
                &mut agent,
                &mut entropy,
                NodeBudget::THIRTY_TWO,
            )
            .expect("the planner turn executes");
            assert!(own.completed);
            assert_eq!(own.rejected_commands, 0);

            let own_swing = army_balance(&own.state, seat) - starting_balance;
            let mut reply = PlannerAgent::with_config(REPLY_SEED, PlannerConfig::V3);
            reply.start_match();
            let replied =
                run_agent_turn_unmeasured(own.state, &mut reply, &mut entropy, NodeBudget::SIXTEEN)
                    .expect("the V3 reply executes");
            assert!(replied.completed);
            assert_eq!(replied.rejected_commands, 0);

            let swing = army_balance(&replied.state, seat) - starting_balance;
            total_swing += swing;
            println!(
                "{name} seed={seed} generator={:?} line_score={:.0} prefix={prefix:?} own_swing={own_swing:.0} v3_reply_swing={swing:.0} defense_wins={} plans={} mismatches={} candidates={} work={} first_commands={:?}",
                line.generator,
                line.score,
                agent.stats().chose_defense,
                agent.stats().plans,
                agent.stats().mismatches,
                agent.stats().candidates,
                agent.stats().work,
                own.commands.iter().take(2).collect::<Vec<_>>(),
            );
        }
        println!("{name} day11 mean V3-reply swing={:.0}", total_swing / 3.0);
    }
}
