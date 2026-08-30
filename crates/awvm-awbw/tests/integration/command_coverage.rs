use std::collections::BTreeMap;

use awbw_replay::ReplayParser;
use awvm::semantic::PlayerId;
use awvm::transition::Command;
use awvm_awbw::diagnostic_command;

const ACTION_KINDS: [&str; 19] = [
    "AttackSeam",
    "Build",
    "Capt",
    "Delete",
    "End",
    "Explode",
    "Fire",
    "Hide",
    "Join",
    "Launch",
    "Load",
    "Move",
    "Power",
    "Repair",
    "Resign",
    "Supply",
    "Tag",
    "Unhide",
    "Unload",
];

#[test]
fn every_archived_action_maps_or_has_a_named_counted_reason() {
    let mut mapped = BTreeMap::<&str, usize>::new();
    let mut tail = BTreeMap::<(&str, &str), usize>::new();
    let mut total = 0;

    insta::glob!("../../../../assets/replays", "*.zip", |replay_path| {
        let replay_file = replay_path.file_name().unwrap().to_string_lossy();
        let replay = ReplayParser::new()
            .parse(&std::fs::read(replay_path).unwrap())
            .unwrap_or_else(|error| panic!("{replay_file}: {error}"));
        let diagnostic_player = replay
            .games
            .first()
            .and_then(|game| game.players.first())
            .map(|player| PlayerId::from(player.id.as_u32().to_string()))
            .expect("archived replay has a player");

        for action in &replay.turns {
            total += 1;
            match diagnostic_command(diagnostic_player.clone(), action) {
                Ok(value) => {
                    *mapped.entry(action.kind_name()).or_default() += 1;
                    assert_eq!(
                        command_kind(&value),
                        expected_command_kind(action.kind_name()),
                        "{replay_file} {} lowered to the wrong AWVM command",
                        action.kind_name()
                    );
                    let wire = serde_json::to_vec(&value).unwrap();
                    assert_eq!(
                        serde_json::from_slice::<Command>(&wire).unwrap(),
                        value,
                        "{replay_file} {} command did not round trip",
                        action.kind_name()
                    );
                }
                Err(error) => {
                    *tail
                        .entry((action.kind_name(), error.reason()))
                        .or_default() += 1;
                }
            }
        }
    });

    if std::env::var_os("INSTA_GLOB_FILTER").is_none() {
        assert_eq!(total, 12_978);
        for kind in ACTION_KINDS {
            assert!(
                mapped.get(kind).copied().unwrap_or_default() > 0,
                "{kind} has no mapped archive example; mapped={mapped:#?}, tail={tail:#?}"
            );
        }
        assert_eq!(
            mapped,
            BTreeMap::from([
                ("AttackSeam", 47),
                ("Build", 1_671),
                ("Capt", 561),
                ("Delete", 1),
                ("End", 627),
                ("Explode", 1),
                ("Fire", 2_334),
                ("Hide", 2),
                ("Join", 77),
                ("Launch", 1),
                ("Load", 242),
                ("Move", 6_475),
                ("Power", 71),
                ("Repair", 5),
                ("Resign", 14),
                ("Supply", 62),
                ("Tag", 5),
                ("Unhide", 1),
                ("Unload", 230),
            ]),
            "mapped compatibility histogram changed"
        );
        assert_eq!(
            tail,
            BTreeMap::from([
                (("Capt", "missing-move"), 548),
                (("Repair", "missing-move"), 1),
                (("Supply", "missing-move"), 2),
            ]),
            "named compatibility tail changed"
        );
    }
}

fn expected_command_kind(action: &str) -> &'static str {
    match action {
        "AttackSeam" | "Fire" => "MoveAttack",
        "Build" => "ProduceUnit",
        "Capt" => "MoveCapture",
        "Delete" => "DeleteUnit",
        "End" => "EndTurn",
        "Explode" => "MoveExplode",
        "Hide" => "MoveHide",
        "Join" => "MoveJoin",
        "Launch" => "MoveLaunch",
        "Load" => "MoveLoad",
        "Move" => "MoveWait",
        "Power" => "ActivatePower",
        "Repair" => "MoveRepair",
        "Resign" => "Resign",
        "Supply" => "MoveSupply",
        "Tag" => "Tag",
        "Unhide" => "MoveReveal",
        "Unload" => "Unload",
        _ => panic!("unknown archived action kind {action}"),
    }
}

fn command_kind(command: &Command) -> &'static str {
    match command {
        Command::MoveWait { .. } => "MoveWait",
        Command::MoveAttack { .. } => "MoveAttack",
        Command::MoveLaunch { .. } => "MoveLaunch",
        Command::MoveExplode { .. } => "MoveExplode",
        Command::DeleteUnit { .. } => "DeleteUnit",
        Command::MoveHide { .. } => "MoveHide",
        Command::MoveReveal { .. } => "MoveReveal",
        Command::MoveCapture { .. } => "MoveCapture",
        Command::ProduceUnit { .. } => "ProduceUnit",
        Command::MoveJoin { .. } => "MoveJoin",
        Command::MoveSupply { .. } => "MoveSupply",
        Command::MoveRepair { .. } => "MoveRepair",
        Command::MoveLoad { .. } => "MoveLoad",
        Command::Unload { .. } => "Unload",
        Command::ActivatePower { .. } => "ActivatePower",
        Command::Tag { .. } => "Tag",
        Command::EndTurn { .. } => "EndTurn",
        Command::Resign { .. } => "Resign",
        Command::Timeout { .. } => "Timeout",
        Command::Unsupported => "Unsupported",
    }
}
