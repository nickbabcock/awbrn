#[test]
fn txt_json_equivalency() {
    let json_data = std::fs::read("../../assets/maps/162795.json").unwrap();
    let txt_data = std::fs::read_to_string("../../assets/maps/162795.txt").unwrap();

    let json_map = awbrn_map::AwbwMap::parse_json(&json_data[..]).unwrap();
    let txt_map = awbrn_map::AwbwMap::parse_txt(&txt_data[..]).unwrap();

    assert_eq!(json_map.width(), txt_map.width());
    assert_eq!(json_map.height(), txt_map.height());

    for (i, (txt, json)) in txt_map.iter().zip(json_map.iter()).enumerate() {
        assert_eq!(txt.0, json.0, "Pos mismatch at index {}", i);
        assert_eq!(txt.1, json.1, "Terrain mismatch at index {}", i);
    }

    // The text format stores terrain only.
    assert!(txt_map.deployments().is_empty());
    assert_eq!(json_map.deployments().len(), 4);

    // Deployments come out row-major, so the first is the topmost, leftmost.
    let (position, first) = json_map.deployments().iter().next().unwrap();
    assert_eq!(first.unit, awbrn_types::Unit::Infantry);
    assert_eq!(position, awbrn_map::Pos::new(12, 9));
    assert_eq!(first.faction, awbrn_types::PlayerFaction::BlackHole);
    assert_ne!(json_map, txt_map);
}
