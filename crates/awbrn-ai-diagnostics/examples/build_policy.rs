//! Extract the build decisions of human players from AWBW replay archives.
//!
//! One row is one build at one production site, with the board immediately
//! before the build. The features come from
//! [`awbrn_ai::build_features::BuildContext::site`], the function that the
//! greedy policy uses in play. The tool reads the board that the player
//! sees. `fit_build_policy.py` fits the build model to these rows. See
//! `assets/ai-diagnostics/build-policy/README.md`.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use awbrn_ai::build_features::{BuildContext, KIND_FEATURES, decision_feature_names};
use awbrn_map::AwbwMapData;
use awbw_replay::{ReplayParser, turn_models::Action};
use awvm::ruleset::UnitKind;
use awvm::semantic::{AwbwVisibility, Location, Match, Pos, observe};
use awvm::session::{LegalScope, LegalVisitor, Order, OrderKind, Session};
use awvm_awbw::RecordedAdapter;
use serde::Serialize;

#[derive(Serialize)]
struct Row {
    game: u64,
    map: u32,
    fog: bool,
    day: u64,
    seat: usize,
    /// The number of this build in the turn of the player, from zero.
    step: u32,
    funds: u64,
    /// The legal kinds at the site.
    legal: Vec<&'static str>,
    /// The kind that the player built.
    chosen: &'static str,
    /// Whether the legal kinds contain the built kind.
    chosen_legal: bool,
    decision: Vec<f64>,
    /// The kind features of each legal kind, in the order of `legal`.
    kind: Vec<Vec<f64>>,
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
        ["names", output] => {
            let names = serde_json::json!({
                "decision": decision_feature_names(),
                "kind": KIND_FEATURES,
            });
            fs::write(output, serde_json::to_vec_pretty(&names)?)?;
            Ok(())
        }
        _ => bail!(
            "usage: build_policy extract <maps> <output.jsonl> <division>... | names <output>"
        ),
    }
}

fn extract(divisions: &[&str], maps: &Path, output: &Path) -> Result<()> {
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
    if paths.is_empty() {
        bail!("the divisions contain no replay archives");
    }
    let mut writer = BufWriter::new(File::create(output)?);
    let (mut rows, mut failed) = (0_usize, 0_usize);
    // One game at a time, so that memory stays small. Each game writes its
    // rows when it is complete.
    for path in &paths {
        let mut game = Vec::new();
        if let Err(error) = game_rows(path, maps, &mut game) {
            failed += 1;
            eprintln!("{}: {error:#}", path.display());
        }
        for row in &game {
            serde_json::to_writer(&mut writer, row)?;
            writeln!(writer)?;
        }
        rows += game.len();
    }
    writer.flush()?;
    println!(
        "{} archives; {failed} incomplete; {rows} build rows",
        paths.len()
    );
    Ok(())
}

/// The legal kinds that the active player can build at `site`.
fn legal_kinds(session: &Session, site: Pos) -> Vec<UnitKind> {
    struct Collector(Option<awvm::semantic::CellIdx>, Vec<UnitKind>);
    impl LegalVisitor for Collector {
        fn order(&mut self, order: Order) {
            if let OrderKind::Produce(kind) = order.kind()
                && Some(order.destination()) == self.0
            {
                self.1.push(kind);
            }
        }
    }
    let cell = session.state().board.dimensions().cell_index(site);
    let mut collector = Collector(cell, Vec::new());
    session.legal().visit_scoped(
        LegalScope {
            units: &[],
            unitless: true,
        },
        &mut collector,
    );
    collector.1.sort_by_key(|kind| kind.index());
    collector.1
}

/// Append the build rows of one archive to `rows`. A game that stops with
/// an error keeps the rows before the error.
fn game_rows(path: &Path, maps: &Path, rows: &mut Vec<Row>) -> Result<()> {
    let replay = ReplayParser::new()
        .with_debug(true)
        .parse(&fs::read(path)?)?;
    let game = replay.games.first().context("no game entry")?;
    if game.players.len() != 2 {
        bail!("requires a two-player game");
    }
    let map_id = game.maps_id.as_u32();
    let map: AwbwMapData = serde_json::from_slice(&fs::read(maps.join(format!("{map_id}.json")))?)?;
    let game_id: u64 = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("no game ID")?
        .parse()?;
    let mut adapter = RecordedAdapter::new(&replay, &map)?;
    let mut step = 0_u32;
    let mut turn_owner = None;
    for (index, action) in replay.turns.iter().enumerate() {
        if is_join_setup(action, replay.turns.get(index + 1)) {
            continue;
        }
        let state = adapter.state();
        let player = state.turn.active_player.clone();
        let key = (player.clone(), state.turn.day);
        if turn_owner.as_ref() != Some(&key) {
            turn_owner = Some(key);
            step = 0;
        }
        let before = if matches!(action, Action::Build { .. })
            && matches!(state.match_state, Match::Active { .. })
        {
            let session = Session::from_observation(&observe(&AwbwVisibility, state, &player)?)?;
            Some((
                session,
                state.units.iter().map(|unit| unit.id).collect::<Vec<_>>(),
            ))
        } else {
            None
        };
        let transition = adapter
            .advance(action)
            .with_context(|| format!("action {index} ({})", action.kind_name()))?;
        let Some((session, ids)) = before else {
            continue;
        };
        let built = transition
            .post_state()
            .units
            .iter()
            .find(|unit| !ids.contains(&unit.id))
            .context("a build without a new unit")?;
        let Location::Board { position } = built.location else {
            bail!("a build that is not on the board");
        };
        let state = session.state();
        let seat = state
            .players
            .seat(&player)
            .context("unknown acting player")?;
        let legal = legal_kinds(&session, position);
        let features = BuildContext::new(state, seat).site(position);
        rows.push(Row {
            game: game_id,
            map: map_id,
            fog: state.settings.fog,
            day: state.turn.day,
            seat: seat.get(),
            step,
            funds: state.player(seat).funds,
            legal: legal.iter().map(|kind| kind.as_str()).collect(),
            chosen: built.kind.as_str(),
            chosen_legal: legal.contains(&built.kind),
            decision: features.decision.to_vec(),
            kind: legal
                .iter()
                .map(|kind| features.kind[kind.index()].to_vec())
                .collect(),
        });
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
