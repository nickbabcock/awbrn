//! Every map shipped in `assets/` is one the loader accepts.
//!
//! Predeployed units are keyed by tile now, so a map that puts two units on
//! one tile is rejected where a list of them was accepted before. This walks
//! the shipped maps so that tightening cannot quietly drop one of them.

#[test]
fn every_map_asset_parses() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/maps");
    let mut checked = 0;

    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("json") => {
                let bytes = std::fs::read(&path).unwrap();
                awbrn_map::AwbwMap::parse_json(&bytes)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                checked += 1;
            }
            Some("txt") => {
                let text = std::fs::read_to_string(&path).unwrap();
                awbrn_map::AwbwMap::parse_txt(&text)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
                checked += 1;
            }
            _ => {}
        }
    }

    assert!(checked > 0, "no map assets were found to check");
}
