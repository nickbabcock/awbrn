//! Measure evaluator predictions on recorded human games.
//!
//! Sample completed turns from day 3. Use complete two-player standard games
//! with a recorded winner. Exclude timeouts, draws, and incomplete playback.
//! Keep every position from a game in the same validation fold.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use awbrn_ai::eval::{EvalTerms, EvalWeights, Evaluator};
use awbrn_ai::planner::PlannerConfig;
use awbrn_ai_diagnostic_types::{PairKey, SeatOrderVariant, fingerprint_bytes};
use awbrn_ai_diagnostics::feature_analysis::{
    FEATURE_ANALYSIS_SCHEMA_VERSION, FeatureAnalysisReport, FeatureExtraction, FeatureMode,
    FeatureRow, ReducedEvaluator, fit_feature_analysis, observable_features, read_feature_rows,
    write_feature_rows,
};
use awbrn_map::AwbwMapData;
use awbw_replay::{ReplayParser, turn_models::Action};
use awvm::semantic::{AwbwVisibility, Match, Outcome, VictoryReason, observe};
use awvm::session::Session;
use awvm_awbw::RecordedAdapter;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct Position {
    game: u64,
    map: u32,
    mode: FeatureMode,
    day: u64,
    turn: u32,
    seat: usize,
    winner: bool,
    terms: EvalTerms,
    unit_count_delta: f64,
    #[serde(default)]
    extra: BTreeMap<String, f64>,
}

#[derive(Deserialize, Serialize)]
struct Coverage {
    archive: PathBuf,
    archive_fingerprint: String,
    map_fingerprint: String,
    parsed: bool,
    actions: usize,
    rows: usize,
    outcome: String,
    excluded: Option<String>,
}

type Extracted = (Coverage, Vec<Position>, Vec<FeatureRow>);

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["extract", division, maps, output] => {
            extract(Path::new(division), Path::new(maps), Path::new(output))
        }
        ["benchmark", output] => benchmark(Path::new(output)),
        ["fit", output] => fit(Path::new(output)),
        ["validate", training, holdout] => validate(Path::new(training), Path::new(holdout)),
        ["export-planner", training, output] => {
            let bytes = fs::read(Path::new(training).join("feature-analysis.json"))?;
            let report = serde_json::from_slice(&bytes)?;
            let model = awbrn_ai_diagnostics::replay_planner::ReplayPlannerModel::from_report(
                &report, &bytes,
            )
            .map_err(anyhow::Error::msg)?;
            write_json(PathBuf::from(output), &model)
        }
        ["inspect", archive, maps, turn, output] => inspect(
            Path::new(archive),
            Path::new(maps),
            turn.parse()?,
            Path::new(output),
        ),
        ["audit", training, holdout, output] => {
            error_audit(Path::new(training), Path::new(holdout), Path::new(output))
        }
        ["puzzles", model, output] => planner_puzzles(Path::new(model), Path::new(output)),
        ["puzzle-ablations", model, output] => {
            puzzle_ablations(Path::new(model), Path::new(output))
        }
        ["power-probe", model, output] => power_probe(Path::new(model), Path::new(output)),
        _ => bail!(
            "usage: awbw_evaluator extract <division> <maps> <output> | benchmark <output> | fit <output> | validate <training> <holdout> | export-planner <training> <output> | inspect <archive> <maps> <turn> <output> | audit <training> <holdout> <output> | puzzles <model> <output> | puzzle-ablations <model> <output> | power-probe <model> <output>"
        ),
    }
}

fn power_probe(model: &Path, output: &Path) -> Result<()> {
    use awvm::semantic::UnitAction;
    let bytes = fs::read(model)?;
    let model: awbrn_ai_diagnostics::replay_planner::ReplayPlannerModel =
        serde_json::from_slice(&bytes)?;
    model.validate().map_err(anyhow::Error::msg)?;
    let mut state = awbrn_ai::puzzles::suite()
        .into_iter()
        .find(|p| p.name == "focus-fire")
        .context("no focus-fire fixture")?
        .state;
    let seat = state
        .players
        .seat(&state.turn.active_player)
        .context("missing player")?;
    let cost = awvm::commander::power_activation_cost(
        awvm::ruleset::CommanderKind::Eagle,
        awvm::commander::PowerLevel::Scop,
        0,
    )?
    .context("no Eagle super power")?;
    state.player_mut(seat).commanders[0] = awvm::semantic::Commander {
        id: awvm::ruleset::CommanderKind::Eagle,
        active: true,
        power_charge: cost,
        power_uses: 0,
    };
    for unit in state.units.iter_mut().filter(|unit| unit.owner == seat) {
        unit.action = UnitAction::Spent;
    }
    let mut ready = Session::new(state.clone());
    let mut uncharged = state;
    uncharged.player_mut(seat).commanders[0].power_charge = 0;
    let mut uncharged = Session::new(uncharged);
    let command = awvm::transition::Command::ActivatePower {
        player: ready.state().turn.active_player.clone(),
        level: awvm::commander::PowerLevel::Scop,
    };
    let count_attackers = |session: &Session| {
        let mut orders = Vec::new();
        session.legal().orders(&mut orders);
        orders
            .into_iter()
            .filter(|order| matches!(order.kind(), awvm::session::OrderKind::Attack(_)))
            .filter_map(|order| session.unit_of(order))
            .collect::<BTreeSet<_>>()
            .len()
    };
    let before = serde_json::json!({
        "current":Evaluator::new(PlannerConfig::V5.eval_weights).value_in(&ready,seat),
        "candidate":model.value_in(&ready,seat),"legal_attackers":count_attackers(&ready),
    });
    let zero_score = model.value_in(&uncharged, seat);
    let zero_current = Evaluator::new(PlannerConfig::V5.eval_weights).value_in(&uncharged, seat);
    let zero_legal = uncharged
        .apply_command(
            command.clone(),
            &mut awbrn_ai::rng::Rng::from_seed(11),
            &mut (),
        )
        .is_ok();
    let zero = serde_json::json!({
        "current":zero_current,
        "candidate":zero_score,"power_legal":zero_legal,
    });
    let mut cop = Session::new(ready.state().clone());
    let activation = cop.resolve(&awvm::transition::Command::ActivatePower {
        player: cop.state().turn.active_player.clone(),
        level: awvm::commander::PowerLevel::Cop,
    })?;
    cop.apply(activation, &mut awbrn_ai::rng::Rng::from_seed(11), &mut ())?;
    let cop_after = serde_json::json!({
        "legal_attackers":count_attackers(&cop),
        "ready_units":cop.state().units.iter().filter(|unit| unit.owner == seat && unit.action == UnitAction::Ready).count(),
    });
    let activation = ready.resolve(&command)?;
    ready.apply(activation, &mut awbrn_ai::rng::Rng::from_seed(11), &mut ())?;
    let after = serde_json::json!({
        "current":Evaluator::new(PlannerConfig::V5.eval_weights).value_in(&ready,seat),
        "candidate":model.value_in(&ready,seat),"legal_attackers":count_attackers(&ready),
        "ready_units":ready.state().units.iter().filter(|unit| unit.owner == seat && unit.action == UnitAction::Ready).count(),
    });
    write_json(
        output.to_path_buf(),
        &serde_json::json!({
            "model_fingerprint":fingerprint_bytes(&bytes),"fixture":"focus-fire with Eagle and spent units",
            "super_power_cost":cost,"uncharged":zero,"charged":before,"activated":after,
            "cop_activated":cop_after,
        }),
    )
}

fn planner_puzzles(model: &Path, output: &Path) -> Result<()> {
    let bytes = fs::read(model)?;
    write_json(output.to_path_buf(), &planner_puzzle_report(&bytes)?)
}

fn puzzle_ablations(model: &Path, output: &Path) -> Result<()> {
    let bytes = fs::read(model)?;
    let original: awbrn_ai_diagnostics::replay_planner::ReplayPlannerModel =
        serde_json::from_slice(&bytes)?;
    let mut results = Vec::new();
    for feature in [
        "front_position_delta",
        "unit_count_delta",
        "capture_progress_delta",
    ] {
        let mut model = original.clone();
        *model
            .coefficients
            .get_mut(feature)
            .context("missing coefficient")? = 0.0;
        let report = planner_puzzle_report(&serde_json::to_vec_pretty(&model)?)?;
        let failures = |key: &str| -> Result<Vec<serde_json::Value>> {
            Ok(report[key]
                .as_array()
                .context("missing puzzle outcomes")?
                .iter()
                .filter(|row| row["candidate"]["passed"] == false)
                .map(|row| row["name"].clone())
                .collect())
        };
        results.push(serde_json::json!({"zeroed_feature":feature,
            "model_fingerprint":report["model_fingerprint"],
            "node_budget_4_failures":failures("outcomes")?,
            "node_budget_32_failures":failures("outcomes_with_node_budget_32")?}));
    }
    write_json(
        output.to_path_buf(),
        &serde_json::json!({
            "source_model_fingerprint":fingerprint_bytes(&bytes),
            "method":"Set one frozen coefficient to zero. Keep other coefficients and planner settings fixed. This is a tactical audit, not a strength test.",
            "results":results,
        }),
    )
}

fn planner_puzzle_report(bytes: &[u8]) -> Result<serde_json::Value> {
    use awbrn_ai_diagnostics::tournament::AgentFactory;
    let candidate = awbrn_ai_diagnostics::replay_planner::ReplayPlannerFactory::new(
        "planner-v5-replay-cup3",
        serde_json::from_slice(bytes)?,
        fingerprint_bytes(bytes),
    )
    .map_err(anyhow::Error::msg)?;
    let mut outcomes = Vec::new();
    let mut outcomes_32 = Vec::new();
    let solve_32 = |puzzle: &awbrn_ai::puzzles::Puzzle,
                    agent: &mut dyn awbrn_ai::agent::Agent|
     -> Result<serde_json::Value> {
        agent.start_match();
        let result = awbrn_ai::harness::run_agent_turn_unmeasured(
            puzzle.state.clone(),
            agent,
            &mut awbrn_ai::rng::Rng::from_seed(11),
            awbrn_ai::agent::NodeBudget::THIRTY_TWO,
        )?;
        let check = (puzzle.check)(&awbrn_ai::puzzles::PuzzleTurn {
            start: &puzzle.state,
            result: &result,
        });
        Ok(
            serde_json::json!({"passed":check.is_ok(),"detail":check.err().unwrap_or_default(),"commands":result.commands.len()}),
        )
    };
    for puzzle in awbrn_ai::puzzles::suite() {
        let mut baseline = awbrn_ai::planner::PlannerAgent::with_config(7, PlannerConfig::V5);
        let mut candidate_agent = candidate.create(7);
        outcomes_32.push(serde_json::json!({"name":puzzle.name,
            "baseline":solve_32(&puzzle, &mut baseline)?,
            "candidate":solve_32(&puzzle, &mut *candidate_agent)?}));
        let baseline = awbrn_ai::puzzles::solve(&puzzle, &mut baseline, 11);
        let candidate = awbrn_ai::puzzles::solve(&puzzle, &mut *candidate_agent, 11);
        outcomes.push(serde_json::json!({"name":puzzle.name,
            "baseline":{"passed":baseline.passed,"detail":baseline.detail,"commands":baseline.commands},
            "candidate":{"passed":candidate.passed,"detail":candidate.detail,"commands":candidate.commands}}));
    }
    Ok(serde_json::json!({
        "model_fingerprint":fingerprint_bytes(bytes), "agent_seed":7,"entropy_seed":11,
        "node_budget":4,"outcomes":outcomes,"outcomes_with_node_budget_32":outcomes_32,
    }))
}

fn inspect(archive: &Path, maps: &Path, requested_turn: u32, output: &Path) -> Result<()> {
    let bytes = fs::read(archive)?;
    let replay = ReplayParser::new().with_debug(true).parse(&bytes)?;
    let game = replay.games.first().context("no game entry")?;
    let map_bytes = fs::read(maps.join(format!("{}.json", game.maps_id.as_u32())))?;
    let map = serde_json::from_slice(&map_bytes)?;
    let mut adapter = RecordedAdapter::new(&replay, &map)?;
    let mut turn = 0;
    let mut snapshot = None;
    let mut perspective = None;
    let mut suffix = Vec::new();
    for (index, action) in replay.turns.iter().enumerate() {
        if is_join_setup(action, replay.turns.get(index + 1)) {
            continue;
        }
        let player = adapter.state().turn.active_player.clone();
        adapter
            .advance(action)
            .with_context(|| format!("action {index}"))?;
        if snapshot.is_some() && !matches!(action, Action::Move(_) | Action::Build { .. }) {
            suffix.push(serde_json::json!({
                "index": index, "day": adapter.state().turn.day, "player": player,
                "action": action,
            }));
        }
        if matches!(action, Action::End { .. }) {
            turn += 1;
            if turn == requested_turn {
                snapshot = Some(adapter.state().clone());
                perspective = Some(player);
            }
        }
    }
    let state = snapshot.context("requested turn was not recorded")?;
    let player = perspective.context("no perspective")?;
    let visible = Session::from_observation(&observe(&AwbwVisibility, &state, &player)?)?;
    let seat = visible
        .state()
        .players
        .seat(&player)
        .context("missing seat")?;
    let current = Evaluator::new(PlannerConfig::V5.eval_weights).breakdown_in(&visible, seat);
    let mut without_charge = visible.state().clone();
    for (player_seat, _) in state.players.seats() {
        for commander in &mut without_charge.player_mut(player_seat).commanders {
            commander.power_charge = 0;
        }
    }
    let no_charge_score = Evaluator::new(PlannerConfig::V5.eval_weights)
        .value_in(&Session::new(without_charge), seat);
    let readiness = visible
        .state()
        .players
        .seats()
        .flat_map(|(_, player)| {
            player.commanders.iter().map(|commander| {
                let cop = awvm::commander::power_activation_cost(
                    commander.id,
                    awvm::commander::PowerLevel::Cop,
                    commander.power_uses,
                )
                .ok()
                .flatten();
                let scop = awvm::commander::power_activation_cost(
                    commander.id,
                    awvm::commander::PowerLevel::Scop,
                    commander.power_uses,
                )
                .ok()
                .flatten();
                serde_json::json!({"player": player.id(), "commander": commander.id,
                "charge": commander.power_charge, "cop_cost": cop, "scop_cost": scop,
                "cop_ready": cop.is_some_and(|cost| commander.power_charge >= cost),
                "scop_ready": scop.is_some_and(|cost| commander.power_charge >= cost)})
            })
        })
        .collect::<Vec<_>>();
    write_json(
        output.to_path_buf(),
        &serde_json::json!({
            "archive": archive, "archive_fingerprint": fingerprint_bytes(&bytes),
            "map_fingerprint": fingerprint_bytes(&map_bytes), "turn": requested_turn,
            "perspective": player, "state": state,
            "features": observable_features(visible.state(), seat, requested_turn),
            "current": current, "power_readiness": readiness,
            "current_score_without_charge": no_charge_score,
            "future_actions": suffix, "final_outcome": adapter.state().match_state,
        }),
    )
}

fn error_audit(training: &Path, holdout: &Path, output: &Path) -> Result<()> {
    let model_bytes = fs::read(training.join("feature-analysis.json"))?;
    let report: FeatureAnalysisReport = serde_json::from_slice(&model_bytes)?;
    let visible = report
        .modes
        .iter()
        .find(|mode| mode.mode == FeatureMode::FogVisible)
        .context("no AI-visible model")?;
    let model = ReducedEvaluator::from_report(visible);
    let rows = read_feature_rows(holdout.join("features.jsonl"))?;
    let predictions = rows
        .iter()
        .filter(|row| row.mode == FeatureMode::FogVisible)
        .map(|row| (row, model.probability(row.features)))
        .collect::<Vec<_>>();
    let summarize = |filtered: Vec<(&FeatureRow, f64)>| {
        let mut games = BTreeMap::<&str, Vec<(bool, f64)>>::new();
        for (row, probability) in &filtered {
            games
                .entry(&row.match_id)
                .or_default()
                .push((row.winner, *probability));
        }
        let count = games.len() as f64;
        let accuracy = games
            .values()
            .map(|rows| {
                rows.iter()
                    .filter(|(winner, p)| (*p >= 0.5) == *winner)
                    .count() as f64
                    / rows.len() as f64
            })
            .sum::<f64>()
            / count;
        let loss = games
            .values()
            .map(|rows| {
                rows.iter()
                    .map(|(winner, p)| -if *winner { *p } else { 1.0 - p }.clamp(1e-15, 1.0).ln())
                    .sum::<f64>()
                    / rows.len() as f64
            })
            .sum::<f64>()
            / count;
        serde_json::json!({"games": games.len(), "rows": filtered.len(),
            "equal_game_accuracy": accuracy.is_finite().then_some(accuracy),
            "equal_game_loss": loss.is_finite().then_some(loss)})
    };
    let mut days = BTreeMap::new();
    for (low, high) in [(3, 7), (8, 14), (15, 21), (22, 35), (36, u64::MAX)] {
        days.insert(
            format!("{low}-{high}"),
            summarize(
                predictions
                    .iter()
                    .copied()
                    .filter(|(row, _)| (low..=high).contains(&row.day))
                    .collect(),
            ),
        );
    }
    let mut confidence = BTreeMap::new();
    for (low, high) in [(0.5, 0.6), (0.6, 0.7), (0.7, 0.8), (0.8, 1.01)] {
        confidence.insert(
            format!("{low:.1}-{:.1}", f64::min(high, 1.0)),
            summarize(
                predictions
                    .iter()
                    .copied()
                    .filter(|(_, p)| (low..high).contains(&p.max(1.0 - p)))
                    .collect(),
            ),
        );
    }
    let mut worst = BTreeMap::<&str, (&FeatureRow, f64)>::new();
    let loss =
        |row: &FeatureRow, p: f64| -if row.winner { p } else { 1.0 - p }.clamp(1e-15, 1.0).ln();
    for (row, p) in predictions {
        if worst
            .get(row.match_id.as_str())
            .is_none_or(|(previous, pp)| loss(row, p) > loss(previous, *pp))
        {
            worst.insert(&row.match_id, (row, p));
        }
    }
    let mut worst = worst.values().copied().collect::<Vec<_>>();
    worst.sort_by(|(a, ap), (b, bp)| loss(b, *bp).total_cmp(&loss(a, *ap)));
    let worst = worst
        .iter()
        .take(12)
        .map(|(row, p)| {
            serde_json::json!({
                "match_id":row.match_id,"map_id":row.pair.map_id,"turn_index":row.turn_index,
                "day":row.day,"winner":row.winner,"p":p,"loss":loss(row,*p),"features":row.features,
            })
        })
        .collect::<Vec<_>>();
    write_json(
        output.to_path_buf(),
        &serde_json::json!({
            "model_report_fingerprint":fingerprint_bytes(&model_bytes),
            "holdout_features_fingerprint":fingerprint_bytes(&fs::read(holdout.join("features.jsonl"))?),
            "day_buckets":days,"confidence_buckets":confidence,"high_confidence_errors":worst,
            "weighting":"Equal game means within each bucket. Buckets have different game sets.",
        }),
    )
}

fn extract(division: &Path, maps: &Path, output: &Path) -> Result<()> {
    let mut paths = fs::read_dir(division)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| path.extension().is_some_and(|extension| extension == "zip"));
    paths.sort();
    if paths.is_empty() {
        bail!("the division contains no replay archives");
    }
    let jobs = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(8);
    let results = std::thread::scope(|scope| {
        let handles = paths
            .chunks(paths.len().div_ceil(jobs))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|path| extract_game(path, maps))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("replay worker completed"))
            .collect::<Vec<_>>()
    });
    let mut coverage = Vec::new();
    let mut positions = Vec::new();
    let mut features = Vec::new();
    for (game_coverage, game_positions, game_features) in results {
        coverage.push(game_coverage);
        positions.extend(game_positions);
        features.extend(game_features);
    }
    fs::create_dir_all(output)?;
    write_json(output.join("coverage.json"), &coverage)?;
    let mut writer = BufWriter::new(File::create(output.join("positions.jsonl"))?);
    for row in &positions {
        serde_json::to_writer(&mut writer, row)?;
        writeln!(writer)?;
    }
    writer.flush()?;
    write_feature_rows(output.join("features.jsonl"), &features)?;
    println!(
        "{} archives; {} parsed; {} labelled games; {} position rows",
        coverage.len(),
        coverage.iter().filter(|row| row.parsed).count(),
        coverage.iter().filter(|row| row.excluded.is_none()).count(),
        positions.len()
    );
    let mut exclusions = BTreeMap::new();
    for row in &coverage {
        if let Some(reason) = &row.excluded {
            *exclusions.entry(reason).or_insert(0) += 1;
        }
    }
    for (reason, count) in exclusions {
        println!("excluded {count}: {reason}");
    }
    benchmark(output)
}

fn extract_game(path: &Path, maps: &Path) -> Extracted {
    let mut coverage = Coverage {
        archive: path.to_owned(),
        archive_fingerprint: String::new(),
        map_fingerprint: String::new(),
        parsed: false,
        actions: 0,
        rows: 0,
        outcome: String::new(),
        excluded: None,
    };
    match game_rows(path, maps, &mut coverage) {
        Ok((positions, features)) => {
            coverage.rows = positions.len();
            if positions.is_empty() {
                coverage.excluded = Some("no nonterminal positions from day 3".into());
            }
            (coverage, positions, features)
        }
        Err(error) => {
            coverage.excluded = Some(format!("{error:#}"));
            (coverage, Vec::new(), Vec::new())
        }
    }
}

fn game_rows(
    path: &Path,
    maps: &Path,
    coverage: &mut Coverage,
) -> Result<(Vec<Position>, Vec<FeatureRow>)> {
    let bytes = fs::read(path)?;
    coverage.archive_fingerprint = fingerprint_bytes(&bytes);
    let replay = ReplayParser::new().with_debug(true).parse(&bytes)?;
    coverage.parsed = true;
    let game = replay.games.first().context("no game entry")?;
    if game.fog || game.players.len() != 2 {
        bail!("requires a two-player standard game");
    }
    let map_id = game.maps_id.as_u32();
    let map_bytes = fs::read(maps.join(format!("{map_id}.json")))?;
    coverage.map_fingerprint = fingerprint_bytes(&map_bytes);
    let map: AwbwMapData = serde_json::from_slice(&map_bytes)?;
    let game_id: u64 = path
        .file_stem()
        .context("no game ID")?
        .to_str()
        .context("non-UTF8 game ID")?
        .parse()?;
    let mut adapter = RecordedAdapter::new(&replay, &map)?;
    let mut evaluator = Evaluator::new(EvalWeights::STANDARD);
    let mut positions = Vec::new();
    let mut features = Vec::new();
    let mut turn = 0;
    for (index, action) in replay.turns.iter().enumerate() {
        // A join records its move onto the occupied target before the join.
        // Apply the join payload, which supplies the resulting unit state.
        if is_join_setup(action, replay.turns.get(index + 1)) {
            continue;
        }
        let just_acted = adapter.state().turn.active_player.clone();
        adapter
            .advance(action)
            .with_context(|| format!("action {index} ({})", action.kind_name()))?;
        coverage.actions += 1;
        if !matches!(action, Action::End { .. }) {
            continue;
        }
        turn += 1;
        let state = adapter.state();
        if state.turn.day < 3 || !matches!(state.match_state, Match::Active { .. }) {
            continue;
        }
        let perspective = state
            .players
            .seat(&just_acted)
            .context("unknown acting player")?;
        let authoritative = Session::new(state.clone());
        let visible = Session::from_observation(&observe(&AwbwVisibility, state, &just_acted)?)?;
        for (mode, session) in [
            (FeatureMode::Authoritative, authoritative),
            (FeatureMode::FogVisible, visible),
        ] {
            let seat = session
                .state()
                .players
                .seat(&just_acted)
                .context("unknown visible player")?;
            let breakdown = evaluator.breakdown_in(&session, seat);
            if breakdown.context.terminal_reason.is_some() {
                continue;
            }
            let feature_vector =
                observable_features(session.state(), seat, turn).context("no hostile seat")?;
            positions.push(Position {
                game: game_id,
                map: map_id,
                mode,
                day: state.turn.day,
                turn,
                seat: perspective.get(),
                winner: false,
                terms: breakdown.terms,
                unit_count_delta: breakdown.context.friendly_raw.fielded_unit_count
                    - breakdown.context.hostile_raw.fielded_unit_count,
                extra: awbrn_ai_diagnostics::replay_planner::replay_extra_features(&session, seat),
            });
            features.push(FeatureRow {
                schema_version: FEATURE_ANALYSIS_SCHEMA_VERSION,
                mode,
                match_id: format!("awbw-{game_id}"),
                pair: PairKey::new(map_id, 0, game_id),
                group_id: format!("awbw-{game_id}"),
                match_seed: game_id,
                seat_order: SeatOrderVariant::AgentFirst,
                day: state.turn.day,
                turn_index: turn,
                terminal_turn_index: 0,
                perspective_seat: perspective.get() as u8,
                just_acted_seat: perspective.get() as u8,
                active_seat: state
                    .players
                    .seat(&state.turn.active_player)
                    .context("unknown active player")?
                    .get() as u8,
                winner: false,
                features: feature_vector,
                producer_counts: Vec::new(),
                friendly_producer_counts: Default::default(),
                strongest_hostile_producer_counts: Default::default(),
                producer_known_capacity_delta: 0,
                friendly_unknown_producer_count: 0,
                hostile_unknown_producer_count: 0,
            });
        }
    }
    let Match::Finished {
        outcome: Outcome::Victory { winners, reason },
    } = &adapter.state().match_state
    else {
        bail!("no recorded decisive result");
    };
    coverage.outcome = format!("{reason:?}");
    if *reason == VictoryReason::Timeout {
        bail!("timeout result");
    }
    for (position, feature) in positions.iter_mut().zip(&mut features) {
        let seat = adapter
            .state()
            .players
            .seats()
            .find(|(seat, _)| seat.get() == position.seat)
            .context("unknown result seat")?
            .1;
        position.winner = winners.contains(&seat.team);
        feature.winner = position.winner;
        feature.terminal_turn_index = turn;
    }
    Ok((positions, features))
}

fn is_join_setup(action: &Action, next: Option<&Action>) -> bool {
    let (Action::Move(movement), Some(Action::Join { join_action, .. })) = (action, next) else {
        return false;
    };
    let moving = movement
        .unit
        .values()
        .find_map(|unit| unit.get_value())
        .map(|unit| unit.units_id.as_u32());
    let joining = join_action
        .join_id
        .values()
        .find_map(|id| id.get_value())
        .copied();
    moving.is_some() && moving == joining
}

fn write_json(path: PathBuf, value: &impl Serialize) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn fit(output: &Path) -> Result<()> {
    let coverage: Vec<Coverage> = serde_json::from_slice(&fs::read(output.join("coverage.json"))?)?;
    let rows = read_feature_rows(output.join("features.jsonl"))?;
    let extraction = FeatureExtraction {
        rows,
        event_rows: coverage.iter().map(|row| row.actions).sum(),
        matches: coverage.len(),
        matches_with_rows: coverage.iter().filter(|row| row.excluded.is_none()).count(),
        skipped_draws: 0,
        skipped_incomplete: coverage.iter().filter(|row| row.excluded.is_some()).count(),
        corpus_fingerprint: fingerprint_bytes(&fs::read(output.join("features.jsonl"))?),
    };
    let report = fit_feature_analysis(&extraction)?;
    write_json(output.join("feature-analysis.json"), &report)?;
    for mode in report.modes {
        println!(
            "{:?}: full model held-out log loss {:.4}; reduced model {:.4}",
            mode.mode,
            mode.model
                .full_cross_validation
                .as_ref()
                .context("no full model validation")?
                .log_loss
                .mean,
            mode.model.cross_validation.log_loss.mean
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Serialize)]
struct Variant {
    name: &'static str,
    front: f64,
    exposure: f64,
    unit_count: f64,
    temperature: Option<f64>,
}

impl Variant {
    fn score(self, row: &Position) -> f64 {
        row.terms.named_sum()
            + (self.front - 1.0) * row.terms.front
            + (self.exposure - 1.0) * row.terms.exposure
            + self.unit_count * row.unit_count_delta
    }
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
struct Metrics {
    log_loss: f64,
    brier: f64,
    accuracy: f64,
}

#[derive(Serialize)]
struct Validation {
    variant: Variant,
    mode: FeatureMode,
    split: &'static str,
    days: &'static str,
    games: usize,
    rows: usize,
    interval_units: usize,
    metrics: Metrics,
    paired_log_loss_delta: f64,
    paired_delta_ci95: [f64; 2],
    temperatures: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    game_metrics: Option<BTreeMap<u64, Metrics>>,
}

fn benchmark(output: &Path) -> Result<()> {
    let rows = BufReader::new(File::open(output.join("positions.jsonl"))?)
        .lines()
        .map(|line| Ok(serde_json::from_str::<Position>(&line?)?))
        .collect::<Result<Vec<_>>>()?;
    let config = PlannerConfig::V5.eval_weights;
    if (EvalWeights {
        front: EvalWeights::STANDARD.front,
        exposure: EvalWeights::STANDARD.exposure,
        unit_count: EvalWeights::STANDARD.unit_count,
        ..config
    }) != EvalWeights::STANDARD
    {
        bail!("the cached terms require the standard material and property weights");
    }
    let baseline = Variant {
        name: "planner-v5",
        front: config.front,
        exposure: config.exposure,
        unit_count: config.unit_count,
        temperature: None,
    };
    let variants = [
        baseline,
        Variant {
            name: "planner-v5-shipped",
            temperature: Some(config.temperature),
            ..baseline
        },
        Variant {
            name: "standard",
            front: 1.0,
            exposure: 1.0,
            unit_count: 0.0,
            ..baseline
        },
        Variant {
            name: "front-0",
            front: 0.0,
            exposure: 0.0,
            unit_count: 0.0,
            ..baseline
        },
        Variant {
            name: "front-0.5",
            front: 0.5,
            exposure: 0.0,
            unit_count: 0.0,
            ..baseline
        },
        Variant {
            name: "exposure-0.5",
            front: 0.25,
            exposure: 0.5,
            unit_count: 0.0,
            ..baseline
        },
        Variant {
            name: "exposure-1",
            front: 0.25,
            exposure: 1.0,
            unit_count: 0.0,
            ..baseline
        },
        Variant {
            name: "unit-count-200",
            front: 0.25,
            exposure: 0.0,
            unit_count: 200.0,
            ..baseline
        },
        Variant {
            name: "unit-count-1000",
            front: 0.25,
            exposure: 0.0,
            unit_count: 1000.0,
            ..baseline
        },
    ];
    let mut report = Vec::new();
    for mode in FeatureMode::ALL {
        let positions = rows
            .iter()
            .filter(|row| row.mode == mode)
            .collect::<Vec<_>>();
        for split in ["game", "map"] {
            let maps = positions
                .iter()
                .map(|row| row.map)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let mut groups = BTreeMap::<u32, BTreeSet<u64>>::new();
            for row in &positions {
                groups.entry(row.map).or_default().insert(row.game);
            }
            let folds = if split == "game" { 5 } else { maps.len() };
            if groups.values().map(BTreeSet::len).sum::<usize>() < 5 || maps.len() < 2 {
                bail!("benchmark requires at least five games and two maps");
            }
            let mut predictions = vec![BTreeMap::<(u64, u32), Vec<f64>>::new(); variants.len()];
            let mut temperatures = vec![Vec::new(); variants.len()];
            for repeat in 0..if split == "game" { 3 } else { 1 } {
                let folds_by_game = game_folds(&groups, repeat);
                let fold_of = |row: &Position| {
                    if split == "game" {
                        folds_by_game[&row.game]
                    } else {
                        maps.iter()
                            .position(|map| *map == row.map)
                            .expect("map has a fold")
                    }
                };
                for fold in 0..folds {
                    let train = positions
                        .iter()
                        .copied()
                        .filter(|row| fold_of(row) != fold)
                        .collect::<Vec<_>>();
                    let test = positions
                        .iter()
                        .copied()
                        .filter(|row| fold_of(row) == fold)
                        .collect::<Vec<_>>();
                    if train.is_empty() || test.is_empty() {
                        continue;
                    }
                    for (index, variant) in variants.iter().enumerate() {
                        let temperature = variant
                            .temperature
                            .unwrap_or_else(|| fit_temperature(&train, *variant));
                        temperatures[index].push(temperature);
                        for row in &test {
                            predictions[index]
                                .entry((row.game, row.turn))
                                .or_default()
                                .push(sigmoid(variant.score(row) / temperature));
                        }
                    }
                }
            }
            for (days, low, high) in [
                ("all", 3, u64::MAX),
                ("3-7", 3, 7),
                ("8-14", 8, 14),
                ("15+", 15, u64::MAX),
            ] {
                let segment = positions
                    .iter()
                    .copied()
                    .filter(|row| (low..=high).contains(&row.day))
                    .collect::<Vec<_>>();
                for (index, variant) in variants.iter().enumerate() {
                    let by_game = game_metrics(&segment, &predictions[index]);
                    let baseline = game_metrics(&segment, &predictions[0]);
                    let (units, baseline_units) = if split == "map" {
                        (
                            map_metrics(&by_game, &segment),
                            map_metrics(&baseline, &segment),
                        )
                    } else {
                        (by_game.clone(), baseline)
                    };
                    let deltas = units
                        .iter()
                        .map(|(unit, metrics)| metrics.log_loss - baseline_units[unit].log_loss)
                        .collect::<Vec<_>>();
                    let mean = deltas.iter().sum::<f64>() / deltas.len().max(1) as f64;
                    let sd = (deltas
                        .iter()
                        .map(|delta| (delta - mean).powi(2))
                        .sum::<f64>()
                        / deltas.len().saturating_sub(1).max(1) as f64)
                        .sqrt();
                    let margin = 1.96 * sd / (deltas.len().max(1) as f64).sqrt();
                    let metrics = average_metrics(&units);
                    if days == "all" {
                        println!(
                            "{mode:?} {split} {}: loss {:.4}, accuracy {:.3}, delta {mean:+.4}",
                            variant.name, metrics.log_loss, metrics.accuracy
                        );
                    }
                    report.push(Validation {
                        variant: *variant,
                        mode,
                        split,
                        days,
                        games: by_game.len(),
                        rows: segment.len(),
                        interval_units: units.len(),
                        metrics,
                        paired_log_loss_delta: mean,
                        paired_delta_ci95: [mean - margin, mean + margin],
                        temperatures: temperatures[index].clone(),
                        game_metrics: (index == 0 && days == "all").then_some(by_game),
                    });
                }
            }
        }
    }
    write_json(output.join("benchmark.json"), &report)?;
    let trained_temperatures = FeatureMode::ALL
        .into_iter()
        .map(|mode| {
            let positions = rows
                .iter()
                .filter(|row| row.mode == mode)
                .collect::<Vec<_>>();
            (mode, fit_temperature(&positions, baseline))
        })
        .collect::<BTreeMap<_, _>>();
    write_json(
        output.join("metadata.json"),
        &serde_json::json!({
            "schema_version": 1,
            "evaluator_basis": EvalWeights::STANDARD,
                "planner_config": PlannerConfig::V5,
                "trained_baseline_temperatures": trained_temperatures,
            "evaluator_source_fingerprint": fingerprint_bytes(include_bytes!("../../awbrn-ai/src/eval.rs")),
            "experiment_source_fingerprint": fingerprint_bytes(include_bytes!("awbw_evaluator.rs")),
            "positions_fingerprint": fingerprint_bytes(&fs::read(output.join("positions.jsonl"))?),
            "coverage_fingerprint": fingerprint_bytes(&fs::read(output.join("coverage.json"))?),
            "minimum_day": 3,
            "sampling": "post-End, before the next player's first action; terminal states excluded",
        "game_validation": "three repeats of five folds; whole games; map strata; equal game weights; same folds as feature analysis",
            "map_validation": "one held-out map per fold; equal map weights in reported metrics",
            "temperature_fit": "training games only; equal game weights; range 250 to 1000000",
            "interval": "descriptive normal interval over game or map mean loss differences",
            "map_snapshot": "current at capture; historical terrain is not confirmed",
        }),
    )
}

fn validate(training: &Path, holdout: &Path) -> Result<()> {
    let report: FeatureAnalysisReport =
        serde_json::from_slice(&fs::read(training.join("feature-analysis.json"))?)?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(training.join("metadata.json"))?)?;
    let holdout_metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(holdout.join("metadata.json"))?)?;
    if metadata["evaluator_basis"] != holdout_metadata["evaluator_basis"] {
        bail!("training and holdout use different evaluator terms");
    }
    for (directory, record) in [(training, &metadata), (holdout, &holdout_metadata)] {
        if record["positions_fingerprint"]
            != fingerprint_bytes(&fs::read(directory.join("positions.jsonl"))?)
        {
            bail!(
                "cached positions do not match metadata in {}",
                directory.display()
            );
        }
    }
    if report.corpus_fingerprint != fingerprint_bytes(&fs::read(training.join("features.jsonl"))?) {
        bail!("the fitted model does not match the training features");
    }
    let temperatures: BTreeMap<FeatureMode, f64> =
        serde_json::from_value(metadata["trained_baseline_temperatures"].clone())?;
    let weights: EvalWeights =
        serde_json::from_value(metadata["planner_config"]["eval_weights"].clone())?;
    let baseline = Variant {
        name: "planner-v5",
        front: weights.front,
        exposure: weights.exposure,
        unit_count: weights.unit_count,
        temperature: None,
    };
    let training_rows = read_feature_rows(training.join("features.jsonl"))?;
    let training_maps = training_rows
        .iter()
        .map(|row| row.pair.map_id)
        .collect::<BTreeSet<_>>();
    let training_games = training_rows
        .iter()
        .map(|row| row.pair.pair_index)
        .collect::<BTreeSet<_>>();
    let features = read_feature_rows(holdout.join("features.jsonl"))?;
    if features
        .iter()
        .any(|row| training_games.contains(&row.pair.pair_index))
    {
        bail!("training and holdout contain the same game");
    }
    let rows = BufReader::new(File::open(holdout.join("positions.jsonl"))?)
        .lines()
        .map(|line| Ok(serde_json::from_str::<Position>(&line?)?))
        .collect::<Result<Vec<_>>>()?;
    let mut transfer = Vec::new();
    for mode in &report.modes {
        let learned = ReducedEvaluator::from_report(mode);
        let predictions = features
            .iter()
            .filter(|row| row.mode == mode.mode)
            .map(|row| {
                (
                    (row.pair.pair_index, row.turn_index),
                    vec![learned.probability(row.features)],
                )
            })
            .collect::<BTreeMap<_, _>>();
        for subset in ["all", "new-maps"] {
            let positions = rows
                .iter()
                .filter(|row| {
                    row.mode == mode.mode && (subset == "all" || !training_maps.contains(&row.map))
                })
                .collect::<Vec<_>>();
            if positions.is_empty() {
                continue;
            }
            let current_predictions = positions
                .iter()
                .map(|row| {
                    (
                        (row.game, row.turn),
                        vec![sigmoid(baseline.score(row) / temperatures[&mode.mode])],
                    )
                })
                .collect();
            let shipped_predictions = positions
                .iter()
                .map(|row| {
                    (
                        (row.game, row.turn),
                        vec![sigmoid(baseline.score(row) / weights.temperature)],
                    )
                })
                .collect();
            let learned_games = game_metrics(&positions, &predictions);
            let current_games = game_metrics(&positions, &current_predictions);
            let shipped_games = game_metrics(&positions, &shipped_predictions);
            let deltas = learned_games
                .iter()
                .map(|(game, metrics)| metrics.log_loss - current_games[game].log_loss)
                .collect::<Vec<_>>();
            let mean = deltas.iter().sum::<f64>() / deltas.len() as f64;
            let variance = deltas
                .iter()
                .map(|delta| (delta - mean).powi(2))
                .sum::<f64>()
                / deltas.len().saturating_sub(1).max(1) as f64;
            let margin = 1.96 * (variance / deltas.len() as f64).sqrt();
            let learned_metrics = average_metrics(&learned_games);
            let current_metrics = average_metrics(&current_games);
            println!(
                "{:?} {subset}: {} games; frozen current loss {:.4}, learned {:.4}; paired delta {mean:+.4}",
                mode.mode,
                learned_games.len(),
                current_metrics.log_loss,
                learned_metrics.log_loss
            );
            transfer.push(serde_json::json!({
                "mode": mode.mode, "subset": subset, "games": learned_games.len(), "rows": positions.len(),
                "maps": positions.iter().map(|row| row.map).collect::<BTreeSet<_>>(),
                "current": current_metrics, "shipped": average_metrics(&shipped_games), "learned": learned_metrics,
                "equal_map_current": average_metrics(&map_metrics(&current_games, &positions)),
                "equal_map_learned": average_metrics(&map_metrics(&learned_games, &positions)),
                "paired_log_loss_delta": mean, "paired_delta_ci95": [mean - margin, mean + margin],
            }));
        }
    }
    write_json(
        holdout.join("transfer.json"),
        &serde_json::json!({
            "training": training, "holdout": holdout,
            "training_feature_fingerprint": report.corpus_fingerprint,
            "holdout_feature_fingerprint": fingerprint_bytes(&fs::read(holdout.join("features.jsonl"))?),
            "models_frozen_before_holdout": true, "results": transfer,
        }),
    )
}

fn sigmoid(score: f64) -> f64 {
    1.0 / (1.0 + (-score.clamp(-700.0, 700.0)).exp())
}

fn fit_temperature(rows: &[&Position], variant: Variant) -> f64 {
    let mut counts = BTreeMap::new();
    for row in rows {
        *counts.entry(row.game).or_insert(0) += 1;
    }
    // Solve for inverse temperature. The log loss is convex in this value.
    let mut low = 1.0 / 1_000_000.0;
    let mut high = 1.0 / 250.0;
    for _ in 0..48 {
        let inverse = (low + high) / 2.0;
        let gradient = rows
            .iter()
            .map(|row| {
                let score = variant.score(row);
                score * (sigmoid(score * inverse) - f64::from(row.winner))
                    / f64::from(counts[&row.game])
            })
            .sum::<f64>();
        if gradient > 0.0 {
            high = inverse;
        } else {
            low = inverse;
        }
    }
    2.0 / (low + high)
}

fn game_metrics(
    rows: &[&Position],
    predictions: &BTreeMap<(u64, u32), Vec<f64>>,
) -> BTreeMap<u64, Metrics> {
    let mut result = BTreeMap::<u64, (Metrics, usize)>::new();
    for row in rows {
        let Some(probabilities) = predictions.get(&(row.game, row.turn)) else {
            continue;
        };
        for probability in probabilities.iter().copied() {
            let label = f64::from(row.winner);
            let p = probability.clamp(1e-12, 1.0 - 1e-12);
            let (metrics, count) = result.entry(row.game).or_default();
            metrics.log_loss -= label * p.ln() + (1.0 - label) * (-p).ln_1p();
            metrics.brier += (p - label).powi(2);
            metrics.accuracy += if probability == 0.5 {
                0.5
            } else {
                f64::from((probability > 0.5) == row.winner)
            };
            *count += 1;
        }
    }
    result
        .into_iter()
        .map(|(game, (metrics, count))| {
            (
                game,
                Metrics {
                    log_loss: metrics.log_loss / count as f64,
                    brier: metrics.brier / count as f64,
                    accuracy: metrics.accuracy / count as f64,
                },
            )
        })
        .collect()
}

fn game_folds(groups: &BTreeMap<u32, BTreeSet<u64>>, repeat: u64) -> BTreeMap<u64, usize> {
    // Match feature_analysis::grouped_fold so comparisons use the same games.
    groups
        .values()
        .flat_map(|games| {
            let mut games = games.iter().copied().collect::<Vec<_>>();
            games.sort_by_key(|game| {
                let name = format!("awbw-{game}");
                let hash = name.bytes().fold(0xcbf29ce484222325, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
                });
                (
                    splitmix64(hash ^ splitmix64(0x9e37_79b9_7f4a_7c15 ^ repeat)),
                    name,
                )
            });
            games
                .into_iter()
                .enumerate()
                .map(|(index, game)| (game, index % 5))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn average_metrics(rows: &BTreeMap<u64, Metrics>) -> Metrics {
    let count = rows.len().max(1) as f64;
    Metrics {
        log_loss: rows.values().map(|row| row.log_loss).sum::<f64>() / count,
        brier: rows.values().map(|row| row.brier).sum::<f64>() / count,
        accuracy: rows.values().map(|row| row.accuracy).sum::<f64>() / count,
    }
}

fn map_metrics(games: &BTreeMap<u64, Metrics>, rows: &[&Position]) -> BTreeMap<u64, Metrics> {
    let map_for_game = rows
        .iter()
        .map(|row| (row.game, row.map))
        .collect::<BTreeMap<_, _>>();
    let mut by_map = BTreeMap::<u32, BTreeMap<u64, Metrics>>::new();
    for (game, metrics) in games {
        by_map
            .entry(map_for_game[game])
            .or_default()
            .insert(*game, *metrics);
    }
    by_map
        .into_iter()
        .map(|(map, games)| (u64::from(map), average_metrics(&games)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eagle_probe_checks_activation_and_restored_attackers() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("power-probe.json");
        let model = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/human-evaluator/cup-03-planner-model.json");
        power_probe(&model, &output).unwrap();
        let result: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(result["uncharged"]["power_legal"], false);
        assert_eq!(result["charged"]["legal_attackers"], 0);
        assert_eq!(result["cop_activated"]["legal_attackers"], 0);
        assert_eq!(result["cop_activated"]["ready_units"], 0);
        assert_eq!(result["activated"]["legal_attackers"], 3);
        assert_eq!(result["activated"]["ready_units"], 3);
        for reader in ["current", "candidate"] {
            assert_eq!(result["uncharged"][reader], result["charged"][reader]);
            assert_eq!(result["charged"][reader], result["activated"][reader]);
        }
    }

    #[test]
    fn cached_variants_match_direct_evaluation() {
        let state: awvm::semantic::State = serde_json::from_str(include_str!(
            "../../awbrn-ai/tests/fixtures/replay_regressions/amber-valley-day10.json"
        ))
        .unwrap();
        let session = Session::new(state);
        for (seat, _) in session.state().players.seats() {
            let base = Evaluator::new(EvalWeights::STANDARD).breakdown_in(&session, seat);
            let row = Position {
                game: 1,
                map: 1,
                mode: FeatureMode::Authoritative,
                day: 10,
                turn: 20,
                seat: seat.get(),
                winner: false,
                terms: base.terms,
                unit_count_delta: base.context.friendly_raw.fielded_unit_count
                    - base.context.hostile_raw.fielded_unit_count,
                extra: BTreeMap::new(),
            };
            for (front, exposure, unit_count) in
                [(0.0, 0.0, 0.0), (0.25, 0.5, 200.0), (1.0, 1.0, 1000.0)]
            {
                let variant = Variant {
                    name: "test",
                    front,
                    exposure,
                    unit_count,
                    temperature: None,
                };
                let exact = Evaluator::new(EvalWeights {
                    front,
                    exposure,
                    unit_count,
                    ..EvalWeights::STANDARD
                })
                .value_in(&session, seat);
                assert!((variant.score(&row) - exact).abs() < 1e-8);
            }
        }
    }

    #[test]
    fn calibration_gives_each_game_equal_weight() {
        let mut rows = (0..101)
            .map(|turn| Position {
                game: if turn == 0 { 1 } else { 2 },
                map: 1,
                mode: FeatureMode::Authoritative,
                day: 10,
                turn,
                seat: 0,
                winner: turn != 0,
                terms: EvalTerms {
                    army: 10_000.0,
                    ..Default::default()
                },
                unit_count_delta: 0.0,
                extra: BTreeMap::new(),
            })
            .collect::<Vec<_>>();
        let variant = Variant {
            name: "test",
            front: 1.0,
            exposure: 1.0,
            unit_count: 0.0,
            temperature: None,
        };
        assert!(fit_temperature(&rows.iter().collect::<Vec<_>>(), variant) > 999_000.0);
        rows.iter_mut().for_each(|row| row.winner = true);
        assert!((fit_temperature(&rows.iter().collect::<Vec<_>>(), variant) - 250.0).abs() < 1e-6);
    }

    #[test]
    fn map_summary_gives_each_map_equal_weight() {
        let positions = (1..=3)
            .map(|game| Position {
                game,
                map: if game == 3 { 20 } else { 10 },
                mode: FeatureMode::Authoritative,
                day: 10,
                turn: 1,
                seat: 0,
                winner: false,
                terms: Default::default(),
                unit_count_delta: 0.0,
                extra: BTreeMap::new(),
            })
            .collect::<Vec<_>>();
        let games = [
            (
                1,
                Metrics {
                    log_loss: 1.0,
                    ..Default::default()
                },
            ),
            (
                2,
                Metrics {
                    log_loss: 1.0,
                    ..Default::default()
                },
            ),
            (
                3,
                Metrics {
                    log_loss: 0.0,
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect();
        let maps = map_metrics(&games, &positions.iter().collect::<Vec<_>>());
        assert_eq!(average_metrics(&maps).log_loss, 0.5);
    }
}
