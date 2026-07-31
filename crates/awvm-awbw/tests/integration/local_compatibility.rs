use awbrn_map::AwbwMapData;
use awbw_replay::{ReplayParser, turn_models::Action};
use awvm::{event::AttackTarget, transition::Command};
use awvm_awbw::{
    LocalCompatibility, RecordedAdapter, diagnose_local_compatibility, diagnostic_command,
};

use crate::common::{map_path, replay_path};

#[test]
fn archived_fog_off_fire_has_a_local_awvm_witness() {
    let replay_path = replay_path("1362397.zip");
    let replay = ReplayParser::new()
        .parse(&std::fs::read(&replay_path).unwrap())
        .unwrap();
    assert!(!replay.games.first().unwrap().fog);
    let map_id = replay.games.first().unwrap().maps_id.as_u32();
    let map: AwbwMapData =
        serde_json::from_slice(&std::fs::read(map_path(&format!("{map_id}.json"))).unwrap())
            .unwrap();
    let mut adapter = RecordedAdapter::new(&replay, &map).unwrap();

    const ACTION_INDEX: usize = 937;
    for (index, action) in replay.turns.iter().enumerate() {
        let prior = adapter.state().clone();
        let transition = adapter.advance(action).unwrap();
        if matches!(action, Action::Fire { .. })
            && let Ok(Command::MoveAttack {
                target: AttackTarget::Unit { unit: defender },
                ..
            }) = diagnostic_command(prior.turn.active_player.clone(), action)
            && let (Some(before), Some(after)) = (
                prior.units.get(defender),
                transition.post_state().units.get(defender),
            )
        {
            assert_eq!(
                after.action, before.action,
                "Fire action {index} incorrectly spent its defender"
            );
        }
        if index == ACTION_INDEX {
            assert!(matches!(action, Action::Fire { .. }));
            let result = diagnose_local_compatibility(&prior, action, transition.post_state());
            let LocalCompatibility::LocallyCompatible(witness) = result else {
                panic!("archived Fire action {index} was not locally compatible: {result:#?}");
            };
            assert_eq!(witness.counts.hp_assignments, 100);
            assert!(witness.counts.accepted > 0);
            assert!(witness.counts.matching > 0);
            assert_eq!(witness.exact_hp.len(), 2);
            assert_eq!(witness.random.len(), 2);
            return;
        }
    }
    panic!("archived replay has no action {ACTION_INDEX}");
}
