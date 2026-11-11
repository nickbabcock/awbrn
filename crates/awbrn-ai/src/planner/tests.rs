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

#[test]
fn production_does_not_replan_after_unit_names_change() {
    let config = PlannerConfig {
        turn_work: None,
        ..PlannerConfig::V7
    };
    let mut agent = PlannerAgent::with_config(7, config);
    let mut session = Session::new(arena(false, 7));
    let view = awvm::semantic::observe(
        &awvm::semantic::AwbwVisibility,
        session.state(),
        &session.state().turn.active_player,
    )
    .unwrap();
    agent.start_match();
    let first = agent.act(&view, NodeBudget::FOUR).unwrap();
    assert!(matches!(first.kind(), OrderKind::Produce(_)));
    let plans = agent.stats().plans;
    let command = first.command(&session).unwrap();
    session
        .apply_command(command, &mut MeanLuck, &mut ())
        .unwrap();
    let view = awvm::semantic::observe(
        &awvm::semantic::AwbwVisibility,
        session.state(),
        &session.state().turn.active_player,
    )
    .unwrap();
    agent.act(&view, NodeBudget::FOUR);
    assert_eq!(agent.stats().plans, plans);
}

#[test]
fn equivalent_positions_remap_actors_and_reject_changed_unit_facts() {
    use awvm::ruleset::{self, Terrain, UnitKind};
    use awvm::semantic::{Board, Concealment, Pos, Tile, Unit, UnitAction, UnitStore};

    let mut expected = arena(false, 7);
    let ours = expected.players.seat(&expected.turn.active_player).unwrap();
    let other = expected
        .players
        .seats()
        .map(|(seat, _)| seat)
        .find(|seat| *seat != ours)
        .unwrap();
    expected.board = Board::new(9, 9, vec![Tile::new(Terrain::Plain); 81]).unwrap();
    let unit = |id, owner, position| {
        let profile = ruleset::profile(UnitKind::Infantry);
        Unit {
            id: UnitId::new(id),
            kind: UnitKind::Infantry,
            owner,
            hp: 100,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        }
    };
    let actor = unit(200, ours, Pos::new(2, 2));
    let enemy = unit(100, other, Pos::new(8, 8));
    expected.units = UnitStore::new(vec![enemy, actor]).unwrap();
    expected.next_unit_id = Some(201);
    let mut current = expected.clone();
    current.units = UnitStore::new(vec![
        Unit {
            id: UnitId::new(3),
            ..actor
        },
        Unit {
            id: UnitId::new(4),
            ..enemy
        },
    ])
    .unwrap();
    current.next_unit_id = Some(5);
    assert!(position::matches(&expected, &current));
    let destination = current
        .board
        .dimensions()
        .cell_index(Pos::new(2, 3))
        .unwrap();
    let play = Play::new(actor.id, destination, OrderKind::Wait);
    let mapped = position::remap(&expected, &current, play).unwrap();
    assert_eq!(mapped.unit(), Some(UnitId::new(3)));
    assert_eq!(mapped.kind(), play.kind());
    assert_eq!(mapped.destination(), destination);
    current.units.get_mut(UnitId::new(4)).unwrap().hp = 90;
    assert!(!position::matches(&expected, &current));
}

#[test]
fn the_replay_score_survives_match_reset_and_changes_only_the_score() {
    let mut candidate = PlannerAgent::with_config(7, PlannerConfig::V6);
    let state = suite().remove(0).state;
    let view = awvm::semantic::observe(
        &awvm::semantic::AwbwVisibility,
        &state,
        &state.turn.active_player,
    )
    .unwrap();
    let mut plays = Vec::new();
    for _ in 0..2 {
        candidate.start_match();
        assert!(candidate.replay.is_some());
        plays.push(candidate.act(&view, NodeBudget::new(32).unwrap()));
    }
    assert_eq!(plays[0], plays[1]);
    assert!(plays[0].is_some());
    assert_ne!(
        PlannerConfig::V6.fingerprint(),
        PlannerConfig::V5.fingerprint()
    );
    assert!(
        PlannerAgent::with_config(7, PlannerConfig::V5)
            .replay
            .is_none()
    );
}

#[test]
fn the_replay_score_keeps_terminal_scores() {
    let mut state = arena(false, 3);
    let seat = state.players.seats().next().unwrap().0;
    state.match_state = Match::Finished {
        outcome: awvm::semantic::Outcome::Cancelled {
            reason: "test".into(),
        },
    };
    let session = Session::new(state);
    let mut evaluator = Evaluator::new(PlannerConfig::V6.eval_weights);
    let mut replay = ReplayReader::new(ReplayScore::CUP_3_4);
    let mut planner = TurnPlanner {
        rollout: None,
        config: &PlannerConfig::V6,
        seed: 7,
        seat,
        evaluator: &mut evaluator,
        replay: Some(&mut replay),
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: 32,
        fog: None,
        march: MarchTurn::default(),
    };
    assert_eq!(planner.leaf_value(&session), 0.0);
    let own_team = session.state().player(seat).team.clone();
    let other_team = session
        .state()
        .players
        .seats()
        .find(|(other, _)| *other != seat)
        .unwrap()
        .1
        .team
        .clone();
    for (winner, expected) in [
        (own_team, crate::eval::DECISIVE),
        (other_team, -crate::eval::DECISIVE),
    ] {
        let mut state = session.state().clone();
        state.match_state = Match::Finished {
            outcome: awvm::semantic::Outcome::Victory {
                winners: vec![winner],
                reason: awvm::semantic::VictoryReason::HqCapture,
            },
        };
        assert_eq!(planner.leaf_value(&Session::new(state)), expected);
    }
}

#[test]
fn the_replay_score_is_antisymmetric_and_fog_keeps_the_stock_score() {
    let mut state = arena(false, 3);
    let seats = state
        .players
        .seats()
        .map(|(seat, _)| seat)
        .collect::<Vec<_>>();
    state.player_mut(seats[0]).funds = 20_000;
    state.player_mut(seats[1]).funds = 5_000;
    let session = Session::new(state.clone());
    let mut replay = ReplayReader::new(ReplayScore::CUP_3_4);
    let a = replay.value_in(&session, seats[0]);
    let b = replay.value_in(&session, seats[1]);
    assert!(a > 0.0);
    assert!((a + b).abs() < 1e-9);

    state.settings.fog = true;
    let fog = Session::new(state);
    let mut evaluator = Evaluator::new(PlannerConfig::V6.eval_weights);
    let expected = evaluator.value_in(&fog, seats[0]);
    let mut planner = TurnPlanner {
        rollout: None,
        config: &PlannerConfig::V6,
        seed: 7,
        seat: seats[0],
        evaluator: &mut evaluator,
        replay: Some(&mut replay),
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: 32,
        fog: None,
        march: MarchTurn::default(),
    };
    assert_eq!(planner.leaf_value(&fog), expected);
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
            rollout: None,
            config: &config,
            seed: 1,
            seat,
            evaluator: &mut evaluator,
            replay: None,
            candidates: 0,
            work: 0,
            work_left: None,
            nodes_left: u32::MAX,
            fog: None,
            march: MarchTurn::default(),
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
fn stored_line_positions_and_replies_match_command_replay() {
    for puzzle in suite() {
        for explicit_boundary in [false, true] {
            let mut session = Session::new(puzzle.state.clone());
            let initial = session.state().clone();
            let seat = initial.players.seat(&initial.turn.active_player).unwrap();
            let config = PlannerConfig::V3;
            let mut evaluator = Evaluator::new(config.eval_weights);
            let mut planner = TurnPlanner {
                rollout: None,
                config: &config,
                seed: 7,
                seat,
                evaluator: &mut evaluator,
                replay: None,
                candidates: 0,
                work: 0,
                work_left: None,
                nodes_left: 16,
                fog: None,
                march: MarchTurn::default(),
            };
            let prefix = if explicit_boundary {
                vec![Play::unitless(
                    initial
                        .board
                        .dimensions()
                        .cell_index(awvm::semantic::Pos::new(0, 0))
                        .unwrap(),
                    OrderKind::EndTurn,
                )]
            } else {
                Vec::new()
            };
            let line = planner
                .line(&mut session, Generator::Seed, &prefix)
                .unwrap();
            assert_eq!(session.state(), &initial);
            let mut replay = Session::new(initial.clone());
            let mut entropy = MeanLuck;
            for play in &line.plays {
                let order = play_order(&replay, play).unwrap();
                replay.apply(order, &mut entropy, &mut ()).unwrap();
            }
            if !explicit_boundary && matches!(replay.state().match_state, Match::Active { .. }) {
                let end = replay
                    .resolve(&Command::EndTurn {
                        player: initial.turn.active_player.clone(),
                    })
                    .unwrap();
                replay.apply(end, &mut entropy, &mut ()).unwrap();
            }
            assert_eq!(replay.state(), &line.end_state);
            if explicit_boundary && matches!(replay.state().match_state, Match::Active { .. }) {
                let end = replay
                    .resolve(&Command::EndTurn {
                        player: initial.turn.active_player.clone(),
                    })
                    .unwrap();
                replay.apply(end, &mut entropy, &mut ()).unwrap();
            }
            let enemy = replay.state().turn.active_player.clone();
            let mut hard = config
                .baseline
                .build_greedy(Rng::mix(planner.seed ^ 0x5eed));
            let mut decisions = 0;
            while replay.state().turn.active_player == enemy
                && matches!(replay.state().match_state, Match::Active { .. })
            {
                decisions += 1;
                let order = hard.order_in_session(&replay).unwrap_or_else(|| {
                    replay
                        .resolve(&Command::EndTurn {
                            player: enemy.clone(),
                        })
                        .unwrap()
                });
                replay.apply(order, &mut entropy, &mut ()).unwrap();
            }
            let expected = planner.leaf_value(&replay);
            let work = planner.work;
            assert_eq!(planner.replied_value(&mut session, &line), Some(expected));
            assert_eq!(planner.work - work, decisions);
            assert_eq!(session.state(), &initial);
        }
    }
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
        rollout: None,
        config: &config,
        seed: 7,
        seat,
        evaluator: &mut evaluator,
        replay: None,
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: 16,
        fog: None,
        march: MarchTurn::default(),
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
    // planner-v8 does not pass this puzzle: the wide search of
    // planner-v7-wide chooses another line. Remove the name when it passes.
    const NOT_PASSED: [&str; 1] = ["capture-denial"];
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
        let solved = (puzzle.check)(&turn);
        if NOT_PASSED.contains(&puzzle.name) {
            assert!(solved.is_err(), "{} passes now", puzzle.name);
        } else {
            solved.unwrap_or_else(|error| panic!("{}: {error}", puzzle.name));
        }
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
        rollout: None,
        config: &config,
        seed: 1,
        seat,
        evaluator: &mut evaluator,
        replay: None,
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: u32::MAX,
        fog: None,
        march: MarchTurn::default(),
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
            rollout: None,
            config: &config,
            seed: 1,
            seat,
            evaluator: &mut evaluator,
            replay: None,
            candidates: 0,
            work: 0,
            work_left: None,
            nodes_left: u32::MAX,
            fog: None,
            march: MarchTurn::default(),
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
        rollout: None,
        config: &config,
        seed: 1,
        seat,
        evaluator: &mut evaluator,
        replay: None,
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: u32::MAX,
        fog: None,
        march: MarchTurn::default(),
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

/// A fog game with the fog memory has no rejected command, and the memory
/// reads the enemy. A game without fog plays as planner-v6 does.
#[test]
fn the_fog_memory_plays_legal_fog_games_and_leaves_standard_games_alone() {
    let game = |config: PlannerConfig, fog: bool| {
        let state = arena(fog, 3);
        let mut session = Session::new(state.clone());
        let mut first = PlannerAgent::with_config(1, config);
        let mut second = crate::profile::HARD_V2.agent(2);
        let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut *second];
        let mut entropy = Rng::from_seed(5);
        let limits = Limits {
            days: 10,
            nodes: NodeBudget::new(8).expect("a budget"),
            ..Limits::DEFAULT
        };
        let record =
            play(state, &mut session, &mut agents, &mut entropy, limits).expect("the game runs");
        let enemies = first
            .memory
            .estimate(session.state())
            .map(|estimate| estimate.enemies.len());
        (record, session.state().clone(), enemies)
    };

    let (record, _, enemies) = game(PlannerConfig::V6_FOG, true);
    assert_eq!(record.refusals, 0);
    assert_eq!(record.preflight_rejections, 0);
    assert_eq!(record.unrealizable_plays, 0);
    assert_eq!(enemies, Some(1), "the memory has an estimate of the enemy");

    let (standard, standard_state, enemies) = game(PlannerConfig::V6_FOG, false);
    let (v6, v6_state, _) = game(PlannerConfig::V6, false);
    assert_eq!(enemies, None, "the memory reads only fog games");
    assert_eq!(standard.commands, v6.commands);
    assert_eq!(standard_state, v6_state);
}

#[test]
fn a_fog_prediction_keeps_the_observed_state_separate_from_the_model() {
    let state = arena(true, 1);
    let view = awvm::semantic::observe(
        &awvm::semantic::AwbwVisibility,
        &state,
        &state.turn.active_player,
    )
    .expect("the active player has a view");
    let observed = Session::from_observation(&view).expect("the view reifies");
    let mut memory = FogMemory::new();
    memory.update(&view);
    let modeled = Session::new(memory.planning_state(observed.state()));
    assert_ne!(modeled.state(), observed.state());
    let predicted = line_positions(&modeled, &[], true).expect("the model projects");
    assert_eq!(predicted, vec![observed.state().clone()]);
}

#[test]
fn a_predicted_loss_of_sight_does_not_require_a_replan() {
    use awvm::ruleset::{self, Terrain, UnitKind};
    use awvm::semantic::{Concealment, Pos, TileOwner, Unit, UnitAction};
    let mut state = arena(true, 1);
    state.units.retain(|_| false);
    for position in state.board.dimensions().positions() {
        let tile = state.board.tile_mut(position);
        tile.terrain = Terrain::Plain;
        tile.owner = TileOwner::NotOwnable;
        tile.capture_points = None;
        tile.silo = None;
    }
    let ours = state.players.seat(&state.turn.active_player).unwrap();
    let theirs = state
        .players
        .seats()
        .map(|(seat, _)| seat)
        .find(|seat| *seat != ours)
        .unwrap();
    for (id, kind, owner, position) in [
        (1, UnitKind::Recon, ours, Pos::new(2, 2)),
        (2, UnitKind::Tank, theirs, Pos::new(6, 2)),
    ] {
        let profile = ruleset::profile(kind);
        state.units.push(Unit {
            id: UnitId::new(id),
            kind,
            owner,
            hp: 100,
            fuel: profile.max_fuel,
            ammo: profile.max_ammo,
            action: UnitAction::Ready,
            concealment: Concealment::Exposed,
            location: Location::Board { position },
        });
    }
    state.next_unit_id = Some(3);
    let observe = |state: &State| {
        awvm::semantic::observe(
            &awvm::semantic::AwbwVisibility,
            state,
            &state.turn.active_player,
        )
        .expect("the active player has a view")
    };
    let observed = Session::from_observation(&observe(&state)).expect("the view reifies");
    assert!(observed.state().units.get(UnitId::new(2)).is_some());
    let destination = state.board.dimensions().cell_index(Pos::new(2, 6)).unwrap();
    let play = Play::new(UnitId::new(1), destination, OrderKind::Wait);
    let predicted = line_positions(&observed, &[play], true).expect("the line projects");
    let mut truth = Session::new(state);
    let command = play.command(&truth).expect("the recon can move");
    truth
        .apply_command::<()>(command, &mut MeanLuck, &mut ())
        .expect("the move plays");
    let actual = Session::from_observation(&observe(truth.state())).expect("the view reifies");
    assert!(actual.state().units.get(UnitId::new(2)).is_none());
    assert_eq!(predicted.last(), Some(actual.state()));
    let unprojected = line_positions(&observed, &[play], false).expect("the old line plays");
    assert_ne!(unprojected.last(), Some(actual.state()));
}

/// planner-v8 without the march plan.
const WITHOUT_MARCH: PlannerConfig = PlannerConfig {
    march: false,
    ..PlannerConfig::V8
};

/// planner-v8 without the power policy. Its seed plan does not use a
/// power first. On a board of [`win_board`], only the march plan finds the
/// power that wins at once.
const MARCH_ONLY: PlannerConfig = PlannerConfig {
    baseline: crate::baseline::BaselineConfig {
        weights: crate::agents::Weights {
            power_policy: false,
            ..PlannerConfig::V8.baseline.weights
        },
        ..PlannerConfig::V8.baseline
    },
    ..PlannerConfig::V8
};

/// The new fields and configurations do not change the fingerprints of the
/// earlier configurations.
#[test]
fn the_earlier_fingerprints_do_not_change() {
    use crate::baseline::BaselineConfig;
    for (config, expected) in [
        (PlannerConfig::V1, "04631b39a118c112"),
        (PlannerConfig::V2, "4bce3cce581114fd"),
        (PlannerConfig::V3, "009b7ab9367a53fa"),
        (PlannerConfig::V4, "274ca94a2c2a9114"),
        (PlannerConfig::V5, "e4c8c86f953dad0f"),
        (PlannerConfig::V6, "26569da0906e9bc4"),
        (PlannerConfig::V6_CANONICAL, "e73912d2f3594cb1"),
        (PlannerConfig::V7, "2b71493852d42c91"),
        (PlannerConfig::V7_WIDE, "c7634e08efd41e91"),
        (PlannerConfig::V6_FOG, "3aac2b950e552bab"),
    ] {
        assert_eq!(config.fingerprint(), expected, "{}", config.identifier);
    }
    for (config, expected) in [
        (BaselineConfig::LOCKED, "79aa8a6e0491065f"),
        (BaselineConfig::PRODUCTION, "af672904643d966c"),
        (crate::profile::HARD_V2_CONFIG, "1be508294d967fa6"),
        (crate::profile::HARD_V3_CONFIG, "12bfc85325aa3cdc"),
    ] {
        assert_eq!(config.fingerprint(), expected, "{}", config.identifier);
    }
}

/// The largest number of nodes that one plan can use.
///
/// Each complete line uses one node: the seed, the winning capture of the
/// march plan, each kill plan, at most four power positions with the power
/// policy (three for the power of the seed plan, one for the other power),
/// each safety and block plan, and each reroute
/// plan. Each reply check uses one node: the seed and each checked
/// alternative.
fn plan_node_bound(config: &PlannerConfig) -> usize {
    let lines = 1
        + usize::from(config.march)
        + config.kill_plans
        + if config.baseline.weights.power_policy {
            4
        } else {
            usize::from(config.power_first) * 2
        }
        + config.safety_plans
        + config.block_plans
        + config.reroute_units * config.reroute_orders;
    let checks = if config.hard_reply_top == 0 {
        0
    } else {
        1 + config.hard_reply_top
    };
    lines + checks
}

#[test]
fn the_node_budget_does_not_limit_the_wide_configurations() {
    // The v7 plans use a node budget of 32.
    assert_eq!(plan_node_bound(&PlannerConfig::V7), 29);
    // The Hard profile and the plans of the wide configurations use a node
    // budget of 256.
    assert_eq!(plan_node_bound(&PlannerConfig::V7_WIDE), 212);
    assert_eq!(plan_node_bound(&PlannerConfig::V8), 215);
}

/// A game of planner-v8 against planner-v7 has no refused command, and
/// the same seeds give the same game.
#[test]
fn a_planner_v8_game_is_legal_and_repeatable() {
    let game = || {
        let state = arena(false, 3);
        let mut session = Session::new(state.clone());
        let mut first = PlannerAgent::with_config(
            1,
            PlannerConfig {
                plans_per_turn: 4,
                ..PlannerConfig::V8
            },
        );
        let mut second = PlannerAgent::with_config(2, PlannerConfig::V7);
        let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];
        let mut entropy = Rng::from_seed(5);
        let limits = Limits {
            days: 8,
            ..Limits::DEFAULT
        };
        let record =
            play(state, &mut session, &mut agents, &mut entropy, limits).expect("the game runs");
        (record, session.state().clone())
    };
    let (record, state) = game();
    assert_eq!(record.refusals, 0);
    assert_eq!(record.preflight_rejections, 0);
    assert_eq!(record.unrealizable_plays, 0);
    let (again, again_state) = game();
    assert_eq!(record.commands, again.commands);
    assert_eq!(state, again_state);
}

/// A plain board with no units and no properties, without fog, and the
/// active seat and its enemy.
fn empty_board(seed: u64) -> (State, PlayerIdx, PlayerIdx) {
    use awvm::ruleset::Terrain;
    use awvm::semantic::TileOwner;
    let mut state = arena(false, seed);
    state.units.retain(|_| false);
    for position in state.board.dimensions().positions() {
        let tile = state.board.tile_mut(position);
        tile.terrain = Terrain::Plain;
        tile.owner = TileOwner::NotOwnable;
        tile.capture_points = None;
        tile.silo = None;
    }
    let ours = state.players.seat(&state.turn.active_player).unwrap();
    let theirs = state
        .players
        .seats()
        .map(|(seat, _)| seat)
        .find(|seat| *seat != ours)
        .unwrap();
    (state, ours, theirs)
}

fn board_unit(
    id: u32,
    kind: awvm::ruleset::UnitKind,
    owner: PlayerIdx,
    location: Location,
) -> awvm::semantic::Unit {
    let profile = awvm::ruleset::profile(kind);
    awvm::semantic::Unit {
        id: UnitId::new(id),
        kind,
        owner,
        hp: 100,
        fuel: profile.max_fuel,
        ammo: profile.max_ammo,
        action: awvm::semantic::UnitAction::Ready,
        concealment: awvm::semantic::Concealment::Exposed,
        location,
    }
}

fn set_property(
    state: &mut State,
    position: awvm::semantic::Pos,
    terrain: awvm::ruleset::Terrain,
    owner: awvm::semantic::TileOwner,
) {
    let tile = state.board.tile_mut(position);
    tile.terrain = terrain;
    tile.owner = owner;
    tile.capture_points = Some(awvm::semantic::CAPTURE_REQUIRED_POINTS);
}

/// A board with sea, an own island that holds nothing more to take, and a
/// small island with a neutral city.
fn island_board(seed: u64) -> (State, PlayerIdx, PlayerIdx) {
    use awvm::ruleset::Terrain;
    use awvm::semantic::{Pos, TileOwner};
    let (mut state, ours, theirs) = empty_board(seed);
    for position in state.board.dimensions().positions() {
        state.board.tile_mut(position).terrain = Terrain::Sea;
    }
    for x in 0..4 {
        for y in 0..4 {
            state.board.tile_mut(Pos::new(x, y)).terrain = Terrain::Plain;
        }
    }
    set_property(
        &mut state,
        Pos::new(0, 0),
        Terrain::City,
        TileOwner::Owned(ours),
    );
    set_property(
        &mut state,
        Pos::new(7, 1),
        Terrain::City,
        TileOwner::Neutral,
    );
    // The enemy island, with its headquarters and an infantry.
    let width = state.board.dimensions().width();
    let height = state.board.dimensions().height();
    let far = Pos::new(width - 1, height - 1);
    state
        .board
        .tile_mut(Pos::new(width - 2, height - 1))
        .terrain = Terrain::Plain;
    set_property(&mut state, far, Terrain::Hq, TileOwner::Owned(theirs));
    state.units.push(board_unit(
        90,
        awvm::ruleset::UnitKind::Infantry,
        theirs,
        Location::Board {
            position: Pos::new(width - 2, height - 1),
        },
    ));
    state.next_unit_id = Some(100);
    (state, ours, theirs)
}

/// The ferry loads a spare capturer, carries it to the island, and puts it
/// down on the city. Each play is legal when it is played.
#[test]
fn the_ferry_carries_a_spare_capturer_to_an_island() {
    use awvm::ruleset::UnitKind;
    use awvm::semantic::Pos;
    let (mut state, ours, _) = island_board(5);
    let board = |position| Location::Board { position };
    state.units.push(board_unit(
        1,
        UnitKind::Infantry,
        ours,
        board(Pos::new(1, 1)),
    ));
    state.units.push(board_unit(
        2,
        UnitKind::TCopter,
        ours,
        board(Pos::new(2, 1)),
    ));
    let cell = |state: &State, position| state.board.dimensions().cell_index(position).unwrap();

    // The capturer boards.
    let session = Session::new(state.clone());
    let plays = ferry_plays(&session);
    let load = Play::new(
        UnitId::new(1),
        cell(&state, Pos::new(2, 1)),
        OrderKind::Load,
    );
    assert_eq!(plays, vec![load]);

    // The loaded transport flies next to the city and puts the capturer down.
    let mut loaded = state.clone();
    loaded.units.get_mut(UnitId::new(1)).unwrap().location = Location::Cargo {
        transport: UnitId::new(2),
        slot: 0,
    };
    let mut session = Session::new(loaded.clone());
    let plays = ferry_plays(&session);
    assert_eq!(plays.len(), 2, "{plays:?}");
    assert_eq!(plays[0].unit(), Some(UnitId::new(2)));
    assert_eq!(plays[0].kind(), OrderKind::Wait);
    assert_eq!(plays[1].kind(), OrderKind::Unload(0));
    assert_eq!(plays[1].destination(), cell(&loaded, Pos::new(7, 1)));
    for play in &plays {
        let command = play.command(&session).expect("the play is legal");
        session
            .apply_command::<()>(command, &mut MeanLuck, &mut ())
            .expect("the play applies");
    }
    assert_eq!(
        session.state().units.get(UnitId::new(1)).unwrap().location,
        Location::Board {
            position: Pos::new(7, 1)
        }
    );
    // The island now holds an own capturer, so the ferry has nothing to do.
    assert!(ferry_plays(&session).is_empty());
}

/// Without a transport, the ferry builds the cheapest one. A capturer with
/// a property left on its own landmass is not spare, and fog turns the
/// ferry off.
#[test]
fn the_ferry_builds_a_transport_only_for_a_spare_capturer() {
    use awvm::ruleset::{Terrain, UnitKind};
    use awvm::semantic::{Pos, TileOwner};
    let (mut state, ours, _) = island_board(6);
    let board = |position| Location::Board { position };
    state.units.push(board_unit(
        1,
        UnitKind::Infantry,
        ours,
        board(Pos::new(1, 1)),
    ));
    set_property(
        &mut state,
        Pos::new(3, 3),
        Terrain::Airport,
        TileOwner::Owned(ours),
    );
    state.player_mut(ours).funds = 20_000;
    let session = Session::new(state.clone());
    let plays = ferry_plays(&session);
    let site = state.board.dimensions().cell_index(Pos::new(3, 3)).unwrap();
    assert_eq!(
        plays,
        vec![Play::unitless(site, OrderKind::Produce(UnitKind::TCopter))]
    );
    let mut busy = state.clone();
    set_property(&mut busy, Pos::new(0, 3), Terrain::City, TileOwner::Neutral);
    assert!(ferry_plays(&Session::new(busy)).is_empty());
    let mut fog = state;
    fog.settings.fog = true;
    assert!(ferry_plays(&Session::new(fog)).is_empty());
}

/// A board with an enemy infantry of `commander` at 7 visible health on our
/// headquarters, which has `points` left. The enemy turn starts.
fn capture_board(
    commander: awvm::ruleset::CommanderKind,
    points: u8,
) -> (State, PlayerIdx, UnitId, CellIdx) {
    use awvm::ruleset::{Terrain, UnitKind};
    use awvm::semantic::{Pos, TileOwner};
    let (mut state, ours, theirs) = empty_board(5);
    let hq = Pos::new(2, 2);
    set_property(&mut state, hq, Terrain::Hq, TileOwner::Owned(ours));
    state.board.tile_mut(hq).capture_points = Some(points);
    let mut capturer = board_unit(
        1,
        UnitKind::Infantry,
        theirs,
        Location::Board { position: hq },
    );
    capturer.hp = 70;
    state.units.push(capturer);
    state.next_unit_id = Some(2);
    for co in state.player_mut(theirs).commanders.iter_mut() {
        co.id = commander;
        co.power_charge = 0;
    }
    state.turn.active_player = state.player(theirs).id().clone();
    let cell = state.board.dimensions().cell_index(hq).unwrap();
    (state, ours, UnitId::new(1), cell)
}

/// The points that the engine takes off the property in one capture.
fn engine_capture(state: &State, unit: UnitId, cell: CellIdx) -> (u8, bool) {
    let mut session = Session::new(state.clone());
    let command = Play::new(unit, cell, OrderKind::Capture)
        .command(&session)
        .expect("the capture is legal");
    session
        .apply_command::<()>(command, &mut MeanLuck, &mut ())
        .expect("the capture plays");
    let position = state.board.dimensions().position_of(cell).unwrap();
    let before = state.board.tile(position).capture_points.unwrap();
    let after = session.state().board.tile(position);
    let finished = !matches!(session.state().match_state, Match::Active { .. })
        || after.owner != state.board.tile(position).owner;
    let left = if finished {
        0
    } else {
        after.capture_points.unwrap()
    };
    (before - left, finished)
}

/// A Sami capturer at 7 visible health takes 10 points in a turn. With the
/// capture bonus, the reply estimate agrees with the engine: two turns for a
/// whole property, and one for a property with 10 points left. Without it,
/// the estimate reads 7 points and three turns.
#[test]
fn the_capture_bonus_follows_the_engine_for_sami() {
    use crate::agents::greedy::{CaptureTurn, capture_strength};
    use awvm::ruleset::CommanderKind;
    let (state, ours, unit, cell) = capture_board(CommanderKind::Sami, 20);
    let (taken, finished) = engine_capture(&state, unit, cell);
    assert_eq!((taken, finished), (10, false));
    let capturer = state.units.get(unit).unwrap();
    assert_eq!(
        capture_strength(&state, capturer, 7, CaptureTurn::Next),
        taken
    );
    assert_eq!(
        capture_strength(&state, capturer, 7, CaptureTurn::Now),
        taken
    );

    let (state, ours_again, unit, cell) = capture_board(CommanderKind::Sami, 10);
    assert_eq!(ours, ours_again);
    assert_eq!(engine_capture(&state, unit, cell), (10, true));
    let session = Session::new(state.clone());
    let values = PropertyValues::STANDARD;
    assert!(
        reply::estimate(
            &session,
            ours,
            values,
            reply::ReplyOptions {
                commander_effects: true,
                march: false
            }
        )
        .headquarters_lost
    );
    assert!(
        !reply::estimate(
            &session,
            ours,
            values,
            reply::ReplyOptions {
                commander_effects: false,
                march: false
            }
        )
        .headquarters_lost
    );
    assert!(
        !reply::estimate(&session, ours, values, reply::ReplyOptions::default()).headquarters_lost
    );
}

/// When the enemy meter can pay for the Sami super power, a capture in the
/// next enemy turn can take the whole property.
#[test]
fn the_capture_bonus_reads_an_affordable_super_power() {
    use crate::agents::greedy::{CaptureTurn, capture_strength};
    use awvm::commander::{PowerLevel, power_activation_cost};
    use awvm::ruleset::CommanderKind;
    let (mut state, ours, unit, _) = capture_board(CommanderKind::Sami, 20);
    let theirs = state.units.get(unit).unwrap().owner;
    let cost = power_activation_cost(CommanderKind::Sami, PowerLevel::Scop, 0)
        .unwrap()
        .unwrap();
    for co in state.player_mut(theirs).commanders.iter_mut() {
        co.power_charge = cost;
    }
    let capturer = state.units.get(unit).unwrap();
    assert_eq!(capture_strength(&state, capturer, 7, CaptureTurn::Next), 20);
    assert_eq!(capture_strength(&state, capturer, 7, CaptureTurn::Now), 10);
    let session = Session::new(state.clone());
    let values = PropertyValues::STANDARD;
    assert!(
        reply::estimate(
            &session,
            ours,
            values,
            reply::ReplyOptions {
                commander_effects: true,
                march: false
            }
        )
        .headquarters_lost
    );
}

/// A commander with no capture bonus gets the same estimates with and
/// without the switch, and they agree with the engine.
#[test]
fn the_capture_bonus_does_not_change_a_commander_without_one() {
    use crate::agents::greedy::{CaptureTurn, capture_strength};
    use awvm::ruleset::CommanderKind;
    for commander in [CommanderKind::Andy, CommanderKind::Max, CommanderKind::Grit] {
        let (mut state, ours, unit, cell) = capture_board(commander, 10);
        let theirs = state.units.get(unit).unwrap().owner;
        for co in state.player_mut(theirs).commanders.iter_mut() {
            co.power_charge = u64::MAX / 4;
        }
        assert_eq!(engine_capture(&state, unit, cell), (7, false));
        let capturer = state.units.get(unit).unwrap();
        assert_eq!(capture_strength(&state, capturer, 7, CaptureTurn::Next), 7);
        let session = Session::new(state.clone());
        let values = PropertyValues::STANDARD;
        assert_eq!(
            reply::estimate(
                &session,
                ours,
                values,
                reply::ReplyOptions {
                    commander_effects: true,
                    march: false
                }
            ),
            reply::estimate(
                &session,
                ours,
                values,
                reply::ReplyOptions {
                    commander_effects: false,
                    march: false
                }
            ),
            "{commander:?}"
        );
    }
}

/// A turn planner for `config` with no work limit, for probes of one
/// position.
fn probe_planner<'a>(
    config: &'a PlannerConfig,
    evaluator: &'a mut Evaluator,
    replay: Option<&'a mut ReplayReader>,
    seat: PlayerIdx,
) -> TurnPlanner<'a> {
    TurnPlanner {
        rollout: None,
        config,
        seed: 1,
        seat,
        evaluator,
        replay,
        candidates: 0,
        work: 0,
        work_left: None,
        nodes_left: u32::MAX,
        fog: None,
        march: MarchTurn::default(),
    }
}

/// One power position of a probe.
#[derive(Debug)]
struct Placed {
    /// The unit orders before the power.
    orders_before: usize,
    /// The attacks before the power, and after it.
    attacks: (usize, usize),
    /// The score of the line.
    score: f64,
    /// The value after the simulated Hard reply. Only the debug output
    /// reads it.
    #[allow(dead_code)]
    replied: f64,
}

/// Score the seed plan and each power position of
/// [`TurnPlanner::placements`] in `state`, and return them with the line
/// that the planner chooses.
fn placement_probe(state: State, config: &PlannerConfig) -> (Vec<Placed>, Line) {
    let seat = state.players.seat(&state.turn.active_player).unwrap();
    let mut evaluator = Evaluator::new(config.eval_weights);
    let mut replay = config.replay_score.map(ReplayReader::new);
    let mut planner = probe_planner(config, &mut evaluator, replay.as_mut(), seat);
    let mut session = Session::new(state);
    let before = session.state().clone();
    let seed = planner.line(&mut session, Generator::Seed, &[]).unwrap();
    let prefixes: Vec<Vec<Play>> = planner
        .placements(&session, &seed)
        .into_iter()
        .map(|(prefix, _, _)| prefix)
        .collect();
    let mut lines = vec![seed];
    for prefix in prefixes {
        let line = planner.line(&mut session, Generator::Power, &prefix);
        let line = line.unwrap_or_else(|| panic!("no line: {prefix:?}"));
        lines.push(line);
    }
    let placed = lines
        .iter()
        .map(|line| {
            let at = line
                .plays
                .iter()
                .position(|play| matches!(play.kind(), OrderKind::Power(_)))
                .unwrap_or_else(|| {
                    panic!(
                        "no power: {:?}",
                        line.plays.iter().map(Play::kind).collect::<Vec<_>>()
                    )
                });
            let attack = |play: &&Play| matches!(play.kind(), OrderKind::Attack(_));
            let replied = planner.replied_value(&mut session, line).unwrap();
            Placed {
                orders_before: line.plays[..at]
                    .iter()
                    .filter(|p| p.unit().is_some())
                    .count(),
                attacks: (
                    line.plays[..at].iter().filter(attack).count(),
                    line.plays[at..].iter().filter(attack).count(),
                ),
                score: line.score,
                replied,
            }
        })
        .collect();
    let chosen = planner.plan(&session).unwrap();
    assert_eq!(session.state(), &before);
    (placed, chosen)
}

/// An empty board where the active seat has `commander` with a full meter,
/// and the enemy has Andy with an empty meter.
fn power_board(commander: awvm::ruleset::CommanderKind) -> (State, PlayerIdx, PlayerIdx) {
    let (mut state, ours, theirs) = empty_board(5);
    for co in state.player_mut(ours).commanders.iter_mut() {
        co.id = commander;
        co.power_charge = 1_000_000;
    }
    for co in state.player_mut(theirs).commanders.iter_mut() {
        co.id = awvm::ruleset::CommanderKind::Andy;
        co.power_charge = 0;
    }
    (state, ours, theirs)
}

/// The index of the power position in `line`: the unit orders before the
/// power.
fn power_position(line: &Line) -> (usize, usize) {
    let at = line
        .plays
        .iter()
        .position(|play| matches!(play.kind(), OrderKind::Power(_)))
        .expect("the line uses the power");
    let units = line.plays.iter().filter(|p| p.unit().is_some()).count();
    (
        line.plays[..at]
            .iter()
            .filter(|p| p.unit().is_some())
            .count(),
        units,
    )
}

/// The boards of the power probes. `distance` is the distance from the Grit
/// artillery to the enemy tank, and `hp` the health of the Andy tanks.
fn eagle_board() -> State {
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    let (mut state, ours, theirs) = power_board(CommanderKind::Eagle);
    let board = |position| Location::Board { position };
    for x in 0..4u8 {
        state.units.push(board_unit(
            1 + u32::from(x),
            UnitKind::Tank,
            ours,
            board(Pos::new(3 + 2 * x, 3)),
        ));
        state.units.push(board_unit(
            11 + u32::from(x),
            UnitKind::Tank,
            theirs,
            board(Pos::new(3 + 2 * x, 4)),
        ));
    }
    for (id, x) in [(15, 14), (16, 16)] {
        state.units.push(board_unit(
            id,
            UnitKind::Tank,
            theirs,
            board(Pos::new(x, 14)),
        ));
    }
    state.next_unit_id = Some(20);
    state
}

fn grit_board(distance: u8) -> State {
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    let (mut state, ours, theirs) = power_board(CommanderKind::Grit);
    let board = |position| Location::Board { position };
    state.units.push(board_unit(
        1,
        UnitKind::Artillery,
        ours,
        board(Pos::new(4, 2)),
    ));
    state
        .units
        .push(board_unit(2, UnitKind::Tank, ours, board(Pos::new(1, 1))));
    state.units.push(board_unit(
        11,
        UnitKind::Tank,
        theirs,
        board(Pos::new(4, 2 + distance)),
    ));
    state.units.push(board_unit(
        12,
        UnitKind::Infantry,
        theirs,
        board(Pos::new(12, 12)),
    ));
    state.next_unit_id = Some(20);
    state
}

fn andy_board(hp: u8) -> State {
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    let (mut state, ours, theirs) = power_board(CommanderKind::Andy);
    let board = |position| Location::Board { position };
    for x in 0..3u8 {
        let mut unit = board_unit(
            1 + u32::from(x),
            UnitKind::Tank,
            ours,
            board(Pos::new(3 + 2 * x, 3)),
        );
        unit.hp = hp;
        state.units.push(unit);
        state.units.push(board_unit(
            11 + u32::from(x),
            UnitKind::Tank,
            theirs,
            board(Pos::new(3 + 2 * x, 4)),
        ));
    }
    state.units.push(board_unit(
        5,
        UnitKind::Infantry,
        ours,
        board(Pos::new(1, 10)),
    ));
    state.next_unit_id = Some(20);
    state
}

/// Eagle's super power lets the units that acted act again. planner-v8
/// keeps it last, so it goes after the attacks, and the placement does not
/// move it.
#[test]
fn planner_v8_keeps_the_eagle_super_power_after_the_attacks() {
    let (placed, chosen) = placement_probe(eagle_board(), &PlannerConfig::V8);
    println!("eagle: {placed:#?}\nchosen {:?}", power_position(&chosen));
    assert_eq!(placed.len(), 1, "the placement adds no position");
    assert!(placed[0].attacks.0 >= 4, "{placed:?}");
    assert!(power_position(&chosen).0 >= 4);
}

/// The Grit super power goes first: when the enemy is in range only with
/// the power, and also when the enemy is already in range and the position
/// does not change the score.
#[test]
fn the_first_placement_puts_the_grit_super_power_first() {
    for distance in [3, 5] {
        let (placed, chosen) = placement_probe(grit_board(distance), &PlannerConfig::V8);
        println!(
            "grit distance {distance}: {placed:#?}\nchosen {:?}",
            power_position(&chosen)
        );
        assert_eq!(power_position(&chosen).0, 0, "distance {distance}");
    }
}

/// Andy's super power goes after the attacks of tanks at full health only
/// when that is better than the power first by the margin, and first with
/// tanks at half health.
#[test]
fn the_first_placement_moves_the_andy_super_power_only_past_the_margin() {
    for hp in [100, 50] {
        let (placed, chosen) = placement_probe(andy_board(hp), &PlannerConfig::V8);
        println!(
            "andy hp {hp}: {placed:#?}\nchosen {:?}",
            power_position(&chosen)
        );
        let first = placed
            .iter()
            .find(|placed| placed.orders_before == 0)
            .unwrap();
        let last = placed
            .iter()
            .find(|placed| placed.attacks == (3, 0))
            .unwrap();
        let expected = if last.score - first.score > POWER_LATER_MARGIN {
            3
        } else {
            0
        };
        if hp == 50 {
            assert_eq!(expected, 0);
        }
        assert_eq!(power_position(&chosen).0, expected, "hp {hp}");
    }
}

/// A board for the march tests. Our seat moves. Both seats have a
/// headquarters: ours at (0, 0) and the enemy one at (9, 2). An enemy tank
/// stands far away at (0, 9). The commanders are `ours_co` and `theirs_co`,
/// and both meters are empty.
fn march_board(
    ours_co: awvm::ruleset::CommanderKind,
    theirs_co: awvm::ruleset::CommanderKind,
) -> (State, PlayerIdx, PlayerIdx) {
    use awvm::ruleset::{Terrain, UnitKind};
    use awvm::semantic::{Pos, TileOwner};
    let (mut state, ours, theirs) = empty_board(5);
    assert!(state.board.dimensions().width() >= 10 && state.board.dimensions().height() >= 10);
    set_property(
        &mut state,
        Pos::new(0, 0),
        Terrain::Hq,
        TileOwner::Owned(ours),
    );
    set_property(
        &mut state,
        Pos::new(9, 2),
        Terrain::Hq,
        TileOwner::Owned(theirs),
    );
    state.units.push(board_unit(
        1,
        UnitKind::Tank,
        theirs,
        Location::Board {
            position: Pos::new(0, 9),
        },
    ));
    state.next_unit_id = Some(2);
    for (seat, commander) in [(ours, ours_co), (theirs, theirs_co)] {
        for co in state.player_mut(seat).commanders.iter_mut() {
            co.id = commander;
            co.power_charge = 0;
        }
    }
    (state, ours, theirs)
}

/// Add a unit of `kind` for `owner` at `position`.
fn add_unit(
    state: &mut State,
    kind: awvm::ruleset::UnitKind,
    owner: PlayerIdx,
    position: awvm::semantic::Pos,
) -> UnitId {
    let id = state.next_unit_id.unwrap_or(1);
    state
        .units
        .push(board_unit(id, kind, owner, Location::Board { position }));
    state.next_unit_id = Some(id + 1);
    UnitId::new(id)
}

/// Fill the meter of `seat` to the cost of its power `level`.
fn charge_power(state: &mut State, seat: PlayerIdx, level: awvm::commander::PowerLevel) {
    let player = state.player_mut(seat);
    for co in player.commanders.iter_mut() {
        co.power_charge = awvm::commander::power_activation_cost(co.id, level, co.power_uses)
            .unwrap()
            .unwrap();
    }
}

/// Play the turn of the active seat with `agent`, and return the plays.
fn play_turn(agent: &mut PlannerAgent, session: &mut Session) -> Vec<Play> {
    let side = session.state().turn.active_player.clone();
    let mut plays = Vec::new();
    for _ in 0..64 {
        if session.state().turn.active_player != side
            || !matches!(session.state().match_state, Match::Active { .. })
        {
            break;
        }
        let view = awvm::semantic::observe(&awvm::semantic::AwbwVisibility, session.state(), &side)
            .unwrap();
        let command = match agent.act(&view, NodeBudget::new(256).unwrap()) {
            Some(play) => {
                plays.push(play);
                play.command(session).expect("the play is legal")
            }
            None => Command::EndTurn {
                player: side.clone(),
            },
        };
        session
            .apply_command(command, &mut MeanLuck, &mut ())
            .expect("the command plays");
    }
    plays
}

/// Whether `seat` won the match in `state`.
fn won(state: &State, seat: PlayerIdx) -> bool {
    Evaluator::terminal_value(state, seat).is_some_and(|value| value > 0.0)
}

/// With the meter for Victory March, a Sami infantry five tiles from the
/// enemy headquarters wins at once: the check uses the power and captures.
/// Without the meter, or with a commander whose power does not capture at
/// once, there is no win.
#[test]
fn the_win_now_check_finds_a_super_power_capture() {
    use awvm::commander::PowerLevel;
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    let (mut state, ours, _) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(4, 2));
    let mut session = Session::new(state.clone());
    assert_eq!(march::win_now(&mut session, ours), None);
    assert_eq!(session.state(), &state);

    charge_power(&mut state, ours, PowerLevel::Scop);
    let mut session = Session::new(state.clone());
    let plays = march::win_now(&mut session, ours).expect("Victory March wins");
    assert_eq!(session.state(), &state);
    assert_eq!(plays.len(), 2);
    assert_eq!(plays[0].kind(), OrderKind::Power(PowerLevel::Scop));
    assert_eq!(plays[1].kind(), OrderKind::Capture);

    // Six tiles is out of reach.
    let (mut far, ours, _) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    add_unit(&mut far, UnitKind::Infantry, ours, Pos::new(3, 2));
    charge_power(&mut far, ours, PowerLevel::Scop);
    assert_eq!(march::win_now(&mut Session::new(far), ours), None);

    let (mut andy, ours, _) = march_board(CommanderKind::Andy, CommanderKind::Andy);
    add_unit(&mut andy, UnitKind::Infantry, ours, Pos::new(8, 2));
    charge_power(&mut andy, ours, PowerLevel::Scop);
    assert_eq!(march::win_now(&mut Session::new(andy), ours), None);
}

/// The check also finds a normal last capture of the headquarters, with no
/// power.
#[test]
fn the_win_now_check_finds_a_last_capture() {
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    let (mut state, ours, _) = march_board(CommanderKind::Andy, CommanderKind::Andy);
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(7, 2));
    state.board.tile_mut(Pos::new(9, 2)).capture_points = Some(10);
    let plays = march::win_now(&mut Session::new(state), ours).expect("the capture wins");
    assert_eq!(plays.len(), 1);
    assert_eq!(plays[0].kind(), OrderKind::Capture);
}

/// A march board where our Sami can take the enemy headquarters with
/// Victory March in this turn. Twenty neutral cities far away make the
/// properties of the power a small share of the map, so the swing rule does
/// not use the power.
fn win_board() -> (State, PlayerIdx) {
    use awvm::commander::PowerLevel;
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::{Pos, TileOwner};
    let (mut state, ours, _) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(4, 2));
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(3, 4));
    charge_power(&mut state, ours, PowerLevel::Scop);
    let dimensions = state.board.dimensions();
    let (width, height) = (dimensions.width(), dimensions.height());
    for row in [height - 1, height - 2] {
        add_cities(
            &mut state,
            TileOwner::Neutral,
            Pos::new(width - 10, row),
            10,
        );
    }
    (state, ours)
}

/// The march planner plays the winning Victory March. The planner with no
/// march plan also finds it: its power plan uses the power first, and the
/// Hard policy then captures. planner-v8 finds it in its seed plan,
/// because its seed plan uses the power first.
#[test]
fn the_march_planner_wins_with_victory_march() {
    let (state, ours) = win_board();
    let mut agent = PlannerAgent::with_config(7, MARCH_ONLY);
    let mut session = Session::new(state.clone());
    play_turn(&mut agent, &mut session);
    assert!(won(session.state(), ours));
    assert_eq!(agent.stats().chose_march, 1);

    let mut base = PlannerAgent::with_config(
        7,
        PlannerConfig {
            march: false,
            ..MARCH_ONLY
        },
    );
    let mut session = Session::new(state.clone());
    play_turn(&mut base, &mut session);
    assert!(won(session.state(), ours));

    let mut v8 = PlannerAgent::with_config(7, PlannerConfig::V8);
    let mut session = Session::new(state);
    play_turn(&mut v8, &mut session);
    assert!(won(session.state(), ours));
}

/// The meter log measures the charge in one own turn over the last four own
/// turns, starts again after a power, and counts the turns in a row with a
/// held decisive power.
#[test]
fn the_meter_log_measures_the_charge_rate() {
    use awvm::ruleset::CommanderKind;
    let (mut state, ours, _) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    let set = |state: &mut State, charge: u64, uses: u64| {
        for co in state.player_mut(ours).commanders.iter_mut() {
            co.power_charge = charge;
            co.power_uses = uses;
        }
    };
    let mut log = MeterLog::default();
    // With one entry and no power yet, the meter has charged since day 1.
    set(&mut state, 10_000, 0);
    log.start_turn(&state, ours, 3);
    assert_eq!(log.rate, Some(5_000.0));
    // A second call in the same turn changes nothing.
    set(&mut state, 99_999, 0);
    log.start_turn(&state, ours, 3);
    assert_eq!(log.charges, vec![10_000]);
    // The rate reads the last four entries.
    for (day, charge) in [(4, 13_000), (5, 16_000), (6, 19_000), (7, 25_000)] {
        set(&mut state, charge, 0);
        log.start_turn(&state, ours, day);
    }
    assert_eq!(log.charges, vec![13_000, 16_000, 19_000, 25_000]);
    assert_eq!(log.rate, Some(4_000.0));
    // A power starts a new series. The rate stays until it has two entries.
    set(&mut state, 2_000, 1);
    log.start_turn(&state, ours, 8);
    assert_eq!(log.charges, vec![2_000]);
    assert_eq!(log.rate, Some(4_000.0));
    // A turn with a held decisive power counts in the next turn.
    log.holding = true;
    log.start_turn(&state, ours, 9);
    assert_eq!(log.held, 1);
    assert!(!log.holding);
    log.start_turn(&state, ours, 10);
    assert_eq!(log.held, 0);
}

/// With one node, the seed plan uses it, and the planner does not try the
/// winning capture of the march plan.
#[test]
fn the_march_line_needs_a_node_after_the_seed() {
    let (state, ours) = win_board();
    let config = MARCH_ONLY;
    let mut evaluator = Evaluator::new(config.eval_weights);
    let mut replay = config.replay_score.map(ReplayReader::new);
    let mut planner = probe_planner(&config, &mut evaluator, replay.as_mut(), ours);
    planner.nodes_left = 1;
    let line = planner.plan(&Session::new(state)).unwrap();
    assert_eq!(line.generator, Generator::Seed);
    assert_eq!(planner.nodes_left, 0);
}

/// When the meter is two turns from Victory March, the plan holds the
/// normal power and steers an infantry toward the enemy headquarters. Andy
/// has no power that captures at once, so he gets no plan.
#[test]
fn the_march_plan_holds_the_normal_power_and_steers() {
    use awvm::commander::{PowerLevel, power_activation_cost};
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    let (mut state, ours, theirs) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    let runner = add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(0, 2));
    // The army ratio must not be too low.
    add_unit(&mut state, UnitKind::Tank, ours, Pos::new(0, 6));
    let cost = power_activation_cost(CommanderKind::Sami, PowerLevel::Scop, 0)
        .unwrap()
        .unwrap();
    for co in state.player_mut(ours).commanders.iter_mut() {
        co.power_charge = cost - 2 * 9_000;
    }
    let session = Session::new(state.clone());
    // No rate: no plan.
    assert_eq!(march::prepare(&session, ours, None, 0), None);
    let plan = march::prepare(&session, ours, Some(9_000.0), 0).expect("a plan");
    assert!(plan.hold.cop);
    assert!(!plan.hold.scop);
    assert!(!plan.holding);
    assert_eq!(plan.steer.len(), 1);
    assert_eq!(plan.steer[0].unit(), Some(runner));
    let to = state
        .board
        .dimensions()
        .position_of(plan.steer[0].destination())
        .unwrap();
    assert_eq!(to.distance(Pos::new(9, 2)), 6);

    // The meter can pay: the plan also holds the super power.
    let mut ready = state.clone();
    charge_power(&mut ready, ours, PowerLevel::Scop);
    let plan = march::prepare(&Session::new(ready.clone()), ours, Some(9_000.0), 0).unwrap();
    assert!(plan.hold.scop && plan.holding);
    // After the most held turns, the plan stops.
    assert_eq!(
        march::prepare(&Session::new(ready), ours, Some(9_000.0), 3),
        None
    );

    // An enemy unit on the headquarters stops the plan.
    let mut garrison = state.clone();
    add_unit(&mut garrison, UnitKind::Infantry, theirs, Pos::new(9, 2));
    assert_eq!(
        march::prepare(&Session::new(garrison), ours, Some(9_000.0), 0),
        None
    );

    let (mut andy, ours, _) = march_board(CommanderKind::Andy, CommanderKind::Andy);
    add_unit(&mut andy, UnitKind::Infantry, ours, Pos::new(0, 2));
    add_unit(&mut andy, UnitKind::Tank, ours, Pos::new(0, 6));
    assert_eq!(
        march::prepare(&Session::new(andy), ours, Some(9_000.0), 0),
        None
    );
}

/// At the start of the enemy turn, an enemy Sami with the meter for Victory
/// March and an infantry five tiles from our headquarters can take it. The
/// estimate with the power reach sees this. With Andy, the estimate does not
/// change.
#[test]
fn the_reply_estimate_reads_an_enemy_victory_march() {
    use awvm::commander::PowerLevel;
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    for (commander, lost) in [(CommanderKind::Sami, true), (CommanderKind::Andy, false)] {
        let (mut state, ours, theirs) = march_board(CommanderKind::Andy, commander);
        add_unit(&mut state, UnitKind::Infantry, theirs, Pos::new(5, 0));
        charge_power(&mut state, theirs, PowerLevel::Scop);
        state.turn.active_player = state.player(theirs).id().clone();
        let session = Session::new(state.clone());
        let values = PropertyValues::STANDARD;
        let with = reply::estimate(
            &session,
            ours,
            values,
            reply::ReplyOptions {
                commander_effects: true,
                march: true,
            },
        );
        let without = reply::estimate(
            &session,
            ours,
            values,
            reply::ReplyOptions {
                commander_effects: true,
                march: false,
            },
        );
        assert!(!without.headquarters_lost, "{commander:?}");
        assert_eq!(with.headquarters_lost, lost, "{commander:?}");
        if lost {
            let hq = state.board.dimensions().cell_index(Pos::new(0, 0)).unwrap();
            assert_eq!(with.threatened.first(), Some(&hq));
        } else {
            assert_eq!(with, without);
        }
    }
}

/// Our turn: the enemy Sami can pay for Victory March, and an enemy
/// infantry is in reach of our headquarters. The march planner ends the turn
/// with no enemy capture of the headquarters left.
#[test]
fn the_march_planner_answers_an_enemy_victory_march() {
    use awvm::commander::PowerLevel;
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    let (mut state, ours, theirs) = march_board(CommanderKind::Andy, CommanderKind::Sami);
    add_unit(&mut state, UnitKind::Infantry, theirs, Pos::new(4, 1));
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(1, 2));
    charge_power(&mut state, theirs, PowerLevel::Scop);
    let threat_after = |config: PlannerConfig| {
        let mut agent = PlannerAgent::with_config(7, config);
        let mut session = Session::new(state.clone());
        play_turn(&mut agent, &mut session);
        assert_ne!(
            session.state().turn.active_player,
            state.turn.active_player,
            "the turn ends"
        );
        !march::powered_headquarters_captures(session.state(), ours).is_empty()
    };
    assert!(!threat_after(PlannerConfig::V8));
    // Without the march switch, the simulated enemy reply does not use the
    // power, so the planner attacks the infantry and leaves the capture.
    assert!(threat_after(WITHOUT_MARCH));
}

/// Put `count` cities of `owner` in a row from `start`, to the right.
fn add_cities(
    state: &mut State,
    owner: awvm::semantic::TileOwner,
    start: awvm::semantic::Pos,
    count: u8,
) {
    for step in 0..count {
        set_property(
            state,
            awvm::semantic::Pos::new(start.x + step, start.y),
            awvm::ruleset::Terrain::City,
            owner,
        );
    }
}

/// With Victory March, each Sami infantry takes one property. Two infantry
/// near one city take it once. Without the power, a whole enemy city does
/// not fall in one turn.
#[test]
fn the_swing_counts_distinct_captures_with_the_power() {
    use awvm::commander::PowerLevel;
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::{Pos, TileOwner};
    let (mut state, ours, theirs) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    // Enemy cities at (10..13, 8), a neutral city at (2, 12).
    add_cities(&mut state, TileOwner::Owned(theirs), Pos::new(10, 8), 3);
    add_cities(&mut state, TileOwner::Neutral, Pos::new(2, 12), 1);
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(10, 4));
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(11, 4));
    // This one can reach only the neutral city.
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(2, 9));
    let none = march::swing(&state, ours, None);
    assert_eq!(none, march::Swing::default());
    let with = march::swing(&state, ours, Some(PowerLevel::Scop));
    assert_eq!(
        with,
        march::Swing {
            enemy: 2,
            neutral: 1
        }
    );
    assert_eq!(with.value(), 2.5);
    // Andy takes nothing with his super power.
    let (mut andy, ours, theirs) = march_board(CommanderKind::Andy, CommanderKind::Andy);
    add_cities(&mut andy, TileOwner::Owned(theirs), Pos::new(10, 8), 3);
    add_unit(&mut andy, UnitKind::Infantry, ours, Pos::new(10, 6));
    assert_eq!(
        march::swing(&andy, ours, Some(PowerLevel::Scop)),
        march::Swing::default()
    );
}

/// A board for the swing rule: Sami with the meter for Victory March, three
/// infantry in reach of three enemy cities, and `filler` more enemy cities
/// far away. The swing gain is 3 of 2 + 3 + `filler` properties.
fn swing_board(filler: u8) -> (State, PlayerIdx) {
    use awvm::commander::PowerLevel;
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::{Pos, TileOwner};
    let (mut state, ours, theirs) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    add_cities(&mut state, TileOwner::Owned(theirs), Pos::new(14, 8), 3);
    // The filler cities fill rows 11 to 13 from x = 1.
    assert!(state.board.dimensions().height() >= 14);
    for index in 0..filler {
        let position = Pos::new(1 + index % 19, 11 + index / 19);
        set_property(
            &mut state,
            position,
            awvm::ruleset::Terrain::City,
            TileOwner::Owned(theirs),
        );
    }
    for x in 14..17 {
        add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(x, 5));
    }
    add_unit(&mut state, UnitKind::Tank, ours, Pos::new(0, 6));
    charge_power(&mut state, ours, PowerLevel::Scop);
    (state, ours)
}

/// The rule fires the super power for a swing of at least 6% of the
/// properties, and not below. It holds the normal power when the meter pays
/// soon. A near headquarters plan waits unless the swing is 12% or more.
#[test]
fn the_swing_rule_follows_the_threshold_and_the_march_plan() {
    use awvm::commander::PowerLevel;
    use awvm::semantic::Pos;
    // 3 of 50 properties is 6%: fire.
    let (state, ours) = swing_board(45);
    let share = march::swing_share(&state, ours, PowerLevel::Scop);
    assert!((share - 0.06).abs() < 1e-9, "{share}");
    let session = Session::new(state.clone());
    match march::swing_turn(&session, ours, None, None) {
        march::SwingTurn::Fire(play, captures) => {
            assert_eq!(play.kind(), OrderKind::Power(PowerLevel::Scop));
            assert_eq!(captures.len(), 3);
            assert!(
                captures
                    .iter()
                    .all(|capture| capture.kind() == OrderKind::Capture)
            );
        }
        other => panic!("{other:?}"),
    }
    // 3 of 51: no swing.
    let (state, ours) = swing_board(46);
    assert_eq!(
        march::swing_turn(&Session::new(state), ours, None, None),
        march::SwingTurn::None
    );
    // Two turns from the meter: hold the normal power only.
    let (mut state, ours) = swing_board(10);
    for co in state.player_mut(ours).commanders.iter_mut() {
        co.power_charge -= 18_000;
    }
    match march::swing_turn(&Session::new(state.clone()), ours, Some(9_000.0), None) {
        march::SwingTurn::Hold(hold) => assert!(hold.cop && !hold.scop),
        other => panic!("{other:?}"),
    }
    // No rate: no hold.
    assert_eq!(
        march::swing_turn(&Session::new(state), ours, None, None),
        march::SwingTurn::None
    );
    // A headquarters plan with a capturer one turn from staging.
    let near = march::March {
        hold: crate::power::PowerHold {
            seat: ours,
            cop: true,
            scop: true,
        },
        steer: Vec::new(),
        holding: true,
        runner_turns: 1,
    };
    // 3 of 30 is 10%: wait for the headquarters.
    let (state, ours) = swing_board(25);
    let session = Session::new(state);
    assert_eq!(
        march::swing_turn(&session, ours, None, Some(&near)),
        march::SwingTurn::None
    );
    // 3 of 25 is 12%: fire.
    let (state, ours) = swing_board(20);
    let session = Session::new(state);
    assert!(matches!(
        march::swing_turn(&session, ours, None, Some(&near)),
        march::SwingTurn::Fire(..)
    ));
    // A far headquarters plan does not stop a swing of 10%.
    let far = march::March {
        runner_turns: 3,
        ..near
    };
    let (state, ours) = swing_board(25);
    assert!(matches!(
        march::swing_turn(&Session::new(state), ours, None, Some(&far)),
        march::SwingTurn::Fire(..)
    ));
    let _ = Pos::new(0, 0);
}

/// The march planner uses the power on the swing turn and takes the
/// properties, one capturer each.
#[test]
fn the_march_planner_takes_the_swing() {
    use awvm::semantic::{Pos, TileOwner};
    let (state, ours) = swing_board(10);
    let mut agent = PlannerAgent::with_config(7, PlannerConfig::V8);
    let mut session = Session::new(state);
    let plays = play_turn(&mut agent, &mut session);
    assert!(matches!(
        plays.first().map(Play::kind),
        Some(OrderKind::Power(_))
    ));
    let owned = (14..17)
        .filter(|x| session.state().board.tile(Pos::new(*x, 8)).owner == TileOwner::Owned(ours))
        .count();
    assert_eq!(owned, 3);
    assert_eq!(agent.stats().swing_fired, 1);
    assert_eq!(agent.stats().swing_expected, 3);
}

/// At the start of the enemy turn, an enemy Sami with the meter for Victory
/// March and three infantry in reach of three of our cities can take them.
/// The estimate with the march plan counts their loss. With Andy, it does
/// not change.
#[test]
fn the_reply_estimate_counts_an_enemy_swing() {
    use awvm::commander::PowerLevel;
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::{Pos, TileOwner};
    for (commander, swing) in [(CommanderKind::Sami, true), (CommanderKind::Andy, false)] {
        let (mut state, ours, theirs) = march_board(CommanderKind::Andy, commander);
        add_cities(&mut state, TileOwner::Owned(ours), Pos::new(10, 8), 3);
        for x in 10..13 {
            add_unit(&mut state, UnitKind::Infantry, theirs, Pos::new(x, 4));
        }
        charge_power(&mut state, theirs, PowerLevel::Scop);
        state.turn.active_player = state.player(theirs).id().clone();
        let session = Session::new(state.clone());
        let values = PropertyValues::STANDARD;
        let with = reply::estimate(
            &session,
            ours,
            values,
            reply::ReplyOptions {
                commander_effects: true,
                march: true,
            },
        );
        let without = reply::estimate(
            &session,
            ours,
            values,
            reply::ReplyOptions {
                commander_effects: true,
                march: false,
            },
        );
        assert!(!with.headquarters_lost);
        if swing {
            assert!(
                with.property_loss >= without.property_loss + 3.0 * values.income * 0.99,
                "{with:?} {without:?}"
            );
            assert_eq!(with.threatened.len(), 3);
        } else {
            assert_eq!(with, without);
        }
    }
}

/// The swing count stays small and fast on a crowded board: 60 Sami
/// infantry and about 100 neutral cities. Each capturer takes at most one
/// property, and each property falls once.
#[test]
fn the_swing_count_is_bounded_on_a_crowded_board() {
    use awvm::commander::PowerLevel;
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::{Pos, TileOwner};
    let (mut state, ours, _) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    let width = state.board.dimensions().width();
    let height = state.board.dimensions().height();
    let mut infantry = 0;
    for y in 3..height {
        for x in 0..width {
            let position = Pos::new(x, y);
            if (x + y) % 3 == 0 {
                set_property(
                    &mut state,
                    position,
                    awvm::ruleset::Terrain::City,
                    TileOwner::Neutral,
                );
            } else if (x + 2 * y) % 5 == 0 && infantry < 60 && position != Pos::new(0, 9) {
                add_unit(&mut state, UnitKind::Infantry, ours, position);
                infantry += 1;
            }
        }
    }
    let start = std::time::Instant::now();
    let with = march::swing(&state, ours, Some(PowerLevel::Scop));
    let elapsed = start.elapsed();
    let count = with.enemy + with.neutral;
    assert!(count > 0);
    assert!(count <= infantry);
    assert!(count <= march::properties(&state));
    assert!(elapsed < std::time::Duration::from_secs(2), "{elapsed:?}");
}

/// The positions of the commander powers in `plays`.
fn power_plays(plays: &[Play]) -> Vec<(usize, awvm::commander::PowerLevel)> {
    plays
        .iter()
        .enumerate()
        .filter_map(|(index, play)| match play.kind() {
            OrderKind::Power(level) => Some((index, level)),
            _ => None,
        })
        .collect()
}

/// Sami has a live headquarters plan, and the meter pays for the normal
/// power. The march plan holds the normal power. With the power placement,
/// the seed plan does not use it, the placement makes no plan for it, and
/// the chosen line does not use it. Without the hold, the placement makes
/// plans for it.
#[test]
fn the_placement_does_not_use_a_power_that_the_march_plan_holds() {
    use awvm::commander::{PowerLevel, power_activation_cost};
    use awvm::ruleset::{CommanderKind, UnitKind};
    use awvm::semantic::Pos;
    let (mut state, ours, _) = march_board(CommanderKind::Sami, CommanderKind::Andy);
    add_unit(&mut state, UnitKind::Infantry, ours, Pos::new(0, 2));
    add_unit(&mut state, UnitKind::Tank, ours, Pos::new(0, 6));
    let cost = power_activation_cost(CommanderKind::Sami, PowerLevel::Scop, 0)
        .unwrap()
        .unwrap();
    let cop = power_activation_cost(CommanderKind::Sami, PowerLevel::Cop, 0)
        .unwrap()
        .unwrap();
    for co in state.player_mut(ours).commanders.iter_mut() {
        co.power_charge = cost - 2 * 9_000;
    }
    assert!(
        cost - 2 * 9_000 >= cop,
        "the meter pays for the normal power"
    );

    let config = PlannerConfig::V8;
    let mut evaluator = Evaluator::new(config.eval_weights);
    let mut replay = config.replay_score.map(ReplayReader::new);
    let mut planner = probe_planner(&config, &mut evaluator, replay.as_mut(), ours);
    planner.march.rate = Some(9_000.0);
    let mut session = Session::new(state.clone());
    let chosen = planner.plan(&session).unwrap();
    assert_eq!(session.state(), &state);
    assert!(planner.march.live);
    assert!(planner.march.hold.is_some_and(|hold| hold.cop));
    assert_eq!(power_plays(&chosen.plays), Vec::new(), "{:?}", chosen.plays);

    // The hold stays on the planner, so the seed plan and the placement see
    // it.
    let seed = planner.line(&mut session, Generator::Seed, &[]).unwrap();
    assert_eq!(power_plays(&seed.plays), Vec::new());
    let held = planner.placements(&session, &seed);
    assert!(
        held.iter()
            .all(|(prefix, _, _)| power_plays(prefix).is_empty()),
        "{held:?}"
    );

    // Without the hold, the placement tries the normal power.
    planner.march.hold = None;
    let free = planner.placements(&session, &seed);
    assert!(
        free.iter().any(|(prefix, _, _)| power_plays(prefix)
            .iter()
            .any(|(_, level)| *level == PowerLevel::Cop)),
        "{free:?}"
    );
}

/// On a swing turn, the planner with the power placement uses the super
/// power once, before all other orders, and then takes the properties, one
/// capturer each. The placement makes no plan that moves the power.
#[test]
fn the_placement_keeps_the_swing_power_first_and_once() {
    use awvm::commander::PowerLevel;
    use awvm::semantic::{Pos, TileOwner};
    let config = PlannerConfig::V8;
    let (state, ours) = swing_board(10);

    let mut evaluator = Evaluator::new(config.eval_weights);
    let mut replay = config.replay_score.map(ReplayReader::new);
    let mut planner = probe_planner(&config, &mut evaluator, replay.as_mut(), ours);
    let session = Session::new(state.clone());
    let line = planner.plan(&session).unwrap();
    assert!(planner.march.swing_fired);
    assert_eq!(power_plays(&line.plays), vec![(0, PowerLevel::Scop)]);
    assert!(
        line.plays[1..4]
            .iter()
            .all(|play| play.kind() == OrderKind::Capture),
        "{:?}",
        line.plays
    );
    // The placement makes no plan on the swing turn. Without the swing, it
    // makes plans that put the power after the captures.
    assert!(planner.placements(&session, &line).is_empty());
    planner.march.swing_fired = false;
    let moved = planner.placements(&session, &line);
    assert!(
        moved.iter().any(|(prefix, _, _)| {
            let capture = prefix
                .iter()
                .position(|play| play.kind() == OrderKind::Capture);
            let power = prefix
                .iter()
                .position(|play| matches!(play.kind(), OrderKind::Power(_)));
            matches!((capture, power), (Some(capture), Some(power)) if capture < power)
        }),
        "{moved:?}"
    );

    let mut agent = PlannerAgent::with_config(7, config);
    let mut session = Session::new(state);
    let plays = play_turn(&mut agent, &mut session);
    assert_eq!(power_plays(&plays), vec![(0, PowerLevel::Scop)]);
    let owned = (14..17)
        .filter(|x| session.state().board.tile(Pos::new(*x, 8)).owner == TileOwner::Owned(ours))
        .count();
    assert_eq!(owned, 3);
    assert_eq!(agent.stats().swing_fired, 1);
    assert_eq!(agent.stats().swing_expected, 3);
}
