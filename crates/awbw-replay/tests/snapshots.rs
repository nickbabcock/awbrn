use awbw_replay::{ReplayEntriesKind, ReplayFile};
use highway::HighwayHash;
use insta::{assert_json_snapshot, glob};
use serde::{Deserialize, Serialize};
use std::io::{BufReader, BufWriter};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ZipEntry {
    file_path: String,
    file_size: u64,
    data: ZipEntryKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum ZipEntryKind {
    Game { count: usize },
    Turn { turns: Vec<TurnEntry> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TurnEntry {
    player_id: u32,
    day: u32,
    elements: usize,
}

#[test]
fn test_replay_snapshots() {
    glob!("../../../assets/replays", "*.zip", |path| {
        let data = std::fs::read(path).unwrap();

        let file = ReplayFile::open(&data[..]).unwrap();

        let mut file_entries = Vec::new();
        let mut sink = Vec::new();
        for entry in file.iter() {
            let entry_reader = BufReader::new(entry.get_reader().unwrap());
            let data = match ReplayEntriesKind::classify(entry_reader).unwrap() {
                ReplayEntriesKind::Game(mut game_entries) => {
                    let mut entries = 0;
                    while game_entries.next_entry(&mut sink).unwrap().is_some() {
                        entries += 1;
                    }
                    ZipEntryKind::Game { count: entries }
                }
                ReplayEntriesKind::Turn(mut turn_entries) => {
                    let mut turns = Vec::new();
                    while let Some(turn) = turn_entries.next_entry(&mut sink).unwrap() {
                        let turn_content = turn.parse().unwrap();
                        let mut elements = 0;
                        for _ in turn_content.actions().unwrap() {
                            elements += 1;
                        }

                        turns.push(TurnEntry {
                            player_id: turn_content.player_id(),
                            day: turn_content.day(),
                            elements,
                        });
                    }

                    ZipEntryKind::Turn { turns }
                }
            };

            file_entries.push(ZipEntry {
                file_path: String::from(entry.file_path().try_normalize().unwrap().as_str()),
                file_size: entry.uncompressed_size_hint(),
                data,
            });
        }

        let parser = awbw_replay::ReplayParser::new().with_debug(true);
        let parsed = parser.parse(&data[..]);

        let replay = match parsed {
            Ok(x) => x,
            Err(e) => panic!(
                "failed parsing: (INSTA_GLOB_FILTER={}) {}",
                path.file_name().unwrap().to_string_lossy(),
                e
            ),
        };

        let hasher = highway::HighwayHasher::new(highway::Key::default());

        // HighwayHash is fast, but we still want to buffer writes as much as
        // possible. Makes tests run 3x faster in release mode.
        let mut writer = BufWriter::with_capacity(0x8000, hasher);
        serde_json::to_writer(&mut writer, &replay).unwrap();
        let hash = writer.into_inner().unwrap().finalize256();
        let hex = format!(
            "0x{:016x}{:016x}{:016x}{:016x}",
            hash[0], hash[1], hash[2], hash[3]
        );

        let snapshot = serde_json::json!({
            "file_entries": file_entries,
            "checksum": hex,
        });
        assert_json_snapshot!(snapshot);
    });
}
