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

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use awbrn_ai::agent::Agent;
use awbrn_ai::agents::{GreedyAgent, RandomAgent, Weights};
use awbrn_ai::board::{SEATS, arena, arena_map};
use awbrn_ai::harness::{Limits, Record, play, play_observed};
use awbrn_ai::rng::Rng;
use awbrn_image::{Tilesets, render_state};
use awbrn_map::AwbrnMap;
use awvm::semantic::{Match, Outcome, State, TeamId};
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
usage: arena [--seed N] [--games N] [--fog] [--day-cap N] [--first NAME] [--second NAME] [--sample DIR]

  --seed N       Seed for the tournament. The same seed gives the same result.
                 Default 1.
  --games N      Game pairs to play. Each pair is the same seed played with
                 both seat orders, so the tournament plays 2N games. Default 50.
  --fog          Play with fog of war on. Default off.
  --day-cap N    Abandon a game after this many days. Default 35.
  --first NAME   The agent under test. Default random.
  --second NAME  The agent it plays. Default random.
  --sample DIR   Capture the first game as turn PNGs and a JSONL sidecar.

agents: random, greedy, greedy-threat, greedy-deny";

struct Options {
    seed: u64,
    pairs: usize,
    fog: bool,
    day_cap: u32,
    first: &'static str,
    second: &'static str,
    sample: Option<PathBuf>,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            seed: 1,
            pairs: 50,
            fog: false,
            day_cap: Limits::default().days,
            first: "random",
            second: "random",
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
                "--seed" => options.seed = parse_number(&value()?)?,
                "--games" => options.pairs = parse_number(&value()?)?,
                "--day-cap" => options.day_cap = parse_number(&value()?)?,
                "--fog" => options.fog = true,
                "--first" => options.first = agent_name(&value()?)?,
                "--second" => options.second = agent_name(&value()?)?,
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
        Ok(options)
    }

    const fn limits(&self) -> Limits {
        Limits {
            days: self.day_cap,
            ..Limits::DEFAULT
        }
    }
}

fn parse_number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{text} is not a number this argument accepts"))
}

/// The agents this binary can seat, by the name the arguments use.
///
/// Each name adds one term to the one before it, so any adjacent pair is the
/// measurement of that term and nothing else. `greedy` is tier 1 as it
/// landed, `greedy-threat` adds the threat map, and `greedy-deny` adds the
/// price of stopping an enemy capture.
const AGENTS: [&str; 4] = ["random", "greedy", "greedy-threat", "greedy-deny"];

fn agent_name(name: &str) -> Result<&'static str, String> {
    AGENTS
        .into_iter()
        .find(|known| *known == name)
        .ok_or_else(|| {
            format!(
                "unknown agent {name}, known agents are {}",
                AGENTS.join(", ")
            )
        })
}

fn build(name: &str, seed: u64) -> Box<dyn Agent> {
    match name {
        "random" => Box::new(RandomAgent::from_seed(seed)),
        "greedy" => Box::new(GreedyAgent::with_weights(seed, Weights::THREATLESS)),
        "greedy-threat" => Box::new(GreedyAgent::with_weights(seed, Weights::WITHOUT_DENIAL)),
        "greedy-deny" => Box::new(GreedyAgent::from_seed(seed)),
        other => unreachable!("{other} passed the argument check"),
    }
}

#[derive(Default)]
struct Tally {
    /// Games the agent named by `--first` won.
    wins: u32,
    losses: u32,
    draws: u32,
    /// Games that reached the day cap with no winner.
    ///
    /// These count as draws in the score. A tournament in which most games are
    /// abandoned measures the cap and not the agents, so the report says so.
    abandoned: u32,
    commands: u64,
    refusals: u64,
}

impl Tally {
    const fn games(&self) -> u32 {
        self.wins + self.losses + self.draws
    }

    /// The score of the agent under test, a draw counting a half.
    fn score(&self) -> f64 {
        (f64::from(self.wins) + f64::from(self.draws) / 2.0) / f64::from(self.games())
    }
}

fn run(options: &Options) -> Result<Tally> {
    let mut tally = Tally::default();
    // One session for the whole tournament. It keeps the board-sized tables it
    // allocated, so a game after the first asks the allocator for nothing.
    let mut session = Session::new(arena(options.fog, options.seed));

    for pair in 0..options.pairs {
        for under_test_first in [true, false] {
            let game = Rng::mix(options.seed ^ ((pair as u64) << 32));
            let mut entropy = Rng::from_seed(Rng::mix(game ^ 0x1));
            let mut first = build(options.first, Rng::mix(game ^ 0x2));
            let mut second = build(options.second, Rng::mix(game ^ 0x3));

            // The seat the agent under test sits in. Playing the same seed
            // both ways is what removes the first-player advantage from the
            // score rather than averaging over it.
            let seat = usize::from(!under_test_first);
            let mut agents: [&mut dyn Agent; 2] = if under_test_first {
                [first.as_mut(), second.as_mut()]
            } else {
                [second.as_mut(), first.as_mut()]
            };

            let state = arena(options.fog, game);
            let teams: Vec<TeamId> = state
                .players
                .seats()
                .map(|(_, player)| player.team.clone())
                .collect();

            let record = if pair == 0 && under_test_first {
                if let Some(directory) = &options.sample {
                    let mut sample = Sample::new(directory, arena_map(), options, game)?;
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
                    play(
                        state,
                        &mut session,
                        &mut agents,
                        &mut entropy,
                        options.limits(),
                    )
                }
            } else {
                play(
                    state,
                    &mut session,
                    &mut agents,
                    &mut entropy,
                    options.limits(),
                )
            };
            score(&mut tally, &record, &teams[seat]);
        }
    }

    Ok(tally)
}

struct Sample {
    directory: PathBuf,
    map: AwbrnMap,
    tilesets: Tilesets,
    log: BufWriter<File>,
    frame: u32,
    commands: Vec<serde_json::Value>,
    error: Option<anyhow::Error>,
}

impl Sample {
    fn new(directory: &Path, map: AwbrnMap, options: &Options, game_seed: u64) -> Result<Self> {
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
                "tournament_seed": options.seed,
                "game_seed": game_seed,
                "pair": 0,
                "seat_order": [options.first, options.second],
                "fog": options.fog,
                "day_cap": options.day_cap,
            }),
        )?;
        Ok(Self {
            directory: directory.to_owned(),
            map,
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
        render_state(&self.map, state, &SEATS, &self.tilesets)
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
fn score(tally: &mut Tally, record: &Record, team: &TeamId) {
    tally.commands += record.commands;
    tally.refusals += record.refusals;
    match &record.outcome {
        Some(Outcome::Victory { winners, .. }) if winners.contains(team) => tally.wins += 1,
        Some(Outcome::Victory { .. }) => tally.losses += 1,
        Some(_) => tally.draws += 1,
        None => {
            tally.abandoned += 1;
            tally.draws += 1;
        }
    }
}

/// The 95% Wilson score interval for a proportion.
///
/// A normal interval is wrong near zero and one, and a first agent against a
/// random one lands near one. This one does not, and it never leaves `0..=1`.
fn wilson(wins: f64, games: f64) -> (f64, f64) {
    const Z: f64 = 1.96;
    let p = wins / games;
    let denominator = 1.0 + Z * Z / games;
    let centre = (p + Z * Z / (2.0 * games)) / denominator;
    let half = Z * (p * (1.0 - p) / games + Z * Z / (4.0 * games * games)).sqrt() / denominator;
    ((centre - half).max(0.0), (centre + half).min(1.0))
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
    let (low, high) = wilson(
        f64::from(tally.wins) + f64::from(tally.draws) / 2.0,
        f64::from(games),
    );

    println!(
        "{} vs {}   seed {}  fog {}  day cap {}",
        options.first, options.second, options.seed, options.fog, options.day_cap
    );
    println!("{} pairs, {games} games, both seat orders", options.pairs);
    println!();
    println!(
        "wins {}  losses {}  draws {}",
        tally.wins, tally.losses, tally.draws
    );
    println!("score                    {score:.4}  ({low:.4} to {high:.4}, Wilson 95%)");
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

    if tally.abandoned > 0 {
        let share = f64::from(tally.abandoned) / f64::from(games) * 100.0;
        println!();
        println!(
            "{} of {games} games ({share:.1}%) reached the day cap and are scored as\n\
             draws. A tournament in which most games are abandoned measures the cap,\n\
             not the agents.",
            tally.abandoned
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wilson_interval_holds_the_score_and_stays_in_range() {
        let (low, high) = wilson(55.0, 100.0);
        assert!(low < 0.55 && 0.55 < high);
        // The reason for Wilson rather than a normal interval: at the ends the
        // normal one leaves the range a proportion lives in.
        let (low, high) = wilson(100.0, 100.0);
        assert!(low > 0.9 && (high - 1.0).abs() < 1e-9);
        let (low, high) = wilson(0.0, 100.0);
        assert!(low.abs() < 1e-9 && high < 0.1);
        // More games, a tighter interval.
        let (few, _) = wilson(9.0, 10.0);
        let (many, _) = wilson(900.0, 1000.0);
        assert!(many > few);
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
