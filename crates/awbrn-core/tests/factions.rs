use awbrn_core::PlayerFaction;
use std::collections::HashSet;

// Helper function to generate all PlayerFaction variants
fn all_player_factions() -> Vec<PlayerFaction> {
    vec![
        PlayerFaction::AcidRain,
        PlayerFaction::AmberBlaze,
        PlayerFaction::AzureAsteroid,
        PlayerFaction::BlackHole,
        PlayerFaction::BlueMoon,
        PlayerFaction::BrownDesert,
        PlayerFaction::CobaltIce,
        PlayerFaction::GreenEarth,
        PlayerFaction::GreySky,
        PlayerFaction::JadeSun,
        PlayerFaction::NoirEclipse,
        PlayerFaction::OrangeStar,
        PlayerFaction::PinkCosmos,
        PlayerFaction::PurpleLightning,
        PlayerFaction::RedFire,
        PlayerFaction::SilverClaw,
        PlayerFaction::TealGalaxy,
        PlayerFaction::WhiteNova,
        PlayerFaction::YellowComet,
    ]
}

#[test]
fn test_country_code_is_reversible() {
    // Test that converting from a PlayerFaction to a country code and back
    // returns the original PlayerFaction
    for faction in all_player_factions() {
        let country_code = faction.to_country_code();
        let parsed_faction = PlayerFaction::from_country_code(country_code).unwrap();
        assert_eq!(
            faction, parsed_faction,
            "Country code '{}' did not reverse back to the original faction",
            country_code
        );
    }
}

#[test]
fn test_country_codes_are_unique() {
    // Test that no two PlayerFactions share the same country code
    let mut codes = HashSet::new();
    for faction in all_player_factions() {
        let country_code = faction.to_country_code();
        assert!(
            codes.insert(country_code),
            "Country code '{}' is used by multiple factions",
            country_code
        );
    }
}
