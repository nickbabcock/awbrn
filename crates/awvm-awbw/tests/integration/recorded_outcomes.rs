use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::BufWriter;
use std::path::Path;

use awbrn_map::AwbwMapData;
use awbw_replay::{ReplayParser, turn_models::Action};
use awvm::ruleset::{Domain, profile};
use awvm::semantic::{Location, ObservedTransition, PlayerId, PowerState, UnitAction};
use awvm_awbw::RecordedAdapter;
use highway::HighwayHash;
use rayon::prelude::*;
use serde::Serialize;

use crate::common::{append_framed, append_framed_json, map_path, workspace_path};

/// One replay's per-action digest lines plus the power transitions it exercised.
struct ReplayOutcome {
    snapshot: String,
    retained_intervening_powers: usize,
    expired_returning_powers: usize,
}

#[test]
fn every_recorded_outcome_produces_valid_typed_transitions() {
    // The archive is replayed in parallel and asserted serially: insta's glob
    // settings are thread local, and a single-threaded pass over every replay
    // made this the slowest test in the workspace.
    let outcomes = replay_archive_in_parallel();
    let mut retained_intervening_powers = 0;
    let mut expired_returning_powers = 0;
    insta::glob!("../../../../assets/replays", "*.zip", |replay_path| {
        let replay_file = replay_path.file_name().unwrap().to_string_lossy();
        let outcome = outcomes
            .get(replay_file.as_ref())
            .expect("every globbed replay was replayed");
        retained_intervening_powers += outcome.retained_intervening_powers;
        expired_returning_powers += outcome.expired_returning_powers;
        insta::with_settings!({snapshot_suffix => replay_file.to_string()}, {
            insta::assert_snapshot!("recorded_outcomes", outcome.snapshot);
        });
    });
    if std::env::var_os("INSTA_GLOB_FILTER").is_none() {
        assert!(
            retained_intervening_powers > 0,
            "archive never exercised a power persisting through an intervening turn"
        );
        assert!(
            expired_returning_powers > 0,
            "archive never exercised power expiry when its owner regained the turn"
        );
    }
}

/// Replay every archived game on rayon's pool.
///
/// The pool is sized to the machine rather than to the archive: a thread per
/// replay oversubscribes once the archive outgrows the core count, and every
/// in-flight replay holds a parsed archive and its adapter state, so peak
/// memory would grow with the archive instead of with the pool. Replays vary
/// in length by an order of magnitude, so the work stealing also matters --
/// a static partition would strand workers behind the longest games.
fn replay_archive_in_parallel() -> HashMap<String, ReplayOutcome> {
    let paths = std::fs::read_dir(workspace_path("assets/replays"))
        .expect("the replay archive is readable")
        .filter_map(|entry| {
            let path = entry.expect("replay archive entries are readable").path();
            (path.extension().is_some_and(|extension| extension == "zip")).then_some(path)
        })
        .collect::<Vec<_>>();

    paths
        .par_iter()
        .map(|path| (replay_file_name(path), replay_outcome(path)))
        .collect()
}

fn replay_file_name(path: &Path) -> String {
    path.file_name()
        .expect("a replay path names a file")
        .to_string_lossy()
        .into_owned()
}

fn replay_outcome(replay_path: &Path) -> ReplayOutcome {
    let mut retained_intervening_powers = 0;
    let mut expired_returning_powers = 0;
    let replay_file = replay_file_name(replay_path);
    let replay_file = replay_file.as_str();
    let replay = ReplayParser::new()
        .parse(&std::fs::read(replay_path).unwrap())
        .unwrap();
    let map_file = format!(
        "{}.json",
        replay
            .games
            .first()
            .expect("archived replay has a game")
            .maps_id
            .as_u32()
    );
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path(&map_file)).unwrap()).unwrap();
    let mut adapter = RecordedAdapter::new(&replay, &map).unwrap();
    let mut snapshot = String::with_capacity(replay.turns.len() * 96);
    // Decoding is a property of the observation types, so it is asserted
    // once per action kind rather than on all ~13k archived actions: the
    // full round trip dominated this suite's CI runtime.
    let mut round_tripped = HashSet::new();
    // Digests are checkpointed at turn starts instead of taken per action.
    // State is cumulative, so a divergence anywhere inside a turn still shows
    // up at the next checkpoint, and serializing every action's state and
    // observations dominated this suite's runtime.
    let mut checkpoint = turn_key(adapter.state());
    let last_index = replay.turns.len().saturating_sub(1);

    for (index, action) in replay.turns.iter().enumerate() {
        let prior = adapter.state().clone();
        let transition = adapter.advance(action).unwrap_or_else(|error| {
            panic!(
                "{replay_file} action {index} ({}): {error}\n{action:#?}",
                action.kind_name(),
            )
        });
        transition.post_state().validate().unwrap_or_else(|error| {
            panic!(
                "{replay_file} action {index} ({}) produced invalid state: {error}",
                action.kind_name()
            )
        });

        let post_key = turn_key(transition.post_state());
        let is_checkpoint = post_key != checkpoint || index == last_index;
        let decode = round_tripped.insert(action.kind_name());
        let mut observed_hasher = highway::HighwayHasher::new(highway::Key::default());
        if is_checkpoint || decode {
            for player in &transition.post_state().players {
                let observed = transition.observe(player.id()).unwrap_or_else(|error| {
                    panic!(
                        "{replay_file} action {index} ({}) for {}: {error}",
                        action.kind_name(),
                        player.id()
                    )
                });
                if decode {
                    let wire = serde_json::to_vec(&observed).unwrap();
                    assert_eq!(
                        serde_json::from_slice::<ObservedTransition>(&wire).unwrap(),
                        observed,
                        "{replay_file} action {index} ({}) for {} did not round trip",
                        action.kind_name(),
                        player.id()
                    );
                }
                if is_checkpoint {
                    append_framed(&mut observed_hasher, player.id().to_string().as_bytes());
                    append_framed_json(&mut observed_hasher, &observed);
                }
            }
        }

        let (retained, expired) =
            assert_recorded_turn_start(replay_file, index, action, &prior, transition.post_state());
        retained_intervening_powers += retained;
        expired_returning_powers += expired;
        if is_checkpoint {
            checkpoint = post_key;
            writeln!(
                snapshot,
                "{index:05} d={} p={} {:<10} s={:016x} o={:016x}",
                transition.post_state().turn.day,
                transition.post_state().turn.active_player,
                action.kind_name(),
                checksum(transition.post_state()),
                observed_hasher.finalize64(),
            )
            .unwrap();
        }
    }

    // The archive is the only evidence for how AWBW ends a match, so the
    // terminal state is spelled out instead of folded into a digest.
    let final_state = adapter.state();
    writeln!(
        snapshot,
        "final {}",
        serde_json::to_string(&serde_json::json!({
            "day": final_state.turn.day,
            "phase": final_state.turn.phase,
            "match": final_state.match_state,
            // Owned properties are spelled out beside the outcome: they are
            // what a day limit and a capture limit are decided on, so the
            // snapshot shows whether AWBW's verdict agrees with the count.
            "players": final_state
                .players
                .seats()
                .map(|(seat, player)| {
                    (
                        player.id().to_string(),
                        player.status,
                        final_state
                            .board
                            .tiles()
                            .filter(|tile| tile.owner.is_owned_by(seat))
                            .count(),
                    )
                })
                .collect::<Vec<_>>(),
        }))
        .unwrap()
    )
    .unwrap();

    ReplayOutcome {
        snapshot,
        retained_intervening_powers,
        expired_returning_powers,
    }
}

fn assert_recorded_turn_start(
    replay_file: &str,
    index: usize,
    action: &Action,
    prior: &awvm::semantic::State,
    post: &awvm::semantic::State,
) -> (usize, usize) {
    let next = match action {
        Action::End { updated_info } | Action::Tag { updated_info } => updated_info
            .next_turn()
            .map(|next_turn| next_turn.next_player_id),
        Action::Resign {
            next_turn_action: Some(next),
            ..
        } => Some(next.next_player_id),
        _ => None,
    };
    let Some(next) = next.map(|id| PlayerId::from(id.as_u32().to_string())) else {
        return (0, 0);
    };
    let tagged = matches!(action, Action::Tag { .. });
    let mut retained_intervening_powers = 0;
    let mut expired_returning_powers = 0;

    for before in &prior.units {
        let Some(after) = post.units.get(before.id) else {
            assert_eq!(
                Some(before.owner),
                prior.player_index(&next),
                "{replay_file} action {index} ({}) removed another player's unit at turn start",
                action.kind_name()
            );
            let upkeep_unit = matches!(profile(before.kind).domain, Domain::Air | Domain::Sea);
            let crashed_cargo = match before.location {
                Location::Cargo { transport, .. } => {
                    prior.units.get(transport).is_some_and(|unit| {
                        matches!(profile(unit.kind).domain, Domain::Air | Domain::Sea)
                            && !post.units.contains(transport)
                    })
                }
                Location::Board { .. } => false,
            };
            assert!(
                upkeep_unit || crashed_cargo,
                "{replay_file} action {index} ({}) removed non-upkeep unit {} at turn start",
                action.kind_name(),
                before.id
            );
            continue;
        };
        let expected = if Some(before.owner) == prior.player_index(&next) {
            UnitAction::Ready
        } else {
            before.action
        };
        assert_eq!(
            after.action,
            expected,
            "{replay_file} action {index} ({}) normalized the wrong unit owner",
            action.kind_name()
        );
    }

    for before in &prior.players {
        let after = post.find_player(before.id()).unwrap();
        let expires = *before.id() == next || (tagged && *before.id() == prior.turn.active_player);
        let expected = if expires {
            &PowerState::None
        } else {
            &before.power_state
        };
        if before.power_state != PowerState::None {
            if *before.id() == next {
                expired_returning_powers += 1;
            } else if !expires {
                retained_intervening_powers += 1;
            }
        }
        assert_eq!(
            &after.power_state,
            expected,
            "{replay_file} action {index} ({}) expired the wrong player's power",
            action.kind_name()
        );
    }
    (retained_intervening_powers, expired_returning_powers)
}

/// The turn a state belongs to: digests are checkpointed when this changes.
fn turn_key(state: &awvm::semantic::State) -> (u64, PlayerId) {
    (state.turn.day, state.turn.active_player.clone())
}

fn checksum(value: &impl Serialize) -> u64 {
    let hasher = highway::HighwayHasher::new(highway::Key::default());
    let mut writer = BufWriter::with_capacity(0x8000, hasher);
    serde_json::to_writer(&mut writer, value).unwrap();
    writer.into_inner().unwrap().finalize64()
}
