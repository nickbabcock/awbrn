//! Tactical positions with a known correct answer.
//!
//! A match result is a slow and noisy signal. A puzzle is a fast and exact
//! one: one position, one turn for the agent, and one predicate on the
//! result. Each puzzle names one mistake that a human player can use against
//! the AI. A puzzle suite is therefore a regression test for tactics, and it
//! runs in milliseconds.
//!
//! All puzzles use a small open board. Seat 0 (Orange Star) moves first and
//! is the seat the agent plays. Both seats hold no funds, so production does
//! not change the answer unless a puzzle says so.
//!
//! A puzzle does not ask for one exact command list. It asks for a result:
//! "the enemy tank is destroyed", "our tank does not end where the enemy can
//! destroy it". Many command lists can give that result.

use awbrn_game::{GameSetup, PlayerSetup, state_from_setup};
use awbrn_map::{AwbrnMap, AwbwMap};
use awbrn_types::{Co, PlayerFaction};
use awvm::ruleset::{self, UnitKind};
use awvm::semantic::{Concealment, Location, PlayerIdx, Pos, State, Unit, UnitAction, UnitId};
use awvm::session::{AttackCandidate, LegalVisitor, Order, Session};

use crate::agent::{Agent, NodeBudget};
use crate::harness::{TurnResult, run_agent_turn_unmeasured};
use crate::rng::Rng;

/// The board width and height of every puzzle.
pub const BOARD_SIZE: u8 = 12;

/// The AWBW terrain identifiers that the puzzle board uses.
mod terrain {
    pub const PLAINS: u16 = 1;
    pub const NEUTRAL_CITY: u16 = 34;
    pub const ORANGE_STAR_CITY: u16 = 38;
    pub const ORANGE_STAR_BASE: u16 = 39;
    pub const ORANGE_STAR_HQ: u16 = 42;
    pub const BLUE_MOON_BASE: u16 = 44;
    pub const BLUE_MOON_HQ: u16 = 47;
}

/// Where the agent's headquarters stands.
pub const OUR_HQ: Pos = Pos { x: 1, y: 1 };
/// Where the agent's base stands.
pub const OUR_BASE: Pos = Pos { x: 2, y: 1 };
/// Where the agent's city stands.
pub const OUR_CITY: Pos = Pos { x: 5, y: 1 };
/// Where a neutral city stands.
pub const NEUTRAL_CITY: Pos = Pos { x: 4, y: 3 };

/// One tactical position and the test of the agent's answer.
#[derive(Clone)]
pub struct Puzzle {
    /// A stable name.
    pub name: &'static str,
    /// Whether ending the turn with no command is a correct answer.
    pub passive_solution: bool,
    /// What a correct turn does.
    pub description: &'static str,
    /// The position before the agent's turn.
    pub state: State,
    /// Return `Ok` when the turn solves the puzzle.
    pub check: fn(&PuzzleTurn<'_>) -> Result<(), String>,
}

impl std::fmt::Debug for Puzzle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Puzzle").field("name", &self.name).finish()
    }
}

/// The facts a puzzle check reads.
#[derive(Debug)]
pub struct PuzzleTurn<'a> {
    /// The position before the turn.
    pub start: &'a State,
    /// The turn the agent played. Its state is the enemy's turn start.
    pub result: &'a TurnResult,
}

/// The result of one puzzle for one agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzleOutcome {
    pub name: &'static str,
    pub passed: bool,
    /// Why the check failed, or an empty string.
    pub detail: String,
    pub commands: usize,
}

/// Return every puzzle, in a stable order.
pub fn suite() -> Vec<Puzzle> {
    vec![
        kill_the_tank(),
        three_attacker_kill(),
        focus_fire(),
        hq_block(),
        capture_denial(),
        hq_capture_denial(),
        no_suicide(),
    ]
}

/// Play one turn of `puzzle` with `agent` and check the result.
pub fn solve(puzzle: &Puzzle, agent: &mut dyn Agent, entropy_seed: u64) -> PuzzleOutcome {
    agent.start_match();
    let mut entropy = Rng::from_seed(entropy_seed);
    let result = match run_agent_turn_unmeasured(
        puzzle.state.clone(),
        agent,
        &mut entropy,
        NodeBudget::FOUR,
    ) {
        Ok(result) => result,
        Err(error) => {
            return PuzzleOutcome {
                name: puzzle.name,
                passed: false,
                detail: format!("the turn failed: {error}"),
                commands: 0,
            };
        }
    };
    let turn = PuzzleTurn {
        start: &puzzle.state,
        result: &result,
    };
    let check = (puzzle.check)(&turn);
    PuzzleOutcome {
        name: puzzle.name,
        passed: check.is_ok(),
        detail: check.err().unwrap_or_default(),
        commands: result.commands.len(),
    }
}

/// The two seats: ours (Orange Star) and the enemy (Blue Moon).
pub fn seats(state: &State) -> (PlayerIdx, PlayerIdx) {
    let mut seats = state.players.seats().map(|(seat, _)| seat);
    let ours = seats.next().expect("the puzzle board seats two players");
    let theirs = seats.next().expect("the puzzle board seats two players");
    (ours, theirs)
}

/// The best low damage that each enemy unit can do to `target` now.
///
/// Read this on a state where the enemy is the active seat, for example the
/// state after the agent's end-turn command. The sum over all attackers is
/// the damage that focused fire can do with the worst luck for the enemy.
pub fn enemy_low_damage_on(state: &State, target: UnitId) -> Vec<(UnitId, u16)> {
    struct Damage {
        target: UnitId,
        best: Vec<(UnitId, u16)>,
    }
    impl LegalVisitor for Damage {
        const ATTACK_CONTEXT: bool = true;

        fn order(&mut self, _order: Order) {}

        fn attack(&mut self, candidate: AttackCandidate<'_>) {
            let (Some(defender), Some(forecast)) = (candidate.target_unit, candidate.forecast)
            else {
                return;
            };
            if defender.id != self.target {
                return;
            }
            let low = forecast.attack.low;
            match self
                .best
                .iter_mut()
                .find(|(attacker, _)| *attacker == candidate.attacker.id)
            {
                Some((_, best)) => *best = (*best).max(low),
                None => self.best.push((candidate.attacker.id, low)),
            }
        }
    }
    let session = Session::new(state.clone());
    let mut visitor = Damage {
        target,
        best: Vec::new(),
    };
    session.legal().visit_orders(&mut visitor);
    visitor.best
}

/// A puzzle board with plains, the given properties, and no units.
fn board(properties: &[(Pos, u16)]) -> State {
    let size = usize::from(BOARD_SIZE);
    let mut columns = vec![vec![terrain::PLAINS; size]; size];
    let mut place = |position: Pos, id: u16| {
        columns[usize::from(position.x)][usize::from(position.y)] = id;
    };
    place(OUR_HQ, terrain::ORANGE_STAR_HQ);
    place(OUR_BASE, terrain::ORANGE_STAR_BASE);
    place(Pos { x: 10, y: 10 }, terrain::BLUE_MOON_HQ);
    place(Pos { x: 9, y: 10 }, terrain::BLUE_MOON_BASE);
    for (position, id) in properties {
        place(*position, *id);
    }
    let json = serde_json::json!({
        "Name": "Puzzle",
        "Author": "awbrn",
        "Player Count": 2,
        "Published Date": "2026-10-02",
        "Size X": BOARD_SIZE,
        "Size Y": BOARD_SIZE,
        "Terrain Map": columns,
        "Predeployed Units": [],
    });
    let map = AwbwMap::parse_json(json.to_string().as_bytes()).expect("the puzzle map parses");
    let setup = GameSetup {
        map: AwbrnMap::from_map(&map),
        players: [PlayerFaction::OrangeStar, PlayerFaction::BlueMoon]
            .into_iter()
            .map(|faction| PlayerSetup {
                faction,
                team: None,
                starting_funds: 0,
                co: Co::Andy,
            })
            .collect(),
        fog_enabled: false,
        rng_seed: 1,
    };
    let mut state = state_from_setup(&setup).expect("the puzzle setup is valid");
    state.units.retain(|_| false);
    let seats = seats(&state);
    for seat in [seats.0, seats.1] {
        state.players.player_mut(seat).funds = 0;
    }
    state
}

/// Put one whole unit on the board.
fn put(state: &mut State, id: u32, kind: UnitKind, x: u8, y: u8, owner: PlayerIdx) -> UnitId {
    let profile = ruleset::profile(kind);
    let id = UnitId::new(id);
    state.units.push(Unit {
        id,
        kind,
        owner,
        hp: 100,
        fuel: profile.max_fuel,
        ammo: profile.max_ammo,
        action: UnitAction::Ready,
        concealment: Concealment::Exposed,
        location: Location::Board {
            position: Pos { x, y },
        },
    });
    id
}

/// The state after the turn, as the enemy sees it at its turn start.
fn after<'a>(turn: &'a PuzzleTurn<'_>) -> &'a State {
    &turn.result.state
}

fn alive(state: &State, id: UnitId) -> Option<&Unit> {
    state.units.get(id)
}

fn position(unit: &Unit) -> Option<Pos> {
    match unit.location {
        Location::Board { position } => Some(position),
        _ => None,
    }
}

const ENEMY_TANK: u32 = 100;
const ENEMY_TANK_2: u32 = 101;
const ENEMY_INFANTRY: u32 = 102;
const ENEMY_RECON: u32 = 103;
const OUR_TANK: u32 = 1;

/// Two attackers can destroy the enemy tank. A weak infantry is a decoy.
fn kill_the_tank() -> Puzzle {
    let mut state = board(&[]);
    let (ours, theirs) = seats(&state);
    put(&mut state, ENEMY_TANK, UnitKind::Tank, 6, 6, theirs);
    put(&mut state, ENEMY_INFANTRY, UnitKind::Infantry, 6, 9, theirs);
    put(&mut state, 1, UnitKind::MdTank, 3, 6, ours);
    put(&mut state, 2, UnitKind::Tank, 6, 3, ours);
    Puzzle {
        name: "kill-the-tank",
        passive_solution: false,
        description: "A medium tank and a tank together destroy the enemy tank.",
        state,
        check: |turn| match alive(after(turn), UnitId::new(ENEMY_TANK)) {
            None => Ok(()),
            Some(unit) => Err(format!("the enemy tank survives at {} hp", unit.hp)),
        },
    }
}

/// Only all three attackers together destroy the enemy medium tank. Each
/// attack alone is a poor trade, and a recon is a better target for the tank.
fn three_attacker_kill() -> Puzzle {
    let mut state = board(&[]);
    let (ours, theirs) = seats(&state);
    put(&mut state, ENEMY_TANK, UnitKind::MdTank, 6, 6, theirs);
    put(&mut state, ENEMY_RECON, UnitKind::Recon, 9, 10, theirs);
    put(&mut state, 1, UnitKind::MdTank, 3, 6, ours);
    put(&mut state, 2, UnitKind::Artillery, 6, 3, ours);
    put(&mut state, 3, UnitKind::Tank, 6, 9, ours);
    Puzzle {
        name: "three-attacker-kill",
        passive_solution: false,
        description: "A medium tank, an artillery, and a tank together destroy the enemy medium tank.",
        state,
        check: |turn| match alive(after(turn), UnitId::new(ENEMY_TANK)) {
            None => Ok(()),
            Some(unit) => Err(format!("the enemy medium tank survives at {} hp", unit.hp)),
        },
    }
}

/// Spread fire leaves two damaged tanks. Focused fire destroys one.
fn focus_fire() -> Puzzle {
    let mut state = board(&[]);
    let (ours, theirs) = seats(&state);
    put(&mut state, ENEMY_TANK, UnitKind::Tank, 6, 6, theirs);
    put(&mut state, ENEMY_TANK_2, UnitKind::Tank, 8, 6, theirs);
    put(&mut state, 1, UnitKind::Artillery, 6, 4, ours);
    put(&mut state, 2, UnitKind::Tank, 3, 6, ours);
    put(&mut state, 3, UnitKind::Tank, 3, 8, ours);
    Puzzle {
        name: "focus-fire",
        passive_solution: false,
        description: "The artillery and a tank destroy one enemy tank instead of hitting both.",
        state,
        check: |turn| {
            let state = after(turn);
            let left = [ENEMY_TANK, ENEMY_TANK_2]
                .into_iter()
                .filter(|id| alive(state, UnitId::new(*id)).is_some())
                .count();
            if left < 2 {
                Ok(())
            } else {
                Err("both enemy tanks survive".into())
            }
        },
    }
}

/// An enemy infantry can reach our headquarters next turn. Our infantry can
/// stand on it first.
fn hq_block() -> Puzzle {
    let mut state = board(&[(NEUTRAL_CITY, terrain::NEUTRAL_CITY)]);
    let (ours, theirs) = seats(&state);
    put(&mut state, ENEMY_INFANTRY, UnitKind::Infantry, 1, 4, theirs);
    put(&mut state, 1, UnitKind::Infantry, 3, 2, ours);
    Puzzle {
        name: "hq-block",
        passive_solution: false,
        description: "Our infantry stands on the headquarters before the enemy infantry reaches it.",
        state,
        check: |turn| {
            let state = after(turn);
            if alive(state, UnitId::new(ENEMY_INFANTRY)).is_none() {
                return Ok(());
            }
            let (ours, _) = seats(state);
            let guarded = state
                .units
                .iter()
                .any(|unit| unit.owner == ours && position(unit) == Some(OUR_HQ));
            if guarded {
                Ok(())
            } else {
                Err("the headquarters is open to the enemy infantry".into())
            }
        },
    }
}

/// An enemy infantry completes the capture of our city next turn. Any hit
/// stops it. A recon is a better trade on its own, and is a decoy.
fn capture_denial() -> Puzzle {
    let mut state = board(&[(OUR_CITY, terrain::ORANGE_STAR_CITY)]);
    let (ours, theirs) = seats(&state);
    put(&mut state, ENEMY_INFANTRY, UnitKind::Infantry, 5, 1, theirs);
    put(&mut state, ENEMY_RECON, UnitKind::Recon, 8, 5, theirs);
    put(&mut state, OUR_TANK, UnitKind::Tank, 5, 5, ours);
    state.board.tile_mut(OUR_CITY).capture_points = Some(10);
    Puzzle {
        name: "capture-denial",
        passive_solution: false,
        description: "Our tank hits the infantry on our city so that its capture cannot finish.",
        state,
        check: |turn| {
            let state = after(turn);
            let Some(capturer) = alive(state, UnitId::new(ENEMY_INFANTRY)) else {
                return Ok(());
            };
            let remaining = state
                .board
                .get(OUR_CITY)
                .and_then(|tile| tile.capture_points)
                .unwrap_or(20);
            let strength = capturer.hp.div_ceil(10);
            if strength < remaining {
                Ok(())
            } else {
                Err(format!(
                    "the infantry at {} hp finishes the capture of {remaining} points",
                    capturer.hp
                ))
            }
        },
    }
}

/// An enemy infantry completes the capture of our headquarters next turn.
fn hq_capture_denial() -> Puzzle {
    let mut state = board(&[]);
    let (ours, theirs) = seats(&state);
    put(&mut state, ENEMY_INFANTRY, UnitKind::Infantry, 1, 1, theirs);
    put(&mut state, ENEMY_RECON, UnitKind::Recon, 7, 4, theirs);
    put(&mut state, OUR_TANK, UnitKind::Tank, 4, 4, ours);
    state.board.tile_mut(OUR_HQ).capture_points = Some(10);
    Puzzle {
        name: "hq-capture-denial",
        passive_solution: false,
        description: "Our tank hits the infantry on our headquarters before it wins the match.",
        state,
        check: |turn| {
            let state = after(turn);
            let Some(capturer) = alive(state, UnitId::new(ENEMY_INFANTRY)) else {
                return Ok(());
            };
            if capturer.hp.div_ceil(10) < 10 {
                Ok(())
            } else {
                Err("the infantry is whole and takes the headquarters".into())
            }
        },
    }
}

/// An enemy infantry is bait. Two enemy tanks can destroy our tank on every
/// tile from which it can hit the infantry.
fn no_suicide() -> Puzzle {
    let mut state = board(&[]);
    let (ours, theirs) = seats(&state);
    put(&mut state, ENEMY_INFANTRY, UnitKind::Infantry, 6, 6, theirs);
    put(&mut state, ENEMY_TANK, UnitKind::Tank, 9, 4, theirs);
    put(&mut state, ENEMY_TANK_2, UnitKind::Tank, 9, 8, theirs);
    put(&mut state, OUR_TANK, UnitKind::Tank, 2, 6, ours);
    Puzzle {
        name: "no-suicide",
        passive_solution: true,
        description: "Our tank does not end its turn where the two enemy tanks can destroy it.",
        state,
        check: |turn| {
            let state = after(turn);
            let Some(tank) = alive(state, UnitId::new(OUR_TANK)) else {
                return Err("our tank is destroyed during our own turn".into());
            };
            let damage: u32 = enemy_low_damage_on(state, tank.id)
                .into_iter()
                .map(|(_, low)| u32::from(low))
                .sum();
            if damage < u32::from(tank.hp) {
                Ok(())
            } else {
                Err(format!(
                    "focused enemy fire does at least {damage} damage to our tank at {} hp",
                    tank.hp
                ))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::HARD_V2 as HARD;

    /// Every puzzle board is legal, and the agent's seat moves first.
    #[test]
    fn every_puzzle_starts_on_our_turn() {
        for puzzle in suite() {
            let (ours, _) = seats(&puzzle.state);
            assert_eq!(
                puzzle.state.players.seat(&puzzle.state.turn.active_player),
                Some(ours),
                "{} does not start on our turn",
                puzzle.name
            );
        }
    }

    /// Doing nothing fails each puzzle that needs an action. Otherwise a puzzle tests nothing.
    #[test]
    fn passing_the_turn_fails_every_puzzle() {
        struct Pass;
        impl Agent for Pass {
            fn act(
                &mut self,
                _view: &awvm::semantic::Observation,
                _budget: NodeBudget,
            ) -> Option<crate::agent::Play> {
                None
            }
        }
        for puzzle in suite().iter().filter(|puzzle| !puzzle.passive_solution) {
            let outcome = solve(puzzle, &mut Pass, 1);
            assert!(!outcome.passed, "{} passes with no commands", puzzle.name);
        }
    }

    /// The fixed Hard baseline. Update this table when Hard changes.
    ///
    /// A `false` entry is a known weakness of Hard, not a test failure.
    #[test]
    fn hard_baseline() {
        let expected = HARD_BASELINE.to_vec();
        let actual = suite()
            .iter()
            .map(|puzzle| {
                let mut agent = HARD.agent(7);
                let outcome = solve(puzzle, &mut *agent, 11);
                (outcome.name, outcome.passed)
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    /// Print what Hard plays in each puzzle.
    #[test]
    #[ignore = "prints a report"]
    fn print_hard_turns() {
        for puzzle in suite() {
            let mut agent = HARD.agent(7);
            agent.start_match();
            let mut entropy = Rng::from_seed(11);
            let result = run_agent_turn_unmeasured(
                puzzle.state.clone(),
                &mut *agent,
                &mut entropy,
                NodeBudget::FOUR,
            )
            .expect("the turn runs");
            let turn = PuzzleTurn {
                start: &puzzle.state,
                result: &result,
            };
            println!("{}: {:?}", puzzle.name, (puzzle.check)(&turn));
            for command in &result.commands {
                println!("  {command:?}");
            }
        }
    }

    /// Whether `ai-hard-v2` solves each puzzle, in suite order.
    const HARD_BASELINE: [(&str, bool); 7] = [
        ("kill-the-tank", true),
        ("three-attacker-kill", true),
        ("focus-fire", true),
        ("hq-block", false),
        ("capture-denial", true),
        ("hq-capture-denial", true),
        ("no-suicide", false),
    ];
}
