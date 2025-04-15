#[test]
fn txt_json_equivalency() {
    let json_data = std::fs::read("../../assets/maps/162795.json").unwrap();
    let txt_data = std::fs::read_to_string("../../assets/maps/162795.txt").unwrap();

    let json_map = awbrn_map::AwbwMap::parse_json(&json_data[..]).unwrap();
    let txt_map = awbrn_map::AwbwMap::parse_txt(&txt_data[..]).unwrap();

    assert_eq!(json_map.width(), txt_map.width());
    assert_eq!(json_map.height(), txt_map.height());

    for (i, (txt, json)) in txt_map.iter().zip(json_map.iter()).enumerate() {
        assert_eq!(txt.0, json.0, "Position mismatch at index {}", i);
        assert_eq!(txt.1, json.1, "Terrain mismatch at index {}", i);
    }

    assert_eq!(json_map, txt_map);
}
