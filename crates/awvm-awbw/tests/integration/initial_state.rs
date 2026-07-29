use awbrn_map::AwbwMapData;
use awbw_replay::ReplayParser;
use awvm::semantic::{AwbwVisibility, Observation, State, observe};
use awvm_awbw::initial_state;
use highway::HighwayHash;

use crate::common::{append_framed, map_path};

#[test]
fn every_archived_replay_produces_a_valid_round_tripping_observable_state() {
    insta::glob!("../../../../assets/replays", "*.zip", |replay_path| {
        let replay_file = replay_path.file_name().unwrap().to_string_lossy();
        let replay = ReplayParser::new()
            .parse(&std::fs::read(replay_path).unwrap())
            .unwrap();
        let map_id = replay
            .games
            .first()
            .expect("archived replay has a game")
            .maps_id
            .as_u32();
        let map_file = format!("{map_id}.json");
        let map_data: AwbwMapData =
            serde_json::from_slice(&std::fs::read(map_path(&map_file)).unwrap()).unwrap();

        let state = initial_state(&replay, &map_data)
            .unwrap_or_else(|error| panic!("{replay_file} with {map_file}: {error}"));
        state
            .validate()
            .unwrap_or_else(|error| panic!("{replay_file} produced invalid state: {error}"));

        let wire = serde_json::to_vec(&state).unwrap();
        let decoded: State = serde_json::from_slice(&wire).unwrap();
        assert_eq!(decoded, state, "{replay_file} did not round trip");

        let mut observation_hasher = highway::HighwayHasher::new(highway::Key::default());
        for player in &state.players {
            let observation =
                observe(&AwbwVisibility, &state, &player.id).unwrap_or_else(|error| {
                    panic!(
                        "{replay_file} could not be observed by player {}: {error}",
                        player.id
                    )
                });
            let observation_wire = serde_json::to_vec(&observation).unwrap();
            assert_eq!(
                serde_json::from_slice::<Observation>(&observation_wire).unwrap(),
                observation,
                "{replay_file} observation for {} did not round trip",
                player.id
            );
            append_framed(&mut observation_hasher, player.id.to_string().as_bytes());
            append_framed(&mut observation_hasher, &observation_wire);
        }

        assert_eq!(usize::from(state.board.width()), map_data.size_x as usize);
        assert_eq!(usize::from(state.board.height()), map_data.size_y as usize);
        assert_eq!(
            state.units.len(),
            replay.games.first().unwrap().units.len(),
            "{replay_file} lost units during adaptation"
        );

        let snapshot = format!(
            "map={map_id} size={}x{} players={} units={} state={:016x} observations={:016x}",
            state.board.width(),
            state.board.height(),
            state.players.len(),
            state.units.len(),
            checksum(&wire),
            observation_hasher.finalize64(),
        );
        insta::with_settings!({snapshot_suffix => replay_file.to_string()}, {
            insta::assert_snapshot!("initial_state", snapshot);
        });
    });
}

fn checksum(bytes: &[u8]) -> u64 {
    highway::HighwayHasher::new(highway::Key::default()).hash64(bytes)
}
