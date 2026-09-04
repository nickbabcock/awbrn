//! Playback of an archive the server wrote.
//!
//! The fixture is a downloaded match archive, changed only to replace the
//! account identifiers with a placeholder. It is here because the reader and
//! the writer are in different languages: nothing but a real file catches the
//! reader drifting from the shape the server stores.

use std::path::Path;

use awbrn_client::replay_archive::{ReplayArchive, ReplayTimeline};
use awbrn_game::Authority;

fn fixture() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/replays/awbrn-replay-3zceh5a97b31o.json");
    std::fs::read(path).expect("replay fixture should be readable")
}

#[test]
fn reads_a_stored_archive_as_the_server_wrote_it() {
    let archive = ReplayArchive::parse(&fixture()).expect("archive should parse");
    let ReplayArchive::Awbrn(replay) = &archive else {
        panic!("JSON archive should use the AWBRN adapter");
    };

    assert_eq!(replay.setup.match_id, "3zceh5a97b31o");
    // A map identifier is a string, and the map travels with the archive.
    assert_eq!(replay.setup.map_id, "x9lw2qab0jlf");
    assert_eq!(replay.setup.revision, 1);
    assert_eq!(replay.setup.map.map().width(), 20);
    assert_eq!(replay.actions.len(), 1);

    // One seat is a person and the other is the server.
    let players = &replay.setup.players;
    assert_eq!(players.len(), 2);
    assert!(players[0].user_id.is_some());
    assert_eq!(players[0].ai_profile_id, None);
    assert_eq!(players[1].user_id, None);
    assert_eq!(players[1].ai_profile_id.as_deref(), Some("ai-hard-v1"));
}

#[test]
fn replays_every_archived_action() {
    let archive = ReplayArchive::parse(&fixture()).expect("archive should parse");
    let ReplayArchive::Awbrn(replay) = &archive else {
        panic!("JSON archive should use the AWBRN adapter");
    };

    let setup = replay.game_setup().expect("setup should build");
    let authority = Authority::new(&setup).expect("authority should start");
    let mut timeline = ReplayTimeline::Awbrn {
        setup,
        current: Box::new(authority),
    };

    for index in 0..archive.len() {
        timeline
            .advance(&archive, index)
            .unwrap_or_else(|error| panic!("archived action {index} should replay: {error}"));
    }
}
