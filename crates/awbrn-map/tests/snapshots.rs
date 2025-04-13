use awbrn_core::GraphicalTerrain;
use awbrn_map::Position;
use insta::{assert_snapshot, glob};

#[test]
fn test_map_snapshots() {
    glob!("../../../assets/maps", "*.txt", |path| {
        let data = std::fs::read_to_string(path).unwrap();
        let map = awbrn_map::AwbwMap::parse(&data[..]);

        let map = match map {
            Ok(x) => x,
            Err(e) => panic!(
                "failed parsing: (INSTA_GLOB_FILTER={}) {}",
                path.file_name().unwrap().to_string_lossy(),
                e
            ),
        };

        let awbrn_map = awbrn_map::AwbrnMap::from_map(&map);
        let mut stubby_mountains: Vec<Position> = awbrn_map
            .iter()
            .filter_map(|(pos, terrain)| {
                if matches!(terrain, GraphicalTerrain::StubbyMoutain) {
                    Some(pos)
                } else {
                    None
                }
            })
            .collect();
        stubby_mountains.sort_unstable();
        let stubbies = stubby_mountains
            .iter()
            .map(|pos| format!("({}, {})", pos.x, pos.y))
            .collect::<Vec<_>>()
            .join(", ");

        assert_snapshot!(format!("{}\nstubby mountains: {}", map, stubbies))
    });
}
