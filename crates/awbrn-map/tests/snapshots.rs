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

        assert_snapshot!(map.to_string())
    });
}
