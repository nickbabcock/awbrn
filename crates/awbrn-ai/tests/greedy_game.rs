//! The greedy agent against the floor of the ladder.
//!
//! The arena binary is where a tournament is measured. This is the claim that
//! has to keep holding: an agent that ranks its plays beats an agent that
//! draws them, on the board both play, with fog and without.

use awbrn_ai::agent::Agent;
use awbrn_ai::agents::{GreedyAgent, RandomAgent};
use awbrn_ai::board::arena;
use awbrn_ai::harness::{Limits, Record, play};
use awbrn_ai::rng::Rng;
use awvm::semantic::{Outcome, TeamId};
use awvm::session::Session;

/// Games each seat order plays. The claim is a sweep rather than an edge, so a
/// few games say as much as many and cost a test that runs in a moment.
const GAMES: u64 = 4;

/// One game, with the greedy agent in `seat`.
fn game(fog: bool, seed: u64, seat: usize) -> (Record, TeamId) {
    let state = arena(fog, seed);
    let team = state.players[seat].team.clone();
    let mut session = Session::new(arena(fog, seed));
    let mut entropy = Rng::from_seed(Rng::mix(seed ^ 0x1));
    let mut greedy = GreedyAgent::from_seed(Rng::mix(seed ^ 0x2));
    let mut random = RandomAgent::from_seed(Rng::mix(seed ^ 0x3));
    let mut agents: [&mut dyn Agent; 2] = if seat == 0 {
        [&mut greedy, &mut random]
    } else {
        [&mut random, &mut greedy]
    };

    let record = play(
        state,
        &mut session,
        &mut agents,
        &mut entropy,
        Limits::DEFAULT,
    );
    (record, team)
}

fn assert_greedy_wins(fog: bool) {
    for seed in 1..=GAMES {
        for seat in 0..2 {
            let (record, team) = game(fog, seed, seat);
            let Some(Outcome::Victory { winners, .. }) = &record.outcome else {
                panic!(
                    "game {seed} from seat {seat} (fog {fog}) ended as {:?} after {} turns, \
                     so the greedy agent never finished a capture",
                    record.outcome, record.turns
                );
            };
            assert!(
                winners.contains(&team),
                "game {seed} from seat {seat} (fog {fog}) was won by {winners:?}, \
                 not by the greedy agent on {team:?}"
            );
        }
    }
}

#[test]
fn greedy_beats_random_in_a_standard_game() {
    assert_greedy_wins(false);
}

/// The same claim under fog, where the agent's projection hides the enemy and
/// a play it offers can be refused by a blocker it cannot see.
#[test]
fn greedy_beats_random_under_fog() {
    assert_greedy_wins(true);
}
