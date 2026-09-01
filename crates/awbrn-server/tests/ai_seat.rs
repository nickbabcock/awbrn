//! A seat the server plays, judged by the record it leaves.
//!
//! The thing that would be wrong and look right is a match an agent plays that
//! cannot be replayed, because the agent decided in a vocabulary the log does
//! not hold. So every test here ends the same way: reconstruct the match from
//! its stored events and compare.

use awbrn_ai::{HARD, STANDARD, profile};
use awbrn_map::{AwbrnMap, Dimensions, Pos};
use awbrn_server::{
    AiSeat, Co, GameCommand, GameServer, GameSetup, PlayerId, PlayerSetup, StoredActionEvent,
    reconstruct_from_events,
};
use awbrn_types::{Faction as TerrainFaction, GraphicalTerrain, PlayerFaction, Property};

fn p1() -> PlayerId {
    PlayerId(0)
}

fn p2() -> PlayerId {
    PlayerId(1)
}

/// A board with something on it worth doing: a base and a city for each side.
fn contested_setup() -> GameSetup {
    let mut setup = GameSetup {
        map: AwbrnMap::new(Dimensions::new(9, 5), GraphicalTerrain::Plain),
        players: vec![
            PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: None,
                starting_funds: 10_000,
                co: Co::Andy,
            },
            PlayerSetup {
                faction: PlayerFaction::BlueMoon,
                team: None,
                starting_funds: 10_000,
                co: Co::Andy,
            },
        ],
        fog_enabled: false,
        rng_seed: 0x5eed,
    };

    let mut set = |position, property| setup.map.set_terrain(position, property);
    set(
        Pos::new(0, 2),
        GraphicalTerrain::Property(Property::HQ(PlayerFaction::OrangeStar)),
    );
    set(
        Pos::new(1, 2),
        GraphicalTerrain::Property(Property::Base(TerrainFaction::Player(
            PlayerFaction::OrangeStar,
        ))),
    );
    set(
        Pos::new(8, 2),
        GraphicalTerrain::Property(Property::HQ(PlayerFaction::BlueMoon)),
    );
    set(
        Pos::new(7, 2),
        GraphicalTerrain::Property(Property::Base(TerrainFaction::Player(
            PlayerFaction::BlueMoon,
        ))),
    );
    set(
        Pos::new(4, 1),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
    );
    set(
        Pos::new(4, 3),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
    );
    setup
}

/// Play one seat's whole turn, recording it exactly as a host does.
fn play_ai_turn(
    server: &mut GameServer,
    events: &mut Vec<StoredActionEvent>,
    player: PlayerId,
    profile_id: &str,
    seed: u64,
) -> Vec<GameCommand> {
    let profile = profile(profile_id).expect("the profile is seated");
    let mut seat = AiSeat::new(player, profile, seed);
    seat.begin_turn(server);

    let mut played = Vec::new();
    while let Some(command) = seat.next_command(server) {
        match server.submit_command(player, command.clone()) {
            Ok(_) => {
                events.push(StoredActionEvent {
                    player,
                    command: command.clone(),
                    random: server.last_random().to_vec(),
                });
                played.push(command);
                seat.accepted(server);
            }
            Err(_) => seat.refused(),
        }
    }
    played
}

#[test]
fn a_played_turn_ends_the_turn() {
    let setup = contested_setup();
    let mut server = GameServer::new(setup).expect("the setup is valid");
    let mut events = Vec::new();

    let played = play_ai_turn(&mut server, &mut events, p1(), "ai-standard-v1", 1);

    assert_eq!(
        played.last(),
        Some(&GameCommand::EndTurn),
        "a turn the server plays has to hand the board on"
    );
    assert!(
        played.len() > 1,
        "the seat has funds and a base and did nothing but pass: {played:?}"
    );
}

#[test]
fn a_played_match_replays_from_its_own_log() {
    let setup = contested_setup();
    let mut server = GameServer::new(setup.clone()).expect("the setup is valid");
    let mut events = Vec::new();

    for day in 0..6 {
        for (slot, player) in [(0usize, p1()), (1, p2())] {
            let profile = if slot == 0 { STANDARD } else { HARD };
            play_ai_turn(
                &mut server,
                &mut events,
                player,
                profile.id,
                profile.turn_seed(setup.rng_seed, slot, day),
            );
        }
    }

    assert!(
        events.len() > 12,
        "six days of two seats should be more than the passes: {}",
        events.len()
    );

    let replayed = reconstruct_from_events(setup, &events).expect("the log replays");
    assert!(
        replayed.state() == server.state(),
        "the replay of an AI match has to reach the position the match reached"
    );
}

#[test]
fn a_seat_plays_the_same_way_from_the_same_seed() {
    let setup = contested_setup();
    let play_once = || {
        let mut server = GameServer::new(setup.clone()).expect("the setup is valid");
        let mut events = Vec::new();
        play_ai_turn(&mut server, &mut events, p1(), "ai-hard-v1", 99)
    };

    assert_eq!(play_once(), play_once());
}

#[test]
fn a_seat_that_cannot_see_the_board_still_ends_its_turn() {
    let mut setup = contested_setup();
    setup.fog_enabled = true;
    let mut server = GameServer::new(setup).expect("the setup is valid");
    let mut events = Vec::new();

    let played = play_ai_turn(&mut server, &mut events, p1(), "ai-easy-v1", 3);

    assert_eq!(played.last(), Some(&GameCommand::EndTurn));
}
