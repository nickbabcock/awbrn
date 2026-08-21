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
//! The agent enumerates through the observed query family, and it plays the
//! complete execute-observe-reify-enumerate cycle that measurement 3 prices.
//! An agent that enumerated the authoritative state would skip the projection
//! and the reification, and its cost could not be compared with the model.

use std::time::Instant;

use awvm::commander::Domain;
use awvm::event::AttackTarget;
use awvm::query;
use awvm::random::{Entropy, Luck, RandomError};
use awvm::ruleset::{UnitKind as UnitKindId, WeatherKind, terrain};
use awvm::semantic::{
    AwbwVisibility, Dimensions, Location, Match, Observation, ObservedTileOwner, ObservedUnitRef,
    PlayerId, Pos, State, UnitId, observe,
};
use awvm::session::{Legal, Order, OrderKind, Session, UnitIdx};
use awvm::transition::{Command, ExecuteOutcome, execute_with};

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

/// Refusals in a row that end the turn by force.
///
/// A refused offer changes nothing, so a loop that only ends a turn when there
/// is no offer left has no guarantee it makes progress. This bounds it. The
/// value is high enough that an ordinary fog refusal does not end a turn early:
/// the report gives the refusal count so that this can be checked.
const REJECTION_LIMIT: u32 = 64;

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            std::process::exit(2);
        }
    };

    let mut outcomes = Vec::with_capacity(options.games);

    let start = Instant::now();
    for game in 0..options.games {
        outcomes.push(play(&options, game as u64));
    }
    let elapsed = start.elapsed().as_secs_f64();

    report(&options, &outcomes, elapsed);
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

/// A seeded generator, used for the agent's choices and for the reducer's luck.
///
/// This is the xorshift that `awbrn-server` seeds a match with. It is repeated
/// here rather than exported from that crate: a benchmark is not a reason to
/// widen a server interface, and this binary needs two independent streams so
/// that an agent choice cannot move the combat draws.
struct Rng {
    state: u64,
}

impl Rng {
    const fn from_seed(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// A value in `0..range`, without the modulo bias that a bare remainder has.
    fn below(&mut self, range: u64) -> u64 {
        if range <= 1 {
            return 0;
        }
        let limit = u64::MAX - (u64::MAX % range);
        loop {
            let sample = self.next_u64();
            if sample < limit {
                return sample % range;
            }
        }
    }
}

impl Entropy for Rng {
    fn luck(&mut self, _polarity: Luck, domain: Domain) -> Result<i64, RandomError> {
        let width = u64::try_from(domain.maximum - domain.minimum)
            .expect("commander luck domains are ordered");
        Ok(domain.minimum + self.below(width + 1) as i64)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        // `state_from_setup` sets a clear weather setting, and the reducer draws
        // weather only for the random setting, so this is unreachable today. It
        // answers rather than panics so that a later fixture change does not
        // stop the measurement.
        Ok(match self.below(3) {
            0 => WeatherKind::Clear,
            1 => WeatherKind::Rain,
            _ => WeatherKind::Snow,
        })
    }
}

/// A seed for one game, derived so that game `n` does not continue game `n-1`.
///
/// Deriving each game's seed from the run seed and the game index is what makes
/// one game reproducible on its own, and what keeps `--games 10` and
/// `--games 200` playing the same first ten games.
const fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

struct GameOutcome {
    turns: u32,
    commands: u64,
    /// Offers the reducer refused. Each one costs a complete node and counts as
    /// no command, so a large share makes the reported rate pessimistic.
    rejected: u64,
    /// Units on the board when the game stopped.
    ///
    /// The cost model assumes each command costs the same. Units collect over a
    /// game, and the state clone, the projection and enumeration all grow with
    /// them, so this is the first thing to look at when the measured rate and
    /// the modeled rate disagree.
    units: usize,
    capped: bool,
}

fn play(options: &Options, game: u64) -> GameOutcome {
    let mut state = server::state(server::DUEL, options.fog);
    // Two streams from one seed, so that an agent choice cannot move a combat
    // draw. Both are derived from the game index, which is what makes one game
    // reproducible on its own.
    let mut entropy = Rng::from_seed(mix(options.seed ^ (game << 32) ^ 0x1));
    let mut choices = Rng::from_seed(mix(options.seed ^ (game << 32) ^ 0x2));

    let mut turns = 0;
    let mut commands = 0;
    let mut rejected = 0;
    let mut rejected_in_a_row = 0;

    while matches!(state.match_state, Match::Active { .. }) {
        if turns >= options.turn_cap {
            return GameOutcome {
                turns,
                commands,
                rejected,
                units: state.units.iter().count(),
                capped: true,
            };
        }

        let observation = observe(&AwbwVisibility, &state, &state.turn.active_player)
            .expect("the active player can observe the state they act on");

        // An offer the reducer refuses leaves the state alone, so the next pass
        // sees the same board and draws again. That escapes on its own while
        // one offer is acceptable, but nothing in the loop guarantees it. Ending
        // the turn by force after a run of refusals bounds the loop: `turns`
        // then rises whatever the reducer says, so the turn cap always stops the
        // game.
        let command = if rejected_in_a_row >= REJECTION_LIMIT {
            rejected_in_a_row = 0;
            turns += 1;
            Command::EndTurn {
                player: state.turn.active_player.clone(),
            }
        } else {
            match choose(&observation, &state, &mut choices) {
                Some(command) => command,
                None => {
                    turns += 1;
                    Command::EndTurn {
                        player: state.turn.active_player.clone(),
                    }
                }
            }
        };

        match execute_with(&state, command, &mut entropy) {
            Ok(ExecuteOutcome::Accepted(execution)) => {
                state = execution.state;
                commands += 1;
                rejected_in_a_row = 0;
            }
            // A rejection means the observed family offered a command the
            // authoritative reducer refuses. With fog on that is expected, and
            // it is the player's own risk: an attack on a unit that fog hid,
            // for example. A rejection still costs a complete node, so the
            // report gives the count: rejections make the rate pessimistic,
            // because the work is done and no command is counted for it.
            Ok(ExecuteOutcome::Rejected(_)) => {
                rejected += 1;
                rejected_in_a_row += 1;
            }
            Err(error) => panic!("the reducer failed on a generated command: {error:?}"),
        }
    }

    GameOutcome {
        turns,
        commands,
        rejected,
        units: state.units.iter().count(),
        capped: false,
    }
}

/// One offer the agent may take, named without building its command.
///
/// A ceiling measures the engine, so the agent must not add work of its own. A
/// turn offers about a thousand commands, and each one holds a path, so a list
/// of commands would put a thousand allocations for each node into a number
/// whose subject is the reducer. This names an offer in bytes instead, and only
/// the taken offer becomes a [`Command`].
///
/// This shape is not free: rebuilding the taken offer asks the movement field
/// for its path a second time. That is one reachability search for each node
/// against the nineteen the enumeration already ran.
#[derive(Clone, Copy)]
enum Choice {
    Move {
        unit: UnitId,
        destination: Pos,
        action: MoveAction,
    },
    Produce {
        position: Pos,
        kind: UnitKindId,
    },
    Unload {
        transport: UnitId,
        cargo: UnitId,
        destination: Pos,
    },
}

#[derive(Clone, Copy)]
enum MoveAction {
    Wait,
    Capture,
    Supply,
    Hide,
    Reveal,
    Explode,
    Join,
    Load,
    Repair(Pos),
    Launch(Pos),
    Attack(Pos),
}

/// A uniform draw over a sequence whose length is not known in advance.
///
/// Offer each candidate once. Candidate `n` replaces the held one with
/// probability `1/n`, which leaves each candidate equally likely. This is what
/// lets the agent choose uniformly without a list to choose from.
#[derive(Default)]
struct Reservoir {
    seen: u64,
    chosen: Option<Choice>,
}

impl Reservoir {
    fn offer(&mut self, rng: &mut Rng, choice: Choice) {
        self.seen += 1;
        if rng.below(self.seen) == 0 {
            self.chosen = Some(choice);
        }
    }
}

/// Draw one legal command uniformly, as the observed family reports the offers.
///
/// The offers are the eleven movement actions, production, and unload. There is
/// no `ActivatePower` and no `Tag`, because `query` has no enumerator for them,
/// so a random agent cannot find them the way it finds a unit action. A power
/// changes what a turn costs, so this rate is a lower bound for a game in which
/// powers are used. There is no `DeleteUnit` and no `Resign`: both end a game
/// earlier for a reason no policy would choose, and both would make the turn
/// count a property of the agent's readiness to give up.
///
/// `None` means the player has no command left, which is how a turn ends.
fn choose(observation: &Observation, truth: &State, rng: &mut Rng) -> Option<Command> {
    let player = observation.recipient.clone();
    let session = Session::from_observation(observation).expect("an observation reifies");
    if !session.is_commandable() {
        return None;
    }
    let projection = session.state();
    let dimensions = projection.board.dimensions();
    let legal = session.legal();

    let mut reservoir = Reservoir::default();

    // One buffer for the whole turn. The session appends into it and the
    // agent reads it, which is the shape the session API asks to be driven
    // in.
    let mut seats = Vec::new();
    let mut orders = Vec::new();
    legal.units(&mut seats);
    for seat in seats.iter().copied() {
        // Every unit the session offers belongs to the recipient, and a
        // projection carries the real id of a unit its holder owns.
        let unit = projection
            .units
            .at(usize::from(seat.get()))
            .expect("a seat the session reported")
            .id;
        orders.clear();
        legal.unit_orders(seat, &mut orders);
        offer_orders(&mut reservoir, rng, observation, unit, &orders, &dimensions);
    }
    offer_unloads(&mut reservoir, rng, &legal, projection);
    offer_production(
        &mut reservoir,
        rng,
        &legal,
        projection,
        observation,
        &player,
    );

    let choice = reservoir.chosen?;
    Some(build(projection, observation, truth, player, choice))
}

/// Offer each order the session named for one unit.
///
/// Join, load and repair name a unit, and a projection carries the id of
/// friendly units only. The agent cannot spell an order that names a tile
/// whose occupant the recipient cannot identify, so it is not offered.
/// Deletion is not a move, and this agent never offers it.
fn offer_orders(
    reservoir: &mut Reservoir,
    rng: &mut Rng,
    observation: &Observation,
    unit: UnitId,
    orders: &[Order],
    dimensions: &Dimensions,
) {
    for order in orders {
        let at = |cell| dimensions.position_of(cell);
        let Some(destination) = at(order.destination()) else {
            continue;
        };
        let occupied = friendly_at(observation, destination).is_some();
        let action = match order.kind() {
            OrderKind::Wait => MoveAction::Wait,
            OrderKind::Capture => MoveAction::Capture,
            OrderKind::Supply => MoveAction::Supply,
            OrderKind::Hide => MoveAction::Hide,
            OrderKind::Reveal => MoveAction::Reveal,
            OrderKind::Explode => MoveAction::Explode,
            OrderKind::Join if occupied => MoveAction::Join,
            OrderKind::Load if occupied => MoveAction::Load,
            OrderKind::Repair(cell) => match at(cell) {
                Some(position) if friendly_at(observation, position).is_some() => {
                    MoveAction::Repair(position)
                }
                _ => continue,
            },
            OrderKind::Launch(cell) => match at(cell) {
                Some(position) => MoveAction::Launch(position),
                None => continue,
            },
            OrderKind::Attack(cell) => match at(cell) {
                Some(position) => MoveAction::Attack(position),
                None => continue,
            },
            _ => continue,
        };
        reservoir.offer(
            rng,
            Choice::Move {
                unit,
                destination,
                action,
            },
        );
    }
}

fn offer_unloads(reservoir: &mut Reservoir, rng: &mut Rng, legal: &Legal<'_>, projection: &State) {
    let dimensions = projection.board.dimensions();
    // Only a transport that holds something can offer an unload. Naming the
    // loaded transports once is cheaper than opening a search at every unit on
    // the board, most of which carry nothing.
    let mut loaded: Vec<_> = projection
        .units
        .as_slice()
        .iter()
        .filter_map(|unit| match unit.location {
            Location::Cargo { transport, .. } => Some(transport),
            Location::Board { .. } => None,
        })
        .collect();
    loaded.sort_unstable();
    let mut unloads = Vec::new();
    for (index, unit) in projection.units.as_slice().iter().enumerate() {
        if !matches!(unit.location, Location::Board { .. }) {
            continue;
        }
        if loaded.binary_search(&unit.id).is_err() {
            continue;
        }
        let Some(seat) = u16::try_from(index).ok().map(UnitIdx::from_raw) else {
            continue;
        };
        unloads.clear();
        legal.unloads(seat, &mut unloads);
        for unload in &unloads {
            let Some(destination) = dimensions.position_of(unload.destination) else {
                continue;
            };
            reservoir.offer(
                rng,
                Choice::Unload {
                    transport: unit.id,
                    cargo: unload.cargo,
                    destination,
                },
            );
        }
    }
}

/// Production, at every facility the recipient holds.
///
/// The facility positions come from walking the projected board, not from
/// naming the fixture's three bases. A game captures property, so at turn
/// thirty the facilities are not the ones the setup gave, and a fixed list
/// would report a production cost that falls as the game goes on.
fn offer_production(
    reservoir: &mut Reservoir,
    rng: &mut Rng,
    legal: &Legal<'_>,
    projection: &State,
    observation: &Observation,
    player: &PlayerId,
) {
    let dimensions = projection.board.dimensions();
    let mut rows = Vec::new();
    for (position, tile) in observation.board.iter() {
        if !matches!(&tile.owner, ObservedTileOwner::Owned(owner) if owner == player) {
            continue;
        }
        if !terrain(tile.terrain).produces_any() {
            continue;
        }
        let Some(cell) = dimensions.cell_index(position) else {
            continue;
        };
        rows.clear();
        legal.production_options(cell, &mut rows);
        for option in rows.iter().filter(|row| row.affordable) {
            reservoir.offer(
                rng,
                Choice::Produce {
                    position,
                    kind: option.kind,
                },
            );
        }
    }
}

fn build(
    projection: &State,
    observation: &Observation,
    truth: &State,
    player: PlayerId,
    choice: Choice,
) -> Command {
    match choice {
        Choice::Produce { position, kind } => Command::ProduceUnit {
            player,
            position,
            kind,
        },
        Choice::Unload {
            transport,
            cargo,
            destination,
        } => Command::Unload {
            player,
            transport,
            cargo,
            destination,
        },
        Choice::Move {
            unit,
            destination,
            action,
        } => {
            let path = query::reachable(projection, unit)
                .expect("the chosen unit had a movement field when it was offered")
                .path_to(destination)
                .expect("the chosen destination came from that field");
            build_movement(observation, truth, player, unit, path, destination, action)
        }
    }
}

fn build_movement(
    observation: &Observation,
    truth: &State,
    player: PlayerId,
    unit: UnitId,
    path: Vec<Pos>,
    destination: Pos,
    action: MoveAction,
) -> Command {
    let occupant =
        || friendly_at(observation, destination).expect("the destination held a unit when offered");
    let friendly = |position| {
        friendly_at(observation, position).expect("the target was friendly when offered")
    };
    match action {
        MoveAction::Wait => Command::MoveWait { player, unit, path },
        MoveAction::Capture => Command::MoveCapture { player, unit, path },
        MoveAction::Supply => Command::MoveSupply { player, unit, path },
        MoveAction::Hide => Command::MoveHide { player, unit, path },
        MoveAction::Reveal => Command::MoveReveal { player, unit, path },
        MoveAction::Explode => Command::MoveExplode { player, unit, path },
        MoveAction::Join => Command::MoveJoin {
            player,
            unit,
            path,
            target: occupant(),
        },
        MoveAction::Load => Command::MoveLoad {
            player,
            unit,
            path,
            transport: occupant(),
        },
        MoveAction::Repair(position) => Command::MoveRepair {
            player,
            unit,
            path,
            target: friendly(position),
        },
        MoveAction::Launch(target) => Command::MoveLaunch {
            player,
            unit,
            path,
            target,
        },
        MoveAction::Attack(position) => Command::MoveAttack {
            player,
            unit,
            path,
            target: attack_target(truth, position),
        },
    }
}

/// Name what stands at `position`, reading the authoritative state.
///
/// This is the one place the agent looks past its own projection, and it is a
/// benchmark shortcut rather than fog-safe play. A projection reports an attack
/// target as a position, because it carries no id for an enemy, and an id taken
/// from a reification is an invention that means nothing to a reducer. A real
/// client resolves this at the server. This binary owns the authoritative
/// state, so it resolves the position directly.
fn attack_target(truth: &State, position: Pos) -> AttackTarget {
    truth
        .units
        .iter()
        .find(|unit| unit.location == Location::Board { position })
        .map_or(AttackTarget::Tile { position }, |unit| AttackTarget::Unit {
            unit: unit.id,
        })
}

fn friendly_at(observation: &Observation, position: Pos) -> Option<UnitId> {
    observation
        .units
        .iter()
        .find_map(|unit| match (&unit.reference, unit.location) {
            (ObservedUnitRef::Friendly { unit: id }, Location::Board { position: at })
                if at == position =>
            {
                Some(*id)
            }
            _ => None,
        })
}

fn report(options: &Options, outcomes: &[GameOutcome], elapsed: f64) {
    let games = outcomes.len();
    let commands: u64 = outcomes.iter().map(|outcome| outcome.commands).sum();
    let rejected: u64 = outcomes.iter().map(|outcome| outcome.rejected).sum();
    let capped = outcomes.iter().filter(|outcome| outcome.capped).count();
    let capped_share = capped as f64 / games as f64 * 100.0;

    let median_turns = median(&sorted(outcomes, |outcome| outcome.turns as u64));
    let median_commands = median(&sorted(outcomes, |outcome| outcome.commands));
    let median_units = median(&sorted(outcomes, |outcome| outcome.units as u64));

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
        "refused offers           {rejected} ({:.2}% of nodes)",
        rejected as f64 / (commands + rejected) as f64 * 100.0
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

fn sorted(outcomes: &[GameOutcome], read: impl Fn(&GameOutcome) -> u64) -> Vec<u64> {
    let mut values: Vec<u64> = outcomes.iter().map(read).collect();
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
