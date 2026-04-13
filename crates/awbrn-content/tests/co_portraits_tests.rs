use awbrn_content::{co_portrait_by_awbw_id, co_portraits};
use awbrn_types::AwbwCoId;
use std::collections::HashSet;

#[test]
fn co_portrait_lookup_uses_stable_keys() {
    let adder = co_portrait_by_awbw_id(AwbwCoId::new(11)).expect("Adder should exist");
    assert_eq!(adder.key(), "adder");
    assert_eq!(adder.display_name(), "Adder");

    let von_bolt = co_portrait_by_awbw_id(AwbwCoId::new(30)).expect("Von Bolt should exist");
    assert_eq!(von_bolt.key(), "von-bolt");
    assert_eq!(von_bolt.display_name(), "Von Bolt");

    let no_co = co_portrait_by_awbw_id(AwbwCoId::new(31)).expect("No CO should exist");
    assert_eq!(no_co.key(), "no-co");
    assert_eq!(no_co.display_name(), "No CO");
}

#[test]
fn co_portrait_catalog_has_unique_ids_and_keys() {
    let portraits = co_portraits();
    assert!(
        !portraits.is_empty(),
        "CO portrait catalog should not be empty"
    );

    let mut ids = HashSet::new();
    let mut keys = HashSet::new();
    for portrait in portraits {
        assert!(
            ids.insert(portrait.awbw_id()),
            "duplicate CO awbw_id {}",
            portrait.awbw_id().as_u32()
        );
        assert!(
            keys.insert(portrait.key()),
            "duplicate CO key {}",
            portrait.key()
        );
    }
}
