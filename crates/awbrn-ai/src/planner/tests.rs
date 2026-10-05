use super::*;
use crate::board::arena;
use crate::harness::{Limits, play};
use crate::puzzles::{solve, suite};

fn deterministic() -> PlannerConfig {
    PlannerConfig {
        turn_work: None,
        ..PlannerConfig::V1
    }
}

/// Print what the planner plays in each puzzle.
#[test]
#[ignore = "prints a report"]
fn print_planner_puzzles() {
    for puzzle in suite() {
        let mut agent = PlannerAgent::with_config(7, deterministic());
        let outcome = solve(&puzzle, &mut agent, 11);
        println!("{}: {} {}", outcome.name, outcome.passed, outcome.detail);
        println!("  {:?}", agent.stats());
    }
}

/// Print the candidate scores at the start of each puzzle.
#[test]
#[ignore = "prints a report"]
fn print_puzzle_candidates() {
    for puzzle in suite() {
        let view = awvm::semantic::observe(
            &awvm::semantic::AwbwVisibility,
            &puzzle.state,
            &puzzle.state.turn.active_player,
        )
        .expect("the active player observes");
        let session = Session::from_observation(&view).expect("the view reifies");
        let config = deterministic();
        let seat = session
            .state()
            .players
            .seat(&session.state().turn.active_player)
            .expect("the active player holds a seat");
        let mut evaluator = Evaluator::new(config.eval_weights);
        let mut planner = TurnPlanner {
            config: &config,
            seed: 1,
            seat,
            evaluator: &mut evaluator,
            candidates: 0,
            work: 0,
            work_left: None,
            nodes_left: u32::MAX,
        };
        println!("{}", puzzle.name);
        let mut work = Session::new(session.state().clone());
        let seed = planner
            .line(&mut work, Generator::Seed, &[])
            .expect("the seed line plays");
        println!(
            "  seed score {:.0} reply {:?} plays {:?}",
            seed.score, seed.reply, seed.plays
        );
        for unit in &seed.reply.destroyed {
            if let Some(play) = hold(&work, *unit) {
                let line = planner
                    .line(&mut work, Generator::Safety, &[play])
                    .expect("the safety line plays");
                println!(
                    "  safety score {:.0} reply {:?} plays {:?}",
                    line.score, line.reply, line.plays
                );
                let mut probe = Session::new(work.state().clone());
                let value = planner.evaluator.value_in(&probe, seat);
                let _ = &mut probe;
                println!("  root value {value:.0}");
            }
        }
    }
}

/// The planner solves every puzzle in the suite.
#[test]
fn the_planner_solves_the_puzzle_suite() {
    let failed = suite()
        .iter()
        .filter_map(|puzzle| {
            let mut agent = PlannerAgent::with_config(7, deterministic());
            let outcome = solve(puzzle, &mut agent, 11);
            (!outcome.passed).then(|| format!("{}: {}", outcome.name, outcome.detail))
        })
        .collect::<Vec<_>>();
    assert!(failed.is_empty(), "failed puzzles: {failed:?}");
}

/// A full arena game between two planners has no rejected command, and the
/// same seeds give the same game.
#[test]
fn a_planner_game_is_legal_and_repeatable() {
    let game = || {
        let state = arena(false, 3);
        let mut session = Session::new(state.clone());
        let mut first = PlannerAgent::with_config(1, deterministic());
        let mut second = crate::profile::HARD_V2.agent(2);
        let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut *second];
        let mut entropy = Rng::from_seed(5);
        let limits = Limits {
            days: 12,
            ..Limits::DEFAULT
        };
        let record =
            play(state, &mut session, &mut agents, &mut entropy, limits).expect("the game runs");
        (record, session.state().clone(), first.stats().clone())
    };
    let (record, state, stats) = game();
    assert_eq!(record.refusals, 0);
    assert_eq!(record.preflight_rejections, 0);
    assert_eq!(record.unrealizable_plays, 0);
    assert!(stats.plans > 0);
    let (again, again_state, _) = game();
    assert_eq!(record.commands, again.commands);
    assert_eq!(state, again_state);
}

#[test]
fn the_caller_limits_position_evaluations() {
    for puzzle in suite() {
        let view = awvm::semantic::observe(
            &awvm::semantic::AwbwVisibility,
            &puzzle.state,
            &puzzle.state.turn.active_player,
        )
        .unwrap();
        for budget in [NodeBudget::ONE, NodeBudget::FOUR, NodeBudget::SIXTEEN] {
            let mut agent = PlannerAgent::with_config(7, PlannerConfig::V3);
            agent.act(&view, budget);
            assert!(agent.stats().evaluations <= u64::from(budget.get()));
            assert!(agent.stats().candidates > 0);
            if budget == NodeBudget::ONE {
                assert_eq!(agent.stats().evaluations, 1);
                assert_eq!(agent.stats().chose_seed, 1);
            }
        }
    }
}

#[test]
fn a_spent_work_threshold_starts_no_alternative_or_reply() {
    let puzzle = suite().remove(0);
    let view = awvm::semantic::observe(
        &awvm::semantic::AwbwVisibility,
        &puzzle.state,
        &puzzle.state.turn.active_player,
    )
    .unwrap();
    let mut agent = PlannerAgent::with_config(
        7,
        PlannerConfig {
            turn_work: Some(1),
            ..PlannerConfig::V3
        },
    );
    agent.act(&view, NodeBudget::SIXTEEN);
    assert_eq!(agent.stats().evaluations, 1);
    assert_eq!(agent.stats().chose_seed, 1);
    agent.reject(&view);
    agent.act(&view, NodeBudget::SIXTEEN);
    assert_eq!(agent.stats().evaluations, 1);
    assert_eq!(agent.stats().fallbacks, 1);
}

#[test]
fn a_terminal_alternative_beats_a_nonterminal_seed_after_reply_check() {
    let mut state = suite().remove(0).state;
    let seat = state.players.seat(&state.turn.active_player).unwrap();
    state
        .units
        .retain(|unit| unit.owner == seat || unit.id == UnitId::new(100));
    let mut session = Session::new(state);
    let config = PlannerConfig::V3;
    let mut evaluator = Evaluator::new(config.eval_weights);
    let mut planner = TurnPlanner {
        config: &config,
        seed: 7,
        seat,
        evaluator: &mut evaluator,
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: 16,
    };
    let holds: Vec<_> = session
        .state()
        .units
        .iter()
        .filter(|unit| unit.owner == seat)
        .map(|unit| hold(&session, unit.id).unwrap())
        .collect();
    let seed = planner.line(&mut session, Generator::Seed, &holds).unwrap();
    assert!(seed.score < crate::eval::DECISIVE);
    let seed_value = planner.replied_value(&mut session, &seed).unwrap();
    let line = planner.line(&mut session, Generator::Kill, &[]).unwrap();
    assert!(line.score > 1e8, "the line wins: {}", line.score);
    let before = session.state().clone();
    let result = planner.replied_value(&mut session, &line);
    assert_eq!(session.state(), &before);
    assert_eq!(result, Some(line.score));
    assert!(result.unwrap() > seed_value);
    assert_eq!(planner.nodes_left, 12);
}

#[test]
fn a_crowded_target_does_not_hide_a_later_kill() {
    use awvm::ruleset::UnitKind;
    let mut options: Vec<_> = (0..100)
        .map(|index| combat::AttackOption {
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
    options.push(combat::AttackOption {
        attacker: UnitId::new(500),
        attacker_kind: UnitKind::MdTank,
        destination: CellIdx::from_raw(101),
        target: UnitId::new(201),
        target_hp: 100,
        target_kind: UnitKind::MdTank,
        low: 100,
        high: 110,
        counter_high: 0,
    });
    let found = combat::kills(&options);
    assert!(
        found
            .iter()
            .any(|kills| kills[0].attacks[0].target == UnitId::new(201))
    );
}

#[test]
fn the_production_profile_solves_all_puzzles() {
    for puzzle in suite() {
        let mut agent = crate::profile::HARD.agent(7);
        let mut entropy = Rng::from_seed(11);
        let result = crate::harness::run_agent_turn_unmeasured(
            puzzle.state.clone(),
            &mut *agent,
            &mut entropy,
            crate::profile::HARD.node_budget(),
        )
        .unwrap();
        let turn = crate::puzzles::PuzzleTurn {
            start: &puzzle.state,
            result: &result,
        };
        (puzzle.check)(&turn).unwrap_or_else(|error| panic!("{}: {error}", puzzle.name));
    }
}

fn replay_fixture(name: &str) -> State {
    let path = format!(
        "{}/tests/fixtures/replay_regressions/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// A unit that focused fire destroys is also an exposed unit.
#[test]
fn the_reply_estimate_lists_destroyed_units_as_exposed() {
    let state = replay_fixture("amber-valley-day07");
    let seat = state.players.seat(&state.turn.active_player).unwrap();
    let config = PlannerConfig::V4;
    let mut evaluator = Evaluator::new(config.eval_weights);
    let mut planner = TurnPlanner {
        config: &config,
        seed: 1,
        seat,
        evaluator: &mut evaluator,
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: u32::MAX,
    };
    let mut session = Session::new(state);
    let seed = planner.line(&mut session, Generator::Seed, &[]).unwrap();
    assert!(!seed.reply.exposed.is_empty());
    for unit in &seed.reply.destroyed {
        assert!(seed.reply.exposed.contains(unit));
    }
}

/// Reroutes move only exposed units, differ from the seed plan, and give each
/// unit at most one order of each kind.
#[test]
fn reroutes_give_exposed_units_new_orders_of_different_kinds() {
    let state = replay_fixture("amber-valley-day06");
    let seat = state.players.seat(&state.turn.active_player).unwrap();
    for config in [PlannerConfig::V3, PlannerConfig::V4] {
        let mut evaluator = Evaluator::new(config.eval_weights);
        let mut planner = TurnPlanner {
            config: &config,
            seed: 1,
            seat,
            evaluator: &mut evaluator,
            candidates: 0,
            work: 0,
            work_left: None,
            nodes_left: u32::MAX,
        };
        let mut session = Session::new(state.clone());
        let before = session.state().clone();
        let seed = planner.line(&mut session, Generator::Seed, &[]).unwrap();
        let plays = planner.reroutes(&mut session, &seed);
        assert_eq!(session.state(), &before);
        if config.reroute_units == 0 {
            assert!(plays.is_empty());
            continue;
        }
        assert!(!plays.is_empty());
        for (index, play) in plays.iter().enumerate() {
            assert!(seed.reply.exposed.contains(&play.unit().unwrap()));
            assert!(!seed.plays.contains(play));
            assert!(
                plays[..index]
                    .iter()
                    .all(|other| other.unit() != play.unit() || other.kind() != play.kind())
            );
        }
    }
}

/// In the day 10 position the Hard policy uses the power of Drake after some
/// of its orders. A power plan uses the power before all other orders.
#[test]
fn a_power_plan_uses_the_power_first() {
    let state = replay_fixture("amber-valley-day10");
    let seat = state.players.seat(&state.turn.active_player).unwrap();
    let config = PlannerConfig::V5;
    let mut evaluator = Evaluator::new(config.eval_weights);
    let mut planner = TurnPlanner {
        config: &config,
        seed: 1,
        seat,
        evaluator: &mut evaluator,
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: u32::MAX,
    };
    let mut session = Session::new(state);
    let before = session.state().clone();
    let seed = planner.line(&mut session, Generator::Seed, &[]).unwrap();
    let is_power = |play: &Play| matches!(play.kind(), OrderKind::Power(_));
    let seed_power = seed.plays.iter().position(is_power).unwrap();
    assert!(seed_power > 0, "the seed plan uses the power first");

    let powers: Vec<Play> = powers(&session).collect();
    assert!(!powers.is_empty());
    for power in powers {
        let line = planner
            .line(&mut session, Generator::Power, &[power])
            .unwrap();
        assert_eq!(line.plays.first(), Some(&power));
        assert_eq!(line.plays.iter().filter(|play| is_power(play)).count(), 1);
    }
    assert_eq!(session.state(), &before);
}
