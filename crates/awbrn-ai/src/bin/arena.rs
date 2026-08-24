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
//!
//! A weighting that wins does not have to be compiled in to play again.
//! `--freeze` writes it into the ladder directory, a seat names it by its file
//! name from then on, and `--round-robin` plays the whole ladder against the
//! weightings this crate holds. Tuning is then a loop of sweep, freeze and
//! round, and only a term that needs code needs a rebuild.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
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
    if let Err(error) = dispatch(&options, start) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

/// Run the mode the arguments name.
///
/// There are three: freeze one weighting into the ladder, play the ladder
/// against itself, or play two seats. Each of them reads the ladder first,
/// because a seat can name what is in it.
fn dispatch(options: &Options, start: Instant) -> Result<()> {
    let ladder = Ladder::load(&options.ladder)?;

    if let Some(name) = &options.freeze {
        let weights = ladder
            .seat(&options.first, options.weights.as_deref())?
            .context("the random agent holds no weights to freeze")?;
        let path = ladder.freeze(name, &weights)?;
        println!("wrote {}", path.display());
        println!("seat it by name: arena --first {name} --second defend");
        return Ok(());
    }

    if options.round_robin {
        let round = round_robin(options, &ladder)?;
        report_round(options, &ladder, &round, start.elapsed().as_secs_f64());
        return Ok(());
    }

    let first = ladder.seat(&options.first, options.weights.as_deref())?;
    let second = ladder.seat(&options.second, options.second_weights.as_deref())?;
    let tally = run(options, first, second, options.sample.as_deref())?;
    report(options, &tally, start.elapsed().as_secs_f64());
    Ok(())
}

const USAGE: &str = "\
usage: arena [--map NAME] [--seed N] [--games N] [--fog] [--day-cap N] [--first NAME] [--second NAME] [--weights FILE] [--second-weights FILE] [--sample DIR] [--ladder DIR] [--round-robin] [--roster NAMES] [--freeze NAME]

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
  --ladder DIR   Where the weightings that are not compiled in live. One JSON
                 file for each, named by the file. Default the crate's ladder
                 directory.
  --round-robin  Play every contender against every other, both seat orders,
                 and report the cross table. The field is the ladder and the
                 built-in weightings unless --roster names it.
  --roster NAMES A comma-separated field for --round-robin, in place of the
                 whole ladder: --roster defend,counter,my-champion.
  --freeze NAME  Write what --first and --weights resolve to into the ladder
                 as NAME, and play nothing. This is how a sweep winner joins
                 later rounds without a rebuild.

agents: random, one of the weightings this crate names, a name the ladder
holds, or a path to a JSON weights file. Each built-in weighting adds one term
to the one before it, so an adjacent pair is the measurement of that term and
nothing else. A ladder file names the fields it moves and, in
its base field, the weighting it moves them from.";

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
    ladder: PathBuf,
    round_robin: bool,
    /// The field of a round robin, or empty for the whole ladder.
    roster: Vec<String>,
    /// The name to write the first agent's weighting into the ladder under.
    freeze: Option<String>,
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
            ladder: PathBuf::from(DEFAULT_LADDER),
            round_robin: false,
            roster: Vec::new(),
            freeze: None,
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
                "--ladder" => options.ladder = value()?.into(),
                "--round-robin" => options.round_robin = true,
                "--roster" => options.roster = roster(&value()?)?,
                "--freeze" => options.freeze = Some(value()?),
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
        if !options.roster.is_empty() && !options.round_robin {
            return Err("--roster names the field of a --round-robin run".to_owned());
        }
        if options.round_robin && options.sample.is_some() {
            return Err(
                "--sample captures one game, which a --round-robin run does not have".to_owned(),
            );
        }
        if options.round_robin && options.freeze.is_some() {
            return Err("--freeze writes a weighting and plays nothing".to_owned());
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

/// Where the weightings that are not compiled in live by default.
const DEFAULT_LADDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ladder");

/// What one seat plays, by the word the arguments use.
///
/// There are two agents and not five. Every greedy doctrine this crate has
/// ever seated is a weighting of the one greedy agent, so a seat names
/// `random`, one of [`Weights::PRESETS`], a name the ladder holds, or a file
/// of weights. A new term then needs a weight and no agent at all, which is
/// what stops the ladder from growing a name for each of them.
///
/// The name is only kept here. What it means is decided in [`Ladder::seat`],
/// after the arguments have said where the ladder is.
fn agent_spec(name: &str) -> Result<String, String> {
    if name.is_empty() {
        return Err("an agent needs a name".to_owned());
    }
    Ok(name.to_owned())
}

/// The field of a round robin, from one comma-separated word.
fn roster(names: &str) -> Result<Vec<String>, String> {
    let names: Vec<String> = names
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    if names.len() < 2 {
        return Err("--roster needs at least two contenders".to_owned());
    }
    Ok(names)
}

/// The weightings that live outside the binary.
///
/// A weighting that wins its round joins the ladder by being written into
/// this directory, and every later round seats it by name. Nothing in here is
/// compiled in, which is the point: the field grows without a rebuild, and
/// the built-in weightings stay the fixed rungs the handoff note measures
/// against.
struct Ladder {
    directory: PathBuf,
    /// Sorted by name, so a round robin plays the same order every run.
    entries: BTreeMap<String, Weights>,
}

impl Ladder {
    /// Read every weighting in `directory`.
    ///
    /// A directory that is not there is an empty ladder and not an error: the
    /// built-in weightings are still a field.
    fn load(directory: &Path) -> Result<Self> {
        let mut entries = BTreeMap::new();
        let listing = match std::fs::read_dir(directory) {
            Ok(listing) => listing,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    directory: directory.to_owned(),
                    entries,
                });
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("reading the ladder at {}", directory.display()));
            }
        };
        for entry in listing {
            let path = entry
                .with_context(|| format!("reading the ladder at {}", directory.display()))?
                .path();
            if path.extension().and_then(OsStr::to_str) != Some("json") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(OsStr::to_str)
                .with_context(|| format!("{} has no name a seat can hold", path.display()))?
                .to_owned();
            if name == RANDOM || Weights::preset(&name).is_some() {
                bail!(
                    "ladder file {} takes a name this crate already holds; rename it",
                    path.display()
                );
            }
            entries.insert(name, read_entry(&path)?);
        }
        Ok(Self {
            directory: directory.to_owned(),
            entries,
        })
    }

    /// The weighting one seat plays, or `None` for the random agent.
    ///
    /// `overrides` is layered over the named weighting rather than over the
    /// defaults, so `--first defend --weights sweep/hold-0.4.json` is the
    /// defend weighting with one field moved. A field the file does not name
    /// keeps what the named weighting gives it, and a name nothing holds is
    /// an error.
    fn seat(&self, spec: &str, overrides: Option<&Path>) -> Result<Option<Weights>> {
        if spec == RANDOM {
            return Ok(None);
        }
        let base = if let Some(weights) = Weights::preset(spec) {
            weights
        } else if let Some(weights) = self.entries.get(spec) {
            *weights
        } else if is_path(spec) {
            read_entry(Path::new(spec))?
        } else {
            bail!(
                "unknown agent {spec}, known agents are {RANDOM}, the weightings {}, \
                 the ladder at {} ({}), or a path to a weights file",
                Weights::preset_names(),
                self.directory.display(),
                self.names()
            );
        };
        match overrides {
            Some(path) => layer_weights(base, path).map(Some),
            None => Ok(Some(base)),
        }
    }

    /// The ladder names, for a message that has to list them.
    fn names(&self) -> String {
        if self.entries.is_empty() {
            return "empty".to_owned();
        }
        self.entries
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Every contender: the built-in weightings, then the ladder.
    ///
    /// The random agent is not one. A round robin measures weightings against
    /// each other, and a seat that plays at random only widens the table.
    fn field(&self) -> Vec<String> {
        Weights::PRESETS
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .chain(self.entries.keys().cloned())
            .collect()
    }

    /// Write one weighting into the ladder under `name`.
    ///
    /// The file holds every field, because a contender has to mean the same
    /// thing after the weighting it was swept from moves. It refuses to
    /// overwrite: a name that already stands has results measured against it.
    fn freeze(&self, name: &str, weights: &Weights) -> Result<PathBuf> {
        freezable(name)?;
        let path = self.directory.join(format!("{name}.json"));
        if path.exists() {
            bail!(
                "{} already stands; delete it or choose another name",
                path.display()
            );
        }
        std::fs::create_dir_all(&self.directory)
            .with_context(|| format!("creating the ladder at {}", self.directory.display()))?;
        let mut text = serde_json::to_string_pretty(weights).context("writing the weighting")?;
        text.push('\n');
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }
}

/// Whether this word is meant as a file and not as a name.
///
/// A name this crate holds is one word with no punctuation in it, so anything
/// that looks like a path is read as one. Saying so here is what lets a
/// misspelled preset report the names instead of an unreadable file.
fn is_path(name: &str) -> bool {
    name.contains(std::path::MAIN_SEPARATOR) || name.ends_with(".json")
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

fn run(
    options: &Options,
    first_weights: Option<Weights>,
    second_weights: Option<Weights>,
    sample: Option<&Path>,
) -> Result<Tally> {
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
                if let Some(directory) = sample {
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
/// The file holds the fields it moves and nothing else, so it is laid over
/// the named weighting rather than read into [`Weights`] directly, which
/// would fill every field it does not name from the defaults and quietly
/// throw the named weighting away.
fn layer_weights(base: Weights, path: &Path) -> Result<Weights> {
    let fields = read_fields(path)?;
    if fields.contains_key(BASE) {
        bail!(
            "{} names a base weighting, which --first already names",
            path.display()
        );
    }
    merge(base, fields).with_context(|| format!("applying weights file {}", path.display()))
}

/// The field that names what a weights file moves its weights from.
const BASE: &str = "base";

/// Read one weighting a seat can play on its own.
///
/// The file names the fields it moves, and in `base` the weighting it moves
/// them from. A file without a base moves from the defaults, so a contender
/// frozen out of a sweep holds every field and means the same thing after the
/// weighting it came from moves.
fn read_entry(path: &Path) -> Result<Weights> {
    let mut fields = read_fields(path)?;
    let base = match fields.remove(BASE) {
        None => Weights::DEFAULT,
        Some(serde_json::Value::String(name)) => Weights::preset(&name).with_context(|| {
            format!(
                "{} takes the base {name}, which is not one of {}",
                path.display(),
                Weights::preset_names()
            )
        })?,
        Some(other) => bail!("{}: base names a weighting, not {other}", path.display()),
    };
    merge(base, fields).with_context(|| format!("reading weights file {}", path.display()))
}

fn read_fields(path: &Path) -> Result<serde_json::Map<String, serde_json::Value>> {
    let file =
        File::open(path).with_context(|| format!("opening weights file {}", path.display()))?;
    serde_json::from_reader(file)
        .with_context(|| format!("reading weights file {}", path.display()))
}

/// Lay named fields over a weighting.
///
/// `Weights` refuses a name it does not hold, so a misspelled weight is an
/// error here rather than a sweep that measured nothing.
fn merge(base: Weights, fields: serde_json::Map<String, serde_json::Value>) -> Result<Weights> {
    let mut merged = serde_json::to_value(base).context("writing the named weighting out")?;
    let Some(written) = merged.as_object_mut() else {
        unreachable!("weights write out as an object");
    };
    written.extend(fields);
    Ok(serde_json::from_value(merged)?)
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

/// One matchup, from the point of view of the contender in the row.
#[derive(Clone, Copy)]
struct Cell {
    score: f64,
    low: f64,
    high: f64,
}

impl Cell {
    /// The same matchup seen from the other side.
    const fn mirrored(self) -> Self {
        Self {
            score: 1.0 - self.score,
            low: 1.0 - self.high,
            high: 1.0 - self.low,
        }
    }

    /// Whether this contender beat the other one and the interval agrees.
    ///
    /// A score over a half that an interval straddling a half comes with is
    /// a run that measured nothing, so it does not count as a win here.
    const fn won(self) -> bool {
        self.low > 0.5
    }
}

/// Every contender against every other, both seat orders.
///
/// This is what answers the question a two-agent run cannot: a weighting that
/// beats the one it was swept from can still lose to the rung below it, and
/// only a field shows that.
struct Round {
    names: Vec<String>,
    /// `cells[row][column]`, and `None` where a contender meets itself.
    cells: Vec<Vec<Option<Cell>>>,
    games: u32,
}

impl Round {
    /// The mean score of one contender over the field.
    ///
    /// Each matchup weighs the same however many games it held, because every
    /// matchup in a round holds the same number.
    fn score(&self, row: usize) -> f64 {
        let played: Vec<f64> = self.cells[row]
            .iter()
            .flatten()
            .map(|cell| cell.score)
            .collect();
        played.iter().sum::<f64>() / played.len().max(1) as f64
    }

    /// Whether this contender beat every other one it played.
    fn swept(&self, row: usize) -> bool {
        self.cells[row].iter().flatten().all(|cell| cell.won())
    }

    /// The rows, strongest first.
    fn ranking(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.names.len()).collect();
        order.sort_by(|left, right| self.score(*right).total_cmp(&self.score(*left)));
        order
    }
}

/// Whether a contender can take this name.
///
/// A name is what a seat says and what the file is called, so it holds to
/// what both can take: a word this crate does not already hold, and not a
/// path.
fn freezable(name: &str) -> Result<()> {
    if name == RANDOM || Weights::preset(name).is_some() {
        bail!("{name} is a name this crate already holds; choose another");
    }
    let word = |letter: char| letter.is_ascii_alphanumeric() || matches!(letter, '-' | '_' | '.');
    if name.is_empty() || name.ends_with(".json") || !name.chars().all(word) {
        bail!("{name} is not a name a ladder file can take; use letters, digits, dashes and dots");
    }
    Ok(())
}

fn round_robin(options: &Options, ladder: &Ladder) -> Result<Round> {
    let names = if options.roster.is_empty() {
        ladder.field()
    } else {
        options.roster.clone()
    };
    if names.len() < 2 {
        bail!(
            "a round needs at least two contenders, and the field is {}",
            names.join(", ")
        );
    }
    let field: Vec<Option<Weights>> = names
        .iter()
        .map(|name| ladder.seat(name, None))
        .collect::<Result<_>>()?;

    let mut round = Round {
        cells: vec![vec![None; names.len()]; names.len()],
        games: 0,
        names,
    };
    let matchups = round.names.len() * (round.names.len() - 1) / 2;
    let mut played = 0;
    for row in 0..round.names.len() {
        for column in (row + 1)..round.names.len() {
            played += 1;
            eprintln!(
                "matchup {played} of {matchups}: {} vs {}",
                round.names[row], round.names[column]
            );
            let tally = run(options, field[row], field[column], None)?;
            let (low, high) = paired_interval(&tally.pair_scores);
            let cell = Cell {
                score: tally.score(),
                low,
                high,
            };
            round.cells[row][column] = Some(cell);
            round.cells[column][row] = Some(cell.mirrored());
            round.games += tally.games();
        }
    }
    Ok(round)
}

fn report_round(options: &Options, ladder: &Ladder, round: &Round, elapsed: f64) {
    let order = round.ranking();
    // Room for the rank, the dot and the longest name.
    let width = round.names.iter().map(String::len).max().unwrap_or(0) + 5;

    println!(
        "ladder round robin   map {}  seed {}  fog {}  day cap {}",
        options.map, options.seed, options.fog, options.day_cap
    );
    println!("ladder               {}", ladder.directory.display());
    println!(
        "{} contenders, {} games, {} pairs in each matchup",
        round.names.len(),
        round.games,
        options.pairs
    );
    println!();

    print!("{:width$}", "");
    for column in 1..=order.len() {
        print!("{:>7}", format!("({column})"));
    }
    println!("{:>9}{:>7}", "score", "elo");

    for (rank, row) in order.iter().enumerate() {
        print!("{:<width$}", format!("{}. {}", rank + 1, round.names[*row]));
        for column in &order {
            match round.cells[*row][*column] {
                Some(cell) => print!("{:>7.3}", cell.score),
                None => print!("{:>7}", "-"),
            }
        }
        let score = round.score(*row);
        match elo(score) {
            Some(elo) => println!("{score:>9.4}{elo:>+7.0}"),
            None => println!("{score:>9.4}{:>7}", "-"),
        }
    }
    println!();
    println!("A cell is the row's score against the column, over both seat orders.");

    let champions: Vec<usize> = order
        .iter()
        .copied()
        .filter(|row| round.swept(*row))
        .collect();
    match champions.as_slice() {
        [] => {
            println!("No contender beat every other one by more than its interval,");
            println!("so this round names no champion.");
        }
        [champion] => {
            let name = &round.names[*champion];
            println!("{name} beat every other contender.");
            if !ladder.entries.contains_key(name) && Weights::preset(name).is_none() {
                println!("Freeze it into the ladder to seat it in later rounds:");
                println!("  arena --first {name} --freeze NAME");
            }
        }
        several => {
            let names = several
                .iter()
                .map(|row| round.names[*row].as_str())
                .collect::<Vec<_>>()
                .join(" and ");
            println!("{names} each beat every other contender they played, which this");
            println!("round cannot rank between. Play them at more pairs.");
        }
    }
    println!();
    println!("elapsed              {elapsed:.3} s");
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

    /// A ladder with the entries named, and no directory behind it.
    fn ladder(entries: &[(&str, Weights)]) -> Ladder {
        Ladder {
            directory: PathBuf::from("ladder"),
            entries: entries
                .iter()
                .map(|(name, weights)| ((*name).to_owned(), *weights))
                .collect(),
        }
    }

    #[test]
    fn a_seat_names_a_weighting_the_ladder_holds() {
        let champion = Weights {
            hold: 12.5,
            ..Weights::DEFEND
        };
        let ladder = ladder(&[("champion", champion)]);

        let seated = ladder
            .seat("champion", None)
            .expect("the ladder holds it")
            .expect("it is not the random agent");
        assert_eq!(seated.hold, 12.5);

        // A built-in weighting still wins the name, and the random agent
        // still reads no weights at all.
        assert_eq!(
            ladder
                .seat("defend", None)
                .expect("a preset")
                .expect("not random")
                .hold,
            Weights::DEFEND.hold
        );
        assert!(ladder.seat(RANDOM, None).expect("random").is_none());
    }

    #[test]
    fn an_unknown_seat_reports_the_whole_field() {
        let error = ladder(&[("champion", Weights::DEFEND)])
            .seat("champoin", None)
            .expect_err("a misspelled name is an error")
            .to_string();
        assert!(error.contains("defend"), "{error}");
        assert!(error.contains("champion"), "{error}");
    }

    #[test]
    fn the_field_is_the_weightings_and_then_the_ladder() {
        let field = ladder(&[("champion", Weights::DEFEND)]).field();
        assert_eq!(field.len(), Weights::PRESETS.len() + 1);
        assert_eq!(field.last().expect("the ladder is last"), "champion");
        assert!(!field.iter().any(|name| name == RANDOM));
    }

    #[test]
    fn a_ladder_file_moves_its_weights_from_the_base_it_names() {
        let fields = |text: &str| serde_json::from_str(text).expect("the file parses");
        let merged = merge(Weights::DEFEND, fields(r#"{"hold":0.4}"#)).expect("the weights merge");
        assert_eq!(merged.hold, 0.4);
        // Every field the file does not name is the base's, not the default's.
        assert_eq!(merged.deny, Weights::DEFEND.deny);
        assert_eq!(merged.threat, Weights::DEFEND.threat);
    }

    #[test]
    fn a_frozen_name_cannot_shadow_a_weighting_this_crate_holds() {
        for name in [
            "defend",
            RANDOM,
            "sweep/champion",
            "champion.json",
            "a b",
            "",
        ] {
            freezable(name).expect_err("the name is not one a ladder file can take");
        }
        // A name a sweep gives a weighting is one of these, dot and all.
        freezable("hold-0.4").expect("a sweep names a weighting like this");
    }

    #[test]
    fn a_roster_needs_a_field_to_play() {
        assert!(roster("defend").is_err());
        assert_eq!(
            roster(" defend , counter ,").expect("two contenders"),
            ["defend", "counter"]
        );
        let error = Options::parse(
            ["--roster", "defend,counter"]
                .map(str::to_owned)
                .into_iter(),
        )
        .expect_err("a roster without a round is an error");
        assert_eq!(error, "--roster names the field of a --round-robin run");
    }

    #[test]
    fn a_matchup_reads_the_same_from_either_side() {
        let cell = Cell {
            score: 0.62,
            low: 0.55,
            high: 0.69,
        };
        let mirrored = cell.mirrored();
        assert!((mirrored.score - 0.38).abs() < 1e-9);
        assert!((mirrored.low - 0.31).abs() < 1e-9);
        assert!((mirrored.high - 0.45).abs() < 1e-9);
        assert!(cell.won());
        assert!(!mirrored.won());
        // A score over a half the interval does not support is not a win.
        assert!(
            !Cell {
                score: 0.52,
                low: 0.48,
                high: 0.56
            }
            .won()
        );
    }

    #[test]
    fn a_round_ranks_by_the_score_over_the_field() {
        let cell = |score: f64| {
            Some(Cell {
                score,
                low: score - 0.02,
                high: score + 0.02,
            })
        };
        let round = Round {
            names: ["weak", "strong", "middling"].map(str::to_owned).to_vec(),
            cells: vec![
                vec![None, cell(0.2), cell(0.4)],
                vec![cell(0.8), None, cell(0.7)],
                vec![cell(0.6), cell(0.3), None],
            ],
            games: 600,
        };
        assert_eq!(round.ranking(), [1, 2, 0]);
        assert!((round.score(1) - 0.75).abs() < 1e-9);
        assert!(round.swept(1));
        assert!(!round.swept(2));
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
