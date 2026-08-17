use std::num::NonZeroU8;

use awbrn_map::{AwbrnMap, Position};
use awbrn_types::{
    Faction as TerrainFaction, GraphicalTerrain, MissileSiloStatus, PlayerFaction, Property,
    SeaDirection, ShoalDirection, Unit, UnitExt,
};

use awbrn_server::{
    CaptureEvent, Co, CommandError, GameCommand, GameServer, GameSetup, PlayerId, PlayerSetup,
    PostMoveAction, PowerLevel, ReplayError, ReplayEventError, ServerUnitId, SetupError,
    StoredActionEvent, reconstruct_from_events, state_from_setup,
};
use awvm::semantic::{ObservedEvent, ObservedUnitRef};

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

fn action_command(
    unit_id: ServerUnitId,
    path: Vec<Position>,
    action: PostMoveAction,
) -> GameCommand {
    GameCommand::MoveUnit {
        unit_id,
        path,
        action: Some(action),
    }
}

fn unload_command(
    transport_id: ServerUnitId,
    cargo_id: ServerUnitId,
    position: Position,
) -> GameCommand {
    GameCommand::Unload {
        transport_id,
        cargo_id,
        position,
    }
}

fn build_command(position: Position, unit_type: Unit) -> GameCommand {
    GameCommand::Build {
        position,
        unit_type,
    }
}

fn submit_and_store(
    server: &mut GameServer,
    events: &mut Vec<StoredActionEvent>,
    player: PlayerId,
    command: GameCommand,
) -> awbrn_server::CommandResult {
    let result = server.submit_command(player, command.clone()).unwrap();
    events.push(StoredActionEvent {
        player,
        command,
        random: server.last_random().to_vec(),
    });
    result
}

fn expect_replay_error(setup: GameSetup, events: &[StoredActionEvent]) -> ReplayError {
    match reconstruct_from_events(setup, events) {
        Ok(_) => panic!("replay unexpectedly succeeded"),
        Err(error) => error,
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

fn two_player_setup_with_funds(width: usize, height: usize, p1_funds: u32) -> GameSetup {
    let mut setup = two_player_setup(width, height);
    setup.players[0].starting_funds = p1_funds;
    setup
}

fn set_property(setup: &mut GameSetup, position: Position, property: Property) {
    setup
        .map
        .set_terrain(position, GraphicalTerrain::Property(property));
}

fn replay_combat_setup() -> GameSetup {
    let mut setup = two_player_setup(5, 3);
    setup.players[0].starting_funds = 50_000;
    setup.players[1].starting_funds = 50_000;
    set_property(
        &mut setup,
        Position::new(0, 0),
        Property::Base(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    set_property(
        &mut setup,
        Position::new(2, 0),
        Property::City(TerrainFaction::Neutral),
    );
    set_property(
        &mut setup,
        Position::new(3, 0),
        Property::Base(TerrainFaction::Player(PlayerFaction::BlueMoon)),
    );
    setup
}

fn valid_attack_replay_prefix() -> (GameSetup, Vec<StoredActionEvent>) {
    let setup = replay_combat_setup();
    let mut server = GameServer::new(setup.clone()).unwrap();
    let mut events = Vec::new();

    submit_and_store(
        &mut server,
        &mut events,
        p1(),
        build_command(Position::new(0, 0), Unit::Infantry),
    );
    submit_and_store(&mut server, &mut events, p1(), GameCommand::EndTurn);
    submit_and_store(
        &mut server,
        &mut events,
        p2(),
        build_command(Position::new(3, 0), Unit::Infantry),
    );
    submit_and_store(&mut server, &mut events, p2(), GameCommand::EndTurn);
    submit_and_store(
        &mut server,
        &mut events,
        p1(),
        action_command(
            ServerUnitId(1),
            vec![Position::new(0, 0), Position::new(1, 0)],
            PostMoveAction::Wait,
        ),
    );
    submit_and_store(&mut server, &mut events, p1(), GameCommand::EndTurn);
    submit_and_store(&mut server, &mut events, p2(), GameCommand::EndTurn);

    (setup, events)
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
fn state_conversion_rejects_empty_player_setup() {
    let setup = GameSetup {
        map: AwbrnMap::new(5, 5, GraphicalTerrain::Plain),
        players: Vec::new(),
        fog_enabled: false,
        rng_seed: 0,
    };
    let err = state_from_setup(&setup).unwrap_err();

    assert_eq!(
        err,
        SetupError::InvalidPlayers {
            reason: "game must contain at least one player".into(),
        }
    );
}

#[test]
fn state_conversion_rejects_more_than_255_players() {
    let setup = GameSetup {
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
    };
    let err = state_from_setup(&setup).unwrap_err();

    assert_eq!(
        err,
        SetupError::InvalidPlayers {
            reason: "game supports at most 255 players, got 256".into(),
        }
    );
}

#[test]
fn server_rejects_map_dimensions_outside_awvm_domain() {
    let mut setup = two_player_setup(256, 1);
    let err = GameServer::new(setup.clone()).err().unwrap();
    assert_eq!(
        err,
        SetupError::InvalidMap {
            reason: "map width 256 exceeds AWVM's 255-tile limit".into(),
        }
    );

    setup.map = AwbrnMap::new(1, 256, GraphicalTerrain::Plain);
    let err = GameServer::new(setup).err().unwrap();
    assert_eq!(
        err,
        SetupError::InvalidMap {
            reason: "map height 256 exceeds AWVM's 255-tile limit".into(),
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

    let view = server.player_view(p1()).unwrap();
    assert_eq!(view.units.len(), 1);
    assert_eq!(view.units[0].unit_type, awbrn_types::Unit::Infantry);
    assert_eq!(view.units[0].position, Position::new(2, 2));
    assert_eq!(view.units[0].hp, Some(10));
    assert_eq!(view.units[0].fuel, Some(99)); // Infantry max fuel
    assert_eq!(view.my_funds, 1000);
    assert_eq!(view.state.day, 1);
    assert_eq!(view.state.active_player, p1());
}

#[test]
fn sonja_unit_hp_is_hidden_only_from_opponents() {
    let mut setup = two_player_setup(3, 1);
    setup.players[0].co = Co::Sonja;
    let mut server = GameServer::new(setup).unwrap();
    let unit = server.spawn_unit(
        Position::new(1, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let opponent = server.spawn_unit(Position::new(2, 0), Unit::Infantry, PlayerFaction::BlueMoon);

    let own_hp = server
        .player_view(p1())
        .unwrap()
        .units
        .into_iter()
        .find(|candidate| candidate.position == Position::new(1, 0))
        .unwrap()
        .hp;
    let opponent_hp = server
        .player_view(p2())
        .unwrap()
        .units
        .into_iter()
        .find(|candidate| candidate.position == Position::new(1, 0))
        .unwrap()
        .hp;

    assert_eq!(own_hp, Some(10));
    assert_eq!(opponent_hp, None);

    let result = server
        .submit_command(
            p1(),
            attack_command(unit, vec![Position::new(1, 0)], Position::new(2, 0)),
        )
        .unwrap();
    let event = result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .and_then(|(_, update)| update.combat_event.as_ref())
        .expect("the opponent sees the adjacent combat");
    assert_eq!(
        event.attacker_hp_after,
        awbrn_game::world::GraphicalHp::Hidden
    );
    assert_eq!(
        event
            .defender_hp_after
            .visible()
            .map(awbrn_types::VisualHp::get),
        server
            .player_view(p2())
            .unwrap()
            .units
            .into_iter()
            .find(|candidate| candidate.id == opponent)
            .unwrap()
            .hp
    );
}

#[test]
fn delete_unit_rejects_an_owned_unit_after_it_acts() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let deleted = server.spawn_unit(
        Position::new(1, 1),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(2, 1),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    server
        .submit_command(
            p1(),
            action_command(deleted, vec![Position::new(1, 1)], PostMoveAction::Wait),
        )
        .unwrap();
    let error = server
        .submit_command(p1(), GameCommand::DeleteUnit { unit_id: deleted })
        .unwrap_err();

    let view = server.player_view(p1()).unwrap();
    assert_eq!(error, CommandError::UnitAlreadyActed(deleted));
    assert!(view.units.iter().any(|unit| unit.id == deleted));
}

#[test]
fn delete_unit_removes_a_ready_owned_unit() {
    let mut server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let deleted = server.spawn_unit(
        Position::new(1, 1),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(
        Position::new(2, 1),
        awbrn_types::Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    let result = server
        .submit_command(p1(), GameCommand::DeleteUnit { unit_id: deleted })
        .unwrap();

    let view = server.player_view(p1()).unwrap();
    assert!(!view.units.iter().any(|unit| unit.id == deleted));
    let (_, update) = result.updates.iter().find(|(id, _)| *id == p1()).unwrap();
    assert!(update.units_removed.contains(&deleted));
}

#[test]
fn player_view_returns_none_for_unknown_player() {
    let mut server = GameServer::new(two_player_setup(2, 2)).unwrap();

    assert!(server.player_view(PlayerId(99)).is_none());
}

#[test]
fn build_infantry_from_owned_base_deducts_funds_and_spawns_unit() {
    let base = Position::new(0, 0);
    let mut setup = two_player_setup(3, 3);
    set_property(
        &mut setup,
        base,
        Property::Base(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    let mut server = GameServer::new(setup).unwrap();

    let result = server
        .submit_command(p1(), build_command(base, Unit::Infantry))
        .unwrap();

    let p1_update = result
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1
        .clone();
    assert_eq!(p1_update.my_funds, Some(0));
    let built = p1_update
        .units_revealed
        .iter()
        .find(|unit| unit.position == base && unit.unit_type == Unit::Infantry)
        .expect("owner should see the built unit");
    assert_eq!(built.hp, Some(10));
    assert_eq!(built.fuel, Some(Unit::Infantry.max_fuel()));
    assert_eq!(built.ammo, Some(Unit::Infantry.max_ammo()));

    let view = server.player_view(p1()).unwrap();
    assert_eq!(view.my_funds, 0);
    let built = view
        .units
        .iter()
        .find(|unit| unit.position == base && unit.unit_type == Unit::Infantry)
        .expect("built unit should appear in player_view");
    assert_eq!(built.hp, Some(10));
    assert_eq!(built.fuel, Some(Unit::Infantry.max_fuel()));
    assert_eq!(built.ammo, Some(Unit::Infantry.max_ammo()));

    let p2_update = result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1
        .clone();
    assert_eq!(p2_update.my_funds, None);
}

#[test]
fn built_unit_cannot_act_until_next_turn_and_id_is_registered() {
    let base = Position::new(0, 0);
    let mut setup = two_player_setup(3, 3);
    set_property(
        &mut setup,
        base,
        Property::Base(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    let mut server = GameServer::new(setup).unwrap();

    let result = server
        .submit_command(p1(), build_command(base, Unit::Infantry))
        .unwrap();
    let built_id = result
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1
        .units_revealed
        .iter()
        .find(|unit| unit.position == base && unit.unit_type == Unit::Infantry)
        .unwrap()
        .id;

    let err = server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id: built_id,
                path: vec![base, Position::new(1, 0)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .unwrap_err();
    assert!(matches!(err, CommandError::UnitAlreadyActed(id) if id == built_id));

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();

    server
        .submit_command(
            p1(),
            GameCommand::MoveUnit {
                unit_id: built_id,
                path: vec![base, Position::new(1, 0)],
                action: Some(PostMoveAction::Wait),
            },
        )
        .expect("built unit id should be registered and active next turn");
}

#[test]
fn build_rejects_insufficient_funds() {
    let base = Position::new(0, 0);
    let mut setup = two_player_setup_with_funds(3, 3, 1000);
    set_property(
        &mut setup,
        base,
        Property::Base(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    let mut server = GameServer::new(setup).unwrap();

    let err = server
        .submit_command(p1(), build_command(base, Unit::Mech))
        .unwrap_err();

    assert!(matches!(
        err,
        CommandError::InsufficientFunds {
            cost: 3000,
            available: 1000
        }
    ));
}

#[test]
fn activate_power_rejects_insufficient_charge() {
    let mut server = GameServer::new(two_player_setup(3, 3)).unwrap();

    let err = server
        .submit_command(
            p1(),
            GameCommand::ActivatePower {
                level: PowerLevel::Cop,
            },
        )
        .unwrap_err();

    assert_eq!(
        err,
        CommandError::InsufficientPower {
            cost: 27_000,
            available: 0
        }
    );
}

#[test]
fn build_rejects_occupied_facility() {
    let base = Position::new(0, 0);
    let mut setup = two_player_setup(3, 3);
    set_property(
        &mut setup,
        base,
        Property::Base(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    let mut server = GameServer::new(setup).unwrap();
    server.spawn_unit(base, Unit::Infantry, PlayerFaction::OrangeStar);

    let err = server
        .submit_command(p1(), build_command(base, Unit::Infantry))
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidBuildLocation));
}

#[test]
fn build_rejects_unit_domain_that_facility_cannot_produce() {
    let base = Position::new(0, 0);
    let mut setup = two_player_setup_with_funds(3, 3, 30000);
    set_property(
        &mut setup,
        base,
        Property::Base(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    let mut server = GameServer::new(setup).unwrap();

    let err = server
        .submit_command(p1(), build_command(base, Unit::Battleship))
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidBuildLocation));
}

#[test]
fn build_rejects_facility_not_owned_by_player() {
    let neutral_base = Position::new(0, 0);
    let enemy_base = Position::new(1, 0);
    let mut setup = two_player_setup(3, 3);
    set_property(
        &mut setup,
        neutral_base,
        Property::Base(TerrainFaction::Neutral),
    );
    set_property(
        &mut setup,
        enemy_base,
        Property::Base(TerrainFaction::Player(PlayerFaction::BlueMoon)),
    );
    let mut server = GameServer::new(setup).unwrap();

    let neutral_err = server
        .submit_command(p1(), build_command(neutral_base, Unit::Infantry))
        .unwrap_err();
    let enemy_err = server
        .submit_command(p1(), build_command(enemy_base, Unit::Infantry))
        .unwrap_err();

    assert!(matches!(neutral_err, CommandError::InvalidBuildLocation));
    assert!(matches!(enemy_err, CommandError::InvalidBuildLocation));
}

#[test]
fn build_supports_airport_and_port_domains() {
    let airport = Position::new(0, 0);
    let port = Position::new(1, 0);
    let mut setup = two_player_setup_with_funds(3, 3, 20000);
    set_property(
        &mut setup,
        airport,
        Property::Airport(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    set_property(
        &mut setup,
        port,
        Property::Port(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    let mut server = GameServer::new(setup).unwrap();

    server
        .submit_command(p1(), build_command(airport, Unit::TCopter))
        .unwrap();
    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(p1(), build_command(port, Unit::Lander))
        .unwrap();

    let view = server.player_view(p1()).unwrap();
    assert!(
        view.units
            .iter()
            .any(|unit| unit.position == airport && unit.unit_type == Unit::TCopter)
    );
    assert!(
        view.units
            .iter()
            .any(|unit| unit.position == port && unit.unit_type == Unit::Lander)
    );
    // AWVM applies the normative turn-start income hook before the second
    // build: the airport and port contribute 2,000 funds.
    assert_eq!(view.my_funds, 5000);
}

#[test]
fn build_fog_update_reveals_unit_only_when_opponent_has_vision() {
    let base = Position::new(0, 0);
    let mut visible_setup = two_player_setup(5, 1);
    visible_setup.fog_enabled = true;
    set_property(
        &mut visible_setup,
        base,
        Property::Base(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    let mut visible_server = GameServer::new(visible_setup).unwrap();
    visible_server.spawn_unit(Position::new(2, 0), Unit::Infantry, PlayerFaction::BlueMoon);

    let visible_result = visible_server
        .submit_command(p1(), build_command(base, Unit::Infantry))
        .unwrap();
    let p2_visible_update = visible_result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1
        .clone();
    assert!(
        p2_visible_update
            .units_revealed
            .iter()
            .any(|unit| unit.position == base && unit.unit_type == Unit::Infantry)
    );
    assert_eq!(p2_visible_update.my_funds, None);

    let mut hidden_setup = two_player_setup(8, 8);
    hidden_setup.fog_enabled = true;
    set_property(
        &mut hidden_setup,
        base,
        Property::Base(TerrainFaction::Player(PlayerFaction::OrangeStar)),
    );
    let mut hidden_server = GameServer::new(hidden_setup).unwrap();
    hidden_server.spawn_unit(Position::new(7, 7), Unit::Infantry, PlayerFaction::BlueMoon);

    let hidden_result = hidden_server
        .submit_command(p1(), build_command(base, Unit::Infantry))
        .unwrap();
    let p2_hidden_update = hidden_result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1
        .clone();
    assert!(p2_hidden_update.units_revealed.is_empty());
    assert_eq!(p2_hidden_update.my_funds, None);
}

#[test]
fn fogged_enemy_gets_new_id_after_leaving_and_returning_to_vision() {
    let mut setup = two_player_setup(5, 1);
    setup.fog_enabled = true;
    let mut server = GameServer::new(setup).unwrap();
    let enemy = server.spawn_unit(
        Position::new(2, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(Position::new(4, 0), Unit::Infantry, PlayerFaction::BlueMoon);

    let first_id = server
        .player_view(p2())
        .unwrap()
        .units
        .into_iter()
        .find(|unit| unit.position == Position::new(2, 0))
        .expect("the enemy starts in vision")
        .id;
    assert_ne!(first_id, enemy);

    let repeated_id = server
        .player_view(p2())
        .unwrap()
        .units
        .into_iter()
        .find(|unit| unit.position == Position::new(2, 0))
        .expect("the enemy remains in vision")
        .id;
    assert_eq!(repeated_id, first_id);

    let disappeared = server
        .submit_command(
            p1(),
            action_command(
                enemy,
                vec![
                    Position::new(2, 0),
                    Position::new(1, 0),
                    Position::new(0, 0),
                ],
                PostMoveAction::Wait,
            ),
        )
        .unwrap();
    let p2_update = &disappeared
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1;
    assert!(p2_update.units_removed.contains(&first_id));

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();
    let appeared = server
        .submit_command(
            p1(),
            action_command(
                enemy,
                vec![
                    Position::new(0, 0),
                    Position::new(1, 0),
                    Position::new(2, 0),
                ],
                PostMoveAction::Wait,
            ),
        )
        .unwrap();
    let second_id = appeared
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1
        .units_revealed
        .iter()
        .find(|unit| unit.position == Position::new(2, 0))
        .expect("the enemy reappears in vision")
        .id;
    assert_ne!(second_id, first_id);
    assert_ne!(second_id, enemy);
}

#[test]
fn friendly_movement_reconciles_stationary_enemy_ids() {
    let mut setup = two_player_setup(5, 1);
    setup.fog_enabled = true;
    let mut server = GameServer::new(setup).unwrap();
    let mover = server.spawn_unit(
        Position::new(2, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let lost_enemy =
        server.spawn_unit(Position::new(0, 0), Unit::Infantry, PlayerFaction::BlueMoon);
    server.spawn_unit(Position::new(4, 0), Unit::Infantry, PlayerFaction::BlueMoon);

    let initial = server.player_view(p1()).unwrap();
    let lost_id = initial
        .units
        .iter()
        .find(|unit| unit.position == Position::new(0, 0))
        .expect("the first enemy starts in vision")
        .id;
    let retained_id = initial
        .units
        .iter()
        .find(|unit| unit.position == Position::new(4, 0))
        .expect("the second enemy starts in vision")
        .id;

    let result = server
        .submit_command(
            p1(),
            action_command(
                mover,
                vec![Position::new(2, 0), Position::new(3, 0)],
                PostMoveAction::Wait,
            ),
        )
        .unwrap();
    let update = &result
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1;
    assert!(update.units_removed.contains(&lost_id));
    assert!(!update.units_removed.contains(&retained_id));

    let post = server.player_view(p1()).unwrap();
    assert!(
        !post
            .units
            .iter()
            .any(|unit| unit.position == Position::new(0, 0))
    );
    assert_eq!(
        post.units
            .iter()
            .find(|unit| unit.position == Position::new(4, 0))
            .expect("the second enemy remains in vision")
            .id,
        retained_id
    );

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(
            p2(),
            GameCommand::DeleteUnit {
                unit_id: lost_enemy,
            },
        )
        .unwrap();
    server.spawn_unit(Position::new(0, 0), Unit::Infantry, PlayerFaction::BlueMoon);
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(
            p1(),
            action_command(
                mover,
                vec![Position::new(3, 0), Position::new(2, 0)],
                PostMoveAction::Wait,
            ),
        )
        .unwrap();
    let replacement_id = server
        .player_view(p1())
        .unwrap()
        .units
        .iter()
        .find(|unit| unit.position == Position::new(0, 0))
        .expect("the replacement enemy enters vision")
        .id;
    assert_ne!(replacement_id, lost_id);
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
    let view = server.player_view(p1()).unwrap();
    assert_eq!(view.units[0].position, Position::new(2, 0));

    // Verify fuel consumed (2 tiles moved).
    assert_eq!(view.units[0].fuel, Some(97));

    // Verify the update was sent to both players.
    assert_eq!(result.updates.len(), 2);
    assert_eq!(result.observed_transitions.len(), 2);
    let (_, transition) = result
        .observed_transitions
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap();
    assert!(transition.events.iter().any(|event| {
        matches!(
            event,
            ObservedEvent::UnitMoved {
                unit: ObservedUnitRef::Friendly { .. },
                from,
                to,
                ..
            } if *from == awvm::semantic::Pos::new(0, 0)
                && *to == awvm::semantic::Pos::new(2, 0)
        )
    }));
    assert_eq!(
        server.player_observation(p1()).unwrap(),
        transition.post,
        "the websocket bootstrap and incremental transition must use the same projection"
    );
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
    let view = server.player_view(p2()).unwrap();
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

    let view = server.player_view(p1()).unwrap();
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
    let view = server.player_view(p1()).unwrap();
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

    let p1_view = server.player_view(p1()).unwrap();
    // Player 1 should see their own unit but not the enemy.
    assert_eq!(p1_view.units.len(), 1);
    assert_eq!(p1_view.units[0].faction, PlayerFaction::OrangeStar);

    let p2_view = server.player_view(p2()).unwrap();
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

    let p1_view = server.player_view(p1()).unwrap();
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

    let view = server.player_view(p1()).unwrap();
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

    let view = server.player_view(p1()).unwrap();
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
    let p2_view = server.player_view(p2()).unwrap();
    assert!(
        !p2_view.units.iter().any(|u| u.id == defender),
        "defender should be destroyed"
    );

    // The p2 update should include the defender in units_removed.
    let (_, p2_update) = result.updates.iter().find(|(id, _)| *id == p2()).unwrap();
    assert!(p2_update.units_removed.contains(&defender));
    assert!(p2_update.combat_event.is_some());
    let event = p2_update.combat_event.as_ref().unwrap();
    assert_eq!(
        event
            .defender_hp_after
            .visible()
            .map(awbrn_types::VisualHp::get),
        Some(0),
        "defender should have 0 HP"
    );
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
    let _defender = server.spawn_unit(
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

    let p1_view = server.player_view(p1()).unwrap();

    // Both units should still exist.
    assert_eq!(p1_view.units.len(), 2);

    // Defender should have less than full HP.
    let defender_unit = p1_view
        .units
        .iter()
        .find(|unit| unit.position == Position::new(1, 0))
        .unwrap();
    assert!(
        defender_unit.hp.is_some_and(|hp| hp < 10),
        "defender should have taken damage"
    );

    // combat_event should be present for both players.
    let (_, p1_update) = result.updates.iter().find(|(id, _)| *id == p1()).unwrap();
    assert!(p1_update.combat_event.is_some());
    let event = p1_update.combat_event.as_ref().unwrap();
    assert!(
        event
            .defender_hp_after
            .visible()
            .is_some_and(|hp| hp.get() > 0),
        "defender should still have HP"
    );
    assert!(
        event
            .attacker_hp_after
            .visible()
            .is_some_and(|hp| hp.get() > 0),
        "attacker should still have HP after counterattack"
    );
    assert!(matches!(
        server.last_random(),
        [
            awvm::random::RandomToken::CombatGoodLuck(_),
            awvm::random::RandomToken::CombatBadLuck(_),
            awvm::random::RandomToken::CombatGoodLuck(_),
            awvm::random::RandomToken::CombatBadLuck(_),
        ]
    ));
    assert_eq!(server.recorded_random(), server.last_random());
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
fn fogged_indirect_attacker_is_not_disclosed_in_combat_event() {
    let mut setup = two_player_setup(7, 1);
    setup.fog_enabled = true;
    let mut server = GameServer::new(setup).unwrap();
    let attacker = server.spawn_unit(Position::new(0, 0), Unit::Rocket, PlayerFaction::OrangeStar);
    server.spawn_unit(
        Position::new(4, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    server.spawn_unit(Position::new(5, 0), Unit::Tank, PlayerFaction::BlueMoon);

    let result = server
        .submit_command(
            p1(),
            attack_command(attacker, vec![Position::new(0, 0)], Position::new(5, 0)),
        )
        .unwrap();

    let p1_update = &result
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1;
    assert!(p1_update.combat_event.is_some());
    let p2_update = &result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1;
    assert!(
        p2_update.combat_event.is_none(),
        "the defender must not learn a fogged indirect attacker's id or HP"
    );
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
        .unwrap()
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
        .unwrap()
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

// ── Non-combat post-move action integration tests ────────────────────────────

#[test]
fn special_post_move_actions_reach_awvm() {
    // Manual repair is accepted and routed to move-repair.
    let mut repair_setup = two_player_setup(3, 2);
    repair_setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::Sea(SeaDirection::Sea),
    );
    let mut repair_server = GameServer::new(repair_setup).unwrap();
    let boat = repair_server.spawn_unit(
        Position::new(0, 0),
        Unit::BlackBoat,
        PlayerFaction::OrangeStar,
    );
    let repair_target = repair_server.spawn_unit(
        Position::new(1, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    repair_server
        .submit_command(
            p1(),
            action_command(
                boat,
                vec![Position::new(0, 0)],
                PostMoveAction::Repair {
                    target_id: repair_target.0,
                },
            ),
        )
        .unwrap();

    // Launch consumes the silo and applies its area strike.
    let mut launch_setup = two_player_setup(5, 5);
    launch_setup.map.set_terrain(
        Position::new(0, 0),
        GraphicalTerrain::MissileSilo(MissileSiloStatus::Loaded),
    );
    let mut launch_server = GameServer::new(launch_setup).unwrap();
    let infantry = launch_server.spawn_unit(
        Position::new(0, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let _victim =
        launch_server.spawn_unit(Position::new(3, 3), Unit::Tank, PlayerFaction::BlueMoon);
    launch_server
        .submit_command(
            p1(),
            action_command(
                infantry,
                vec![Position::new(0, 0)],
                PostMoveAction::Launch {
                    target: Position::new(3, 3),
                },
            ),
        )
        .unwrap();
    let launch_view = launch_server.player_view(p1()).unwrap();
    assert_eq!(
        launch_view
            .units
            .iter()
            .find(|unit| unit.position == Position::new(3, 3))
            .unwrap()
            .hp,
        Some(7)
    );
    assert_eq!(
        launch_view
            .terrain
            .iter()
            .find(|tile| tile.position == Position::new(0, 0))
            .unwrap()
            .terrain,
        GraphicalTerrain::MissileSilo(MissileSiloStatus::Unloaded)
    );

    // Explode removes the bomb after applying damage.
    let mut explode_server = GameServer::new(two_player_setup(5, 5)).unwrap();
    let bomb = explode_server.spawn_unit(
        Position::new(2, 2),
        Unit::BlackBomb,
        PlayerFaction::OrangeStar,
    );
    explode_server.spawn_unit(
        Position::new(0, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let _blast_victim =
        explode_server.spawn_unit(Position::new(3, 2), Unit::Tank, PlayerFaction::BlueMoon);
    explode_server
        .submit_command(
            p1(),
            action_command(bomb, vec![Position::new(2, 2)], PostMoveAction::Explode),
        )
        .unwrap();
    let explode_view = explode_server.player_view(p1()).unwrap();
    assert!(!explode_view.units.iter().any(|unit| unit.id == bomb));
    assert_eq!(
        explode_view
            .units
            .iter()
            .find(|unit| unit.position == Position::new(3, 2))
            .unwrap()
            .hp,
        Some(5)
    );
}

#[test]
fn supply_restores_self_owned_adjacent_fuel_and_ammo() {
    let mut server = GameServer::new(two_player_setup(5, 3)).unwrap();
    let apc = server.spawn_unit(Position::new(1, 1), Unit::Apc, PlayerFaction::OrangeStar);
    let infantry = server.spawn_unit(
        Position::new(0, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let mech = server.spawn_unit(Position::new(2, 1), Unit::Mech, PlayerFaction::OrangeStar);
    server.spawn_unit(Position::new(3, 1), Unit::Tank, PlayerFaction::BlueMoon);

    server
        .submit_command(
            p1(),
            action_command(
                infantry,
                vec![Position::new(0, 0), Position::new(0, 1)],
                PostMoveAction::Wait,
            ),
        )
        .unwrap();
    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();

    server
        .submit_command(
            p1(),
            attack_command(mech, vec![Position::new(2, 1)], Position::new(3, 1)),
        )
        .unwrap();

    let before = server.player_view(p1()).unwrap();
    let low_fuel = before
        .units
        .iter()
        .find(|unit| unit.id == infantry)
        .unwrap();
    assert_eq!(
        low_fuel.fuel,
        Some(Unit::Infantry.max_fuel()),
        "the adjacent APC automatically supplies at turn start"
    );
    let low_ammo = before.units.iter().find(|unit| unit.id == mech).unwrap();
    assert!(low_ammo.ammo.unwrap() < Unit::Mech.max_ammo());

    server
        .submit_command(
            p1(),
            action_command(apc, vec![Position::new(1, 1)], PostMoveAction::Supply),
        )
        .unwrap();

    let after = server.player_view(p1()).unwrap();
    let supplied_infantry = after.units.iter().find(|unit| unit.id == infantry).unwrap();
    assert_eq!(supplied_infantry.fuel, Some(Unit::Infantry.max_fuel()));
    let supplied_mech = after.units.iter().find(|unit| unit.id == mech).unwrap();
    assert_eq!(supplied_mech.ammo, Some(Unit::Mech.max_ammo()));
}

#[test]
fn supply_without_adjacent_self_owned_units_is_a_valid_noop() {
    let mut server = GameServer::new(allied_player_setup(3, 1)).unwrap();
    let apc = server.spawn_unit(Position::new(0, 0), Unit::Apc, PlayerFaction::OrangeStar);
    let neighboring_ally =
        server.spawn_unit(Position::new(1, 0), Unit::Infantry, PlayerFaction::BlueMoon);
    let before = server.player_view(p1()).unwrap();
    let neighboring_ally_before = before
        .units
        .iter()
        .find(|unit| unit.id == neighboring_ally)
        .unwrap();
    let resources_before = (neighboring_ally_before.fuel, neighboring_ally_before.ammo);

    server
        .submit_command(
            p1(),
            action_command(apc, vec![Position::new(0, 0)], PostMoveAction::Supply),
        )
        .unwrap();

    let after = server.player_view(p1()).unwrap();
    let neighboring_ally_after = after
        .units
        .iter()
        .find(|unit| unit.id == neighboring_ally)
        .unwrap();
    assert_eq!(
        (neighboring_ally_after.fuel, neighboring_ally_after.ammo),
        resources_before
    );
}

#[test]
fn supply_does_not_restore_allied_teammate_units() {
    let mut server = GameServer::new(allied_player_setup(4, 2)).unwrap();
    let apc = server.spawn_unit(Position::new(0, 0), Unit::Apc, PlayerFaction::OrangeStar);
    server.spawn_unit(
        Position::new(0, 1),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let allied_infantry =
        server.spawn_unit(Position::new(2, 0), Unit::Infantry, PlayerFaction::BlueMoon);

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(
            p2(),
            action_command(
                allied_infantry,
                vec![Position::new(2, 0), Position::new(1, 0)],
                PostMoveAction::Wait,
            ),
        )
        .unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();

    let fuel_before = server
        .player_view(p1())
        .unwrap()
        .units
        .iter()
        .find(|unit| unit.id == allied_infantry)
        .unwrap()
        .fuel;
    assert_eq!(fuel_before, Some(Unit::Infantry.max_fuel() - 1));

    server
        .submit_command(
            p1(),
            action_command(apc, vec![Position::new(0, 0)], PostMoveAction::Supply),
        )
        .unwrap();

    let fuel_after = server
        .player_view(p1())
        .unwrap()
        .units
        .iter()
        .find(|unit| unit.id == allied_infantry)
        .unwrap()
        .fuel;
    assert_eq!(fuel_after, fuel_before);
}

#[test]
fn load_removes_cargo_from_map_and_unload_restores_it() {
    let mut server = GameServer::new(two_player_setup(5, 1)).unwrap();
    let cargo = server.spawn_unit(
        Position::new(0, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let apc = server.spawn_unit(Position::new(1, 0), Unit::Apc, PlayerFaction::OrangeStar);

    server
        .submit_command(
            p1(),
            action_command(
                cargo,
                vec![Position::new(0, 0), Position::new(1, 0)],
                PostMoveAction::Load {
                    transport_id: apc.0,
                },
            ),
        )
        .unwrap();

    assert!(
        !server
            .player_view(p1())
            .unwrap()
            .units
            .iter()
            .any(|unit| unit.id == cargo),
        "loaded cargo should not occupy a map tile"
    );

    server
        .submit_command(p1(), unload_command(apc, cargo, Position::new(2, 0)))
        .unwrap();

    let view = server.player_view(p1()).unwrap();
    assert_eq!(
        view.units
            .iter()
            .find(|unit| unit.id == cargo)
            .map(|unit| unit.position),
        Some(Position::new(2, 0))
    );

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(
            p1(),
            action_command(
                cargo,
                vec![Position::new(2, 0), Position::new(3, 0)],
                PostMoveAction::Wait,
            ),
        )
        .expect("unloaded cargo id should remain registered");
}

#[test]
fn spent_transport_can_unload_successive_cargo_units() {
    let mut setup = two_player_setup(5, 3);
    setup.map.set_terrain(
        Position::new(2, 1),
        GraphicalTerrain::Shoal(ShoalDirection::S),
    );
    let mut server = GameServer::new(setup).unwrap();
    let first = server.spawn_unit(
        Position::new(1, 1),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let transport = server.spawn_unit(
        Position::new(2, 1),
        Unit::BlackBoat,
        PlayerFaction::OrangeStar,
    );
    let second = server.spawn_unit(Position::new(3, 1), Unit::Mech, PlayerFaction::OrangeStar);

    for cargo in [first, second] {
        let origin = server
            .player_view(p1())
            .unwrap()
            .units
            .iter()
            .find(|unit| unit.id == cargo)
            .unwrap()
            .position;
        server
            .submit_command(
                p1(),
                action_command(
                    cargo,
                    vec![origin, Position::new(2, 1)],
                    PostMoveAction::Load {
                        transport_id: transport.0,
                    },
                ),
            )
            .unwrap();
    }
    server
        .submit_command(
            p1(),
            action_command(transport, vec![Position::new(2, 1)], PostMoveAction::Wait),
        )
        .unwrap();

    server
        .submit_command(p1(), unload_command(transport, first, Position::new(2, 0)))
        .unwrap();
    server
        .submit_command(p1(), unload_command(transport, second, Position::new(2, 2)))
        .unwrap();

    let view = server.player_view(p1()).unwrap();
    assert_eq!(
        view.units
            .iter()
            .find(|unit| unit.id == first)
            .unwrap()
            .position,
        Position::new(2, 0)
    );
    assert_eq!(
        view.units
            .iter()
            .find(|unit| unit.id == second)
            .unwrap()
            .position,
        Position::new(2, 2)
    );
}

#[test]
fn load_rejects_full_transport() {
    let mut server = GameServer::new(two_player_setup(5, 1)).unwrap();
    let first = server.spawn_unit(
        Position::new(0, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let apc = server.spawn_unit(Position::new(1, 0), Unit::Apc, PlayerFaction::OrangeStar);
    let second = server.spawn_unit(Position::new(2, 0), Unit::Mech, PlayerFaction::OrangeStar);

    server
        .submit_command(
            p1(),
            action_command(
                first,
                vec![Position::new(0, 0), Position::new(1, 0)],
                PostMoveAction::Load {
                    transport_id: apc.0,
                },
            ),
        )
        .unwrap();

    let err = server
        .submit_command(
            p1(),
            action_command(
                second,
                vec![Position::new(2, 0), Position::new(1, 0)],
                PostMoveAction::Load {
                    transport_id: apc.0,
                },
            ),
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn post_move_unit_ids_outside_awvm_domain_are_invalid_actions() {
    let actions = [
        PostMoveAction::Load {
            transport_id: u64::from(u32::MAX) + 1,
        },
        PostMoveAction::Join {
            target_id: u64::from(u32::MAX) + 1,
        },
    ];

    for action in actions {
        let mut server = GameServer::new(two_player_setup(2, 1)).unwrap();
        let unit = server.spawn_unit(
            Position::new(0, 0),
            Unit::Infantry,
            PlayerFaction::OrangeStar,
        );
        let error = server
            .submit_command(
                p1(),
                action_command(unit, vec![Position::new(0, 0)], action),
            )
            .unwrap_err();

        assert!(matches!(error, CommandError::InvalidAction { .. }));
    }

    let mut server = GameServer::new(two_player_setup(2, 1)).unwrap();
    let error = server
        .submit_command(
            p1(),
            GameCommand::Unload {
                transport_id: ServerUnitId(0),
                cargo_id: ServerUnitId(u64::from(u32::MAX) + 1),
                position: Position::new(1, 0),
            },
        )
        .unwrap_err();
    assert!(matches!(error, CommandError::InvalidAction { .. }));
}

#[test]
fn load_does_not_leak_fogged_destination_coordinates() {
    let mut setup = two_player_setup(4, 1);
    setup.fog_enabled = true;
    let mut server = GameServer::new(setup).unwrap();
    let cargo = server.spawn_unit(
        Position::new(1, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let transport = server.spawn_unit(Position::new(2, 0), Unit::Apc, PlayerFaction::OrangeStar);
    server.spawn_unit(Position::new(0, 0), Unit::Apc, PlayerFaction::BlueMoon);
    let observed_cargo = server
        .player_view(p2())
        .unwrap()
        .units
        .into_iter()
        .find(|unit| unit.position == Position::new(1, 0))
        .unwrap()
        .id;

    let result = server
        .submit_command(
            p1(),
            action_command(
                cargo,
                vec![Position::new(1, 0), Position::new(2, 0)],
                PostMoveAction::Load {
                    transport_id: transport.0,
                },
            ),
        )
        .unwrap();

    let p2_update = result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1
        .clone();

    assert!(p2_update.units_removed.contains(&observed_cargo));
    assert!(
        p2_update.units_moved.is_empty(),
        "load should not serialize a hidden destination coordinate"
    );
}

#[test]
fn unload_rejects_occupied_or_impassable_target() {
    let mut setup = two_player_setup(4, 3);
    setup.map.set_terrain(
        Position::new(1, 0),
        GraphicalTerrain::Sea(SeaDirection::Sea),
    );
    let mut server = GameServer::new(setup).unwrap();
    let cargo = server.spawn_unit(
        Position::new(0, 1),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let apc = server.spawn_unit(Position::new(1, 1), Unit::Apc, PlayerFaction::OrangeStar);
    server.spawn_unit(Position::new(2, 1), Unit::Tank, PlayerFaction::OrangeStar);

    server
        .submit_command(
            p1(),
            action_command(
                cargo,
                vec![Position::new(0, 1), Position::new(1, 1)],
                PostMoveAction::Load {
                    transport_id: apc.0,
                },
            ),
        )
        .unwrap();

    let occupied_err = server
        .submit_command(p1(), unload_command(apc, cargo, Position::new(2, 1)))
        .unwrap_err();
    assert!(matches!(occupied_err, CommandError::InvalidAction { .. }));
    assert_eq!(
        server
            .player_view(p1())
            .unwrap()
            .units
            .iter()
            .find(|unit| unit.id == apc)
            .unwrap()
            .position,
        Position::new(1, 1),
        "a rejected compound move-plus-unload must roll back the move"
    );

    let impassable_err = server
        .submit_command(p1(), unload_command(apc, cargo, Position::new(1, 0)))
        .unwrap_err();
    assert!(matches!(impassable_err, CommandError::InvalidPath { .. }));
}

#[test]
fn hide_and_unhide_change_enemy_player_view() {
    let mut server = GameServer::new(two_player_setup(5, 1)).unwrap();
    let sub = server.spawn_unit(Position::new(0, 0), Unit::Sub, PlayerFaction::OrangeStar);
    server.spawn_unit(Position::new(4, 0), Unit::Infantry, PlayerFaction::BlueMoon);

    let first_enemy_id = server
        .player_view(p2())
        .unwrap()
        .units
        .into_iter()
        .find(|unit| unit.position == Position::new(0, 0))
        .unwrap()
        .id;
    assert_ne!(first_enemy_id, sub);

    let hide_result = server
        .submit_command(
            p1(),
            action_command(sub, vec![Position::new(0, 0)], PostMoveAction::Hide),
        )
        .unwrap();
    let p1_hide_update = hide_result
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1
        .clone();
    assert!(
        p1_hide_update
            .units_revealed
            .iter()
            .any(|unit| unit.id == sub && unit.hiding),
        "owner update should include the hidden state"
    );
    assert!(
        server
            .player_view(p1())
            .unwrap()
            .units
            .iter()
            .any(|unit| unit.id == sub && unit.hiding),
        "owner view should keep hidden unit visible with hiding=true"
    );
    assert!(
        !server
            .player_view(p2())
            .unwrap()
            .units
            .iter()
            .any(|unit| unit.position == Position::new(0, 0)),
        "hidden sub should disappear from enemy view when not detected"
    );

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();
    let unhide_result = server
        .submit_command(
            p1(),
            action_command(sub, vec![Position::new(0, 0)], PostMoveAction::Unhide),
        )
        .unwrap();
    let p1_unhide_update = unhide_result
        .updates
        .iter()
        .find(|(player, _)| *player == p1())
        .unwrap()
        .1
        .clone();
    assert!(
        p1_unhide_update
            .units_revealed
            .iter()
            .any(|unit| unit.id == sub && !unit.hiding),
        "owner update should include the unhidden state"
    );
    assert!(
        server
            .player_view(p1())
            .unwrap()
            .units
            .iter()
            .any(|unit| unit.id == sub && !unit.hiding),
        "owner view should keep unit visible with hiding=false"
    );

    let second_enemy_id = server
        .player_view(p2())
        .unwrap()
        .units
        .into_iter()
        .find(|unit| unit.position == Position::new(0, 0))
        .expect("unhidden sub should reappear")
        .id;
    assert_ne!(second_enemy_id, first_enemy_id);
    assert_ne!(second_enemy_id, sub);
}

#[test]
fn untracked_disappearance_does_not_allocate_enemy_id() {
    let mut server = GameServer::new(two_player_setup(5, 1)).unwrap();
    let sub = server.spawn_unit(Position::new(0, 0), Unit::Sub, PlayerFaction::OrangeStar);

    let hidden = server
        .submit_command(
            p1(),
            action_command(sub, vec![Position::new(0, 0)], PostMoveAction::Hide),
        )
        .unwrap();
    let p2_update = &hidden
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1;
    assert!(p2_update.units_removed.is_empty());

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();
    server
        .submit_command(
            p1(),
            action_command(sub, vec![Position::new(0, 0)], PostMoveAction::Unhide),
        )
        .unwrap();
    let id = server
        .player_view(p2())
        .unwrap()
        .units
        .iter()
        .find(|unit| unit.position == Position::new(0, 0))
        .expect("the unhidden enemy is visible")
        .id;
    assert_eq!(id, ServerUnitId(u64::MAX));
}

#[test]
fn hide_and_unhide_refresh_detecting_enemy_visible_unit() {
    let mut server = GameServer::new(two_player_setup(3, 1)).unwrap();
    let sub = server.spawn_unit(Position::new(0, 0), Unit::Sub, PlayerFaction::OrangeStar);
    server.spawn_unit(Position::new(1, 0), Unit::Infantry, PlayerFaction::BlueMoon);
    let observed_sub = server
        .player_view(p2())
        .unwrap()
        .units
        .into_iter()
        .find(|unit| unit.position == Position::new(0, 0))
        .unwrap()
        .id;

    let hide_result = server
        .submit_command(
            p1(),
            action_command(sub, vec![Position::new(0, 0)], PostMoveAction::Hide),
        )
        .unwrap();
    let p2_hide_update = hide_result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1
        .clone();
    assert!(
        p2_hide_update
            .units_revealed
            .iter()
            .any(|unit| unit.id == observed_sub && unit.hiding),
        "detected hidden unit should refresh hiding=true for enemy viewer"
    );
    assert!(
        server
            .player_view(p2())
            .unwrap()
            .units
            .iter()
            .any(|unit| unit.id == observed_sub && unit.hiding)
    );

    server.submit_command(p1(), GameCommand::EndTurn).unwrap();
    server.submit_command(p2(), GameCommand::EndTurn).unwrap();

    let unhide_result = server
        .submit_command(
            p1(),
            action_command(sub, vec![Position::new(0, 0)], PostMoveAction::Unhide),
        )
        .unwrap();
    let p2_unhide_update = unhide_result
        .updates
        .iter()
        .find(|(player, _)| *player == p2())
        .unwrap()
        .1
        .clone();
    assert!(
        p2_unhide_update
            .units_revealed
            .iter()
            .any(|unit| unit.id == observed_sub && !unit.hiding),
        "detected unhidden unit should refresh hiding=false for enemy viewer"
    );
}

#[test]
fn hide_rejects_non_hidden_capable_units() {
    let mut server = GameServer::new(two_player_setup(3, 1)).unwrap();
    let tank = server.spawn_unit(Position::new(0, 0), Unit::Tank, PlayerFaction::OrangeStar);

    let err = server
        .submit_command(
            p1(),
            action_command(tank, vec![Position::new(0, 0)], PostMoveAction::Hide),
        )
        .unwrap_err();

    assert!(matches!(err, CommandError::InvalidAction { .. }));
}

#[test]
fn join_rejects_a_target_already_at_full_visual_hp() {
    let mut server = GameServer::new(two_player_setup(4, 1)).unwrap();
    let source = server.spawn_unit(
        Position::new(0, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let target = server.spawn_unit(
        Position::new(1, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );

    let error = server
        .submit_command(
            p1(),
            action_command(
                source,
                vec![Position::new(0, 0), Position::new(1, 0)],
                PostMoveAction::Join {
                    target_id: target.0,
                },
            ),
        )
        .unwrap_err();

    let view = server.player_view(p1()).unwrap();
    assert!(matches!(error, CommandError::InvalidAction { .. }));
    assert!(view.units.iter().any(|unit| unit.id == source));
    assert!(view.units.iter().any(|unit| unit.id == target));
}

#[test]
fn join_rejects_different_type_or_owner() {
    let mut server = GameServer::new(two_player_setup(5, 1)).unwrap();
    let source = server.spawn_unit(
        Position::new(0, 0),
        Unit::Infantry,
        PlayerFaction::OrangeStar,
    );
    let tank = server.spawn_unit(Position::new(1, 0), Unit::Tank, PlayerFaction::OrangeStar);
    let enemy_infantry =
        server.spawn_unit(Position::new(2, 0), Unit::Infantry, PlayerFaction::BlueMoon);

    let different_type_err = server
        .submit_command(
            p1(),
            action_command(
                source,
                vec![Position::new(0, 0), Position::new(1, 0)],
                PostMoveAction::Join { target_id: tank.0 },
            ),
        )
        .unwrap_err();
    assert!(matches!(
        different_type_err,
        CommandError::InvalidAction { .. }
    ));

    let different_owner_err = server
        .submit_command(
            p1(),
            action_command(
                source,
                vec![
                    Position::new(0, 0),
                    Position::new(1, 0),
                    Position::new(2, 0),
                ],
                PostMoveAction::Join {
                    target_id: enemy_infantry.0,
                },
            ),
        )
        .unwrap_err();
    assert!(matches!(
        different_owner_err,
        CommandError::InvalidAction { .. }
    ));
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
            .unwrap()
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
        .unwrap()
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
            .unwrap()
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
            .unwrap()
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
        .unwrap()
        .units
        .iter()
        .find(|unit| unit.id == infantry)
        .unwrap()
        .hp
        .expect("a player sees the exact HP of its own unit");
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
        .unwrap()
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
            .unwrap()
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

#[test]
fn reconstruct_replays_action_log_to_matching_player_views() {
    let setup = replay_combat_setup();
    let mut original = GameServer::new(setup.clone()).unwrap();
    let mut events = Vec::new();

    submit_and_store(
        &mut original,
        &mut events,
        p1(),
        build_command(Position::new(0, 0), Unit::Infantry),
    );
    submit_and_store(&mut original, &mut events, p1(), GameCommand::EndTurn);
    submit_and_store(
        &mut original,
        &mut events,
        p2(),
        build_command(Position::new(3, 0), Unit::Infantry),
    );
    submit_and_store(&mut original, &mut events, p2(), GameCommand::EndTurn);
    submit_and_store(
        &mut original,
        &mut events,
        p1(),
        action_command(
            ServerUnitId(1),
            vec![Position::new(0, 0), Position::new(1, 0)],
            PostMoveAction::Wait,
        ),
    );
    submit_and_store(&mut original, &mut events, p1(), GameCommand::EndTurn);
    submit_and_store(&mut original, &mut events, p2(), GameCommand::EndTurn);

    let attack_result = submit_and_store(
        &mut original,
        &mut events,
        p1(),
        attack_command(
            ServerUnitId(1),
            vec![Position::new(1, 0), Position::new(2, 0)],
            Position::new(3, 0),
        ),
    );
    assert!(
        attack_result
            .updates
            .iter()
            .any(|(_, update)| update.combat_event.is_some())
    );

    submit_and_store(&mut original, &mut events, p1(), GameCommand::EndTurn);
    submit_and_store(&mut original, &mut events, p2(), GameCommand::EndTurn);
    submit_and_store(
        &mut original,
        &mut events,
        p1(),
        capture_command(ServerUnitId(1), Position::new(2, 0)),
    );
    submit_and_store(
        &mut original,
        &mut events,
        p1(),
        build_command(Position::new(0, 0), Unit::Infantry),
    );

    let encoded = serde_json::to_string(&events).unwrap();
    let decoded: Vec<StoredActionEvent> = serde_json::from_str(&encoded).unwrap();
    let mut reconstructed = reconstruct_from_events(setup, &decoded).unwrap();

    for player in [p1(), p2()] {
        assert_eq!(
            serde_json::to_value(original.player_view(player).unwrap()).unwrap(),
            serde_json::to_value(reconstructed.player_view(player).unwrap()).unwrap()
        );
    }
}

#[test]
fn replay_attack_requires_stored_randomness() {
    let (setup, mut events) = valid_attack_replay_prefix();
    let index = events.len();
    events.push(StoredActionEvent {
        player: p1(),
        command: attack_command(
            ServerUnitId(1),
            vec![Position::new(1, 0), Position::new(2, 0)],
            Position::new(3, 0),
        ),
        random: Vec::new(),
    });

    let err = expect_replay_error(setup, &events);

    assert!(matches!(
        err,
        ReplayError::Event {
            index: err_index,
            source: ReplayEventError::Command(CommandError::InvalidAction { .. }),
        } if err_index == index
    ));
}

#[test]
fn replay_uses_stored_submitter_and_rejects_corrupted_turn_order() {
    let setup = replay_combat_setup();
    let events = vec![StoredActionEvent {
        player: p2(),
        command: build_command(Position::new(3, 0), Unit::Infantry),
        random: Vec::new(),
    }];

    let err = expect_replay_error(setup, &events);

    assert!(matches!(
        err,
        ReplayError::Event {
            index: 0,
            source: ReplayEventError::Command(CommandError::NotYourTurn),
        }
    ));
}

#[test]
fn stored_events_require_submitter_and_random_tape() {
    let without_player = serde_json::json!({
        "command": {
            "type": "build",
            "position": {"x": 0, "y": 0},
            "unit_type": "infantry"
        },
        "random": []
    });
    let without_random = serde_json::json!({
        "player": 0,
        "command": {
            "type": "build",
            "position": {"x": 0, "y": 0},
            "unit_type": "infantry"
        }
    });

    assert!(serde_json::from_value::<StoredActionEvent>(without_player).is_err());
    assert!(serde_json::from_value::<StoredActionEvent>(without_random).is_err());
}

#[test]
fn replay_rejects_randomness_on_non_random_command() {
    let setup = replay_combat_setup();
    let events = vec![StoredActionEvent {
        player: p1(),
        command: build_command(Position::new(0, 0), Unit::Infantry),
        random: vec![awvm::random::RandomToken::CombatGoodLuck(1)],
    }];

    let err = expect_replay_error(setup, &events);

    assert!(matches!(
        err,
        ReplayError::Event {
            index: 0,
            source: ReplayEventError::Command(CommandError::InvalidAction { .. }),
        }
    ));
}

#[test]
fn replay_invalid_command_returns_error() {
    let setup = replay_combat_setup();
    let events = vec![StoredActionEvent {
        player: p1(),
        command: action_command(
            ServerUnitId(99),
            vec![Position::new(0, 0)],
            PostMoveAction::Wait,
        ),
        random: Vec::new(),
    }];

    let err = expect_replay_error(setup, &events);

    assert!(matches!(
        err,
        ReplayError::Event {
            index: 0,
            source: ReplayEventError::Command(CommandError::InvalidUnit(ServerUnitId(99))),
        }
    ));
}

#[test]
fn replay_rejects_out_of_domain_randomness() {
    let (setup, mut events) = valid_attack_replay_prefix();
    let index = events.len();
    events.push(StoredActionEvent {
        player: p1(),
        command: attack_command(
            ServerUnitId(1),
            vec![Position::new(1, 0), Position::new(2, 0)],
            Position::new(3, 0),
        ),
        random: vec![
            awvm::random::RandomToken::CombatGoodLuck(200),
            awvm::random::RandomToken::CombatBadLuck(0),
            awvm::random::RandomToken::CombatGoodLuck(0),
            awvm::random::RandomToken::CombatBadLuck(0),
        ],
    });

    let err = expect_replay_error(setup, &events);

    assert!(matches!(
        err,
        ReplayError::Event {
            index: err_index,
            source: ReplayEventError::Command(CommandError::InvalidAction { .. }),
        } if err_index == index
    ));
}
