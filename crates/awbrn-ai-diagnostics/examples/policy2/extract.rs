//! The decisions of the second move model (`extract-policy2`) and the check
//! of its features in play (`check-policy2`).
//!
//! `extract-policy2 <maps> <output> <division>...` writes, for the human unit
//! orders of the archives:
//!
//! - `<output>.jsonl`: one row for each decision.
//! - `<output>.x.f32`: the features of each candidate order of the unit
//!   ([`POLICY2_FEATURE_COUNT`] little-endian 32-bit floats). The `offset` of
//!   a row is the index of its first order.
//! - `<output>.u.f32`: the next-unit features of each unit with candidate
//!   orders ([`UNIT_FEATURE_COUNT`] floats). `units_offset` is the index of
//!   the first unit of a row, and `actor_index` the acting unit among them.
//!
//! The features come from [`GreedyAgent::policy2_candidates`], the code of
//! play. The turn so far is every unit order of the player in this turn
//! before the decision that the tool can read: the move of a unit before an
//! unload, the unload, builds and powers do not count, and neither does an
//! order whose unit the tool cannot find.
//!
//! `check-policy2 <maps> <output> <division>...` plays the greedy policy of
//! `planner-v8` with the tree move model and the next-unit model
//! ([`awbrn_ai::agents::Weights::move2`] and
//! [`awbrn_ai::agents::Weights::move2_order`]) at each human decision of the
//! archives, with the human turn so far, and compares the features that play
//! calculated for every unit with those that the extractor writes. It writes
//! a summary to `<output>.check.json`.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use awbrn_ai::agents::GreedyAgent;
use awbrn_ai::agents::move_policy2::{
    ActionClass, LogEntry, TurnLog, policy2_feature_names, unit_feature_names,
};
use awbrn_map::AwbwMapData;
use awbw_replay::ReplayParser;
use awbw_replay::turn_models::Action;
use awvm::semantic::{AwbwVisibility, Location, Match, observe};
use awvm::session::OrderKind;
use awvm_awbw::RecordedAdapter;
use serde::Serialize;

#[derive(Serialize)]
struct Row {
    game: u64,
    map: u32,
    day: u64,
    seat: usize,
    /// The number of orders in the turn so far.
    step: usize,
    action: &'static str,
    kind: &'static str,
    group: &'static str,
    fog: bool,
    offset: u64,
    count: u32,
    chosen: u32,
    units_offset: u64,
    units_count: u32,
    actor_index: i32,
}

#[derive(Default, Serialize)]
struct Coverage {
    archives: usize,
    fog: usize,
    failed: usize,
    decisions: usize,
    /// Human orders that are not a candidate order of the unit (hide,
    /// repair, explode and the like), or that the tool cannot match.
    unmatched: usize,
    trapped: usize,
    skipped: usize,
    /// Orders in the turn logs.
    logged: usize,
}

#[derive(Default, Serialize)]
struct Check {
    decisions: usize,
    /// Units whose features play calculated.
    units: usize,
    /// Orders compared.
    orders: usize,
    /// Units whose candidate orders in play differ from the extracted ones.
    order_sets_differ: usize,
    /// Orders with at least one feature that differs.
    orders_differ: usize,
    /// Units of the next-unit model compared, and those that differ.
    unit_rows: usize,
    unit_rows_differ: usize,
    /// Decisions where the unit lists of the next-unit model differ.
    unit_lists_differ: usize,
    /// The count of differences of each feature, by name.
    feature_differences: std::collections::BTreeMap<String, usize>,
    unit_feature_differences: std::collections::BTreeMap<String, usize>,
    /// The first few differences, for a reader.
    examples: Vec<String>,
}

pub fn run(divisions: &[&str], maps: &Path, output: &Path, check: bool) -> Result<()> {
    let mut paths = Vec::new();
    for division in divisions {
        for entry in fs::read_dir(division).with_context(|| format!("read {division}"))? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "zip") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    if let Ok(limit) = std::env::var("POLICY2_GAME_LIMIT") {
        paths.truncate(limit.parse()?);
    }
    let with = |suffix: &str| {
        let mut path = output.as_os_str().to_owned();
        path.push(suffix);
        std::path::PathBuf::from(path)
    };
    let mut writer = Writer {
        rows: BufWriter::new(File::create(with(".jsonl"))?),
        orders: BufWriter::new(File::create(with(".x.f32"))?),
        units: BufWriter::new(File::create(with(".u.f32"))?),
        order_offset: 0,
        unit_offset: 0,
    };
    let mut coverage = Coverage::default();
    let mut summary = Check::default();
    for path in &paths {
        coverage.archives += 1;
        let result = game(
            path,
            maps,
            &mut writer,
            &mut coverage,
            check.then_some(&mut summary),
        );
        if let Err(error) = result {
            coverage.failed += 1;
            eprintln!("{}: {error:#}", path.display());
        }
    }
    writer.rows.flush()?;
    writer.orders.flush()?;
    writer.units.flush()?;
    fs::write(
        with(".coverage.json"),
        serde_json::to_vec_pretty(&coverage)?,
    )?;
    if check {
        fs::write(with(".check.json"), serde_json::to_vec_pretty(&summary)?)?;
        println!(
            "{} decisions; {} units, {} orders compared; {} order sets differ; {} orders differ; \
             {} unit rows compared, {} differ; {} unit lists differ",
            summary.decisions,
            summary.units,
            summary.orders,
            summary.order_sets_differ,
            summary.orders_differ,
            summary.unit_rows,
            summary.unit_rows_differ,
            summary.unit_lists_differ
        );
    }
    println!(
        "{} archives; {} fog; {} incomplete; {} decisions; {} unmatched; {} trapped; {} orders",
        coverage.archives,
        coverage.fog,
        coverage.failed,
        coverage.decisions,
        coverage.unmatched,
        coverage.trapped,
        writer.order_offset
    );
    Ok(())
}

pub fn names(output: &Path) -> Result<()> {
    fs::write(
        output,
        serde_json::to_vec_pretty(&serde_json::json!({
            "order": policy2_feature_names(),
            "unit": unit_feature_names(),
        }))?,
    )?;
    Ok(())
}

struct Writer {
    rows: BufWriter<File>,
    orders: BufWriter<File>,
    units: BufWriter<File>,
    order_offset: u64,
    unit_offset: u64,
}

fn game(
    path: &Path,
    maps: &Path,
    writer: &mut Writer,
    coverage: &mut Coverage,
    mut check: Option<&mut Check>,
) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let game_id: u64 = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.parse().ok())
        .with_context(|| format!("{} is not named with a game ID", path.display()))?;
    let replay = ReplayParser::new().with_debug(true).parse(&bytes)?;
    let entry = replay.games.first().context("no game entry")?;
    anyhow::ensure!(entry.players.len() == 2, "requires a two-player game");
    let map_id = entry.maps_id.as_u32();
    let map_path = maps.join(format!("{map_id}.json"));
    let map: AwbwMapData = serde_json::from_slice(
        &fs::read(&map_path).with_context(|| format!("read {}", map_path.display()))?,
    )
    .with_context(|| format!("parse {}", map_path.display()))?;

    let mut adapter = RecordedAdapter::new(&replay, &map)?;
    let fog = adapter.state().settings.fog;
    if fog {
        coverage.fog += 1;
    }
    let mut agent = awbrn_ai::profile::V8_GREEDY_CONFIG.build_greedy(0x6d6f_7665_5f70_6f6c);
    // The policy of play: the greedy weights of the extractor with the tree
    // move model and the next-unit model.
    let mut player_agent = check.is_some().then(|| {
        awbrn_ai::baseline::BaselineConfig {
            weights: awbrn_ai::agents::Weights {
                move2: 1.0,
                move2_order: true,
                ..awbrn_ai::profile::V8_GREEDY_CONFIG.weights
            },
            ..awbrn_ai::profile::V8_GREEDY_CONFIG
        }
        .build_greedy(0x6d6f_7665_5f70_6f6c)
    });
    let mut turn_owner = None;
    let mut log = TurnLog::new();
    // An archive that fails part of the way keeps the rows before the
    // failure, as `extract` does.
    for (index, action) in replay.turns.iter().enumerate() {
        let next = replay.turns.get(index + 1);
        if super::is_join_setup(action, next) {
            continue;
        }
        let state = adapter.state();
        let player = state.turn.active_player.clone();
        let key = (player.clone(), state.turn.day);
        if turn_owner.as_ref() != Some(&key) {
            turn_owner = Some(key);
            log.clear();
        }
        let kind = super::order_kind(action).filter(|_| {
            !matches!(next, Some(Action::Unload { .. })) || !matches!(action, Action::Move(_))
        });
        let pending = match kind {
            Some(kind) if matches!(state.match_state, Match::Active { .. }) => {
                let view = observe(&AwbwVisibility, state, &player)?;
                let (_, decision) = agent.policy2_candidates(&view, &log).unwrap_or_default();
                if let (Some(check), Some(player_agent)) = (check.as_deref_mut(), &mut player_agent)
                {
                    compare(player_agent, &view, &log, &decision, check);
                }
                Some((kind, state.clone(), decision))
            }
            _ => None,
        };
        let transition = adapter
            .advance(action)
            .with_context(|| format!("action {index} ({})", action.kind_name()))?;
        let Some((kind, before, decision)) = pending else {
            continue;
        };
        let seat = before
            .players
            .seat(&player)
            .context("unknown acting player")?;
        let after = transition.post_state();
        let movement = action.move_action();
        let trapped = movement.is_some_and(|movement| movement.trapped);
        let payload = movement.and_then(super::moved_unit);
        let actor = match payload {
            Some((id, _)) => Some(id),
            None => super::changed_unit(&before, after, seat),
        };
        let Some(actor) = actor else {
            coverage.unmatched += 1;
            continue;
        };
        let Some(unit) = before.units.get(actor) else {
            coverage.unmatched += 1;
            continue;
        };
        let Location::Board { position: origin } = unit.location else {
            coverage.skipped += 1;
            continue;
        };
        let destination = match after.units.get(actor).map(|unit| unit.location) {
            Some(Location::Board { position }) if !matches!(kind, "load" | "join") => position,
            _ => payload.and_then(|(_, position)| position).unwrap_or(origin),
        };
        let dimensions = before.board.dimensions();
        let target_position = match kind {
            "attack" => super::fire_target(action),
            _ => None,
        };
        // The order joins the turn so far after its own decision.
        let entry = LogEntry {
            class: ActionClass::of_name(kind),
            kind: unit.kind,
            destination,
            target: target_position,
        };
        if trapped {
            coverage.trapped += 1;
            log.push(entry);
            coverage.logged += 1;
            continue;
        }
        let target = target_position.and_then(|position| dimensions.cell_index(position));
        let Some(destination_cell) = dimensions.cell_index(destination) else {
            coverage.unmatched += 1;
            continue;
        };
        let own: Vec<_> = decision
            .orders
            .iter()
            .filter(|candidate| candidate.unit == actor)
            .collect();
        let chosen = own.iter().position(|candidate| {
            candidate.order.destination() == destination_cell
                && super::same_kind(kind, candidate.order.kind())
                && match (candidate.order.kind(), target) {
                    (OrderKind::Attack(cell), Some(target)) => cell == target,
                    (OrderKind::Attack(_), None) => false,
                    _ => true,
                }
        });
        let step = log.len();
        log.push(entry);
        coverage.logged += 1;
        let Some(chosen) = chosen else {
            coverage.unmatched += 1;
            continue;
        };
        let actor_index = decision
            .units
            .iter()
            .position(|(id, _)| *id == actor)
            .map_or(-1, |index| i32::try_from(index).unwrap_or(-1));
        let row = Row {
            game: game_id,
            map: map_id,
            day: before.turn.day,
            seat: seat.get(),
            step,
            action: kind,
            kind: unit.kind.as_str(),
            group: awbrn_ai::agents::move_policy::group_name(unit.kind),
            fog,
            offset: writer.order_offset,
            count: u32::try_from(own.len())?,
            chosen: u32::try_from(chosen)?,
            units_offset: writer.unit_offset,
            units_count: u32::try_from(decision.units.len())?,
            actor_index,
        };
        serde_json::to_writer(&mut writer.rows, &row)?;
        writer.rows.write_all(b"\n")?;
        for candidate in &own {
            for value in candidate.features {
                writer.orders.write_all(&value.to_le_bytes())?;
            }
        }
        for (_, features) in &decision.units {
            for value in features {
                writer.units.write_all(&value.to_le_bytes())?;
            }
        }
        writer.order_offset += own.len() as u64;
        writer.unit_offset += decision.units.len() as u64;
        coverage.decisions += 1;
    }
    Ok(())
}

/// Play the decision with the features captured, and compare them with the
/// extracted features of the same position and turn so far.
fn compare(
    agent: &mut GreedyAgent,
    view: &awvm::semantic::Observation,
    log: &TurnLog,
    decision: &awbrn_ai::agents::move_policy2::Policy2Outputs,
    check: &mut Check,
) {
    let order_names = policy2_feature_names();
    let unit_names = unit_feature_names();
    agent.capture_policy2();
    let _ = agent.act_with_log(view, log);
    let Some(played) = agent.take_policy2_capture() else {
        return;
    };
    check.decisions += 1;
    let mut units: Vec<_> = played.orders.iter().map(|c| c.unit).collect();
    units.dedup();
    for unit in units {
        check.units += 1;
        let mine: Vec<_> = played.orders.iter().filter(|c| c.unit == unit).collect();
        let theirs: Vec<_> = decision.orders.iter().filter(|c| c.unit == unit).collect();
        if mine.len() != theirs.len() || mine.iter().zip(&theirs).any(|(a, b)| a.order != b.order) {
            check.order_sets_differ += 1;
            if check.examples.len() < 20 {
                check.examples.push(format!(
                    "unit {unit:?}: {} orders in play, {} extracted",
                    mine.len(),
                    theirs.len()
                ));
            }
            continue;
        }
        for (a, b) in mine.iter().zip(&theirs) {
            check.orders += 1;
            let mut differs = false;
            for ((name, mine), theirs) in order_names.iter().zip(&a.features).zip(&b.features) {
                if mine.to_bits() != theirs.to_bits() {
                    differs = true;
                    *check.feature_differences.entry(name.clone()).or_default() += 1;
                    if check.examples.len() < 20 {
                        check.examples.push(format!(
                            "{:?} {name}: play {mine} extracted {theirs}",
                            a.order
                        ));
                    }
                }
            }
            check.orders_differ += usize::from(differs);
        }
    }
    if played.units.len() != decision.units.len()
        || played
            .units
            .iter()
            .zip(&decision.units)
            .any(|(a, b)| a.0 != b.0)
    {
        check.unit_lists_differ += 1;
        return;
    }
    for (a, b) in played.units.iter().zip(&decision.units) {
        check.unit_rows += 1;
        let mut differs = false;
        for ((name, mine), theirs) in unit_names.iter().zip(&a.1).zip(&b.1) {
            if mine.to_bits() != theirs.to_bits() {
                differs = true;
                *check
                    .unit_feature_differences
                    .entry(name.clone())
                    .or_default() += 1;
            }
        }
        check.unit_rows_differ += usize::from(differs);
    }
}
