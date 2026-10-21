//! A unit strikes with its visual HP, not its exact HP.
//!
//! As in Advance Wars, a unit fights with the HP bars that it shows. Each case
//! is an attack from a recorded AWBW game. It starts at the beginning of the
//! turn, where the units keep their exact HP, and tries every luck roll. The
//! result that AWBW recorded must be one of the results. With exact HP in the
//! formula, no luck roll gives it.

use std::collections::BTreeSet;

use awbrn_map::AwbwMapData;
use awbw_replay::{AwbwReplay, ReplayParser, turn_models::Action};
use awvm::commander::Domain;
use awvm::event::AttackTarget;
use awvm::random::{Entropy, Luck, RandomError};
use awvm::semantic::{State, UnitId, WeatherKind};
use awvm::transition::{Command, ExecuteOutcome, execute_with};
use awvm_awbw::{RecordedAdapter, diagnostic_command};

use crate::common::{map_path, replay_path};

/// Luck rolls from a list. A roll after the end of the list is the lowest
/// value of its range. Each range that the reducer asks for is kept.
struct Rolls {
    values: Vec<i64>,
    domains: Vec<Domain>,
}

impl Entropy for Rolls {
    fn luck(&mut self, _: Luck, domain: Domain) -> Result<i64, RandomError> {
        let value = self
            .values
            .get(self.domains.len())
            .copied()
            .unwrap_or(domain.minimum);
        self.domains.push(domain);
        Ok(value)
    }

    fn weather(&mut self) -> Result<WeatherKind, RandomError> {
        Ok(WeatherKind::Clear)
    }
}

/// The state before action `index`, from the snapshot at the start of its
/// turn, and the recorded state after the action.
fn around(replay: &AwbwReplay, map: &AwbwMapData, index: usize) -> (State, State) {
    let turn_start = replay.turns[..index]
        .iter()
        .rposition(|action| matches!(action, Action::End { .. }))
        .map_or(0, |end| end + 1);
    let turn = replay.turns[..turn_start]
        .iter()
        .filter(|action| matches!(action, Action::End { .. }))
        .count();
    let snapshot = AwbwReplay {
        games: vec![replay.games[turn].clone()],
        turns: Vec::new(),
    };
    let mut adapter = RecordedAdapter::new(&snapshot, map).unwrap();
    for action in &replay.turns[turn_start..index] {
        adapter.advance(action).unwrap();
    }
    let prior = adapter.state().clone();
    let post = adapter.advance(&replay.turns[index]).unwrap();
    (prior, post.post_state().clone())
}

/// The bars of the target and the attacker after each possible luck roll.
fn outcomes(
    state: &State,
    command: &Command,
    attacker: UnitId,
    target: UnitId,
) -> BTreeSet<(u8, u8)> {
    let run = |values: Vec<i64>| {
        let mut rolls = Rolls {
            values,
            domains: Vec::new(),
        };
        let outcome = execute_with(state, command.clone(), &mut rolls);
        (outcome, rolls.domains)
    };
    let (_, domains) = run(Vec::new());
    let mut combinations = vec![Vec::new()];
    for domain in domains {
        combinations = combinations
            .into_iter()
            .flat_map(|prefix: Vec<i64>| {
                (domain.minimum..=domain.maximum).map(move |value| {
                    let mut next = prefix.clone();
                    next.push(value);
                    next
                })
            })
            .collect();
    }
    combinations
        .into_iter()
        .filter_map(|values| match run(values).0 {
            Ok(ExecuteOutcome::Accepted(execution)) => Some(execution.state),
            _ => None,
        })
        .map(|state| (bars(&state, target), bars(&state, attacker)))
        .collect()
}

fn bars(state: &State, unit: UnitId) -> u8 {
    state.units.get(unit).map_or(0, |unit| unit.hp.div_ceil(10))
}

/// Check that a luck roll gives the recorded result of attack `index`.
/// `attacker_hp` and `target_hp` are the exact HP before the attack.
fn assert_recorded(file: &str, index: usize, attacker_hp: u8, target_hp: u8) {
    let replay = ReplayParser::new()
        .parse(&std::fs::read(replay_path(file)).unwrap())
        .unwrap();
    let map_id = replay.games[0].maps_id.as_u32();
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path(&format!("{map_id}.json"))).unwrap())
            .unwrap();
    let (prior, post) = around(&replay, &map, index);
    let command =
        diagnostic_command(prior.turn.active_player.clone(), &replay.turns[index]).unwrap();
    let Command::MoveAttack {
        unit: attacker,
        target: AttackTarget::Unit { unit: target },
        ..
    } = command
    else {
        panic!("{file} action {index} is not an attack on a unit");
    };
    assert_eq!(prior.units.get(attacker).unwrap().hp, attacker_hp);
    assert_eq!(prior.units.get(target).unwrap().hp, target_hp);

    let recorded = (bars(&post, target), bars(&post, attacker));
    let possible = outcomes(&prior, &command, attacker, target);
    assert!(
        possible.contains(&recorded),
        "{file} action {index}: AWBW recorded {recorded:?}, AWVM allows {possible:?}"
    );
}

#[test]
fn recorded_attacks_use_visual_attacker_hp() {
    // An attacker at 1.1 HP strikes as 2 bars. As 11 points, it removes one
    // bar less than AWBW recorded.
    assert_recorded("replay_1109718_legacy-game-over.zip", 1215, 11, 100);
    // Two full units. The defender counters from its damaged HP, which only
    // its visual HP makes as strong as AWBW recorded.
    assert_recorded("1419680.zip", 361, 100, 100);
}
