//! Map 174183 is the arena board. This test holds it to what the arena needs.
//!
//! The arena plays each pairing with both seat orders to control for the
//! first-player advantage, and that control isolates agent strength only on a
//! fair board. A board that gives one seat more income, or a shorter road to
//! the enemy headquarters, makes the seat swap measure the map.
//!
//! Fair does not mean identical. The terrain mirrors, and the units do not:
//! the map starts one Blue Moon infantry and nothing for Orange Star, because
//! Orange Star moves first. That unit is the map's payment for the first-turn
//! advantage, and a loader that dropped it would take the payment away while
//! leaving the advantage.

use awbrn_map::{AwbwMap, Pos};
use awbrn_types::{AwbwTerrain, Faction, GameplayTerrain, PlayerFaction};

fn arena_map() -> AwbwMap {
    let data = std::fs::read("../../assets/maps/174183.json").expect("the arena map is in assets");
    AwbwMap::parse_json(&data[..]).expect("the arena map parses")
}

/// The other player's counterpart of `terrain`.
fn swap_seats(terrain: GameplayTerrain) -> GameplayTerrain {
    let GameplayTerrain::Property(property) = terrain else {
        return terrain;
    };
    let owner = match property.faction() {
        Faction::Player(PlayerFaction::OrangeStar) => Faction::Player(PlayerFaction::BlueMoon),
        Faction::Player(PlayerFaction::BlueMoon) => Faction::Player(PlayerFaction::OrangeStar),
        other => other,
    };
    GameplayTerrain::Property(property.with_owner(owner))
}

#[test]
fn the_arena_map_is_a_fair_mirror() {
    let map = arena_map();
    let (width, height) = (map.width(), map.height());

    for (position, terrain) in map.iter() {
        let opposite = Pos::new(width - 1 - position.x, height - 1 - position.y);
        let facing = map
            .terrain_at(opposite)
            .expect("a rotated position is inside the same rectangle");
        // The comparison is on gameplay terrain, so a road that turns one way
        // here and the other way there is the same tile. Only the kind and the
        // owner decide what a seat gets.
        assert_eq!(
            swap_seats(terrain.gameplay_type()),
            facing.gameplay_type(),
            "{position:?} and its opposite {opposite:?} are not a mirrored pair",
        );
    }
}

#[test]
fn the_map_pays_the_second_seat_for_moving_second() {
    let map = arena_map();
    let (position, paid) = map
        .deployments()
        .iter()
        .next()
        .expect("the map starts one unit");

    assert_eq!(paid.faction, PlayerFaction::BlueMoon);
    assert_eq!(paid.unit, awbrn_types::Unit::Infantry);
    assert_eq!(paid.hp.get(), 10, "a full unit");
    // It stands on its owner's own base, which is where compensation for
    // moving second belongs: it is a head start, not a forward position.
    assert!(matches!(
        map.terrain_at(position),
        Some(AwbwTerrain::Property(property))
            if property.kind() == awbrn_types::PropertyKind::Base
                && property.faction() == Faction::Player(PlayerFaction::BlueMoon)
    ));
}

#[test]
fn the_arena_map_is_cheap_and_land_only() {
    let map = arena_map();

    // Arena cost is commands each second, times commands each game, times
    // games. A big board pays that cost three times over.
    assert!(map.width() <= 15 && map.height() <= 15);

    for (position, terrain) in map.iter() {
        // Naval adds a movement class the early tiers do not use, and a port
        // adds production the evaluation function cannot yet price.
        assert!(
            !matches!(
                terrain.gameplay_type(),
                GameplayTerrain::Sea | GameplayTerrain::Reef | GameplayTerrain::Shoal
            ),
            "{position:?} is water",
        );
        assert!(
            !matches!(terrain, AwbwTerrain::Property(property) if property.kind()
                == awbrn_types::PropertyKind::Port),
            "{position:?} is a port",
        );
    }
}

#[test]
fn each_seat_starts_with_a_headquarters_and_a_base() {
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
    assert_eq!(bases, 4, "two bases each seat");
    // Capture and income are the two terms a hand-written tactics AI most
    // often gets wrong, so the board has to offer something to capture.
    assert!(neutral_cities > 0, "the board has neutral cities to take");
}
