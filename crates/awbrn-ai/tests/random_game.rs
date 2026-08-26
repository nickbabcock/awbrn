//! The whole loop, driven by the public interface.
//!
//! Observe, choose, spell, execute, observe again. The board is a fog fixture
//! because fog is what makes the loop worth testing: the agent's projection
//! invents an id for every unit it cannot own, so a play that survives the
//! round trip is a play that named its target by tile.

use awbrn_ai::agent::{Agent, NodeBudget, Play};
use awbrn_ai::agents::RandomAgent;
use awbrn_ai::harness::{Limits, Record, play};
use awbrn_ai::rng::Rng;
use awvm::semantic::{Observation, ObservedUnitRef, State};
use awvm::session::Session;

const FIXTURE: &str = include_str!("../../../spec/fixtures/fog/vision-sources-and-terrain.json");

/// Days one game may play before the test gives up on it.
///
/// A random agent almost never wins, so a game ends at this cap and that is
/// expected. The cap is what stops the test rather than the game's own end.
const LIMITS: Limits = Limits {
    nodes: NodeBudget::FOUR,
    days: 15,
    refusals: 64,
};

fn fixture_state() -> State {
    let document: serde_json::Value = serde_json::from_str(FIXTURE).expect("the fixture parses");
    serde_json::from_value(document["initial_state"].clone()).expect("the fixture holds a state")
}

/// A random agent that checks each play before it hands it over.
///
/// The property is the one that keeps an agent honest under fog: a play names
/// its own unit and its own cargo, and it names everything else by tile. This
/// wrapper is where the check goes now that the loop lives in the harness.
struct Checked(RandomAgent);

impl Agent for Checked {
    fn act(&mut self, view: &Observation, budget: NodeBudget) -> Option<Play> {
        let play = self.0.act(view, budget)?;
        for named in [play.unit(), play.cargo()].into_iter().flatten() {
            let owned = view.units.iter().any(|unit| {
                matches!(unit.reference, ObservedUnitRef::Friendly { unit: id } if id == named)
            });
            assert!(owned, "a play named a unit the player cannot own");
        }
        Some(play)
    }
}

fn game(seed: u64) -> Record {
    let mut session = Session::new(fixture_state());
    let mut entropy = Rng::from_seed(Rng::mix(seed ^ 0x1));
    let mut first = Checked(RandomAgent::from_seed(Rng::mix(seed ^ 0x2)));
    let mut second = Checked(RandomAgent::from_seed(Rng::mix(seed ^ 0x3)));
    let mut agents: [&mut dyn Agent; 2] = [&mut first, &mut second];

    play(
        fixture_state(),
        &mut session,
        &mut agents,
        &mut entropy,
        LIMITS,
    )
}

#[test]
fn a_random_game_plays_commands_the_reducer_accepts() {
    let record = game(1);

    assert!(
        record.commands > 0,
        "no command was accepted, so the loop never ran"
    );
    assert!(record.turns > 0, "no turn ended, so no side ever passed");
    // Fog refusals are expected. A majority of them means the agent is offering
    // plays the true state cannot hold, which is a fault in the round trip.
    assert!(
        record.refusals < record.commands,
        "more commands were refused than accepted: {} refused, {} accepted",
        record.refusals,
        record.commands
    );
}

#[test]
fn one_seed_gives_one_game() {
    let first = game(11);
    let second = game(11);
    assert_eq!(first.commands, second.commands);
    assert_eq!(first.refusals, second.refusals);
    assert_eq!(first.turns, second.turns);
    assert_eq!(first.units, second.units);
}

#[test]
fn report_the_shape_of_a_game() {
    // Not an assertion about a number, but a place to read what the loop did.
    // `cargo test -p awbrn-ai -- --nocapture report_the_shape_of_a_game`
    for seed in 0..4 {
        let record = game(seed);
        println!(
            "seed {seed}: {} commands, {} refused, {} turns, {} days, {} units, abandoned {}",
            record.commands,
            record.refusals,
            record.turns,
            record.days,
            record.units,
            record.abandoned()
        );
    }
}

/// The one play that does not go through [`awvm::session::Session::spell`].
///
/// An unload names two friendly units, and this checks that both ids arrive at
/// the true state as themselves. The fixture's second player holds a loaded
/// APC, so the play exists on its first turn.
#[test]
fn an_unload_names_the_true_cargo() {
    use awvm::semantic::{AwbwVisibility, observe};
    use awvm::session::{Order, OrderKind, UnitIdx};
    use awvm::transition::Command;

    let mut state = fixture_state();
    // Hand the turn to the player that holds the transport.
    let second = state
        .players
        .seats()
        .nth(1)
        .map(|(_, player)| player.id().clone())
        .expect("the fixture seats two players");
    state.turn.active_player = second;
    state.turn.position = 1;

    let authority = Session::new(state);
    let view = observe(
        &AwbwVisibility,
        authority.state(),
        &authority.state().turn.active_player,
    )
    .expect("the active player can observe their own position");

    let projection = Session::from_observation(&view).expect("an observation reifies");
    let mut orders = Vec::new();
    projection.legal().orders(&mut orders);

    let order = orders
        .into_iter()
        .find(|order| matches!(order.kind(), OrderKind::Unload(_)))
        .expect("the loaded transport offers an unload");

    let play = Play::from_order(&projection, order).expect("the order names a unit it holds");
    let command = play.command(&authority).expect("the true state holds both");

    match command {
        Command::Unload {
            transport, cargo, ..
        } => {
            assert_eq!(
                transport,
                play.unit().expect("an unload names its transport")
            );
            assert_eq!(
                authority
                    .state()
                    .units
                    .get(cargo)
                    .expect("the cargo is a unit the true state holds")
                    .location,
                awvm::semantic::Location::Cargo { transport, slot: 0 },
                "the cargo is in the transport the play named"
            );
        }
        other => panic!("an unload play spelled {other:?}"),
    }

    // An order for a unit no position holds is not a play.
    assert!(
        Play::from_order(
            &projection,
            Order::new(UnitIdx::from_raw(400), order.destination(), OrderKind::Wait)
        )
        .is_none()
    );
}

/// An attack play names its target by tile, and the true state names the unit.
///
/// This is the reason [`Play`] exists. The projection carries no id for an
/// enemy, so every attack the agent can see is spelled against the authority,
/// and what comes back must name a unit that authority holds.
#[test]
fn an_attack_resolves_against_the_true_state() {
    use awvm::event::AttackTarget;
    use awvm::semantic::{AwbwVisibility, observe};
    use awvm::session::OrderKind;
    use awvm::transition::Command;

    let authority = Session::new(fixture_state());
    let view = observe(
        &AwbwVisibility,
        authority.state(),
        &authority.state().turn.active_player,
    )
    .expect("the active player can observe their own position");

    let projection = Session::from_observation(&view).expect("an observation reifies");
    let mut orders = Vec::new();
    projection.legal().orders(&mut orders);

    let mut checked = 0;
    for order in orders
        .iter()
        .copied()
        .filter(|order| matches!(order.kind(), OrderKind::Attack(_)))
    {
        let play = Play::from_order(&projection, order).expect("the order names a unit it holds");
        let Some(command) = play.command(&authority) else {
            // A hidden blocker can make an offered attack unroutable. That is
            // a fog answer, not a fault.
            continue;
        };
        let Command::MoveAttack { target, .. } = command else {
            panic!("an attack play spelled something else");
        };
        if let AttackTarget::Unit { unit } = target {
            assert!(
                authority.state().units.get(unit).is_some(),
                "an attack named a unit the true state does not hold"
            );
            checked += 1;
        }
    }

    assert!(
        checked > 0,
        "the fixture offered no attack, so none was checked"
    );
}
