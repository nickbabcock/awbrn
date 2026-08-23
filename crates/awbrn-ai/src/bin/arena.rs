//! Two agents, N games, both seat orders, one seed.
//!
//! From tier 2 onward every question is "is this better than the last one",
//! and only a tournament answers it. This binary exists before the agent that
//! needs it, because an improvement nobody can measure is a claim.
//!
//! Each game index is played twice, once with each agent in the first seat, so
//! the first-player advantage falls out of the score. The board is a mirror
//! (see [`awbrn_ai::board`]), which is what makes that control worth running.
//!
//! One seed gives one tournament. Each game's seed is derived from the run seed
//! and the game index, so a ten-game run and a two-hundred-game run play the
//! same first ten games.
//!
//! The board is Amber Valley. Close Encounters, the first arena board, is
//! decided by a day-six headquarters rush and holds almost no combat, so it
//! cannot measure a term that prices combat. It stays behind `--map
//! close-encounters` because the tier 1 numbers were taken on it.
//!
//! The report says what the games were made of as well as who won them, for
//! the same reason: a score against a mirror of the same agent cannot see that
//! the games hold no combat. See [`awbrn_ai::shape`].

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use awbrn_ai::agent::Agent;
use awbrn_ai::agents::{GreedyAgent, RandomAgent, Weights};
use awbrn_ai::board::{
    AMBER_VALLEY_SEATS, SEATS, amber_valley, amber_valley_map, arena, arena_map,
};
use awbrn_ai::harness::{Limits, Record, play_measured, play_observed};
use awbrn_ai::rng::Rng;
use awbrn_ai::shape::SeatShape;
use awbrn_image::{Tilesets, render_state};
use awbrn_map::AwbrnMap;
use awbrn_types::PlayerFaction;
use awvm::semantic::{Match, Outcome, State, TeamId, VictoryReason};
use awvm::session::Session;
use awvm::transition::Command;

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let start = Instant::now();
    let tally = match run(&options) {
        Ok(tally) => tally,
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    };
    report(&options, &tally, start.elapsed().as_secs_f64());
}

const USAGE: &str = "\
usage: arena [--map NAME] [--seed N] [--games N] [--fog] [--day-cap N] [--first NAME] [--second NAME] [--weights FILE] [--second-weights FILE] [--sample DIR]

  --map NAME     Map to play. Default amber-valley. Also close-encounters,
                 which is the board the first tier 1 numbers were taken on.
  --seed N       Seed for the tournament. The same seed gives the same result.
                 Default 1.
  --games N      Game pairs to play. Each pair is the same seed played with
                 both seat orders, so the tournament plays 2N games. Default 50.
  --fog          Play with fog of war on. Default off.
  --day-cap N    Abandon a game after this many days. Default 35.
  --first NAME   What the agent under test plays: random, a weighting this
                 crate names, or a path to a JSON weights file. Default random.
  --second NAME  The same, for the agent it plays. Default random.
  --weights FILE Read JSON weight overrides for the first agent. A weight the
                 file does not name keeps what --first gives it. The first
                 agent cannot be random.
  --second-weights FILE
                 The same, for the second agent.
  --sample DIR   Capture the first game as turn PNGs and a JSONL sidecar.

agents: random, or one of the weightings tier1, threat, deny, defend, default.
Each weighting adds one term to the one before it, so an adjacent pair is the
measurement of that term and nothing else.";

#[derive(Debug)]
struct Options {
    map: &'static str,
    seed: u64,
    pairs: usize,
    fog: bool,
    day_cap: u32,
    first: String,
    second: String,
    weights: Option<PathBuf>,
    second_weights: Option<PathBuf>,
    sample: Option<PathBuf>,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            map: "amber-valley",
            seed: 1,
            pairs: 50,
            fog: false,
            day_cap: Limits::default().days,
            first: "random".to_owned(),
            second: "random".to_owned(),
            weights: None,
            second_weights: None,
            sample: None,
        };
        let mut arguments = arguments.peekable();
        while let Some(argument) = arguments.next() {
            let mut value = || {
                arguments
                    .next()
                    .ok_or_else(|| format!("{argument} needs a value"))
            };
            match argument.as_str() {
                "--map" => options.map = map_name(&value()?)?,
                "--seed" => options.seed = parse_number(&value()?)?,
                "--games" => options.pairs = parse_number(&value()?)?,
                "--day-cap" => options.day_cap = parse_number(&value()?)?,
                "--fog" => options.fog = true,
                "--first" => options.first = agent_spec(&value()?)?,
                "--second" => options.second = agent_spec(&value()?)?,
                "--weights" => options.weights = Some(value()?.into()),
                "--second-weights" => options.second_weights = Some(value()?.into()),
                "--sample" => options.sample = Some(value()?.into()),
                "--help" | "-h" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        if options.pairs == 0 {
            return Err("--games must be at least 1".to_owned());
        }
        if options.day_cap == 0 {
            return Err("--day-cap must be at least 1".to_owned());
        }
        if options.weights.is_some() && options.first == RANDOM {
            return Err("--weights requires a greedy first agent".to_owned());
        }
        if options.second_weights.is_some() && options.second == RANDOM {
            return Err("--second-weights requires a greedy second agent".to_owned());
        }
        Ok(options)
    }

    const fn limits(&self) -> Limits {
        Limits {
            days: self.day_cap,
            ..Limits::DEFAULT
        }
    }
}

fn map_name(name: &str) -> Result<&'static str, String> {
    match name {
        "close-encounters" => Ok("close-encounters"),
        "amber-valley" => Ok("amber-valley"),
        _ => Err(format!(
            "unknown map {name}, known maps are close-encounters, amber-valley"
        )),
    }
}

fn parse_number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{text} is not a number this argument accepts"))
}

/// The one agent that reads no weights.
const RANDOM: &str = "random";

/// What one seat plays, by the word the arguments use.
///
/// There are two agents and not five. Every greedy doctrine this crate has
/// ever seated is a weighting of the one greedy agent, so a seat names either
/// `random`, one of [`Weights::PRESETS`], or a file of weights. A new term
/// then needs a weight and no agent at all, which is what stops the ladder
/// from growing a name for each of them.
fn agent_spec(name: &str) -> Result<String, String> {
    if name == RANDOM || Weights::preset(name).is_some() || is_path(name) {
        return Ok(name.to_owned());
    }
    Err(format!(
        "unknown agent {name}, known agents are {RANDOM}, {}, or a path to a weights file",
        Weights::preset_names()
    ))
}

/// Whether this word is meant as a file and not as a name.
///
/// A name this crate holds is one word with no punctuation in it, so anything
/// that looks like a path is read as one. Saying so here is what lets a
/// misspelled preset report the names instead of an unreadable file.
fn is_path(name: &str) -> bool {
    name.contains(std::path::MAIN_SEPARATOR) || name.ends_with(".json")
}

/// The weighting one seat plays, or `None` for the random agent.
///
/// `overrides` is layered over the named weighting rather than over the
/// defaults, so `--first defend --weights sweep/hold-0.4.json` is the defend
/// weighting with one field moved. A field the file does not name keeps what
/// the named weighting gives it, and a name no weighting holds is an error.
fn seat_weights(spec: &str, overrides: Option<&Path>) -> Result<Option<Weights>> {
    if spec == RANDOM {
        return Ok(None);
    }
    let base = match Weights::preset(spec) {
        Some(weights) => weights,
        None => read_weights(Path::new(spec))?,
    };
    match overrides {
        Some(path) => layer_weights(base, path).map(Some),
        None => Ok(Some(base)),
    }
}

fn build(seed: u64, weights: Option<Weights>) -> Box<dyn Agent> {
    match weights {
        Some(weights) => Box::new(GreedyAgent::with_weights(seed, weights)),
        None => Box::new(RandomAgent::from_seed(seed)),
    }
}

fn state(options: &Options, seed: u64) -> State {
    match options.map {
        "close-encounters" => arena(options.fog, seed),
        "amber-valley" => amber_valley(options.fog, seed),
        other => unreachable!("{other} passed the argument check"),
    }
}

fn map_and_seats(options: &Options) -> (AwbrnMap, [PlayerFaction; 2]) {
    match options.map {
        "close-encounters" => (arena_map(), SEATS),
        "amber-valley" => (amber_valley_map(), AMBER_VALLEY_SEATS),
        other => unreachable!("{other} passed the argument check"),
    }
}

#[derive(Default)]
struct Tally {
    /// Games the agent named by `--first` won.
    wins: u32,
    losses: u32,
    draws: u32,
    /// Games the harness stopped without the reducer naming an outcome.
    ///
    /// The day cap is the ruleset's own limit now, so it decides a game on the
    /// properties held rather than abandoning it. This counts what is left,
    /// which should be nothing.
    abandoned: u32,
    commands: u64,
    refusals: u64,
    /// How the games that ended ended.
    endings: Endings,
    /// Days played, over every game, and the shortest and the longest.
    days: u64,
    shortest: u32,
    longest: u32,
    /// The shape of the games, from each side of the board.
    under_test: Side,
    opponent: Side,
    /// Pairs completed, and the points the agent under test took over them.
    ///
    /// A pair is one observation and the interval counts these, not games.
    /// See [`paired_interval`].
    pairs: u32,
    pair_points: f64,
    pair_scores: Vec<f64>,
    /// How the pairs went: both games, one each, neither.
    swept: u32,
    split: u32,
    lost_both: u32,
}

impl Tally {
    const fn games(&self) -> u32 {
        self.wins + self.losses + self.draws
    }

    /// The score of the agent under test, a draw counting a half.
    fn score(&self) -> f64 {
        (f64::from(self.wins) + f64::from(self.draws) / 2.0) / f64::from(self.games())
    }

    /// Add one pair, worth the points the agent under test took over its two
    /// games.
    ///
    /// `points` is 0, a half or 1, and a quarter or three quarters when one of
    /// the two games was drawn.
    fn add_pair(&mut self, points: f64) {
        self.pairs += 1;
        self.pair_points += points;
        self.pair_scores.push(points);
        if points > 0.75 {
            self.swept += 1;
        } else if points < 0.25 {
            self.lost_both += 1;
        } else {
            self.split += 1;
        }
    }

    fn mean_days(&self) -> f64 {
        self.days as f64 / f64::from(self.games().max(1))
    }
}

/// How the tournament's games ended.
///
/// A score cannot tell a rush from a war. This can: a board every game leaves
/// by a headquarters capture on day six is a board where nothing that prices
/// combat can be measured, and a change that makes the agent fight shows up
/// here as a rout before it shows up in the score.
#[derive(Default)]
struct Endings {
    hq_capture: u32,
    rout: u32,
    /// The day limit, won on the properties each side holds.
    day_limit: u32,
    /// Another victory reason: a capture limit, a resignation, a timeout.
    other: u32,
    /// A draw, which the day limit gives when the two sides hold the same
    /// number of properties.
    drawn: u32,
}

/// One side of the board, over the whole tournament.
///
/// The two sides are the agent under test and the agent it plays, and not the
/// two seats: the tournament plays each pair both ways round, so a seat is
/// both agents in equal measure and tells nothing about either.
#[derive(Default)]
struct Side {
    games: u32,
    built: u64,
    lost: u64,
    /// Turns behind the sums below, which is what they average over.
    turns: u64,
    units: f64,
    value: f64,
    income: f64,
    last_units: u64,
    last_value: f64,
    last_income: u64,
}

impl Side {
    fn add(&mut self, seat: &SeatShape) {
        self.games += 1;
        self.built += u64::from(seat.built);
        self.lost += u64::from(seat.lost);
        self.turns += u64::from(seat.turns);
        self.units += seat.units_total;
        self.value += seat.value_total;
        self.income += seat.income_total;
        self.last_units += u64::from(seat.last_units);
        self.last_value += seat.last_value;
        self.last_income += seat.last_income;
    }

    /// A count over the games played, which is what "each game" means.
    fn each_game(&self, total: f64) -> f64 {
        total / f64::from(self.games.max(1))
    }

    /// A sample over the turns sampled, which is what "each turn" means.
    ///
    /// The turns of a long game weigh more than the turns of a short one,
    /// because this pools the samples rather than averaging the games.
    fn each_turn(&self, total: f64) -> f64 {
        total / (self.turns.max(1)) as f64
    }
}

fn run(options: &Options) -> Result<Tally> {
    let first_weights = seat_weights(&options.first, options.weights.as_deref())?;
    let second_weights = seat_weights(&options.second, options.second_weights.as_deref())?;
    let mut tally = Tally::default();
    // One session for the whole tournament. It keeps the board-sized tables it
    // allocated, so a game after the first asks the allocator for nothing.
    let mut session = Session::new(state(options, options.seed));

    for pair in 0..options.pairs {
        let mut pair_points = 0.0;
        for under_test_first in [true, false] {
            let game = Rng::mix(options.seed ^ ((pair as u64) << 32));
            let mut entropy = Rng::from_seed(Rng::mix(game ^ 0x1));
            let mut first = build(Rng::mix(game ^ 0x2), first_weights);
            let mut second = build(Rng::mix(game ^ 0x3), second_weights);

            // The seat the agent under test sits in. Playing the same seed
            // both ways is what removes the first-player advantage from the
            // score rather than averaging over it.
            let seat = usize::from(!under_test_first);
            let mut agents: [&mut dyn Agent; 2] = if under_test_first {
                [first.as_mut(), second.as_mut()]
            } else {
                [second.as_mut(), first.as_mut()]
            };

            let state = state(options, game);
            let teams: Vec<TeamId> = state
                .players
                .seats()
                .map(|(_, player)| player.team.clone())
                .collect();

            let record = if pair == 0 && under_test_first {
                if let Some(directory) = &options.sample {
                    let (map, seats) = map_and_seats(options);
                    let mut sample = Sample::new(directory, map, seats, options, game)?;
                    let record = play_observed(
                        state,
                        &mut session,
                        &mut agents,
                        &mut entropy,
                        options.limits(),
                        |state, command| sample.observe(state, command),
                    );
                    sample.finish()?;
                    record
                } else {
                    play_measured(
                        state,
                        &mut session,
                        &mut agents,
                        &mut entropy,
                        options.limits(),
                    )
                }
            } else {
                play_measured(
                    state,
                    &mut session,
                    &mut agents,
                    &mut entropy,
                    options.limits(),
                )
            };
            pair_points += score(&mut tally, &record, &teams[seat], seat);
        }
        tally.add_pair(pair_points / 2.0);
    }

    Ok(tally)
}

/// Read one file of weights over `base`.
///
/// The file holds the fields it moves and nothing else, so it is read as a
/// map and laid over the named weighting written out as one. Reading it into
/// [`Weights`] directly would fill every field it does not name from the
/// defaults, which would quietly throw the named weighting away.
fn layer_weights(base: Weights, path: &Path) -> Result<Weights> {
    let file =
        File::open(path).with_context(|| format!("opening weights file {}", path.display()))?;
    let overrides: serde_json::Map<String, serde_json::Value> = serde_json::from_reader(file)
        .with_context(|| format!("reading weights file {}", path.display()))?;
    let mut merged = serde_json::to_value(base).context("writing the named weighting out")?;
    let Some(fields) = merged.as_object_mut() else {
        unreachable!("weights write out as an object");
    };
    fields.extend(overrides);
    // `Weights` refuses a name it does not hold, so a misspelled weight is an
    // error here rather than a sweep that measured nothing.
    serde_json::from_value(merged)
        .with_context(|| format!("applying weights file {}", path.display()))
}

fn read_weights(path: &Path) -> Result<Weights> {
    let file =
        File::open(path).with_context(|| format!("opening weights file {}", path.display()))?;
    serde_json::from_reader(file)
        .with_context(|| format!("reading weights file {}", path.display()))
}

struct Sample {
    directory: PathBuf,
    map: AwbrnMap,
    seats: [PlayerFaction; 2],
    tilesets: Tilesets,
    log: BufWriter<File>,
    frame: u32,
    commands: Vec<serde_json::Value>,
    error: Option<anyhow::Error>,
}

impl Sample {
    fn new(
        directory: &Path,
        map: AwbrnMap,
        seats: [PlayerFaction; 2],
        options: &Options,
        game_seed: u64,
    ) -> Result<Self> {
        std::fs::create_dir(directory).with_context(|| {
            format!(
                "creating sample directory {} (choose a path that does not exist)",
                directory.display()
            )
        })?;
        let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("the AI crate is inside the workspace")
            .join("assets/textures");
        let tilesets = Tilesets::load_from_dir(&assets)
            .with_context(|| format!("loading sample sprites from {}", assets.display()))?;
        let log = File::create(directory.join("log.jsonl"))
            .with_context(|| format!("creating {}/log.jsonl", directory.display()))?;
        let metadata = File::create(directory.join("metadata.json"))
            .with_context(|| format!("creating {}/metadata.json", directory.display()))?;
        serde_json::to_writer_pretty(
            metadata,
            &serde_json::json!({
                "schema_version": 1,
                "map": options.map,
                "tournament_seed": options.seed,
                "game_seed": game_seed,
                "pair": 0,
                "seat_order": [options.first, options.second],
                "fog": options.fog,
                "day_cap": options.day_cap,
                "weights": options.weights,
                "second_weights": options.second_weights,
            }),
        )?;
        Ok(Self {
            directory: directory.to_owned(),
            map,
            seats,
            tilesets,
            log: BufWriter::new(log),
            frame: 0,
            commands: Vec::new(),
            error: None,
        })
    }

    fn observe(&mut self, state: &State, command: Option<&Command>) {
        if self.error.is_some() {
            return;
        }
        let mut completed_turn = false;
        if let Some(command) = command {
            match serde_json::to_value(command) {
                Ok(value) => self.commands.push(value),
                Err(error) => {
                    self.error = Some(error.into());
                    return;
                }
            }
            completed_turn = matches!(command, Command::EndTurn { .. });
            if !completed_turn && matches!(state.match_state, Match::Active { .. }) {
                return;
            }
        }
        if let Err(error) = self.write_frame(state, completed_turn) {
            self.error = Some(error);
        }
    }

    fn write_frame(&mut self, state: &State, completed_turn: bool) -> Result<()> {
        let name = if self.frame == 0 {
            "frame-0000-start.png".to_owned()
        } else if matches!(state.match_state, Match::Finished { .. }) {
            format!("frame-{:04}-final.png", self.frame)
        } else {
            format!("frame-{:04}-turn.png", self.frame)
        };
        render_state(&self.map, state, &self.seats, &self.tilesets)
            .save(self.directory.join(&name))
            .with_context(|| format!("writing sample frame {name}"))?;
        serde_json::to_writer(
            &mut self.log,
            &serde_json::json!({
                "frame": self.frame,
                "image": name,
                "completed_turn": completed_turn.then_some(self.frame),
                "terminal": matches!(state.match_state, Match::Finished { .. }),
                "commands": &self.commands,
                "state": state,
            }),
        )?;
        self.log.write_all(b"\n")?;
        self.commands.clear();
        self.frame += 1;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        if let Some(error) = self.error.take() {
            return Err(error).context("capturing arena sample");
        }
        self.log.flush().context("flushing arena sample log")?;
        eprintln!(
            "wrote {} frames and log.jsonl to {}",
            self.frame,
            self.directory.display()
        );
        Ok(())
    }
}

/// Add one game to the tally, from the point of view of the agent under test.
///
/// `seat` is the seat the agent under test sat in, which changes with the seat
/// order and is what keeps the shape of the game on the right side.
///
/// Answers the points the agent under test took, which the pair it belongs to
/// sums.
fn score(tally: &mut Tally, record: &Record, team: &TeamId, seat: usize) -> f64 {
    tally.commands += record.commands;
    tally.refusals += record.refusals;
    tally.days += u64::from(record.days);
    tally.shortest = if tally.shortest == 0 {
        record.days
    } else {
        tally.shortest.min(record.days)
    };
    tally.longest = tally.longest.max(record.days);
    if let Some(shape) = record.shape.seats.get(seat) {
        tally.under_test.add(shape);
    }
    if let Some(shape) = record.shape.seats.get(seat ^ 1) {
        tally.opponent.add(shape);
    }
    match &record.outcome {
        Some(Outcome::Victory { winners, reason }) => {
            match reason {
                VictoryReason::HqCapture => tally.endings.hq_capture += 1,
                VictoryReason::Rout => tally.endings.rout += 1,
                VictoryReason::DayLimit => tally.endings.day_limit += 1,
                _ => tally.endings.other += 1,
            }
            if winners.contains(team) {
                tally.wins += 1;
                1.0
            } else {
                tally.losses += 1;
                0.0
            }
        }
        Some(_) => {
            tally.endings.drawn += 1;
            tally.draws += 1;
            0.5
        }
        None => {
            tally.abandoned += 1;
            tally.draws += 1;
            0.5
        }
    }
}

/// A 95% percentile-bootstrap interval over independent pair scores.
///
/// Each observation is the bounded score from a seat-swapped pair and can be
/// 0, 0.25, 0.5, 0.75, or 1. Resampling those observations supports the
/// fractional outcomes that a binomial interval does not. A fixed seed keeps
/// repeated reports identical.
fn paired_interval(scores: &[f64]) -> (f64, f64) {
    const RESAMPLES: usize = 10_000;
    assert!(!scores.is_empty(), "an interval needs at least one pair");

    let mut rng = Rng::from_seed(0x7061_6972_2d63_6921);
    let mut means = Vec::with_capacity(RESAMPLES);
    for _ in 0..RESAMPLES {
        let sum = (0..scores.len())
            .map(|_| scores[rng.below(scores.len() as u64) as usize])
            .sum::<f64>();
        means.push(sum / scores.len() as f64);
    }
    means.sort_unstable_by(f64::total_cmp);
    let percentile = |numerator: usize| means[(RESAMPLES - 1) * numerator / 1_000];
    (percentile(25), percentile(975))
}

/// The rating difference a score implies.
///
/// `None` at a score of zero or one, where the difference is not finite: a
/// clean sweep says the gap is at least large, not that it is infinite.
fn elo(score: f64) -> Option<f64> {
    (score > 0.0 && score < 1.0).then(|| -400.0 * (1.0 / score - 1.0).log10())
}

fn report(options: &Options, tally: &Tally, elapsed: f64) {
    let games = tally.games();
    let score = tally.score();
    let (low, high) = paired_interval(&tally.pair_scores);

    println!(
        "{} vs {}   seed {}  fog {}  day cap {}",
        options.first, options.second, options.seed, options.fog, options.day_cap
    );
    if let Some(path) = &options.weights {
        println!("first-agent weights     {}", path.display());
    }
    if let Some(path) = &options.second_weights {
        println!("second-agent weights    {}", path.display());
    }
    println!("{} pairs, {games} games, both seat orders", options.pairs);
    println!();
    println!(
        "wins {}  losses {}  draws {}",
        tally.wins, tally.losses, tally.draws
    );
    println!("score                    {score:.4}  ({low:.4} to {high:.4}, paired bootstrap 95%)");
    println!(
        "pairs swept {}  split {}  lost {}",
        tally.swept, tally.split, tally.lost_both
    );
    match elo(score) {
        Some(elo) => println!("elo difference           {elo:+.0}"),
        None => println!(
            "elo difference           not finite at a score of {score:.0}; play more games or a stronger opponent"
        ),
    }
    println!();
    println!("elapsed                  {elapsed:.3} s");
    println!(
        "commands each second     {:.1}",
        tally.commands as f64 / elapsed
    );
    println!(
        "refused offers           {} ({:.2}% of nodes)",
        tally.refusals,
        tally.refusals as f64 / (tally.commands + tally.refusals) as f64 * 100.0
    );

    println!();
    shape_report(tally);

    if tally.abandoned > 0 {
        let share = f64::from(tally.abandoned) / f64::from(games) * 100.0;
        println!();
        println!(
            "{} of {games} games ({share:.1}%) stopped with no outcome, which the day\n\
             limit should have decided. That is a defect, not a result.",
            tally.abandoned
        );
    }
}

/// What the games were made of, beside who won them.
///
/// A standard tier 1 game ends by headquarters capture on day six with one
/// unit dead, so a term that prices combat reads zero there whatever it is
/// worth. That is a property of the games and not of the term, and it is only
/// visible in these numbers. Read the days and the losses first.
fn shape_report(tally: &Tally) {
    let games = f64::from(tally.games().max(1));
    let endings = &tally.endings;
    println!("game shape");
    println!(
        "  days each game         {:.1}  ({} to {})",
        tally.mean_days(),
        tally.shortest,
        tally.longest
    );
    println!(
        "  ended by               hq capture {}  rout {}  day limit {}  other {}  drawn {}",
        endings.hq_capture, endings.rout, endings.day_limit, endings.other, endings.drawn
    );
    println!(
        "  units lost each game   {:.1}  over both sides",
        (tally.under_test.lost + tally.opponent.lost) as f64 / games
    );

    let under_test = &tally.under_test;
    let opponent = &tally.opponent;
    println!();
    println!("                                  under test    opponent");
    row(
        "units built each game",
        under_test.each_game(under_test.built as f64),
        opponent.each_game(opponent.built as f64),
    );
    row(
        "units lost each game",
        under_test.each_game(under_test.lost as f64),
        opponent.each_game(opponent.lost as f64),
    );
    row(
        "units each turn",
        under_test.each_turn(under_test.units),
        opponent.each_turn(opponent.units),
    );
    row(
        "unit value each turn",
        under_test.each_turn(under_test.value),
        opponent.each_turn(opponent.value),
    );
    row(
        "income each turn",
        under_test.each_turn(under_test.income),
        opponent.each_turn(opponent.income),
    );
    row(
        "units, last turn",
        under_test.each_game(under_test.last_units as f64),
        opponent.each_game(opponent.last_units as f64),
    );
    row(
        "unit value, last turn",
        under_test.each_game(under_test.last_value),
        opponent.each_game(opponent.last_value),
    );
    row(
        "income, last turn",
        under_test.each_game(under_test.last_income as f64),
        opponent.each_game(opponent.last_income as f64),
    );
}

/// One line of the shape table, both sides of the board.
fn row(name: &str, under_test: f64, opponent: f64) {
    println!("  {name:<30}  {under_test:>9.1}   {opponent:>9.1}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weights_file_can_override_one_default() {
        let weights: Weights =
            serde_json::from_str(r#"{"threat":0.125}"#).expect("the weights parse");
        assert_eq!(weights.threat, 0.125);
        assert_eq!(weights.hq, Weights::DEFAULT.hq);
        assert_eq!(weights.deny, Weights::DEFAULT.deny);
    }

    #[test]
    fn a_weights_file_rejects_an_unknown_name() {
        let error = serde_json::from_str::<Weights>(r#"{"thret":0.125}"#)
            .expect_err("an unknown weight is an error");
        assert!(error.to_string().contains("unknown field `thret`"));
    }

    #[test]
    fn weights_need_greedy_agents() {
        let error = Options::parse(["--weights", "weights.json"].map(str::to_owned).into_iter())
            .expect_err("random does not use weights");
        assert_eq!(error, "--weights requires a greedy first agent");

        let error = Options::parse(
            ["--second-weights", "weights.json"]
                .map(str::to_owned)
                .into_iter(),
        )
        .expect_err("random does not use second weights");
        assert_eq!(error, "--second-weights requires a greedy second agent");

        let options = Options::parse(
            [
                "--first",
                "deny",
                "--weights",
                "first.json",
                "--second",
                "threat",
                "--second-weights",
                "second.json",
            ]
            .map(str::to_owned)
            .into_iter(),
        )
        .expect("greedy agents use weights");
        assert_eq!(options.weights, Some(PathBuf::from("first.json")));
        assert_eq!(options.second_weights, Some(PathBuf::from("second.json")));
    }

    #[test]
    fn the_paired_interval_supports_fractional_scores() {
        let scores = [0.0, 0.25, 0.5, 0.75, 1.0];
        let (low, high) = paired_interval(&scores);
        assert!(low < 0.5 && 0.5 < high);
        assert!(0.0 <= low && high <= 1.0);

        // Fractional observations are values in their own right. They are not
        // randomly rounded to wins and losses before the interval is built.
        assert_eq!(paired_interval(&[0.5; 20]), (0.5, 0.5));
    }

    #[test]
    fn the_interval_counts_pairs_and_is_wider_than_it_was() {
        // Treating the games as independent duplicates each pair score and
        // makes the interval too narrow.
        let pairs = [0.0, 1.0].repeat(50);
        let games = pairs
            .iter()
            .flat_map(|score| [*score, *score])
            .collect::<Vec<_>>();
        let over_pairs = paired_interval(&pairs);
        let over_games = paired_interval(&games);
        let width = |(low, high): (f64, f64)| high - low;
        assert!(width(over_pairs) > width(over_games));
    }

    #[test]
    fn a_pair_is_one_observation_however_its_games_went() {
        let mut tally = Tally::default();
        tally.add_pair(1.0);
        tally.add_pair(0.5);
        tally.add_pair(0.0);
        // A pair with one drawn game is neither swept nor lost.
        tally.add_pair(0.75);
        assert_eq!(tally.pairs, 4);
        assert!((tally.pair_points - 2.25).abs() < 1e-9);
        assert_eq!(tally.pair_scores, [1.0, 0.5, 0.0, 0.75]);
        assert_eq!((tally.swept, tally.split, tally.lost_both), (1, 2, 1));
    }

    #[test]
    fn elo_reads_the_way_a_rating_does() {
        assert!(elo(0.5).expect("an even score is finite").abs() < 1e-9);
        // The definition: 400 points is a score of about ten to one.
        assert!((elo(10.0 / 11.0).expect("finite") - 400.0).abs() < 1.0);
        assert_eq!(elo(-0.0), None);
        assert_eq!(elo(1.0), None);
    }

    #[test]
    fn a_swept_tournament_scores_one() {
        let mut tally = Tally {
            wins: 10,
            ..Tally::default()
        };
        assert!((tally.score() - 1.0).abs() < 1e-9);
        tally.draws = 10;
        assert!((tally.score() - 0.75).abs() < 1e-9);
    }
}
