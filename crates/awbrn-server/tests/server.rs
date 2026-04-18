use std::num::NonZeroU8;

use awbrn_map::{AwbrnMap, Position};
use awbrn_types::{Faction as TerrainFaction, GraphicalTerrain, PlayerFaction, Property};

use awbrn_server::{
    CaptureEvent, Co, CommandError, GameCommand, GameServer, GameSetup, PlayerId, PlayerSetup,
    PostMoveAction, ServerUnitId, SetupError,
};

fn attack_command(unit_id: ServerUnitId, path: Vec<Position>, target: Position) -> GameCommand {
    GameCommand::MoveUnit {
        unit_id,
        path,
        action: Some(PostMoveAction::Attack { target }),
    }
}

fn capture_command(unit_id: ServerUnitId, position: Position) -> GameCommand {
    GameCommand::MoveUnit {
        unit_id,
        path: vec![position],
        action: Some(PostMoveAction::Capture),
    }
}

fn two_player_setup(width: usize, height: usize) -> GameSetup {
    GameSetup {
        map: AwbrnMap::new(width, height, GraphicalTerrain::Plain),
        players: vec![
            PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: None,
                starting_funds: 1000,
                co: Co::Andy,
            },
            PlayerSetup {
                faction: PlayerFaction::BlueMoon,
                team: None,
                starting_funds: 1000,
                co: Co::Andy,
            },
        ],
        fog_enabled: false,
        rng_seed: 0,
    }
}

fn allied_player_setup(width: usize, height: usize) -> GameSetup {
    GameSetup {
        map: AwbrnMap::new(width, height, GraphicalTerrain::Plain),
        players: vec![
            PlayerSetup {
                faction: PlayerFaction::OrangeStar,
                team: Some(NonZeroU8::new(1).unwrap()),
                starting_funds: 1000,
                co: Co::Andy,
            },
            PlayerSetup {
                faction: PlayerFaction::BlueMoon,
                team: Some(NonZeroU8::new(1).unwrap()),
                starting_funds: 1000,
                co: Co::Andy,
            },
        ],
        fog_enabled: false,
        rng_seed: 0,
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
        rng_seed: 0,
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
                co: Co::Andy,
            };
            256
        ],
        fog_enabled: false,
        rng_seed: 0,
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
                co: Co::Andy,
            },
            PlayerSetup {
                faction: PlayerFaction::BlueMoon,
                team: Some(NonZeroU8::new(1).unwrap()),
                starting_funds: 1000,
                co: Co::Andy,
            },
        ],
        fog_enabled: false,
        rng_seed: 0,
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

// ── Attack integration tests ──────────────────────────────────────────────────

#[test]
fn attack_kills_defender() {
    // MegaTank primary vs Infantry = 195 base damage. On plain (1 star) with Andy
    // the minimum damage (luck=0) is 195 * 89/100 = 173, capped at 100, which kills.
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::MegaTank,
        PlayerFaction::OrangeStar,
    );
    let defender = server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    let result = server
        .submit_command(
            p1(),
            attack_command(attacker, vec![Position::new(0, 0)], Position::new(1, 0)),
        )
        .unwrap();

    // Defender should no longer appear in p2's view.
    let p2_view = server.player_view(p2());
    assert!(
        !p2_view.units.iter().any(|u| u.id == defender),
        "defender should be destroyed"
    );

    // The p2 update should include the defender in units_removed.
    let (_, p2_update) = result.updates.iter().find(|(id, _)| *id == p2()).unwrap();
    assert!(p2_update.units_removed.contains(&defender));
    assert!(p2_update.combat_event.is_some());
    let event = p2_update.combat_event.as_ref().unwrap();
    assert_eq!(event.defender_hp_after.0, 0, "defender should have 0 HP");
}

#[test]
fn attack_reduces_hp_without_killing() {
    // Infantry primary vs Infantry on plain: base = 55, damage < 100, both survive.
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let defender = server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    let result = server
        .submit_command(
            p1(),
            attack_command(attacker, vec![Position::new(0, 0)], Position::new(1, 0)),
        )
        .unwrap();

    let p1_view = server.player_view(p1());

    // Both units should still exist.
    assert_eq!(p1_view.units.len(), 2);

    // Defender should have less than full HP.
    let defender_unit = p1_view.units.iter().find(|u| u.id == defender).unwrap();
    assert!(defender_unit.hp < 10, "defender should have taken damage");

    // combat_event should be present for both players.
    let (_, p1_update) = result.updates.iter().find(|(id, _)| *id == p1()).unwrap();
    assert!(p1_update.combat_event.is_some());
    let event = p1_update.combat_event.as_ref().unwrap();
    assert!(
        event.defender_hp_after.0 > 0,
        "defender should still have HP"
    );
    assert!(
        event.attacker_hp_after.0 > 0,
        "attacker should still have HP after counterattack"
    );
}

#[test]
fn indirect_unit_cannot_attack_after_moving() {
    // Artillery is indirect: cannot move then attack.
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Artillery,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(2, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    // Move from (0,0) to (1,0) then try to attack (2,0).
    let err = server
        .submit_command(
            p1(),
            attack_command(
                attacker,
                vec![Position::new(0, 0), Position::new(1, 0)],
                Position::new(2, 0),
            ),
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn indirect_unit_can_attack_without_moving() {
    // Artillery CAN attack without moving (path is just the origin).
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Artillery,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(2, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    // No movement (path = [origin]) then attack at range 2.
    let result = server.submit_command(
        p1(),
        attack_command(attacker, vec![Position::new(0, 0)], Position::new(2, 0)),
    );

    assert!(
        result.is_ok(),
        "artillery should be able to attack without moving"
    );
}

#[test]
fn attack_out_of_range_rejected() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(2, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    // Infantry has range 1; target is 2 tiles away.
    let err = server
        .submit_command(
            p1(),
            attack_command(attacker, vec![Position::new(0, 0)], Position::new(2, 0)),
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn cannot_attack_friendly_unit() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let friendly = server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    let err = server
        .submit_command(
            p1(),
            attack_command(attacker, vec![Position::new(0, 0)], Position::new(1, 0)),
        )
        .unwrap_err();

    // Suppress unused variable warning.
    let _ = friendly;
    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn attack_no_weapon_against_type_rejected() {
    // Infantry has no weapon vs Battleship.
    let mut server = GameServer::new(two_player_setup(10, 10)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Battleship,
        PlayerFaction::BlueMoon,
    );

    let err = server
        .submit_command(
            p1(),
            attack_command(attacker, vec![Position::new(0, 0)], Position::new(1, 0)),
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn attack_no_unit_at_target_rejected() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    // Empty tile.
    let err = server
        .submit_command(
            p1(),
            attack_command(attacker, vec![Position::new(0, 0)], Position::new(1, 0)),
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn primary_weapon_attack_consumes_ammo() {
    // Mech has a bazooka (primary weapon, 3 ammo) that fires against Tanks.
    // After one attack the ammo should drop from 3 to 2.
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();

    let attacker = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Mech,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Tank,
        PlayerFaction::BlueMoon,
    );

    let initial_ammo = server
        .player_view(p1())
        .units
        .iter()
        .find(|u| u.id == attacker)
        .unwrap()
        .ammo
        .unwrap();
    assert_eq!(initial_ammo, awbrn_types::Unit::Mech.max_ammo());

    server
        .submit_command(
            p1(),
            attack_command(attacker, vec![Position::new(0, 0)], Position::new(1, 0)),
        )
        .unwrap();

    let ammo_after = server
        .player_view(p1())
        .units
        .iter()
        .find(|u| u.id == attacker)
        .unwrap()
        .ammo
        .unwrap();

    assert_eq!(
        ammo_after,
        initial_ammo - 1,
        "primary weapon should consume 1 ammo"
    );
}

// ── Capture integration tests ─────────────────────────────────────────────────

#[test]
fn full_hp_infantry_captures_property_in_two_capture_actions() {
    let mut setup = two_player_setup(3, 1);
    setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
    );
    let mut server = GameServer::new(setup).unwrap();
    let infantry = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    let first = server
        .submit_command(p1(), capture_command(infantry, Position::new(0, 0)))
        .unwrap();
    let p1_update = first
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1
        .clone();
    assert!(matches!(
        p1_update.capture_event,
        Some(CaptureEvent::CaptureContinued { progress: 10, .. })
    ));
    assert_eq!(
        server
            .player_view(p1())
            .units
            .iter()
            .find(|unit| unit.id == infantry)
            .unwrap()
            .capture_progress,
        Some(10)
    );

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();

    let second = server
        .submit_command(p1(), capture_command(infantry, Position::new(0, 0)))
        .unwrap();
    let p1_update = second
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1
        .clone();
    assert!(matches!(
        p1_update.capture_event,
        Some(CaptureEvent::PropertyCaptured {
            tile,
            new_faction: PlayerFaction::OrangeStar
        }) if tile == Position::new(0, 0)
    ));
    assert_eq!(
        p1_update.terrain_changed[0].terrain,
        GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::OrangeStar
        )))
    );

    let terrain = server
        .player_view(p1())
        .terrain
        .into_iter()
        .find(|tile| tile.position == Position::new(0, 0))
        .unwrap();
    assert_eq!(
        terrain.terrain,
        GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::OrangeStar
        )))
    );
    assert_eq!(
        server
            .player_view(p1())
            .units
            .iter()
            .find(|unit| unit.id == infantry)
            .unwrap()
            .capture_progress,
        None
    );
}

#[test]
fn mech_can_initiate_capture_on_enemy_property() {
    let mut setup = two_player_setup(3, 1);
    setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::BlueMoon,
        ))),
    );
    let mut server = GameServer::new(setup).unwrap();
    let mech = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Mech,
        PlayerFaction::OrangeStar,
    );

    let result = server
        .submit_command(p1(), capture_command(mech, Position::new(0, 0)))
        .unwrap();
    let p1_update = result
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1
        .clone();

    assert!(matches!(
        p1_update.capture_event,
        Some(CaptureEvent::CaptureContinued { progress: 10, .. })
    ));
}

#[test]
fn moving_away_loses_capture_progress() {
    let mut setup = two_player_setup(3, 1);
    setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
    );
    let mut server = GameServer::new(setup).unwrap();
    let infantry = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    server
        .submit_command(p1(), capture_command(infantry, Position::new(0, 0)))
        .unwrap();
    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();

    server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id: infantry,
                path: vec![Position::new(0, 0), Position::new(1, 0)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap();

    assert_eq!(
        server
            .player_view(p1())
            .units
            .iter()
            .find(|unit| unit.id == infantry)
            .unwrap()
            .capture_progress,
        None
    );
}

#[test]
fn damaged_infantry_takes_more_than_two_capture_actions() {
    let mut setup = two_player_setup(3, 1);
    setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
    );
    let mut server = GameServer::new(setup).unwrap();
    let infantry = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let attacker = server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(
            p2(),
            attack_command(attacker, vec![Position::new(1, 0)], Position::new(0, 0)),
        )
        .unwrap();
    let damaged_hp = server
        .player_view(p1())
        .units
        .iter()
        .find(|unit| unit.id == infantry)
        .unwrap()
        .hp;
    assert!(damaged_hp < 10, "test setup should damage the infantry");

    server.submit_command(p2(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(p1(), capture_command(infantry, Position::new(0, 0)))
        .unwrap();
    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(p1(), capture_command(infantry, Position::new(0, 0)))
        .unwrap();

    let terrain = server
        .player_view(p1())
        .terrain
        .into_iter()
        .find(|tile| tile.position == Position::new(0, 0))
        .unwrap();
    assert_eq!(
        terrain.terrain,
        GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral))
    );
    assert_eq!(
        server
            .player_view(p1())
            .units
            .iter()
            .find(|unit| unit.id == infantry)
            .unwrap()
            .capture_progress,
        Some(damaged_hp * 2)
    );
}

#[test]
fn capture_rejects_non_infantry_and_own_property() {
    let mut setup = two_player_setup(3, 1);
    setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
    );
    setup.map.set_terrain(
        Position::new(1, 0),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::OrangeStar,
        ))),
    );
    let mut server = GameServer::new(setup).unwrap();
    let tank = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Tank,
        PlayerFaction::OrangeStar,
    );
    let infantry = server.spawn_unit(
        Position::new(1, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    let err = server
        .submit_command(p1(), capture_command(tank, Position::new(0, 0)))
        .unwrap_err();
    assert!(matches!(err, CommandError::InvalidAction { .. }));

    let err = server
        .submit_command(p1(), capture_command(infantry, Position::new(1, 0)))
        .unwrap_err();
    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn capture_rejects_allied_property() {
    let mut setup = allied_player_setup(3, 1);
    setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Player(
            PlayerFaction::BlueMoon,
        ))),
    );
    let mut server = GameServer::new(setup).unwrap();
    let infantry = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    let err = server
        .submit_command(p1(), capture_command(infantry, Position::new(0, 0)))
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn fogged_opponent_does_not_receive_capture_event() {
    let mut setup = two_player_setup(8, 8);
    setup.fog_enabled = true;
    setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::Property(Property::City(TerrainFaction::Neutral)),
    );
    let mut server = GameServer::new(setup).unwrap();
    let infantry = server.spawn_unit(
        Position::new(0, 0),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(7, 7),
        awbrn_types::Unit::Infantry,
        PlayerFaction::BlueMoon,
    );

    server
        .submit_command(p1(), capture_command(infantry, Position::new(0, 0)))
        .unwrap();
    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();

    let result = server
        .submit_command(p1(), capture_command(infantry, Position::new(0, 0)))
        .unwrap();
    let p2_update = result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1
        .clone();

    assert!(p2_update.capture_event.is_none());
    assert!(p2_update.terrain_changed.is_empty());
}
