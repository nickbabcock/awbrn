//! Extract the build decisions of human players from AWBW replay archives.
//!
//! One row is one build at one production site, with the board just before
//! the build. The tool also writes one "no build" row for each production
//! site that is empty and can build at the end of a turn. The features come
//! from [`awbrn_ai::build_policy::BuildContext::site`], the function that
//! the greedy policy uses in play. The tool reads the board that the player
//! sees.
//!
//! Each row also records the kind that the `ai-hard-v3` greedy scores rank
//! first at the same site, and the kind that the build model of
//! `ai-hard-v3` with `--model` chooses, when a model is given.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use awbrn_ai::agents::GreedyAgent;
use awbrn_ai::build_policy::{
    BuildChoice, BuildContext, BuildModel, KIND_FEATURES, decision_feature_names,
};
use awbrn_ai_diagnostic_types::fingerprint_bytes;
use awbrn_map::AwbwMapData;
use awbw_replay::{ReplayParser, turn_models::Action};
use awvm::ruleset::UnitKind;
use awvm::semantic::{
    AwbwVisibility, Location, Match, Observation, Outcome, PlayerIdx, Pos, observe,
};
use awvm::session::{LegalScope, LegalVisitor, Order, OrderKind, Session};
use awvm_awbw::RecordedAdapter;
use serde::Serialize;

#[derive(Serialize)]
struct Row {
    game: u64,
    source: String,
    map: u32,
    fog: bool,
    day: u64,
    seat: usize,
    own_co: String,
    enemy_co: String,
    /// The number of this decision in the turn of the player, from zero.
    step: u32,
    site: [u8; 2],
    terrain: String,
    funds: u64,
    /// The legal kinds at the site.
    legal: Vec<String>,
    /// The human choice: a kind, or "no-build".
    chosen: String,
    /// Whether the legal kinds contain the human choice.
    chosen_legal: bool,
    /// The kind that the greedy scores rank first.
    greedy: Option<String>,
    /// The greedy score of each legal kind, in the order of `legal`.
    greedy_scores: Vec<f64>,
    /// Whether the player won the game. `None` when the game has no
    /// decisive recorded result.
    winner: Option<bool>,
    decision: Vec<f64>,
    /// The kind features of each legal kind, in the order of `legal`.
    kind: Vec<Vec<f64>>,
}

#[derive(Serialize)]
struct Coverage {
    archive: PathBuf,
    archive_fingerprint: String,
    parsed: bool,
    fog: bool,
    players: usize,
    actions: usize,
    applied: usize,
    build_actions: usize,
    build_rows: usize,
    no_build_rows: usize,
    illegal_choices: usize,
    error: Option<String>,
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
        ["score", model, rows, output] => {
            score(Path::new(model), Path::new(rows), Path::new(output))
        }
        _ => bail!(
            "usage: build_policy extract <maps> <output> <division>... | names <output> | score <model> <rows.jsonl> <output.jsonl>"
        ),
    }
}

/// Write the model probabilities of each row, as the Rust evaluation reads
/// them. The fit compares them with its own values.
fn score(model: &Path, rows: &Path, output: &Path) -> Result<()> {
    use std::io::BufRead;
    let model = BuildModel::from_json(&fs::read_to_string(model)?)?;
    let names = decision_feature_names();
    let mut writer = BufWriter::new(File::create(output)?);
    for line in std::io::BufReader::new(File::open(rows)?).lines() {
        let row: serde_json::Value = serde_json::from_str(&line?)?;
        let mut features = awbrn_ai::build_policy::BuildFeatures {
            decision: [0.0; awbrn_ai::build_policy::DECISION_FEATURES],
            kind: [[0.0; KIND_FEATURES.len()]; UnitKind::COUNT],
        };
        for (index, value) in row["decision"]
            .as_array()
            .context("no decision features")?
            .iter()
            .enumerate()
        {
            features.decision[index] = value.as_f64().context("feature")?;
        }
        assert_eq!(names.len(), features.decision.len());
        let mut legal = Vec::new();
        for (index, name) in row["legal"]
            .as_array()
            .context("no legal")?
            .iter()
            .enumerate()
        {
            let kind = kind_of(name.as_str().context("kind")?)?;
            for (feature, value) in row["kind"][index]
                .as_array()
                .context("kind")?
                .iter()
                .enumerate()
            {
                features.kind[kind.index()][feature] = value.as_f64().context("feature")?;
            }
            legal.push(BuildChoice::Build(kind));
        }
        legal.push(BuildChoice::NoBuild);
        let probabilities = model.probabilities(&features, &legal);
        serde_json::to_writer(&mut writer, &probabilities)?;
        writeln!(writer)?;
    }
    writer.flush()?;
    Ok(())
}

fn kind_of(name: &str) -> Result<UnitKind> {
    UnitKind::ALL
        .into_iter()
        .find(|kind| kind.as_str() == name)
        .with_context(|| format!("unknown kind {name}"))
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
    let jobs = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(7);
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
    fs::create_dir_all(output)?;
    let mut writer = BufWriter::new(File::create(output.join("rows.jsonl"))?);
    let mut coverage = Vec::new();
    for (game, rows) in results {
        for row in &rows {
            serde_json::to_writer(&mut writer, row)?;
            writeln!(writer)?;
        }
        coverage.push(game);
    }
    writer.flush()?;
    fs::write(
        output.join("coverage.json"),
        serde_json::to_vec_pretty(&coverage)?,
    )?;
    let sum = |field: fn(&Coverage) -> usize| coverage.iter().map(field).sum::<usize>();
    println!(
        "{} archives; {} parsed; {} complete; {} build actions; {} build rows; {} no-build rows; {} illegal choices",
        coverage.len(),
        coverage.iter().filter(|row| row.parsed).count(),
        coverage.iter().filter(|row| row.error.is_none()).count(),
        sum(|row| row.build_actions),
        sum(|row| row.build_rows),
        sum(|row| row.no_build_rows),
        sum(|row| row.illegal_choices),
    );
    let mut errors = BTreeMap::new();
    for row in &coverage {
        if let Some(error) = &row.error {
            *errors.entry(error.clone()).or_insert(0) += 1;
        }
    }
    for (error, count) in errors {
        println!("error {count}: {error}");
    }
    Ok(())
}

fn extract_game(path: &Path, maps: &Path) -> (Coverage, Vec<Row>) {
    let mut coverage = Coverage {
        archive: path.to_owned(),
        archive_fingerprint: String::new(),
        parsed: false,
        fog: false,
        players: 0,
        actions: 0,
        applied: 0,
        build_actions: 0,
        build_rows: 0,
        no_build_rows: 0,
        illegal_choices: 0,
        error: None,
    };
    let mut rows = Vec::new();
    if let Err(error) = game_rows(path, maps, &mut coverage, &mut rows) {
        coverage.error = Some(format!("{error:#}"));
    }
    coverage.build_rows = rows.iter().filter(|row| row.chosen != "no-build").count();
    coverage.no_build_rows = rows.len() - coverage.build_rows;
    coverage.illegal_choices = rows.iter().filter(|row| !row.chosen_legal).count();
    (coverage, rows)
}

/// The legal production orders of the active player, by site.
struct Sites(Vec<(Pos, Vec<UnitKind>)>);

impl Sites {
    fn of(session: &Session) -> Self {
        struct Collector<'a>(&'a mut Vec<(awvm::semantic::CellIdx, Vec<UnitKind>)>);
        impl LegalVisitor for Collector<'_> {
            fn order(&mut self, order: Order) {
                let OrderKind::Produce(kind) = order.kind() else {
                    return;
                };
                let cell = order.destination();
                match self.0.iter_mut().find(|(known, _)| *known == cell) {
                    Some((_, kinds)) => kinds.push(kind),
                    None => self.0.push((cell, vec![kind])),
                }
            }
        }
        let mut cells = Vec::new();
        session.legal().visit_scoped(
            LegalScope {
                units: &[],
                unitless: true,
            },
            &mut Collector(&mut cells),
        );
        let dimensions = session.state().board.dimensions();
        Self(
            cells
                .into_iter()
                .filter_map(|(cell, mut kinds)| {
                    kinds.sort_by_key(|kind| kind.index());
                    Some((dimensions.position_of(cell)?, kinds))
                })
                .collect(),
        )
    }
}

struct Decider {
    greedy: GreedyAgent,
}

impl Decider {
    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        view: &Observation,
        session: &Session,
        seat: PlayerIdx,
        site: Pos,
        legal: &[UnitKind],
        chosen: Option<UnitKind>,
        step: u32,
        game: (u64, &str, u32),
    ) -> Row {
        let state = session.state();
        let context = BuildContext::new(state, seat);
        let features = context.site(site);
        let dimensions = state.board.dimensions();
        let cell = dimensions.cell_index(site);
        let scored = self
            .greedy
            .scored_unitless_orders(view)
            .map(|scored| scored.candidates().to_vec())
            .unwrap_or_default();
        let greedy_scores: Vec<f64> = legal
            .iter()
            .map(|kind| {
                scored
                    .iter()
                    .find(|candidate| {
                        Some(candidate.order.destination()) == cell
                            && candidate.order.kind() == OrderKind::Produce(*kind)
                    })
                    .map_or(f64::NAN, |candidate| candidate.score)
            })
            .collect();
        let mut greedy = None;
        let mut best = f64::NEG_INFINITY;
        for (kind, score) in legal.iter().zip(&greedy_scores) {
            if *score > best {
                best = *score;
                greedy = Some(kind.as_str().to_owned());
            }
        }
        let co = |seat: Option<PlayerIdx>| {
            seat.and_then(|seat| {
                state
                    .player(seat)
                    .commanders
                    .iter()
                    .find(|co| co.active)
                    .map(|co| co.id.as_str().to_owned())
            })
            .unwrap_or_default()
        };
        let enemy = state
            .players
            .seats()
            .map(|(other, _)| other)
            .find(|other| *other != seat);
        Row {
            game: game.0,
            source: game.1.to_owned(),
            map: game.2,
            fog: state.settings.fog,
            day: state.turn.day,
            seat: seat.get(),
            own_co: co(Some(seat)),
            enemy_co: co(enemy),
            step,
            site: [site.x, site.y],
            terrain: state.board.tile(site).terrain.as_str().to_owned(),
            funds: state.player(seat).funds,
            legal: legal.iter().map(|kind| kind.as_str().to_owned()).collect(),
            chosen: chosen.map_or_else(|| "no-build".to_owned(), |kind| kind.as_str().to_owned()),
            chosen_legal: chosen.is_none_or(|kind| legal.contains(&kind)),
            greedy,
            greedy_scores,
            winner: None,
            decision: features.decision.to_vec(),
            kind: legal
                .iter()
                .map(|kind| features.kind[kind.index()].to_vec())
                .collect(),
        }
    }
}

fn game_rows(path: &Path, maps: &Path, coverage: &mut Coverage, rows: &mut Vec<Row>) -> Result<()> {
    let bytes = fs::read(path)?;
    coverage.archive_fingerprint = fingerprint_bytes(&bytes);
    let replay = ReplayParser::new().with_debug(true).parse(&bytes)?;
    coverage.parsed = true;
    let game = replay.games.first().context("no game entry")?;
    coverage.fog = game.fog;
    coverage.players = game.players.len();
    coverage.actions = replay.turns.len();
    coverage.build_actions = replay
        .turns
        .iter()
        .filter(|action| matches!(action, Action::Build { .. }))
        .count();
    if game.players.len() != 2 {
        bail!("requires a two-player game");
    }
    let map_id = game.maps_id.as_u32();
    let map: AwbwMapData = serde_json::from_slice(&fs::read(maps.join(format!("{map_id}.json")))?)?;
    let game_id: u64 = path
        .file_stem()
        .context("no game ID")?
        .to_str()
        .context("non-UTF8 game ID")?
        .parse()?;
    let source = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    let mut adapter = RecordedAdapter::new(&replay, &map)?;
    let mut decider = Decider {
        greedy: awbrn_ai::profile::HARD_V3_CONFIG.build_greedy(game_id),
    };
    let mut step = 0_u32;
    let mut turn_owner = None;
    let start = rows.len();
    let result = (|| -> Result<()> {
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
            let is_build = matches!(action, Action::Build { .. });
            let is_end = matches!(action, Action::End { .. });
            let before =
                if (is_build || is_end) && matches!(state.match_state, Match::Active { .. }) {
                    let view = observe(&AwbwVisibility, state, &player)?;
                    let session = Session::from_observation(&view)?;
                    let seat = session
                        .state()
                        .players
                        .seat(&player)
                        .context("unknown acting player")?;
                    Some((view, session, seat))
                } else {
                    None
                };
            let ids: Vec<_> = state.units.iter().map(|unit| unit.id).collect();
            let transition = adapter
                .advance(action)
                .with_context(|| format!("action {index} ({})", action.kind_name()))?;
            coverage.applied += 1;
            let Some((view, session, seat)) = before else {
                continue;
            };
            let sites = Sites::of(&session);
            if is_build {
                let built = transition
                    .post_state()
                    .units
                    .iter()
                    .find(|unit| !ids.contains(&unit.id))
                    .context("a build without a new unit")?;
                let Location::Board { position } = built.location else {
                    bail!("a build that is not on the board");
                };
                let legal = sites
                    .0
                    .iter()
                    .find(|(site, _)| *site == position)
                    .map(|(_, kinds)| kinds.clone())
                    .unwrap_or_default();
                rows.push(decider.row(
                    &view,
                    &session,
                    seat,
                    position,
                    &legal,
                    Some(built.kind),
                    step,
                    (game_id, &source, map_id),
                ));
                step += 1;
            } else {
                for (site, legal) in &sites.0 {
                    rows.push(decider.row(
                        &view,
                        &session,
                        seat,
                        *site,
                        legal,
                        None,
                        step,
                        (game_id, &source, map_id),
                    ));
                }
            }
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
        for row in &mut rows[start..] {
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
