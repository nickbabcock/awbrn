use std::num::NonZeroU8;

use awbrn_map::{AwbrnMap, Position};
use awbrn_types::{GraphicalTerrain, PlayerFaction};

use awbrn_server::{
    CommandError, GameCommand, GameServer, GameSetup, PlayerId, PlayerSetup, PostMoveAction,
    ServerUnitId, SetupError,
};

fn two_player_setup(width: usize, height: usize) -> GameSetup {
    GameSetup {
        map: AwbrnMap::new(width, height, GraphicalTerrain::Plain),
        players: vec![
            PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: None,
                starting_funds: 1000,
            },
            PlayerSetup {
                faction: PlayerFaction::BlueMoon,
                team: None,
                starting_funds: 1000,
            },
        ],
        fog_enabled: false,
    }
}

fn p1() -> PlayerId {
    PlayerId(0)
}

fn p2() -> PlayerId {
    PlayerId(1)
}

#[test]
fn server_rejects_empty_player_setup() {
    let err = GameServer::new(GameSetup {
        map: AwbrnMap::new(5, 5, GraphicalTerrain::Plain),
        players: Vec::new(),
        fog_enabled: false,
    })
    .err()
    .unwrap();

    assert_eq!(
        err,
        SetupError::InvalidPlayers {
            reason: "game must contain at least one player".into(),
        }
    );
}

#[test]
fn server_rejects_more_than_255_players() {
    let err = GameServer::new(GameSetup {
        map: AwbrnMap::new(5, 5, GraphicalTerrain::Plain),
        players: vec![
            PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: None,
                starting_funds: 1000,
            };
            256
        ],
        fog_enabled: false,
    })
    .err()
    .unwrap();

    assert_eq!(
        err,
        SetupError::InvalidPlayers {
            reason: "game supports at most 255 players, got 256".into(),
        }
    );
}

#[test]
fn create_server_and_spawn_unit() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let id = server.spawn_unit(
        Position::new(2, 2),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    assert_eq!(id, ServerUnitId(1));

    let view = server.player_view(p1());
    assert_eq!(view.units.len(), 1);
    assert_eq!(view.units[0].unit_type, awbrn_types::Unit::Infantry);
    assert_eq!(view.units[0].position, Position::new(2, 2));
    assert_eq!(view.units[0].hp, 10);
    assert_eq!(view.units[0].fuel, Some(99)); // Infantry max fuel
    assert_eq!(view.my_funds, 1000);
    assert_eq!(view.state.day, 1);
    assert_eq!(view.state.active_player, p1());
}

#[test]
fn move_unit_updates_position() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let unit_id = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    let result = server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id,
                path: vec![
                    Position::new(0, 0),
                    Position::new(1, 0),
                    Position::new(2, 0),
                ],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap();

    // Verify unit moved.
    let view = server.player_view(p1());
    assert_eq!(view.units[0].position, Position::new(2, 0));

    // Verify fuel consumed (2 tiles moved).
    assert_eq!(view.units[0].fuel, Some(97));

    // Verify the update was sent to both players.
    assert_eq!(result.updates.len(), 2);
}

#[test]
fn move_unit_deactivates_it() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let unit_id = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id,
                path: vec![Position::new(0, 0), Position::new(1, 0)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap();

    // Trying to move again should fail.
    let err = server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id,
                path: vec![Position::new(1, 0), Position::new(2, 0)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::UnitAlreadyActed(_)));
}

#[test]
fn not_your_turn_rejected() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let unit_id = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    // Player 2 tries to act during player 1's turn.
    let err = server
        .submit_command(
            p2(),
            GameCommand::MoveUnit {
                unit_id,
                path: vec![Position::new(0, 0), Position::new(1, 0)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::NotYourTurn));
}

#[test]
fn cannot_move_enemy_unit() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let enemy_unit = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    // Player 1 tries to move player 2's unit.
    let err = server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id: enemy_unit,
                path: vec![Position::new(0, 0), Position::new(1, 0)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidUnit(_)));
}

#[test]
fn end_turn_switches_active_player() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let result = server.submit_command(p1(), GameCommand::EndTurn).unwrap();

    // Check turn changed.
    let view = server.player_view(p2());
    assert_eq!(view.state.active_player, p2());
    assert_eq!(view.state.day, 1); // Still day 1 (player 2's first turn).

    // Check the update indicates a turn change.
    let (_, p2_update) = result.updates.iter().find(|(id, _)| *id == p2()).unwrap();
    assert!(p2_update.turn_change.is_some());
    assert_eq!(
        p2_update.turn_change.as_ref().unwrap().new_active_player,
        p2()
    );
}

#[test]
fn full_round_increments_day() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    // Player 1 ends turn.
    server.submit_command(p1(), GameCommand::EndTurn).unwrap();

    // Player 2 ends turn → wraps around to player 1, new day.
    let result = server.submit_command(p2(), GameCommand::EndTurn).unwrap();

    let view = server.player_view(p1());
    assert_eq!(view.state.day, 2);
    assert_eq!(view.state.active_player, p1());

    let (_, p1_update) = result.updates.iter().find(|(id, _)| *id == p1()).unwrap();
    assert_eq!(p1_update.turn_change.as_ref().unwrap().new_day, Some(2));
}

#[test]
fn end_turn_reactivates_next_player_units() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    // Spawn a unit for player 2.
    let p2_unit = server.spawn_unit(
        Position::new(3, 3),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    // Player 1 ends turn.
    server.submit_command(p1(), GameCommand::EndTurn).unwrap();

    // Player 2's unit should be active: submitting a move should succeed.
    server
        .submit_command(
            p2(),
            GameCommand::MoveUnit {
                unit_id: p2_unit,
                path: vec![Position::new(3, 3), Position::new(4, 3)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .expect("unit should be active after end turn");
}

#[test]
fn move_with_no_displacement_still_deactivates() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let unit_id = server.spawn_unit(
        Position::new(2, 2),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    // "Move" to the same position (wait in place).
    server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id,
                path: vec![Position::new(2, 2)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap();

    // Unit should be at the same position but deactivated.
    let view = server.player_view(p1());
    assert_eq!(view.units[0].position, Position::new(2, 2));

    // Should not be able to act again.
    let err = server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id,
                path: vec![Position::new(2, 2), Position::new(3, 2)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap_err();
    assert!(matches!(err, CommandError::UnitAlreadyActed(_)));
}

#[test]
fn invalid_path_start_rejected() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let unit_id = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    // Path starts at wrong position.
    let err = server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id,
                path: vec![Position::new(1, 1), Position::new(2, 1)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidPath { .. }));
}

#[test]
fn fog_hides_enemy_units() {
    let mut setup = two_player_setup(10, 1);
    setup.fog_enabled = true;

    let mut server = GameServer::new(setup).unwrap();

    // Player 1 unit at (0,0) with vision 2.
    server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    // Player 2 unit at (5,0) -- outside player 1's vision.
    server.spawn_unit(
        Position::new(5, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    let p1_view = server.player_view(p1());
    // Player 1 should see their own unit but not the enemy.
    assert_eq!(p1_view.units.len(), 1);
    assert_eq!(p1_view.units[0].faction, PlayerFaction::OrangeStar);

    let p2_view = server.player_view(p2());
    // Player 2 should see their own unit but not player 1's.
    assert_eq!(p2_view.units.len(), 1);
    assert_eq!(p2_view.units[0].faction, PlayerFaction::BlueMoon);
}

#[test]
fn fog_reveals_units_within_vision() {
    let mut setup = two_player_setup(5, 1);
    setup.fog_enabled = true;

    let mut server = GameServer::new(setup).unwrap();

    // Player 1 unit at (0,0).
    server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    // Player 2 unit at (2,0) -- within player 1's vision (infantry vision = 2).
    server.spawn_unit(
        Position::new(2, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    let p1_view = server.player_view(p1());
    // Player 1 should see both units.
    assert_eq!(p1_view.units.len(), 2);
}

#[test]
fn own_unit_fuel_visible_enemy_fuel_hidden() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    let view = server.player_view(p1());
    let own = view
        .units
        .iter()
        .find(|u| u.faction == PlayerFaction::OrangeStar)
        .unwrap();
    let enemy = view
        .units
        .iter()
        .find(|u| u.faction == PlayerFaction::BlueMoon)
        .unwrap();

    // Own unit shows fuel/ammo.
    assert!(own.fuel.is_some());
    assert!(own.ammo.is_some());

    // Enemy unit hides fuel/ammo.
    assert!(enemy.fuel.is_none());
    assert!(enemy.ammo.is_none());
}

#[test]
fn allied_units_share_fuel_and_ammo_visibility() {
    let mut server = GameServer::new(GameSetup {
        map: AwbrnMap::new(5, 5, GraphicalTerrain::Plain),
        players: vec![
            PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: Some(NonZeroU8::new(1).unwrap()),
                starting_funds: 1000,
            },
            PlayerSetup {
                faction: PlayerFaction::BlueMoon,
                team: Some(NonZeroU8::new(1).unwrap()),
                starting_funds: 1000,
            },
        ],
        fog_enabled: false,
    })
    .unwrap();

    server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Tank,
        PlayerFaction::BlueMoon,
    );

    let view = server.player_view(p1());
    let allied = view
        .units
        .iter()
        .find(|u| u.faction == PlayerFaction::BlueMoon)
        .unwrap();

    assert!(allied.fuel.is_some());
    assert!(allied.ammo.is_some());
}
