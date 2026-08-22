//! Measurement 4: what one core costs for each command with no client attached.
//!
//! Measurements 1 to 3 price the parts of one search node on a fixture that is
//! held still. This binary is the only case that runs the parts together across
//! a complete game, so it is the only one that shows whether they add up in a
//! straight line. A difference between the modeled cost and the measured cost
//! is the result, in whichever direction it goes. The first suspect is the unit
//! count: units collect over a game, and the state clone, the projection and
//! enumeration all grow with them, so the report gives the units a game ends
//! with beside the rate.
//!
//! **Games for each second is a policy number, not an engine number.** The task
//! that asked for this measurement wanted games for each second, on the
//! reasoning that a random agent thinks as little as an agent can, so its rate
//! bounds every other agent. The first half holds. The second does not: a
//! random agent almost never captures a headquarters and almost never
//! eliminates an army, so its games do not end, and its rate is a rate over
//! abandoned games that falls as the turn cap rises. The report says so when
//! every game reaches the cap.
//!
//! So the number to take from here is **commands for each second**, which is
//! search nodes for each second. Multiply it by the commands a real policy
//! needs for one game to price a tier. Commands for each game is reported too,
//! and it belongs to the agent: this one produces units at every chance, so its
//! games are much longer than a played game.
//!
//! This is a binary and not a Criterion case because Criterion measures a short
//! operation many times. A game is one long operation, and the wanted number is
//! its rate.
//!
//! The workload is `awbrn_ai`: [`RandomAgent`] driven by [`play`], on the
//! game-scale fixture the other measurements use. The agent enumerates through
//! the observed query family, so it plays the complete
//! execute-observe-reify-enumerate cycle that measurement 3 prices. An agent
//! that enumerated the authoritative state would skip the projection and the
//! reification, and its cost could not be compared with the model.

use std::time::Instant;

use awbrn_ai::agent::Agent;
use awbrn_ai::agents::RandomAgent;
use awbrn_ai::harness::{Limits, Record, play};
use awbrn_ai::rng::Rng;
use awvm::session::Session;

use bench::benchmarks::server;

/// The complete cycle that `ai-cycle-complete` recorded, in microseconds.
///
/// The report divides the measured seconds by the commands played to get the
/// same quantity from a whole game, and prints the two beside each other. That
/// comparison is what this measurement exists to make. Update these when
/// `ai.rs` records a new number.
const MODELED_CYCLE_FOG_OFF_US: f64 = 350.0;
const MODELED_CYCLE_FOG_ON_US: f64 = 368.0;

/// How many player turns a game may take before this binary abandons it.
///
/// Random agents make games that never end, so a cap is necessary. Sixty player
/// turns is about a thirty-day duel, which is the length of a played game.
///
/// The cap does more than stop the loop. This agent produces a unit at every
/// chance and loses units slowly, so its armies grow without limit, and the
/// cost of one command grows with them: at a cap of 200 a game ends with about
/// 123 units, against the 40 the other measurements use. A high cap therefore
/// measures armies larger than a game ever holds. The report gives the units a
/// game ended with, so raise the cap when the question is how the cost grows,
/// and keep it low when the question is what one command costs.
const DEFAULT_TURN_CAP: u32 = 60;

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let mut records = Vec::with_capacity(options.games);
    // One session for the whole run. It keeps the board-sized tables it
    // allocated, which is what a self-play run wants between games, and it
    // keeps the allocator out of the measured rate.
    let mut session = Session::new(server::state(server::DUEL, options.fog));

    let start = Instant::now();
    for game in 0..options.games {
        records.push(one_game(&options, &mut session, game as u64));
    }
    let elapsed = start.elapsed().as_secs_f64();

    report(&options, &records, elapsed);
}

const USAGE: &str = "\
usage: throughput [--seed N] [--games N] [--fog] [--turn-cap N]

  --seed N       Seed for the agent and the reducer. The same seed gives the
                 same result. Default 1.
  --games N      Complete games to play. Default 200.
  --fog          Play with fog of war on. Default off.
  --turn-cap N   Abandon a game after this many player turns. Default 60.
                 A high cap measures armies larger than a real game holds.";

struct Options {
    seed: u64,
    games: usize,
    fog: bool,
    turn_cap: u32,
}

impl Options {
    fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            seed: 1,
            games: 200,
            fog: false,
            turn_cap: DEFAULT_TURN_CAP,
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
                "--games" => options.games = parse_number(&value()?)?,
                "--turn-cap" => options.turn_cap = parse_number(&value()?)?,
                "--fog" => options.fog = true,
                "--help" | "-h" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument {other}")),
            }
        }
        if options.games == 0 {
            return Err("--games must be at least 1".to_owned());
        }
        if options.turn_cap == 0 {
            return Err("--turn-cap must be at least 1".to_owned());
        }
        Ok(options)
    }
}

fn parse_number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{text} is not a number this argument accepts"))
}

/// Play one game of the run.
///
/// Two streams from one seed, so that an agent choice cannot move a combat
/// draw. Both are derived from the game index, which is what makes one game
/// reproducible on its own and keeps `--games 10` and `--games 200` playing the
/// same first ten games.
fn one_game(options: &Options, session: &mut Session, game: u64) -> Record {
    let seed = options.seed ^ (game << 32);
    let mut entropy = Rng::from_seed(Rng::mix(seed ^ 0x1));
    let mut first = RandomAgent::from_seed(Rng::mix(seed ^ 0x2));
    let mut second = RandomAgent::from_seed(Rng::mix(seed ^ 0x3));
    let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];

    play(
        server::state(server::DUEL, options.fog),
        session,
        &mut agents,
        &mut entropy,
        Limits {
            turns: options.turn_cap,
            ..Limits::DEFAULT
        },
    )
}

fn report(options: &Options, records: &[Record], elapsed: f64) {
    let games = records.len();
    let commands: u64 = records.iter().map(|record| record.commands).sum();
    let refused: u64 = records.iter().map(|record| record.refusals).sum();
    let capped = records.iter().filter(|record| record.abandoned()).count();
    let capped_share = capped as f64 / games as f64 * 100.0;

    let median_turns = median(&sorted(records, |record| u64::from(record.turns)));
    let median_commands = median(&sorted(records, |record| record.commands));
    let median_units = median(&sorted(records, |record| record.units as u64));

    let commands_a_second = commands as f64 / elapsed;
    let games_a_second = games as f64 / elapsed;
    let node_us = 1.0e6 / commands_a_second;

    let modeled_node_us = if options.fog {
        MODELED_CYCLE_FOG_ON_US
    } else {
        MODELED_CYCLE_FOG_OFF_US
    };

    println!(
        "seed {}  games {}  fog {}",
        options.seed, games, options.fog
    );
    println!("turn cap {}", options.turn_cap);
    println!();

    // The engine number. This is what measurement 4 can actually establish,
    // and it is the one that prices a tier.
    println!("elapsed                  {elapsed:.3} s");
    println!("commands each second     {commands_a_second:.1}");
    println!("one command              {node_us:.3} us measured");
    println!("                         {modeled_node_us:.3} us modeled, from ai-cycle-complete");
    println!("measured / modeled       {:.2}x", node_us / modeled_node_us);
    println!(
        "refused offers           {refused} ({:.2}% of nodes)",
        refused as f64 / (commands + refused) as f64 * 100.0
    );
    println!();

    // The policy numbers. A game's length belongs to the agent, not the engine,
    // so these describe the random agent and not the workspace.
    println!("median turns each game   {median_turns}");
    println!("median commands each game {median_commands}");
    println!("median units at the end  {median_units}");
    println!("reached the turn cap     {capped} of {games} ({capped_share:.1}%)");
    println!();

    if capped == games {
        println!(
            "Every game reached the cap, so no game ended. Games for each second is\n\
             not measured: {games_a_second:.3} is a rate over abandoned games, and it\n\
             falls as the cap rises. A random agent almost never wins, so this is a\n\
             property of the agent. Use commands each second above, and divide by\n\
             the commands a real policy needs."
        );
    } else {
        println!("games for each second    {games_a_second:.3}");
        if capped_share > 10.0 {
            println!(
                "warning: more than 10% of games reached the cap, so the median is a\n\
                 property of --turn-cap and not of the game. Raise the cap and run again."
            );
        }
    }
}

fn sorted(records: &[Record], read: impl Fn(&Record) -> u64) -> Vec<u64> {
    let mut values: Vec<u64> = records.iter().map(read).collect();
    values.sort_unstable();
    values
}

/// The middle game, which the report uses in place of the mean.
///
/// A random agent makes some games that run to the cap, and a mean that a few
/// capped games pull is a number about the cap.
fn median(sorted: &[u64]) -> u64 {
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2
    } else {
        sorted[middle]
    }
}
