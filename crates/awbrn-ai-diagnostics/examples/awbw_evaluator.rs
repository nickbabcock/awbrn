//! Measure evaluator predictions on recorded human games.
//!
//! Sample completed turns from day 3. Use complete two-player standard games
//! with a recorded winner. Exclude timeouts, draws, and incomplete playback.
//! Keep every position from a game in the same validation fold.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use awbrn_ai::eval::{EvalTerms, EvalWeights, Evaluator};
use awbrn_ai::planner::PlannerConfig;
use awbrn_ai_diagnostic_types::{PairKey, SeatOrderVariant, fingerprint_bytes};
use awbrn_ai_diagnostics::feature_analysis::{
    FEATURE_ANALYSIS_SCHEMA_VERSION, FeatureMode, FeatureRow, observable_features,
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
        ["puzzles", model, output] => planner_puzzles(Path::new(model), Path::new(output)),
        ["power-probe", model, output] => power_probe(Path::new(model), Path::new(output)),
        _ => bail!(
            "usage: awbw_evaluator extract <division> <maps> <output> | puzzles <model> <output> | power-probe <model> <output>"
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

fn planner_puzzle_report(bytes: &[u8]) -> Result<serde_json::Value> {
    use awbrn_ai_diagnostics::tournament::AgentFactory;
    let candidate = awbrn_ai_diagnostics::replay_planner::ReplayPlannerFactory::new(
        "replay-planner",
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
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eagle_probe_checks_activation_and_restored_attackers() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("power-probe.json");
        let model = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/human-evaluator/refit-positive-003-model.json");
        power_probe(&model, &output).unwrap();
        let result: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(result["uncharged"]["power_legal"], false);
        assert_eq!(result["charged"]["legal_attackers"], 0);
        assert_eq!(result["cop_activated"]["legal_attackers"], 0);
        assert_eq!(result["cop_activated"]["ready_units"], 0);
        assert_eq!(result["activated"]["legal_attackers"], 3);
        assert_eq!(result["activated"]["ready_units"], 3);
        assert_eq!(result["uncharged"]["current"], result["charged"]["current"]);
        assert_eq!(result["charged"]["current"], result["activated"]["current"]);
        assert!(
            result["charged"]["candidate"].as_f64().unwrap()
                > result["uncharged"]["candidate"].as_f64().unwrap()
        );
    }
}
