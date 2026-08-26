//! Map 61748 is the arena board. This test holds it to what the arena needs.
//!
//! The arena plays each pairing with both seat orders to control for the
//! first-player advantage, and that control isolates agent strength only on a
//! fair board. A board that gives one seat more income, or a shorter road to
//! the enemy headquarters, makes the seat swap measure the map.
//!
//! The map starts one Teal Galaxy infantry and nothing for Pink Cosmos,
//! because Pink Cosmos moves first. That unit is the map's payment for the
//! first-turn advantage, and a loader that dropped it would take the payment
//! away while leaving the advantage.

use awbrn_map::AwbwMap;
use awbrn_types::{AwbwTerrain, Faction, PlayerFaction};

fn arena_map() -> AwbwMap {
    let data = std::fs::read("../../assets/maps/61748.json").expect("the arena map is in assets");
    AwbwMap::parse_json(&data).expect("the arena map parses")
}

#[test]
fn the_map_pays_the_second_seat_for_moving_second() {
    let map = arena_map();
    let (position, paid) = map
        .deployments()
        .iter()
        .next()
        .expect("the map starts one unit");

    assert_eq!(paid.faction, PlayerFaction::TealGalaxy);
    assert_eq!(paid.unit, awbrn_types::Unit::Infantry);
    assert_eq!(paid.hp.get(), 10, "a full unit");
    // It stands on its owner's own base, which is where compensation for
    // moving second belongs: it is a head start, not a forward position.
    assert!(matches!(
        map.terrain_at(position),
        Some(AwbwTerrain::Property(property))
            if property.kind() == awbrn_types::PropertyKind::Base
                && property.faction() == Faction::Player(PlayerFaction::TealGalaxy)
    ));
}

#[test]
fn the_map_has_two_headquarters_and_six_bases() {
    let map = arena_map();
    let mut headquarters = 0;
    let mut bases = 0;
    let mut neutral_cities = 0;

    for (_, terrain) in map.iter() {
        let AwbwTerrain::Property(property) = terrain else {
            continue;
        };
        match property {
            awbrn_types::Property::HQ(_) => headquarters += 1,
            awbrn_types::Property::Base(_) => bases += 1,
            awbrn_types::Property::City(Faction::Neutral) => neutral_cities += 1,
            _ => {}
        }
    }

    assert_eq!(headquarters, 2, "two seats, one headquarters each");
    assert_eq!(bases, 6, "the map has six bases");
    // Capture and income are the two terms a hand-written tactics AI most
    // often gets wrong, so the board has to offer something to capture.
    assert!(neutral_cities > 0, "the board has neutral cities to take");
}
