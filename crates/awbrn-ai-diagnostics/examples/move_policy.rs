//! Extract the unit orders of human players from AWBW replay archives.
//!
//! One decision is one order of one unit, with the board immediately before
//! the order. The choices are all the legal orders of that unit. The features
//! come from [`awbrn_ai::agents::GreedyAgent::move_candidates`], the function
//! that the greedy policy uses in play, with the greedy weights of
//! `planner-v8`. The tool reads the board that the player sees. In a game
//! with fog, this is the observation of the acting player with the known
//! map, as in play.
//!
//! The tool does not read the unloads of a transport or the move of a
//! transport immediately before an unload. The choices of a decision do not
//! include the unload orders, because in play the unload orders of a
//! transport are a separate choice group.
//!
//! The command writes two files:
//!
//! - `<output>.jsonl`: one row for each decision.
//! - `<output>.f32`: the features of the orders of the unit of each decision,
//!   as little-endian 32-bit floats, [`MOVE_FEATURE_COUNT`] for each order.
//!   The `offset` of a row is the index of its first order.
//!
//! `fit_move_policy.py` fits the move model to these files.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use awbrn_ai::agents::move_policy::{self, MOVE_FEATURE_COUNT, MOVE_FEATURES, MoveCandidate};
use awbrn_map::AwbwMapData;
use awbw_replay::turn_models::{Action, MoveAction, TargetedPlayer};
use awbw_replay::{AwbwReplay, ReplayParser};
use awvm::semantic::{AwbwVisibility, Location, Match, Pos, State, UnitAction, UnitId, observe};
use awvm::session::OrderKind;
use awvm_awbw::RecordedAdapter;
use serde::Serialize;

#[derive(Serialize)]
struct Row {
    game: u64,
    map: u32,
    day: u64,
    seat: usize,
    /// The number of this unit order in the turn of the player, from zero.
    step: u32,
    /// The replay action.
    action: &'static str,
    /// The unit kind.
    kind: &'static str,
    /// The unit group of the coefficients in play
    /// ([`move_policy::group_name`]).
    group: &'static str,
    /// Whether the game has fog.
    fog: bool,
    /// The index of the first order of this decision in the feature file.
    offset: u64,
    /// The number of legal orders of the unit.
    count: u32,
    /// The order that the player chose, as an index into the orders.
    chosen: u32,
    /// The greedy score of each order.
    greedy: Vec<f32>,
}

#[derive(Default, Serialize)]
struct Coverage {
    archives: usize,
    fog: usize,
    failed: usize,
    decisions: usize,
    unmatched: usize,
    trapped: usize,
    skipped: usize,
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["extract", maps, output, divisions @ ..] if !divisions.is_empty() => {
            extract(divisions, Path::new(maps), Path::new(output))
        }
        ["score", strength, prefix] => score(strength.parse()?, prefix),
        ["names", output] => Ok(fs::write(
            output,
            serde_json::to_vec_pretty(&MOVE_FEATURES.as_slice())?,
        )?),
        _ => bail!(
            "usage: move_policy extract <maps> <output> <division>... | names <output> \
             | score <strength> <prefix>"
        ),
    }
}

/// Report the top-1 agreement of the built-in move model with the decisions
/// of one row file, as the policy reads the model in play.
fn score(strength: f64, prefix: &str) -> Result<()> {
    use std::io::BufRead;
    let bytes = fs::read(format!("{prefix}.f32"))?;
    let values: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect();
    let (mut hits, mut greedy_hits, mut decisions) = (0.0, 0.0, 0_usize);
    for line in std::io::BufReader::new(File::open(format!("{prefix}.jsonl"))?).lines() {
        let row: serde_json::Value = serde_json::from_str(&line?)?;
        let offset = row["offset"].as_u64().context("offset")? as usize;
        let count = row["count"].as_u64().context("count")? as usize;
        let chosen = row["chosen"].as_u64().context("chosen")? as usize;
        let kind: awvm::ruleset::UnitKind = serde_json::from_value(row["kind"].clone())?;
        let model = if row["fog"].as_bool() == Some(true) {
            move_policy::fog_model()
        } else {
            move_policy::model()
        };
        let utility: Vec<f64> = (0..count)
            .map(|index| {
                let start = (offset + index) * MOVE_FEATURE_COUNT;
                let mut features = [0.0; MOVE_FEATURE_COUNT];
                for (slot, value) in features.iter_mut().zip(&values[start..]) {
                    *slot = f64::from(*value);
                }
                model.utility(kind, &features, strength)
            })
            .collect();
        let tie_share = |scores: &[f64]| {
            let best = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if scores[chosen] < best {
                0.0
            } else {
                1.0 / scores.iter().filter(|value| **value == best).count() as f64
            }
        };
        hits += tie_share(&utility);
        let greedy: Vec<f64> = row["greedy"]
            .as_array()
            .context("greedy")?
            .iter()
            .filter_map(serde_json::Value::as_f64)
            .collect();
        greedy_hits += tie_share(&greedy);
        decisions += 1;
    }
    println!(
        "{decisions} decisions; model top-1 {:.4}; greedy top-1 {:.4}",
        hits / decisions as f64,
        greedy_hits / decisions as f64
    );
    Ok(())
}

fn extract(divisions: &[&str], maps: &Path, output: &Path) -> Result<()> {
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
    if paths.is_empty() {
        bail!("the divisions contain no replay archives");
    }
    let with = |extension: &str| -> PathBuf {
        let mut path = output.as_os_str().to_owned();
        path.push(extension);
        PathBuf::from(path)
    };
    let mut rows = BufWriter::new(File::create(with(".jsonl"))?);
    let mut features = BufWriter::new(File::create(with(".f32"))?);
    let mut coverage = Coverage::default();
    let mut offset = 0_u64;
    for path in &paths {
        coverage.archives += 1;
        let mut game = Game::default();
        if let Err(error) = game_rows(path, maps, &mut game, &mut coverage)? {
            coverage.failed += 1;
            eprintln!("{}: {error:#}", path.display());
        }
        for (mut row, candidates) in game.rows {
            row.offset = offset;
            offset += candidates.len() as u64;
            for candidate in &candidates {
                for value in candidate {
                    features.write_all(&value.to_le_bytes())?;
                }
            }
            serde_json::to_writer(&mut rows, &row)?;
            writeln!(rows)?;
            coverage.decisions += 1;
        }
    }
    rows.flush()?;
    features.flush()?;
    fs::write(
        with(".coverage.json"),
        serde_json::to_vec_pretty(&coverage)?,
    )?;
    println!(
        "{} archives; {} fog; {} incomplete; {} decisions; {} unmatched; {} trapped; {} orders",
        coverage.archives,
        coverage.fog,
        coverage.failed,
        coverage.decisions,
        coverage.unmatched,
        coverage.trapped,
        offset
    );
    Ok(())
}

#[derive(Default)]
struct Game {
    rows: Vec<(Row, Vec<[f32; MOVE_FEATURE_COUNT]>)>,
}

/// Append the decisions of one archive to `game`.
///
/// The outer error is an input error. The inner error is a replay error, and
/// the rows before it stay in `game`.
fn game_rows(
    path: &Path,
    maps: &Path,
    game: &mut Game,
    coverage: &mut Coverage,
) -> Result<Result<()>> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let game_id: u64 = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.parse().ok())
        .with_context(|| format!("{} is not named with a game ID", path.display()))?;
    let replay = match ReplayParser::new().with_debug(true).parse(&bytes) {
        Ok(replay) => replay,
        Err(error) => return Ok(Err(error.into())),
    };
    let Some(entry) = replay.games.first() else {
        return Ok(Err(anyhow::anyhow!("no game entry")));
    };
    if entry.players.len() != 2 {
        return Ok(Err(anyhow::anyhow!("requires a two-player game")));
    }
    let map_id = entry.maps_id.as_u32();
    let map_path = maps.join(format!("{map_id}.json"));
    let map: AwbwMapData = serde_json::from_slice(
        &fs::read(&map_path).with_context(|| format!("read {}", map_path.display()))?,
    )
    .with_context(|| format!("parse {}", map_path.display()))?;
    Ok(replay_rows(&replay, &map, game_id, map_id, game, coverage))
}

/// The order kind of a replay action of one unit, or `None` for an action
/// that is not a unit order that the tool reads.
fn order_kind(action: &Action) -> Option<&'static str> {
    Some(match action {
        Action::Move(_) => "wait",
        Action::Fire { .. } => "attack",
        Action::Capt { .. } => "capture",
        Action::Load { .. } => "load",
        Action::Join { .. } => "join",
        Action::Supply { .. } => "supply",
        Action::Hide { .. } => "hide",
        Action::Unhide { .. } => "reveal",
        Action::Repair { .. } => "repair",
        Action::Explode { .. } => "explode",
        _ => return None,
    })
}

fn same_kind(name: &str, kind: OrderKind) -> bool {
    matches!(
        (name, kind),
        ("wait", OrderKind::Wait)
            | ("attack", OrderKind::Attack(_))
            | ("capture", OrderKind::Capture)
            | ("load", OrderKind::Load)
            | ("join", OrderKind::Join)
            | ("supply", OrderKind::Supply)
            | ("hide", OrderKind::Hide)
            | ("reveal", OrderKind::Reveal)
            | ("repair", OrderKind::Repair(_))
            | ("explode", OrderKind::Explode)
    )
}

/// The moving unit and its destination in a move payload.
fn moved_unit(movement: &MoveAction) -> Option<(UnitId, Option<Pos>)> {
    let global = movement
        .unit
        .get(&TargetedPlayer::Global)
        .and_then(|unit| unit.get_value());
    let unit = global.or_else(|| movement.unit.values().find_map(|unit| unit.get_value()))?;
    let position = match (unit.units_x, unit.units_y) {
        (Some(x), Some(y)) => Some(Pos::new(u8::try_from(x).ok()?, u8::try_from(y).ok()?)),
        _ => None,
    };
    Some((UnitId::new(unit.units_id.as_u32()), position))
}

/// The target of an attack in a fire payload.
fn fire_target(action: &Action) -> Option<Pos> {
    let Action::Fire { fire_action, .. } = action else {
        return None;
    };
    let vision = fire_action
        .combat_info_vision
        .get(&TargetedPlayer::Global)
        .or_else(|| fire_action.combat_info_vision.values().next())?;
    let defender = vision.combat_info.defender.get_value()?;
    Some(Pos::new(
        u8::try_from(defender.units_x).ok()?,
        u8::try_from(defender.units_y).ok()?,
    ))
}

/// The actor of a unit order without a move payload: the one ready unit of
/// the player that the order changes.
fn changed_unit(before: &State, after: &State, seat: awvm::semantic::PlayerIdx) -> Option<UnitId> {
    let mut changed = before.units.iter().filter(|unit| {
        unit.owner == seat
            && unit.action == UnitAction::Ready
            && matches!(unit.location, Location::Board { .. })
            && after.units.get(unit.id).is_none_or(|post| {
                post.action != UnitAction::Ready || post.location != unit.location
            })
    });
    let first = changed.next()?;
    changed.next().is_none().then_some(first.id)
}

fn replay_rows(
    replay: &AwbwReplay,
    map: &AwbwMapData,
    game_id: u64,
    map_id: u32,
    game: &mut Game,
    coverage: &mut Coverage,
) -> Result<()> {
    let mut adapter = RecordedAdapter::new(replay, map)?;
    let fog = adapter.state().settings.fog;
    if fog {
        coverage.fog += 1;
    }
    let mut agent = awbrn_ai::profile::V8_GREEDY_CONFIG.build_greedy(0x6d6f_7665_5f70_6f6c);
    let mut step = 0_u32;
    let mut turn_owner = None;
    for (index, action) in replay.turns.iter().enumerate() {
        let next = replay.turns.get(index + 1);
        if is_join_setup(action, next) {
            continue;
        }
        let state = adapter.state();
        let player = state.turn.active_player.clone();
        let key = (player.clone(), state.turn.day);
        if turn_owner.as_ref() != Some(&key) {
            turn_owner = Some(key);
            step = 0;
        }
        // A move before an unload is half of the unload, which the tool
        // does not read.
        let kind = order_kind(action).filter(|_| {
            !matches!(next, Some(Action::Unload { .. })) || !matches!(action, Action::Move(_))
        });
        let pending = match kind {
            Some(kind) if matches!(state.match_state, Match::Active { .. }) => {
                let view = observe(&AwbwVisibility, state, &player)?;
                let candidates = agent.move_candidates(&view).unwrap_or_default();
                Some((kind, state.clone(), candidates))
            }
            _ => None,
        };
        let transition = adapter
            .advance(action)
            .with_context(|| format!("action {index} ({})", action.kind_name()))?;
        let Some((kind, before, candidates)) = pending else {
            continue;
        };
        let seat = before
            .players
            .seat(&player)
            .context("unknown acting player")?;
        let after = transition.post_state();
        let movement = action.move_action();
        if movement.is_some_and(|movement| movement.trapped) {
            coverage.trapped += 1;
            continue;
        }
        let payload = movement.and_then(moved_unit);
        let actor = match payload {
            Some((id, _)) => Some(id),
            None => changed_unit(&before, after, seat),
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
        let target = match kind {
            "attack" => fire_target(action).and_then(|position| dimensions.cell_index(position)),
            _ => None,
        };
        let Some(destination) = dimensions.cell_index(destination) else {
            coverage.unmatched += 1;
            continue;
        };
        let own: Vec<&MoveCandidate> = candidates
            .iter()
            .filter(|candidate| {
                candidate.unit == actor && !matches!(candidate.order.kind(), OrderKind::Unload(_))
            })
            .collect();
        let chosen = own.iter().position(|candidate| {
            candidate.order.destination() == destination
                && same_kind(kind, candidate.order.kind())
                && match (candidate.order.kind(), target) {
                    (OrderKind::Attack(cell), Some(target)) => cell == target,
                    (OrderKind::Attack(_), None) => false,
                    _ => true,
                }
        });
        let Some(chosen) = chosen else {
            coverage.unmatched += 1;
            continue;
        };
        game.rows.push((
            Row {
                game: game_id,
                map: map_id,
                day: before.turn.day,
                seat: seat.get(),
                step,
                action: kind,
                kind: unit.kind.as_str(),
                group: move_policy::group_name(unit.kind),
                fog,
                offset: 0,
                count: u32::try_from(own.len())?,
                chosen: u32::try_from(chosen)?,
                greedy: own
                    .iter()
                    .map(|candidate| candidate.breakdown.total as f32)
                    .collect(),
            },
            own.iter()
                .map(|candidate| candidate.features.map(|value| value as f32))
                .collect(),
        ));
        step += 1;
    }
    Ok(())
}

/// Whether `action` is the move half of a join, which the next action
/// replays.
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
