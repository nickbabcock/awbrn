//! Canonical map-document and digest tests.

use awbrn_map::{
    AwbrnMapDocument, AwbrnMapMetadata, AwbrnMapUnit, AwbwMap, AwbwMapData, MAX_DIMENSION,
    MapError, Position, ValidatedMapDocument,
};
use awbrn_types::{AwbwTerrain, FactionCode, PlayerFaction, Unit};
use awvm::semantic::Dimensions;
use insta::assert_snapshot;
use rstest::rstest;

fn document(map_id: &str) -> ValidatedMapDocument {
    let raw = std::fs::read(format!(
        "{}/../../assets/maps/{map_id}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let data: AwbwMapData = serde_json::from_slice(&raw).unwrap();
    ValidatedMapDocument::try_from(&data).unwrap()
}

/// Minimal document used by the unit tests.
fn tiny() -> AwbrnMapDocument {
    let map = AwbwMap::parse_txt("1,2\n34,42").unwrap();
    let units = vec![AwbrnMapUnit {
        position: Position::new(1, 1),
        unit: Unit::Tank,
        faction: FactionCode::from(PlayerFaction::OrangeStar),
        hp: 10,
    }];

    AwbrnMapDocument::from_awbw_map(&map, units, metadata("Tiny", "nobody"))
}

/// Validated form of [`tiny`].
fn tiny_valid() -> ValidatedMapDocument {
    tiny().validate().unwrap()
}

fn metadata(name: &str, author: &str) -> AwbrnMapMetadata {
    AwbrnMapMetadata {
        name: name.to_string(),
        author: author.to_string(),
        player_count: 2,
    }
}

#[test]
fn tiny_map_encodes_exactly() {
    let document = tiny_valid();

    assert_eq!(
        document.content_preimage(),
        concat!(
            "awbrn-map-content-v1\n",
            r#"{"width":2,"height":2,"terrain":[1,2,34,42],"#,
            r#""units":[{"x":1,"y":1,"unit":"tank","faction":"os","hp":10}]}"#
        )
    );

    assert_eq!(
        document.property_preimage(),
        concat!(
            "awbrn-map-property-v1\n",
            r#"[{"x":0,"y":1,"terrain":34},{"x":1,"y":1,"terrain":42}]"#
        )
    );

    assert_eq!(
        document.unit_preimage(),
        concat!(
            "awbrn-map-unit-v1\n",
            r#"[{"x":1,"y":1,"unit":"tank","faction":"os"}]"#
        )
    );

    let digests = document.digests();
    assert_eq!(
        digests.content_hash.to_string(),
        "d587037827969ce690f27e43ef6dbab4fe1488a4c9bd48ab57a2931c8cccf3e0"
    );
    assert_eq!(
        digests.property_signature.to_string(),
        "175c5eee5c916dacf9bb0db4e1fb79856c6643a469be901a0bc42cdc5484fc4b"
    );
    assert_eq!(
        digests.unit_signature.to_string(),
        "feeb83ca535d4e549900bf0cea5cac58c0d9c75b42a18f162e7fa7a2f1ff88b5"
    );
}

/// Golden vectors from two real AWBW maps.
#[rstest]
#[case::foreign_invasion(
    "162795",
    "796d9e654604bae8b8dd6a946c07f0b83d500c6527f0d8ce40ac9117228b869b",
    "93836ec25c9aa17ab5d8092ce1a7fcb65073b629cdcc141aeafb36a488e52183",
    "14582243c58918a48ed9e66457eb2426b10ac145026c2401fe91f897c6237900"
)]
#[case::predeploys(
    "178597",
    "5e7fa0a76a0b35b69933f6fd239d172fc7078428123728304b2d8b11389ed885",
    "171f5ea00586c5990c5a2d05ad4287fb277f63de6e07f3ea95ee2def4bf9d250",
    "626e894f8bf2a864bfc9b151b8d3053f7aa43c3c98b3a0958e7dfba1527b5776"
)]
fn golden_digests(
    #[case] map_id: &str,
    #[case] content_hash: &str,
    #[case] property_signature: &str,
    #[case] unit_signature: &str,
) {
    let digests = document(map_id).digests();

    assert_eq!(digests.content_hash.to_string(), content_hash);
    assert_eq!(digests.property_signature.to_string(), property_signature);
    assert_eq!(digests.unit_signature.to_string(), unit_signature);
}

#[rstest]
#[case::foreign_invasion("162795")]
#[case::predeploys("178597")]
fn golden_preimages(#[case] map_id: &str) {
    let document = document(map_id);

    assert_snapshot!(format!("{map_id}-content"), document.content_preimage());
    assert_snapshot!(format!("{map_id}-property"), document.property_preimage());
    assert_snapshot!(format!("{map_id}-unit"), document.unit_preimage());
}

#[test]
fn signature_tiles_cover_properties_seams_and_silos() {
    // Expected replay building count.
    let document = document("162795");
    assert_eq!(
        document.property_preimage().matches("\"terrain\"").count(),
        89
    );
}

#[test]
fn predeployed_units_survive_the_import() {
    let document = document("178597");
    assert_eq!(document.units.len(), 27);

    // HP affects content, but not the unit signature.
    let mut damaged: Vec<u32> = document
        .units
        .iter()
        .filter(|unit| unit.hp != 10)
        .map(|unit| unit.hp)
        .collect();
    damaged.sort_unstable();
    assert_eq!(damaged, vec![4, 6]);
}

#[test]
fn metadata_does_not_affect_the_content_hash() {
    let original = document("162795");

    // Metadata edits require revalidation but do not change digests.
    let mut edited = original.clone().into_document();
    edited.metadata.name = "Domestic Invasion".to_string();
    edited.metadata.author = "somebody-else".to_string();
    let renamed = edited.validate().unwrap();

    assert_ne!(original.metadata, renamed.metadata);
    assert_eq!(original.content_hash(), renamed.content_hash());
    assert_eq!(original.property_signature(), renamed.property_signature());
    assert_eq!(original.unit_signature(), renamed.unit_signature());
}

#[test]
fn terrain_edits_change_the_content_hash() {
    let original = document("162795");

    let mut raw = original.clone().into_document();
    raw.terrain[0] = AwbwTerrain::Mountain;
    let edited = raw.validate().unwrap();

    assert_ne!(original.content_hash(), edited.content_hash());
    // Terrain is content, not replay fingerprint input.
    assert_eq!(original.property_signature(), edited.property_signature());
    assert_eq!(original.unit_signature(), edited.unit_signature());
}

#[test]
fn unit_hp_changes_the_content_hash_but_not_the_unit_signature() {
    let original = document("178597");

    let mut raw = original.clone().into_document();
    for unit in &mut raw.units {
        unit.hp = 10;
    }
    let healed = raw.validate().unwrap();

    assert_ne!(original.content_hash(), healed.content_hash());
    assert_eq!(original.unit_signature(), healed.unit_signature());
}

#[test]
fn unit_order_does_not_affect_the_digests() {
    let original = document("178597");

    let mut raw = original.clone().into_document();
    raw.units.reverse();
    let shuffled = raw.validate().unwrap();

    assert_eq!(original.content_hash(), shuffled.content_hash());
    assert_eq!(original.unit_signature(), shuffled.unit_signature());
}

#[rstest]
#[case::foreign_invasion("162795")]
#[case::predeploys("178597")]
fn round_trips_through_awbw_map(#[case] map_id: &str) {
    let document = document(map_id);
    let map = AwbwMap::try_from(&document).unwrap();

    // Both representations use row-major terrain.
    assert_eq!(map.width(), document.width as usize);
    assert_eq!(map.height(), document.height as usize);
    assert_eq!(
        map.terrain_at(Position::new(0, 0)),
        Some(document.terrain[0])
    );

    let rebuilt =
        AwbrnMapDocument::from_awbw_map(&map, document.units.clone(), document.metadata.clone());
    assert_eq!(rebuilt, *document);
}

#[rstest]
#[case::foreign_invasion("162795")]
#[case::predeploys("178597")]
fn round_trips_through_json(#[case] map_id: &str) {
    let document = document(map_id);

    let encoded = serde_json::to_string(&document).unwrap();
    // Checked deserialization revalidates.
    let decoded: ValidatedMapDocument = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, document);
    assert_eq!(decoded.content_hash(), document.content_hash());
}

#[test]
fn validation_rejects_contradictory_documents() {
    let mut truncated = tiny();
    truncated.terrain.pop();
    assert!(matches!(
        truncated.validate(),
        Err(MapError::TerrainSizeMismatch {
            expected: 4,
            found: 3
        })
    ));

    let mut escaped = tiny();
    escaped.units[0].position.x = 9;
    assert!(matches!(
        escaped.validate(),
        Err(MapError::UnitOutOfBounds { x: 9, y: 1 })
    ));

    let mut future = tiny();
    future.map_format = 2;
    assert!(matches!(
        future.validate(),
        Err(MapError::UnsupportedMapFormat { format: 2 })
    ));
}

/// Replay buildings omit pipe rubble.
#[test]
fn pipe_rubble_is_not_a_signature_tile() {
    let map = AwbwMap::parse_txt("115,116\n113,114").unwrap();
    let document = AwbrnMapDocument::from_awbw_map(&map, Vec::new(), metadata("Pipes", "nobody"))
        .validate()
        .unwrap();

    // Seams are included; rubble is not.
    assert_eq!(
        document.property_preimage().matches("\"terrain\"").count(),
        2
    );
    assert!(document.property_preimage().contains("\"terrain\":113"));
    assert!(document.property_preimage().contains("\"terrain\":114"));
    assert!(!document.property_preimage().contains("\"terrain\":115"));
    assert!(!document.property_preimage().contains("\"terrain\":116"));
}

/// Canonical ordering is row-major.
#[test]
fn canonical_orders_are_all_row_major() {
    let map = AwbwMap::parse_txt("1,1\n1,1").unwrap();
    let units = vec![
        AwbrnMapUnit {
            position: Position::new(1, 0),
            unit: Unit::Tank,
            faction: FactionCode::from(PlayerFaction::OrangeStar),
            hp: 10,
        },
        AwbrnMapUnit {
            position: Position::new(0, 1),
            unit: Unit::Infantry,
            faction: FactionCode::from(PlayerFaction::BlueMoon),
            hp: 10,
        },
    ];
    let document = AwbrnMapDocument::from_awbw_map(&map, units, metadata("Order", "nobody"))
        .validate()
        .unwrap();

    // Row-major order puts (1,0) before (0,1).
    let unit_preimage = document.unit_preimage();
    let first = unit_preimage.find("\"x\":1,\"y\":0").unwrap();
    let second = unit_preimage.find("\"x\":0,\"y\":1").unwrap();
    assert!(
        first < second,
        "unit preimage is not row-major: {unit_preimage}"
    );

    let content_preimage = document.content_preimage();
    let first = content_preimage.find("\"x\":1,\"y\":0").unwrap();
    let second = content_preimage.find("\"x\":0,\"y\":1").unwrap();
    assert!(
        first < second,
        "content preimage is not row-major: {content_preimage}"
    );
}

/// Checked and unchecked deserialization have different validation guarantees.
#[test]
fn deserializing_the_checked_type_rejects_invalid_documents() {
    let valid = serde_json::to_string(&tiny_valid()).unwrap();
    assert!(serde_json::from_str::<ValidatedMapDocument>(&valid).is_ok());

    let truncated = valid.replace("\"terrain\":[1,2,34,42]", "\"terrain\":[1,2,34]");
    assert!(serde_json::from_str::<AwbrnMapDocument>(&truncated).is_ok());

    let error = serde_json::from_str::<ValidatedMapDocument>(&truncated).unwrap_err();
    assert!(
        error.to_string().contains("Terrain size mismatch"),
        "unexpected error: {error}"
    );
}

#[test]
fn validate_rejects_impossible_units() {
    let mut dead = tiny();
    dead.units[0].hp = 0;
    assert!(matches!(
        dead.validate(),
        Err(MapError::UnitHpOutOfRange { hp: 0, .. })
    ));

    let mut overheal = tiny();
    overheal.units[0].hp = 11;
    assert!(matches!(
        overheal.validate(),
        Err(MapError::UnitHpOutOfRange { hp: 11, .. })
    ));

    let mut stacked = tiny();
    stacked.units.push(stacked.units[0]);
    assert!(matches!(
        stacked.validate(),
        Err(MapError::UnitPositionOccupied { x: 1, y: 1 })
    ));
}

/// The VM's maximum axis is accepted.
#[test]
fn validate_accepts_the_widest_board_the_vm_can_address() {
    let mut widest = tiny();
    widest.width = MAX_DIMENSION;
    widest.height = 1;
    widest.terrain = vec![widest.terrain[0]; MAX_DIMENSION as usize];
    widest.units.clear();

    let widest = widest
        .validate()
        .expect("the widest board is a valid document");

    // Validated documents fit the VM board.
    let width = u8::try_from(widest.width).expect("a valid document fits the VM board");
    let height = u8::try_from(widest.height).expect("a valid document fits the VM board");
    assert_eq!(Dimensions::new(width, height).width(), Dimensions::MAX_AXIS);
}

/// Dimensions beyond the VM limit are rejected during document validation.
#[test]
fn validate_rejects_dimensions_wider_than_the_vm_board() {
    let mut huge = tiny();
    huge.width = MAX_DIMENSION + 1;
    huge.height = 65_536;

    assert!(matches!(
        huge.validate(),
        Err(MapError::DimensionsOutOfRange { limit, .. }) if limit == MAX_DIMENSION
    ));
}
