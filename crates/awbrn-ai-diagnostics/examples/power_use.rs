//! Extract the commander power use of human players from AWBW replay
//! archives.
//!
//! One row is one player turn. The row holds the power meter at the start of
//! the turn, the power that the player used in the turn (if any), and some
//! facts about the board at the start and at the end of the turn. The facts
//! come from the full state, also in a game with fog.
//!
//! Usage: `power_use <maps> <output.jsonl> <division>...`

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use awbrn_ai::planner::{PropertyValues, reply_estimate};
use awbrn_map::AwbwMapData;
use awbw_replay::{ReplayParser, turn_models::Action};
use awvm::commander::{PowerLevel, power_activation_cost};
use awvm::ruleset::{self, TerrainTrait};
use awvm::semantic::{
    CAPTURE_REQUIRED_POINTS, Location, Match, Outcome, PlayerIdx, PowerState, State, TileOwner,
};
use awvm::session::Session;
use awvm_awbw::RecordedAdapter;
use serde::Serialize;

#[derive(Serialize, Default, Clone)]
struct Row {
    game: u64,
    source: String,
    map: u32,
    fog: bool,
    day: u64,
    seat: usize,
    co: String,
    enemy_co: String,
    /// The meter at the start of the turn.
    charge: u64,
    uses: u64,
    cop_cost: Option<u64>,
    scop_cost: Option<u64>,
    /// "none", "cop" or "scop".
    used: String,
    /// The number of attacks of the turn before the power.
    attacks_before_power: u32,
    attacks: u32,
    /// The number of unit orders of the turn (all actions except builds,
    /// powers, tags, deletes and the end of the turn).
    orders: u32,
    /// The number of unit orders of the turn before the power.
    orders_before_power: u32,
    /// The number of builds of the turn.
    builds: u32,
    /// The number of builds of the turn before the power.
    builds_before_power: u32,
    captures: u32,
    own_value: f64,
    enemy_value: f64,
    own_units: u32,
    enemy_units: u32,
    own_income: u32,
    enemy_income: u32,
    /// The unit value that the player can remove with focused fire at the
    /// start of the turn, without a power.
    attack_potential: f64,
    kill_potential: u32,
    /// Own properties with an enemy capturer on them at the start of the
    /// turn.
    own_under_capture: u32,
    /// The least capture points of an own headquarters or lab with an enemy
    /// capturer on it, or 20.
    own_hq_points: u8,
    /// The enemy unit value removed in the turn.
    damage_dealt: f64,
    /// The own unit value removed in the turn.
    damage_taken: f64,
    winner: Option<bool>,
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [maps, output, divisions @ ..] = args.as_slice() else {
        bail!("usage: power_use <maps> <output.jsonl> <division>...");
    };
    if divisions.is_empty() {
        bail!("no division");
    }
    let mut paths = Vec::new();
    for division in divisions {
        for entry in fs::read_dir(division)? {
            let path = entry?.path();
            if path.extension().is_some_and(|extension| extension == "zip") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    let jobs = std::env::var("JOBS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2_usize);
    let maps = Path::new(maps);
    let results = std::thread::scope(|scope| {
        let handles = paths
            .chunks(paths.len().div_ceil(jobs).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|path| (path.clone(), game_rows(path, maps)))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("replay worker completed"))
            .collect::<Vec<(PathBuf, (Vec<Row>, Option<String>))>>()
    });
    let mut writer = BufWriter::new(File::create(output)?);
    let mut errors = 0;
    let mut rows = 0;
    for (path, (game, error)) in results {
        if let Some(error) = error {
            errors += 1;
            eprintln!("{}: {error}", path.display());
        }
        for row in game {
            rows += 1;
            serde_json::to_writer(&mut writer, &row)?;
            writeln!(writer)?;
        }
    }
    writer.flush()?;
    println!(
        "{} archives; {rows} rows; {errors} with errors",
        paths.len()
    );
    Ok(())
}

fn army(state: &State, seat: PlayerIdx) -> (f64, u32) {
    state
        .units
        .iter()
        .filter(|unit| unit.owner == seat)
        .fold((0.0, 0), |(value, count), unit| {
            (
                value + ruleset::profile(unit.kind).cost as f64 * f64::from(unit.hp) / 100.0,
                count + 1,
            )
        })
}

fn co_name(state: &State, seat: PlayerIdx) -> String {
    state
        .player(seat)
        .commanders
        .iter()
        .find(|co| co.active)
        .map(|co| co.id.as_str().to_owned())
        .unwrap_or_default()
}

fn start_row(state: &State, seat: PlayerIdx, enemy: PlayerIdx) -> Row {
    let co = state.player(seat).commanders.iter().find(|co| co.active);
    let (charge, uses, id) = co.map_or((0, 0, None), |co| {
        (co.power_charge, co.power_uses, Some(co.id))
    });
    let cost = |level| id.and_then(|id| power_activation_cost(id, level, uses).ok().flatten());
    let (own_value, own_units) = army(state, seat);
    let (enemy_value, enemy_units) = army(state, enemy);
    let income = |holder: PlayerIdx| {
        state
            .board
            .tiles()
            .filter(|tile| {
                tile.owner.is_owned_by(holder)
                    && ruleset::terrain_has(tile.terrain, TerrainTrait::Income)
            })
            .count() as u32
    };
    let session = Session::new(state.clone());
    let potential = reply_estimate(&session, enemy, PropertyValues::STANDARD);
    let mut own_under_capture = 0;
    let mut own_hq_points = CAPTURE_REQUIRED_POINTS;
    for unit in state.units.iter().filter(|unit| unit.owner == enemy) {
        let Location::Board { position } = unit.location else {
            continue;
        };
        let Some(tile) = state.board.get(position) else {
            continue;
        };
        if tile.owner != TileOwner::Owned(seat) {
            continue;
        }
        let Some(points) = tile.capture_points else {
            continue;
        };
        if points >= CAPTURE_REQUIRED_POINTS {
            continue;
        }
        own_under_capture += 1;
        if ruleset::terrain_has(tile.terrain, TerrainTrait::CaptureDefeatsOwner) {
            own_hq_points = own_hq_points.min(points);
        }
    }
    Row {
        fog: state.settings.fog,
        day: state.turn.day,
        seat: seat.get(),
        co: co_name(state, seat),
        enemy_co: co_name(state, enemy),
        charge,
        uses,
        cop_cost: cost(PowerLevel::Cop),
        scop_cost: cost(PowerLevel::Scop),
        used: "none".to_owned(),
        own_value,
        enemy_value,
        own_units,
        enemy_units,
        own_income: income(seat),
        enemy_income: income(enemy),
        attack_potential: potential.unit_loss,
        kill_potential: potential.units_destroyed,
        own_under_capture,
        own_hq_points,
        ..Row::default()
    }
}

fn finish(row: &mut Row, state: &State, seat: PlayerIdx, enemy: PlayerIdx) {
    row.damage_dealt = (row.enemy_value - army(state, enemy).0).max(0.0);
    row.damage_taken = (row.own_value - army(state, seat).0).max(0.0);
}

fn game_rows(path: &Path, maps: &Path) -> (Vec<Row>, Option<String>) {
    let mut rows = Vec::new();
    let error = run(path, maps, &mut rows)
        .err()
        .map(|error| format!("{error:#}"));
    (rows, error)
}

fn run(path: &Path, maps: &Path, rows: &mut Vec<Row>) -> Result<()> {
    let bytes = fs::read(path)?;
    let replay = ReplayParser::new().parse(&bytes)?;
    let game = replay.games.first().context("no game entry")?;
    if game.players.len() != 2 {
        bail!("requires a two-player game");
    }
    let map_id = game.maps_id.as_u32();
    let map: AwbwMapData = serde_json::from_slice(&fs::read(maps.join(format!("{map_id}.json")))?)?;
    let game_id: u64 = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("game ID")?
        .parse()?;
    let source = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut adapter = RecordedAdapter::new(&replay, &map)?;
    let mut current: Option<(PlayerIdx, PlayerIdx, u64, Row)> = None;
    let result = (|| -> Result<()> {
        for (index, action) in replay.turns.iter().enumerate() {
            let state = adapter.state();
            if !matches!(state.match_state, Match::Active { .. }) {
                break;
            }
            let player = state.turn.active_player.clone();
            let seat = state.players.seat(&player).context("unknown player")?;
            let enemy = state
                .players
                .seats()
                .map(|(other, _)| other)
                .find(|other| *other != seat)
                .context("no enemy")?;
            let day = state.turn.day;
            if current
                .as_ref()
                .is_none_or(|(known, _, known_day, _)| *known != seat || *known_day != day)
            {
                if let Some((seat, enemy, _, mut row)) = current.take() {
                    finish(&mut row, state, seat, enemy);
                    rows.push(row);
                }
                let mut row = start_row(state, seat, enemy);
                row.game = game_id;
                row.source.clone_from(&source);
                row.map = map_id;
                current = Some((seat, enemy, day, row));
            }
            let name = action.kind_name();
            let is_end = matches!(action, Action::End { .. });
            if is_end && let Some((seat, enemy, _, row)) = current.as_mut() {
                finish(row, adapter.state(), *seat, *enemy);
            }
            adapter
                .advance(action)
                .with_context(|| format!("action {index} ({name})"))?;
            let after = adapter.state();
            if let Some((seat, _, _, row)) = current.as_mut() {
                match name {
                    "Build" => row.builds += 1,
                    "End" | "Power" | "Tag" | "Delete" | "Resign" => {}
                    _ => row.orders += 1,
                }
                match name {
                    "Fire" => row.attacks += 1,
                    "Capt" => row.captures += 1,
                    "Power" => {
                        row.attacks_before_power = row.attacks;
                        row.orders_before_power = row.orders;
                        row.builds_before_power = row.builds;
                        row.used = match after.player(*seat).power_state {
                            PowerState::Cop { .. } => "cop",
                            PowerState::Scop { .. } => "scop",
                            PowerState::None => "unknown",
                        }
                        .to_owned();
                    }
                    _ => {}
                }
                if is_end {
                    let (seat, enemy, _, row) = current.take().expect("a current turn");
                    let _ = (seat, enemy);
                    rows.push(row);
                }
            }
        }
        if let Some((seat, enemy, _, mut row)) = current.take() {
            finish(&mut row, adapter.state(), seat, enemy);
            rows.push(row);
        }
        Ok(())
    })();
    let winners = match &adapter.state().match_state {
        Match::Finished {
            outcome: Outcome::Victory { winners, .. },
        } => Some(winners.clone()),
        _ => None,
    };
    if let Some(winners) = winners {
        for row in rows.iter_mut() {
            let team = adapter
                .state()
                .players
                .seats()
                .find(|(seat, _)| seat.get() == row.seat)
                .map(|(_, player)| player.team.clone());
            row.winner = team.map(|team| winners.contains(&team));
        }
    }
    result
}
