//! Canonical map-document and digest tests.

use awbrn_map::{
    AwbrnMapDocument, AwbrnMapMetadata, AwbrnMapUnit, AwbwMap, AwbwMapData, MAP_FORMAT,
    MAX_DIMENSION, MapError, Pos, ValidatedMapDocument,
};
use awbrn_types::{AwbwTerrain, FactionCode, PlayerFaction, Unit, VisualHp};
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
///
/// Built as the wire shape rather than from a map, because most of these tests
/// edit it into something a map could not hold.
fn tiny() -> AwbrnMapDocument {
    let map = AwbwMap::parse_txt("1,2\n34,42").unwrap();

    AwbrnMapDocument {
        map_format: MAP_FORMAT,
        width: 2,
        height: 2,
        terrain: map.iter().map(|(_, terrain)| terrain).collect(),
        units: vec![AwbrnMapUnit {
            position: Pos::new(1, 1),
            unit: Unit::Tank,
            faction: FactionCode::from(PlayerFaction::OrangeStar),
            hp: VisualHp::new(10),
        }],
        metadata: metadata("Tiny", "nobody"),
    }
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
            r#""units":[{"position":[1,1],"unit":"tank","faction":"os","hp":10}]}"#
        )
    );

    assert_eq!(
        document.property_preimage(),
        concat!(
            "awbrn-map-property-v1\n",
            r#"[{"position":[0,1],"terrain":34},{"position":[1,1],"terrain":42}]"#
        )
    );

    assert_eq!(
        document.unit_preimage(),
        concat!(
            "awbrn-map-unit-v1\n",
            r#"[{"position":[1,1],"unit":"tank","faction":"os"}]"#
        )
    );

    let digests = document.digests();
    assert_eq!(
        digests.content_hash.to_string(),
        "27cc3819a486150de060e9ef1c88cb3a4d075de13c0fe4234c9f687c439912e0"
    );
    assert_eq!(
        digests.property_signature.to_string(),
        "50fcf6dcf5b059776004693c43a18630ab4fa67a261e9603901589949c23f65f"
    );
    assert_eq!(
        digests.unit_signature.to_string(),
        "a0a13fa3743f0879345d76df36cd6da41eb8be853592cb37fbadad34828b96ad"
    );
}

/// Golden vectors from two real AWBW maps.
#[rstest]
#[case::foreign_invasion(
    "162795",
    "be64764fdc31f5678b311b1e2bc33481bf9be9bdb293f3a0d9987429bf477fde",
    "880c0f66e63fc0779cd7ab9a39b0a792c5ae558e0eaeb66762f1935ad57d327f",
    "544cbe32215ef3182757aa0d05ce4c30b23b2d2e18cd5096682a97c655df4fcc"
)]
#[case::predeploys(
    "178597",
    "dd00fba3fb8ba692b778b01ada39ddc0673ae654e732d24490f1f0515303ad40",
    "20afe95cf6626b44b594a976c20bfce6db81827d98f3695b12792f745298d21e",
    "55453914790832d66556ca34be53389b5c2ccd13decc358e27f13d81dff76b6b"
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
    assert_eq!(document.map().deployments().len(), 27);

    // HP affects content, but not the unit signature.
    let mut damaged: Vec<u8> = document
        .map()
        .deployments()
        .iter()
        .map(|(_, deployment)| deployment.hp.get())
        .filter(|hp| *hp != 10)
        .collect();
    damaged.sort_unstable();
    assert_eq!(damaged, vec![4, 6]);
}

#[test]
fn metadata_does_not_affect_the_content_hash() {
    let original = document("162795");

    // Metadata edits require revalidation but do not change digests.
    let mut edited = original.to_document();
    edited.metadata.name = "Domestic Invasion".to_string();
    edited.metadata.author = "somebody-else".to_string();
    let renamed = edited.validate().unwrap();

    assert_ne!(original.metadata(), renamed.metadata());
    assert_eq!(original.content_hash(), renamed.content_hash());
    assert_eq!(original.property_signature(), renamed.property_signature());
    assert_eq!(original.unit_signature(), renamed.unit_signature());
}

#[test]
fn terrain_edits_change_the_content_hash() {
    let original = document("162795");

    let mut raw = original.to_document();
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

    let mut raw = original.to_document();
    for unit in &mut raw.units {
        unit.hp = VisualHp::new(10);
    }
    let healed = raw.validate().unwrap();

    assert_ne!(original.content_hash(), healed.content_hash());
    assert_eq!(original.unit_signature(), healed.unit_signature());
}

#[test]
fn unit_order_does_not_affect_the_digests() {
    let original = document("178597");

    let mut raw = original.to_document();
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
    let wire = document.to_document();
    let map = document.map();

    // Both representations use row-major terrain.
    assert_eq!(u32::from(map.width()), wire.width);
    assert_eq!(u32::from(map.height()), wire.height);
    assert_eq!(map.terrain_at(Pos::new(0, 0)), Some(wire.terrain[0]));
    assert_eq!(map.deployments().len(), wire.units.len());

    // The validated form holds the map, so rebuilding the wire shape from that
    // map is the identity rather than a conversion that could disagree.
    let rebuilt = AwbrnMapDocument::from_awbw_map(map, wire.metadata.clone());
    assert_eq!(rebuilt, wire);
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
    let document = AwbrnMapDocument::from_awbw_map(&map, metadata("Pipes", "nobody"))
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

    // Listed bottom-left first, so only the collection's own order can put the
    // top-right unit ahead of it.
    let mut document = AwbrnMapDocument::from_awbw_map(&map, metadata("Order", "nobody"));
    document.units = vec![
        AwbrnMapUnit {
            position: Pos::new(0, 1),
            unit: Unit::Infantry,
            faction: FactionCode::from(PlayerFaction::BlueMoon),
            hp: VisualHp::new(10),
        },
        AwbrnMapUnit {
            position: Pos::new(1, 0),
            unit: Unit::Tank,
            faction: FactionCode::from(PlayerFaction::OrangeStar),
            hp: VisualHp::new(10),
        },
    ];
    let document = document.validate().unwrap();

    // Row-major order puts (1,0) before (0,1).
    let unit_preimage = document.unit_preimage();
    let first = unit_preimage.find("[1,0]").unwrap();
    let second = unit_preimage.find("[0,1]").unwrap();
    assert!(
        first < second,
        "unit preimage is not row-major: {unit_preimage}"
    );

    let content_preimage = document.content_preimage();
    let first = content_preimage.find("[1,0]").unwrap();
    let second = content_preimage.find("[0,1]").unwrap();
    assert!(
        first < second,
        "content preimage is not row-major: {content_preimage}"
    );
}

/// Checked and unchecked deserialization have different validation guarantees.
#[test]
fn deserializing_the_checked_type_rejects_invalid_documents() {
    let valid = serde_json::to_string(&tiny_valid()).unwrap();
    serde_json::from_str::<ValidatedMapDocument>(&valid).unwrap();

    let truncated = valid.replace("\"terrain\":[1,2,34,42]", "\"terrain\":[1,2,34]");
    serde_json::from_str::<AwbrnMapDocument>(&truncated).unwrap();

    let error = serde_json::from_str::<ValidatedMapDocument>(&truncated).unwrap_err();
    assert!(
        error.to_string().contains("Terrain size mismatch"),
        "unexpected error: {error}"
    );
}

#[test]
fn validate_rejects_impossible_units() {
    let mut dead = tiny();
    dead.units[0].hp = VisualHp::new(0);
    assert!(matches!(
        dead.validate(),
        Err(MapError::UnitHpOutOfRange { hp: 0, .. })
    ));

    let mut overheal = tiny();
    overheal.units[0].hp = VisualHp::new(11);
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

    // A validated document holds a board, so its shape needs no reconversion.
    assert_eq!(widest.map().dimensions().width(), Dimensions::MAX_AXIS);
    assert_eq!(widest.map().dimensions().height(), 1);
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
