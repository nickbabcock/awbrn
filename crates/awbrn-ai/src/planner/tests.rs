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
