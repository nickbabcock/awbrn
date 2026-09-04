//! Stepping through a played match.
//!
//! The thing that would be wrong and look right is a boundary that reports a
//! board the match never stood at, because the cursor arrived there by a route
//! that skipped something. So the tests here reach every boundary twice — once
//! through the cursor, once by replaying the log from its first action — and
//! compare what a recipient is shown.

use awbrn_ai::{HARD, STANDARD, profile};
use awbrn_map::{AwbrnMap, Dimensions, Pos};
use awbrn_server::{
    AiSeat, Co, GameServer, GameSetup, MatchReview, PlayerId, PlayerSetup, StoredActionEvent,
    reconstruct_from_events, review::transition_for,
};
use awbrn_types::{Faction as TerrainFaction, GraphicalTerrain, PlayerFaction, Property};

fn p1() -> PlayerId {
    PlayerId(0)
}

fn p2() -> PlayerId {
    PlayerId(1)
}

/// A board with something on it worth doing: a base and a city for each side.
fn contested_setup(fog: bool) -> GameSetup {
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
        fog_enabled: fog,
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

fn play_ai_turn(
    server: &mut GameServer,
    events: &mut Vec<StoredActionEvent>,
    player: PlayerId,
    profile_id: &str,
    seed: u64,
) {
    let profile = profile(profile_id).expect("the profile is seated");
    let mut seat = AiSeat::new(player, profile, seed);
    seat.begin_turn(server);
    while let Some(command) = seat.next_command(server) {
        match server.submit_command(player, command.clone()) {
            Ok(_) => {
                events.push(StoredActionEvent {
                    player,
                    command,
                    random: server.last_random().to_vec(),
                });
                seat.accepted(server);
            }
            Err(_) => seat.refused(),
        }
    }
}

/// A log long enough to cross the checkpoint spacing more than once.
fn played_match(fog: bool) -> (GameSetup, Vec<StoredActionEvent>) {
    let setup = contested_setup(fog);
    let mut server = GameServer::new(setup.clone()).expect("the setup is valid");
    let mut events = Vec::new();
    for day in 0..8 {
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
    (setup, events)
}

#[test]
fn every_boundary_shows_the_board_the_match_stood_at() {
    let (setup, events) = played_match(false);
    assert!(
        events.len() > 64,
        "the log has to cross the checkpoint spacing: {}",
        events.len()
    );
    let mut review = MatchReview::new(setup.clone(), events.clone()).expect("the log replays");

    // Backwards, so the cursor is rebuilt from a checkpoint every time, and
    // then forwards, so it is stepped. Both routes have to agree with a log
    // replayed from its first action.
    let boundaries = (0..=events.len())
        .rev()
        .chain(0..=events.len())
        .collect::<Vec<_>>();
    for index in boundaries {
        review.seek(index).expect("the boundary is reachable");
        assert_eq!(review.index(), index);
        let expected = reconstruct_from_events(setup.clone(), &events[..index])
            .expect("the log replays")
            .player_observation(p1());
        assert_eq!(
            review.observation(Some(p1())),
            expected,
            "boundary {index} disagrees with a replay of the same actions"
        );
    }
}

#[test]
fn the_outline_names_a_day_and_a_seat_for_every_boundary() {
    let (setup, events) = played_match(false);
    let review = MatchReview::new(setup, events.clone()).expect("the log replays");

    let outline = review.outline();
    assert_eq!(outline.len(), events.len() + 1);
    assert_eq!(outline[0].day, 1);
    assert_eq!(
        outline[0].acting_slot, None,
        "no action opens the match, so no seat took one"
    );
    for (index, event) in events.iter().enumerate() {
        assert_eq!(
            outline[index + 1].acting_slot,
            Some(event.player.0),
            "boundary {} names the seat that reached it",
            index + 1
        );
    }
    assert!(
        outline.last().expect("a boundary").day > 1,
        "eight days of play should leave the first one"
    );
}

#[test]
fn a_single_step_forward_reports_the_action_it_took() {
    let (setup, events) = played_match(false);
    let mut review = MatchReview::new(setup, events).expect("the log replays");

    review.seek(0).expect("the opening is reachable");
    let observed = review
        .seek(1)
        .expect("the first action is reachable")
        .expect("a single step reports the action it took");
    assert!(
        transition_for(&observed, p1()).is_some(),
        "the seat that acted has to be shown what it did"
    );

    assert!(
        review.seek(3).expect("the boundary is reachable").is_none(),
        "a jump reports the position it arrived at and no action"
    );
}

#[test]
fn a_recorded_action_extends_the_log_without_moving_the_cursor() {
    let (setup, mut events) = played_match(false);
    let tail = events.pop().expect("the log has an action");
    let mut review = MatchReview::new(setup, events.clone()).expect("the log replays");

    review.seek(2).expect("the boundary is reachable");
    review.append(tail).expect("the action replays");

    assert_eq!(
        review.index(),
        2,
        "the viewer stays where they were reading"
    );
    assert_eq!(review.latest_index(), events.len() + 1);
    assert_eq!(review.outline().len(), events.len() + 2);
}

#[test]
fn a_fogged_match_shows_a_watcher_nothing() {
    let (setup, events) = played_match(true);
    let mut review = MatchReview::new(setup, events).expect("the log replays");
    review.seek(1).expect("the boundary is reachable");

    assert!(review.fog_enabled());
    assert_eq!(
        review.observation(None),
        None,
        "a fogged match has no public board to show somebody watching"
    );
    assert!(
        review.observation(Some(p2())).is_some(),
        "a seat still reads its own view of a fogged match"
    );
}

#[test]
fn a_seat_reviewing_a_fogged_match_is_shown_only_its_own_view() {
    let (setup, events) = played_match(true);
    let mut review = MatchReview::new(setup.clone(), events.clone()).expect("the log replays");
    review
        .seek(events.len() / 2)
        .expect("the boundary is reachable");

    let seen = review.observation(Some(p1())).expect("a seat has a view");
    let truth = reconstruct_from_events(setup, &events[..events.len() / 2])
        .expect("the log replays")
        .player_observation(p1())
        .expect("a seat has a view");
    assert_eq!(
        seen, truth,
        "a reviewed boundary is the same projection the match would have sent"
    );
}
