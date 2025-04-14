use highway::HighwayHash;
use insta::{assert_json_snapshot, glob};
use std::io::BufWriter;

#[test]
fn test_replay_snapshots() {
    glob!("../../../assets/replays", "*.zip", |path| {
        let data = std::fs::read(path).unwrap();
        let parser = awbw_replay::ReplayParser::new();
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
            "checksum": hex,
        });
        assert_json_snapshot!(snapshot);
    });
}
